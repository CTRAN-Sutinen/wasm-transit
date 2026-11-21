// Package main is the sync server: the Go host for the capability
// modules.
//
// The division of labor is deliberate (see the repo README): the
// modules own all domain logic — the server loads the exact bytes
// the browser runs (sha256 in the manifest proves it) and calls the
// flat ABI. This host is scaffolding only: HTTP routing, static
// serving with Range support, the CORS proxy, the geocoder proxy
// (geocode.go), the published network
// (network.go) and streets (streets.go), publishing picks to the store
// (plans.go), and the manifest that pins their hashes and the sync
// module's. No domain decision is ever made here.
package main

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"log"
	"mime"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/signal"
	"path"
	"path/filepath"
	"reflect"
	"regexp"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"rustwasm/server/store"
)

func main() {
	f := serverFlags(flag.CommandLine)
	flag.Parse()
	if err := applyConfig(flag.CommandLine, f.configs); err != nil {
		log.Fatalf("config: %v", err)
	}

	mime.AddExtensionType(".wasm", "application/wasm")
	mime.AddExtensionType(".mjs", "text/javascript")
	mime.AddExtensionType(".pmtiles", "application/vnd.pmtiles")

	ctx := context.Background()
	host, err := NewWasmHost(ctx, *f.wasmPath)
	if err != nil {
		log.Fatalf("wasm host: %v", err)
	}
	log.Printf("sync module loaded: %s (%s)", *f.wasmPath, host.Hash())

	s := &server{
		dist:          *f.dist,
		wasm:          host,
		publisher:     *f.publisher,
		editsPath:     *f.editsPath,
		retentionDays: *f.retentionDays,
		fixedToday:    *f.today,
		migrateWasm:   *f.migrateWasm,
	}
	if *f.attachmentsURL != "" {
		if s.attachments, err = store.OpenBackend(context.Background(), *f.attachmentsURL); err != nil {
			log.Fatalf("attachments: %v", err)
		}
		log.Printf("attachments: %s (D-162)", *f.attachmentsURL)
	}
	// the work order adapter before the store's first manifest, which
	// offers it (services.work_orders)
	var woErr error
	if s.workOrders, woErr = newWorkOrders(*f.workOrdersSpec); woErr != nil {
		log.Fatal(woErr)
	}
	if s.workOrders != nil {
		log.Printf("work orders: the %s adapter (D-165)", s.workOrders.Name())
	}
	if *f.uiDist != "" && *f.storeURL == "" {
		log.Fatalf("-ui-dist commits each UI's bundle to the store: it needs -store")
	}
	if *f.storeURL != "" {
		st, err := store.OpenServing(*f.storeURL)
		if err != nil {
			log.Fatalf("store: %v", err)
		}
		s.store = st
		if *f.storeCache != "" {
			if err := st.KeepCache(*f.storeCache, *f.storeURL); err != nil {
				log.Fatalf("-store-cache: %v", err)
			}
			log.Printf("store cache: %s (a copy that starts while the bucket does not answer serves from it, read only)", *f.storeCache)
		}
		if s.plans, err = NewWasmHost(ctx, *f.plansWasm); err != nil {
			log.Fatalf("plans module: %v", err)
		}
		s.networkWasm = *f.networkWasm
		s.logWasm = *f.logWasm
		s.kindsPath = *f.kindsPath
		s.checkWasm = map[string]string{"authz": *f.authzWasm}
		if _, err := s.kindTable(); err != nil {
			log.Fatalf("store mode: %v", err)
		}
		// The modules by capability, before the seeds: a log's first
		// entry runs through its receive op (logkinds.go).
		s.serverModules = map[string]string{"sync": *f.wasmPath, "network": *f.networkWasm, "streets": *f.streetsWasm, "plans": *f.plansWasm, "migrate": *f.migrateWasm, "stops": *f.stopsWasm, "requests": *f.requestsWasm, "alerts": *f.alertsWasm, "notes": *f.notesWasm, "log": *f.logWasm}
		if *f.oidcIssuer != "" || *f.oidcJWKS != "" || *f.oidcKey != "" {
			s.serverModules["authz"] = *f.authzWasm
		}
		s.seedStore(*f.editsPath)
		s.commitPicksForward()
		s.seedPolicy(*f.policyFile)
		s.maybeCompactEdits()
		uiDists, err := parseUIDists(*f.uiDist)
		if err != nil {
			log.Fatalf("%v", err)
		}
		s.uiDists = uiDists
		s.syncBuild()
		log.Printf("sync store open: %s (serving: creates and reads, never deletes — `store gc` collects orphans)", *f.storeURL)
	}
	if *f.oidcIssuer != "" || *f.oidcJWKS != "" || *f.oidcKey != "" {
		if *f.oidcJWKS != "" && (*f.oidcIssuer == "" || *f.oidcAud == "") {
			log.Fatal("-oidc-jwks-url needs -oidc-issuer and -oidc-audience")
		}
		a, err := loadAuthz(ctx, oidcConfig{
			issuer: *f.oidcIssuer, jwksURL: *f.oidcJWKS, audience: *f.oidcAud,
			keyFile: *f.oidcKey, rolesClaim: *f.oidcRoles, nameClaim: *f.oidcName,
			clientID: *f.oidcClientID, scope: *f.oidcScope,
		}, *f.authzWasm, *f.policyFile)
		if err != nil {
			log.Fatalf("authz: %v", err)
		}
		s.authz = a
		log.Printf("auth ON: iss=%q aud=%q jwks=%q key=%q roles-claim=%q name-claim=%q (authz module %s)",
			*f.oidcIssuer, *f.oidcAud, *f.oidcJWKS, *f.oidcKey, a.cfg.rolesClaim, a.cfg.nameClaim, a.wasm.Hash())
	} else {
		log.Printf("auth OFF: no -oidc flags — writes are UNAUTHENTICATED (dev only; bind localhost)")
	}
	if *f.geocoderURL != "" {
		if s.geocoder, err = loadGeocoder(*f.geocoderURL, *f.geocoderPolicy); err != nil {
			log.Fatalf("geocoder: %v", err)
		}
	}
	if *f.feedPath != "" {
		s.feeder = &feedSource{src: newSource(*f.feedPath, "source.feed", "gtfs_feed", s.store), networkWasm: *f.networkWasm}
		s.feeder.onImport = func(feed *loadedFeed) {
			if err := s.recordFeed(context.Background(), feed); err != nil {
				log.Printf("stops: the feed's differences were not recorded: %v", err)
			}
		}
		if _, err := s.feeder.Load(); err != nil {
			log.Fatalf("feed: %v", err)
		}
	}
	if *f.streetsPath != "" {
		s.streets = &streetsSource{src: newSource(*f.streetsPath, "source.streets", "street_layer", s.store), streetsWasm: *f.streetsWasm}
		if _, err := s.publishedStreets(); err != nil {
			log.Fatalf("streets: %v", err)
		}
	}
	if *f.feedURL != "" && (*f.exportDir == "" || *f.feedPath != "") {
		log.Fatal("-feed-url is an export's (-export), in place of -feed: a server imports its feed")
	}
	s.feedURL = *f.feedURL
	if *f.exportDir != "" {
		if err := s.export(*f.exportDir, *f.liveFeeds); err != nil {
			log.Fatalf("export: %v", err)
		}
		return
	}
	s.edits = loadEdits(*f.editsPath)
	if s.store == nil {
		where := "in memory (lost on restart)"
		if *f.editsPath != "" {
			where = "in the file " + *f.editsPath + " (this machine only)"
		}
		log.Printf("DEV STORE: no -store — edits are kept %s, with no history; publishing picks and builds is off. Development only: a deployment runs with -store", where)
	}

	s.rtProxy = *f.rtProxy
	if s.rtProxy {
		log.Printf("-rt-proxy: /rt relays any http(s) URL — dev only; bind localhost")
	}

	ln, err := net.Listen("tcp", *f.addr)
	if err != nil {
		log.Fatal(err)
	}
	log.Printf("listening on http://%s (dist: %s)", ln.Addr(), *f.dist)
	if err := s.serve(ln, newHTTPServer(s.routes()), *f.drain); err != nil {
		log.Fatal(err)
	}
}

// dataPath is a default file's path: under $KO_DATA_PATH when the
// server runs from its image (`just server-image`: ko copies
// server/kodata/ there), else the path in a source tree.
func dataPath(inImage, inTree string) string {
	if root := os.Getenv("KO_DATA_PATH"); root != "" {
		return filepath.Join(root, inImage)
	}
	return inTree
}

// newHTTPServer bounds what one client can hold: a header must arrive
// within 10s (a slow client cannot pin a connection), a request body
// within 5 min (a build or a feed is large), an idle keep-alive closes
// after 2 min, and no response runs past 10 min. The response bound is
// loose on purpose: a phone on a poor signal downloads the street data
// slowly, and a cut-off download only makes it start again.
func newHTTPServer(h http.Handler) *http.Server {
	return &http.Server{
		Handler:           h,
		ReadHeaderTimeout: 10 * time.Second,
		ReadTimeout:       5 * time.Minute,
		WriteTimeout:      10 * time.Minute,
		IdleTimeout:       2 * time.Minute,
	}
}

// serve runs srv until SIGTERM or SIGINT, then shuts down gracefully:
// /readyz answers "draining" for drain (the balancer stops sending),
// then the listener closes and requests in flight get up to 30s.
func (s *server) serve(ln net.Listener, srv *http.Server, drain time.Duration) error {
	errc := make(chan error, 1)
	go func() { errc <- srv.Serve(ln) }()
	sig := make(chan os.Signal, 1)
	signal.Notify(sig, syscall.SIGTERM, os.Interrupt)
	defer signal.Stop(sig)
	select {
	case err := <-errc:
		return err
	case got := <-sig:
		log.Printf("%v: draining for %v (/readyz answers 503), then shutting down", got, drain)
	}
	s.draining.Store(true)
	time.Sleep(drain)
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	if err := srv.Shutdown(ctx); err != nil {
		return fmt.Errorf("shutdown: %w", err)
	}
	log.Printf("shut down cleanly")
	return nil
}

// routes answers the server's mux. /rt is a route only with -rt-proxy
// (D-67): a deployment's pages fetch the agency's feeds directly.
func (s *server) routes() *http.ServeMux {
	mux := http.NewServeMux()
	if s.rtProxy {
		mux.HandleFunc("/rt", s.handleProxy)
	}
	mux.HandleFunc("/healthz", s.handleHealthz)
	mux.HandleFunc("/readyz", s.handleReadyz)
	mux.HandleFunc("/manifest", s.handleManifest)
	mux.HandleFunc("/network", s.handleNetwork)
	mux.HandleFunc("/network/document", s.handleNetworkDocument)
	mux.HandleFunc("/network/boundary", s.handleNetworkBoundary)
	mux.HandleFunc("/streets", s.handleStreets)
	mux.HandleFunc("/walks", s.handleWalks)
	mux.HandleFunc("/edits", s.protectWrite(func(*http.Request) string { return "edits" }, s.handleEdits))
	mux.HandleFunc("/pointers/", s.protectWrite(pointerNameOf, s.handlePointers))
	mux.HandleFunc("/logs/", s.protectLog(s.handleLogs))
	mux.HandleFunc("/plans", s.protectWrite(func(*http.Request) string { return "plans" }, s.handlePlans))
	mux.HandleFunc("/plans/retire", s.protectWrite(func(*http.Request) string { return "plans" }, s.handleRetire))
	mux.HandleFunc("/plans/freeze", s.protectWrite(func(*http.Request) string { return "plans" }, s.handleFreeze))
	mux.HandleFunc("/plans/adopt", s.protectWrite(func(*http.Request) string { return "plans" }, s.handleAdopt))
	// Each kind's own write route (the kind table's `post`), answered by
	// its log's receive op (logkinds.go). Its write rule is its store kind.
	if t, err := s.kindTable(); err == nil {
		for _, k := range t.Kinds {
			if k.Post != nil && k.Receive != "" {
				mux.HandleFunc(k.Post.Path, s.protectWrite(s.keptRouteRule, s.handleKeptRoute))
			}
		}
	}
	mux.HandleFunc("/attachments", s.protectWrite(func(*http.Request) string { return "attachments" }, s.handleAttachments))
	mux.HandleFunc("/attachments/", s.protectSignedIn(s.handleAttachmentFile))
	mux.HandleFunc("/stop-requests/work-order", s.protectWrite(func(*http.Request) string { return "stop_requests" }, s.handleWorkOrder))
	mux.HandleFunc("/builds", s.protectWrite(func(r *http.Request) string { p, _ := buildsPointerOf(r); return p }, s.handleBuilds))
	mux.HandleFunc("/feed", s.handleFeed)
	mux.HandleFunc("/permissions", s.requireAuth(s.handlePermissions))
	mux.HandleFunc("/blobs/", s.handleBlob)
	mux.HandleFunc("/geocode/", s.handleGeocode)
	mux.HandleFunc("/", s.handleStatic)
	return mux
}

// outbound carries every request the server itself sends (the feed and
// streets sources, the JWKS, the geocoder, the /rt relay). It is
// http.DefaultTransport as it stands when the package starts. The
// tests replace http.DefaultTransport to check their own requests
// against the HTTP contract, and the server's requests must not pass
// through that check.
var outbound = http.DefaultTransport

// server holds the host's few pieces of state.
type server struct {
	dist      string
	wasm      *WasmHost
	feeder    *feedSource    // nil when no feed is configured
	streets   *streetsSource // nil when no street layer is configured
	publisher string
	geocoder  *geocoder // nil when no geocoder is configured
	// The GTFS URL an export names for the pages to fetch (D-132);
	// "" for a server, which imports its own feed.
	feedURL string

	// The /rt feed proxy (-rt-proxy, a dev convenience — D-67) and its
	// upstream timeout (0 = proxyTimeoutDefault).
	rtProxy      bool
	proxyTimeout time.Duration

	// The edit document. Opaque JSON: the artifact owns the schema
	// and every domain decision about it (merge, submit, validation)
	// — this host only stores and ferries it.
	editsMu sync.Mutex
	edits           json.RawMessage // the edit document (current version)
	editsPath       string          // the dev store's file; empty = in memory only

	// The durable sync store (store mode, -store): blobs + CAS'd
	// pointers. nil = the dev store (no -store): edits in memory or the
	// -edits file, no history, no publishing — development and -export.
	store         *store.Store
	retentionDays int

	// draining is set once shutdown begins: /readyz answers 503 so a
	// load balancer stops sending before the listener closes.
	draining atomic.Bool

	// Authorization state (nil = auth off, dev only): the IdP config,
	// the hosted cap-authz module, and the policy source.
	authz *authzState

	// Publishing picks (store mode): the hosted cap-plans module, the
	// cap-network module each publish instantiates, and the lock that
	// makes one publish one step.
	plans       *WasmHost
	networkWasm string
	// The log capability (cap-log) behind the logs the server folds
	// (caplog.go), loaded on first use and held for the lifetime.
	logWasm string
	logOnce sync.Once
	logHost *WasmHost
	logErr  error
	// The logs a receive op writes (logkinds.go), from the kind table;
	// and the server's one instance of each capability they use.
	keptOnce  sync.Once
	kept      map[string]*keptLog
	modulesMu sync.Mutex
	modules   map[string]*heldModule
	// The kind table (kinds.go, -kinds), read once; and the modules
	// that check a pointer write, by capability id (default
	// ../web/pkg/cap_<id>.wasm).
	kindsPath string
	kindsOnce sync.Once
	kinds     *kindTable
	kindsErr  error
	checkWasm map[string]string
	// The attachments' own bucket (attachments.go, -attachments); nil:
	// none are taken.
	attachments store.Backend
	// The work order system's adapter (workorders.go, -work-orders); nil:
	// none configured, a request's work order entered by hand.
	workOrders workOrderSystem
	// The migration capability (cap-migrate), loaded on first use
	// (migrator): an older document comes forward at receive.
	migrateWasm string
	migrateOnce sync.Once
	migrateHost *WasmHost
	migrateErr  error
	fixedToday  string           // -today: the date publishing reads, for test servers; "" = the host clock
	now         func() time.Time // the host clock (nil = time.Now); tests pin it
	// The agency's zone, as the last published pick answered it
	// (cap-network's document answer): today() reads it when no feed
	// is configured.
	zoneMu sync.Mutex
	zone   string

	// The picks' GTFS exports, by network hash (exportedPick).
	feedMu    sync.Mutex
	feedCache map[string][]byte

	// Gzipped blobs for transport, by hash (gzippedBlob).
	gzMu    sync.Mutex
	gzCache map[string][]byte

	// The manifest pointer's last committed key (store mode): the
	// manifest is re-committed when the published set changes.
	manifestMu  sync.Mutex
	manifestKey string

	// Builds (store mode; builds.go): -dist's release committed to the
	// `build` pointer and the pages served from its head. serverModules
	// names the modules this server hosts (role → path), retained in
	// each build; buildEvery bounds how often -dist is re-listed (0 =
	// buildEveryDefault).
	builds        buildState
	buildEvery    time.Duration
	serverModules map[string]string
	// uiDists are the UIs' bundles (-ui-dist), each committed to its own
	// build.<id> pointer (D-171).
	uiDists []*uiDist
}

// handleStatic serves the built frontend with the deployment's headers
// (headers.go): the baseline on every file, the policies on each rider
// page, and gzip for the text and wasm types.
func (s *server) handleStatic(w http.ResponseWriter, r *http.Request) {
	h := w.Header()
	for _, b := range baselineHeaders() {
		h.Set(b.name, b.value)
	}
	// Only a rider page needs its origins read (a store mode read).
	if page := strings.TrimPrefix(path.Clean(r.URL.Path), "/"); isRiderPage(page) {
		for _, p := range pageHeaders(page, s.connectOrigins()) {
			h.Set(p.name, p.value)
		}
	}
	gzipStatic(w, r, s.serveStatic)
}

// connectOrigins is the origins the rider pages fetch beyond the
// server's own: the live feeds the deployment names, and the agency's
// GTFS URL.
func (s *server) connectOrigins() []string {
	return feedOrigins(s.staticBytes("live-feeds.json"), s.feedURL)
}

// staticBytes is a served file's bytes: the pinned build's in store
// mode, else -dist's. Nil when there is none.
func (s *server) staticBytes(name string) []byte {
	if s.store != nil {
		if _, files := s.pinnedFiles(); files != nil {
			if f, ok := files[name]; ok {
				if data, _, err := s.store.GetBlob(f.Hash); err == nil {
					return data
				}
			}
		}
		return nil
	}
	data, _ := os.ReadFile(filepath.Join(s.dist, name))
	return data
}

// serveStatic serves the built frontend. http.ServeContent gives
// Range requests (PMTiles needs them), If-Modified-Since, and content
// types by extension — no hand-rolled 206s.
func (s *server) serveStatic(w http.ResponseWriter, r *http.Request) {
	upath := r.URL.Path
	if strings.Contains(upath, "..") {
		http.Error(w, "bad path", http.StatusBadRequest)
		return
	}
	name := path.Clean(strings.TrimPrefix(upath, "/"))
	if name == "" || name == "." {
		name = "index.html"
	}
	// Store mode: the pages are the pinned build's (builds.go); what the
	// build does not hold (the basemap) is -dist's.
	if s.store != nil {
		s.syncBuild()
		if s.serveBuildFile(w, r, name) {
			return
		}
	}
	file, err := os.Open(filepath.Join(s.dist, name))
	if err != nil {
		http.NotFound(w, r)
		return
	}
	defer file.Close()
	info, err := file.Stat()
	if err != nil || info.IsDir() {
		http.NotFound(w, r)
		return
	}
	w.Header().Set("Cache-Control", staticCacheControl(name))
	// ServeContent sniffs the content type by extension.
	http.ServeContent(w, r, info.Name(), info.ModTime(), file)
}

// A file whose name carries its content hash never changes: Vite's
// build output under assets/ (name-<8 chars>.ext) and the
// hash-addressed modules under caps/ (name-<sha256 hex>.wasm).
var (
	viteHashed   = regexp.MustCompile(`^assets/[^/]+-[A-Za-z0-9_-]{8}\.[a-z0-9]+$`)
	moduleHashed = regexp.MustCompile(`^caps/[^/]+-[0-9a-f]{64}\.wasm$`)
)

// staticCacheControl: content-addressed files are cached forever;
// everything else — the pages, caps/manifest.json, the deployment's
// config — revalidates on every use (a 304 by modification time), so
// an open page meets the new build on its next load.
func staticCacheControl(name string) string {
	if viteHashed.MatchString(name) || moduleHashed.MatchString(name) {
		return "public, max-age=31536000, immutable"
	}
	return "no-cache"
}

// The proxy's caps: an upstream that answers slower or larger is a
// 502 — a feed is a few MB at most.
const (
	proxyTimeoutDefault = 15 * time.Second
	proxyMaxBytes       = 32 << 20
)

// handleProxy relays an agency feed the browser cannot fetch directly
// (no Access-Control-Allow-Origin upstream) — a dev convenience, on
// only with -rt-proxy (D-67). It relays any http(s) URL for any
// caller, so the server binds localhost when it is on.
func (s *server) handleProxy(w http.ResponseWriter, r *http.Request) {
	raw := r.URL.Query().Get("url")
	if raw == "" {
		http.Error(w, "missing url parameter", http.StatusBadRequest)
		return
	}
	target, err := url.Parse(raw)
	if err != nil || (target.Scheme != "http" && target.Scheme != "https") {
		http.Error(w, "url must be http(s)", http.StatusBadRequest)
		return
	}
	timeout := s.proxyTimeout
	if timeout == 0 {
		timeout = proxyTimeoutDefault
	}
	ctx, cancel := context.WithTimeout(r.Context(), timeout)
	defer cancel()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, raw, nil)
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	resp, err := (&http.Client{Transport: outbound}).Do(req)
	if err != nil {
		http.Error(w, fmt.Sprintf("upstream fetch failed: %v", err), http.StatusBadGateway)
		return
	}
	defer resp.Body.Close()
	body, err := io.ReadAll(io.LimitReader(resp.Body, proxyMaxBytes+1))
	if err != nil {
		http.Error(w, fmt.Sprintf("upstream read failed: %v", err), http.StatusBadGateway)
		return
	}
	if len(body) > proxyMaxBytes {
		http.Error(w, "upstream answer past the proxy's size cap", http.StatusBadGateway)
		return
	}
	if ct := resp.Header.Get("Content-Type"); ct != "" {
		w.Header().Set("Content-Type", ct)
	}
	w.Header().Set("Access-Control-Allow-Origin", "*")
	w.WriteHeader(resp.StatusCode)
	w.Write(body)
}

// Manifest pins the published network, the GTFS zip it was imported
// from, the published plan set (the picks clients resolve), the
// published streets and walk table, the build the pages are served
// from (store mode), and the sync module this server runs, and lists the external services it proxies. `feed_version` is read
// from the feed's own feed_info.txt by the host (the host owns files
// and I/O; the module owns the format's domain).
type Manifest struct {
	NetworkHash string `json:"network_hash"` // the network GET /network serves
	// The import's boundary (the feed's numbering the live feeds name),
	// a blob of its own: sha256 of its bytes, as every blob is named.
	BoundaryHash string `json:"boundary_hash,omitempty"`
	// A serverless deployment that follows the agency's GTFS URL
	// (D-132; -export with -feed-url): no network is published, and
	// each page fetches this zip, imports it, and checks it daily.
	FeedURL     string  `json:"feed_url,omitempty"`
	FeedHash    string  `json:"feed_hash"` // "sha256:<hex>" of the GTFS zip it was imported from
	FeedVersion *string `json:"feed_version,omitempty"`
	StreetsHash string  `json:"streets_hash,omitempty"` // the streets document GET /streets serves
	WalksHash   string  `json:"walks_hash,omitempty"`   // the walk table GET /walks serves
	WasmHash    string  `json:"wasm_hash"`              // sha256 of the sync module this server runs
	AuthzHash   string  `json:"authz_hash,omitempty"`   // sha256 of the cap-authz module, when auth is on
	PolicyHash  string  `json:"policy_hash,omitempty"`  // the store's policy pointer head, store mode (D-61)
	// Each document of a log a receive op writes (logkinds.go), written
	// as `<store kind>_hash` (stops_hash, stop_requests_hash, ...), store
	// mode.
	Logged map[string]string `json:"-"`
	// The seq of each log's fold whose documents this manifest pins
	// (each log a receive op writes), store mode: a page sends it with a
	// write as read_seq (D-189; logkinds.go).
	LogSeqs     map[string]int64 `json:"log_seqs,omitempty"`
	BuildHash   string           `json:"build_hash,omitempty"` // the build the pages are served from (the build pointer head), store mode
	GeneratedAt int64            `json:"generated_at"`
	Publisher   *string          `json:"publisher,omitempty"`
	// The agency's zone (the network's operator timezone, as today()
	// counts in it): a page reads its clock there, never in the
	// device's zone (web/src/clock.ts). Empty while none is known.
	Timezone string `json:"timezone,omitempty"`
	// How a page signs in to write (with -oidc-client-id): the issuer,
	// the pages' public client, and the scopes it asks for. Absent:
	// sign-in is off, or the pages have no client of their own.
	Auth *ManifestAuth `json:"auth,omitempty"`
	// The external services this deployment offers, by name — a
	// service it does not list is off, and the frames that need it
	// say so (docs/architecture/web.md, "External services").
	Services map[string]Service `json:"services,omitempty"`
}

// MarshalJSON writes the manifest with each logged document's pin as a
// field of its own (`<store kind>_hash`).
func (m Manifest) MarshalJSON() ([]byte, error) {
	type plain Manifest
	raw, err := json.Marshal(plain(m))
	if err != nil || len(m.Logged) == 0 {
		return raw, err
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil {
		return nil, err
	}
	for field, hash := range m.Logged {
		fields[field], _ = json.Marshal(hash)
	}
	return json.Marshal(fields)
}

// UnmarshalJSON reads a manifest, each `<kind>_hash` field the struct does
// not name into Logged.
func (m *Manifest) UnmarshalJSON(raw []byte) error {
	type plain Manifest
	if err := json.Unmarshal(raw, (*plain)(m)); err != nil {
		return err
	}
	var fields map[string]json.RawMessage
	if err := json.Unmarshal(raw, &fields); err != nil {
		return err
	}
	known := map[string]bool{}
	t := reflect.TypeOf(plain{})
	for i := 0; i < t.NumField(); i++ {
		name, _, _ := strings.Cut(t.Field(i).Tag.Get("json"), ",")
		known[name] = true
	}
	for field, v := range fields {
		var hash string
		if strings.HasSuffix(field, "_hash") && !known[field] && json.Unmarshal(v, &hash) == nil {
			if m.Logged == nil {
				m.Logged = map[string]string{}
			}
			m.Logged[field] = hash
		}
	}
	return nil
}

// ManifestAuth is how a page signs in (D-58: writes need a token; reads
// stay public): the provider's issuer (its discovery document is
// <issuer>/.well-known/openid-configuration), the pages' public client,
// and the scopes a page asks for.
type ManifestAuth struct {
	Issuer   string `json:"issuer"`
	ClientID string `json:"client_id"`
	Scope    string `json:"scope"`
}

// Service is where a page asks an external service on this origin,
// and the query policy the path applies.
type Service struct {
	Path  string                    `json:"path"`
	Query map[string]map[string]any `json:"query,omitempty"`
}

// buildManifest assembles the manifest from the published set. The
// generated_at stamp is the publish time, not the request time, so a
// store-mode manifest blob is immutable between republishes.
func (s *server) buildManifest() (Manifest, error) {
	manifest := Manifest{
		WasmHash:    s.wasm.Hash(),
		GeneratedAt: time.Now().Unix(),
	}
	if s.feeder != nil {
		feed, err := s.feeder.Load()
		if err != nil {
			return manifest, fmt.Errorf("feed unavailable: %w", err)
		}
		manifest.NetworkHash = feed.networkHash
		manifest.BoundaryHash = feed.boundaryHash
		manifest.FeedHash = feed.hash
		if v := feed.version; v != "" {
			manifest.FeedVersion = &v
		}
	}
	if s.streets != nil {
		streets, err := s.publishedStreets()
		if err != nil {
			return manifest, fmt.Errorf("streets unavailable: %w", err)
		}
		manifest.StreetsHash = streets.streets.hash
		manifest.WalksHash = streets.walks.hash
	}
	if s.publisher != "" {
		manifest.Publisher = &s.publisher
	}
	manifest.FeedURL = s.feedURL
	if s.authz != nil && s.authz.cfg.clientID != "" {
		manifest.Auth = &ManifestAuth{Issuer: s.authz.cfg.issuer, ClientID: s.authz.cfg.clientID, Scope: s.authz.cfg.scope}
	}
	if loc := s.agencyZone(); loc != nil {
		manifest.Timezone = loc.String()
	}
	if s.geocoder != nil {
		manifest.Services = map[string]Service{"geocoder": s.geocoder.service()}
	}
	if s.attachments != nil {
		if manifest.Services == nil {
			manifest.Services = map[string]Service{}
		}
		manifest.Services["attachments"] = Service{Path: "/attachments"}
	}
	if s.workOrders != nil && s.store != nil {
		if manifest.Services == nil {
			manifest.Services = map[string]Service{}
		}
		manifest.Services["work_orders"] = Service{Path: "/stop-requests/work-order"}
	}
	if s.authz != nil {
		manifest.AuthzHash = s.authz.wasm.Hash()
	}
	if s.store != nil {
		if head, found, err := s.store.Head("policy"); err == nil && found {
			manifest.PolicyHash = head.Hash
		}
		// Each log's documents and the seq of the fold they are, read
		// under one lock: a page sends that seq with a write (read_seq),
		// so the write is checked against the records it changes.
		pins, seqs := s.keptPins()
		manifest.Logged = pins
		for name, seq := range seqs {
			if manifest.LogSeqs == nil {
				manifest.LogSeqs = map[string]int64{}
			}
			manifest.LogSeqs[name] = seq
		}
		if head, found, err := s.store.Head(buildPointer); err == nil && found {
			manifest.BuildHash = head.Hash
		}
	}
	return manifest, nil
}

// manifestKeyOf identifies the published set a manifest pins (everything
// but the generated_at stamp): a change re-commits the manifest pointer.
func manifestKeyOf(m Manifest) string {
	services, _ := json.Marshal(m.Services)
	seqs, _ := json.Marshal(m.LogSeqs)
	sum := sha256.Sum256([]byte(strings.Join([]string{string(seqs),
		m.WasmHash, m.AuthzHash, m.PolicyHash, fmt.Sprint(m.Logged), m.BuildHash, m.NetworkHash, m.BoundaryHash, m.FeedHash, orEmpty(m.FeedVersion),
		m.StreetsHash, m.WalksHash, orEmpty(m.Publisher), m.Timezone, string(services), m.FeedURL, fmt.Sprint(m.Auth),
	}, "|")))
	return hex.EncodeToString(sum[:])
}

func orEmpty(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

func (s *server) handleManifest(w http.ResponseWriter, r *http.Request) {
	s.syncBuild() // a release that landed in -dist is the build the manifest pins
	manifest, err := s.buildManifest()
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	if s.store == nil {
		// The dev store: the manifest is ephemeral, served as built; its
		// ETag names what it pins, so an open page's poll is a 304
		// while nothing moved (D-63).
		etag := `"` + manifestKeyOf(manifest) + `"`
		w.Header().Set("ETag", etag)
		w.Header().Set("Cache-Control", "no-cache")
		if r.Header.Get("If-None-Match") == etag {
			w.WriteHeader(http.StatusNotModified)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(manifest)
		return
	}
	// Store mode: the manifest is the ROOT pointer — publishing is one
	// CAS on it, and clients resolve the published set through it
	// (docs/FORMATS.md). GET serves the pointer's blob.
	s.manifestMu.Lock()
	key := manifestKeyOf(manifest)
	if key != s.manifestKey {
		body, _ := json.Marshal(manifest)
		env := store.Envelope{Kind: "manifest", SchemaVersion: 1, ProducedBy: "server"}
		for attempt := 0; attempt < 5; attempt++ {
			head, found, err := s.store.Head("manifest")
			if err != nil {
				s.manifestMu.Unlock()
				fail(w, err, http.StatusInternalServerError)
				return
			}
			from := ""
			if found {
				from = head.Hash
			}
			_, _, err = s.store.CAS("manifest", from, body, env, "", "server", time.Now().UnixMilli())
			if errors.Is(err, store.ErrUnavailable) {
				break // D-119: serve the cached manifest below, labelled
			}
			if err != nil {
				var ce *store.ConflictError
				if errors.As(err, &ce) && attempt < 4 {
					continue
				}
				s.manifestMu.Unlock()
				fail(w, err, http.StatusInternalServerError)
				return
			}
			s.manifestKey = key
			break
		}
	}
	s.manifestMu.Unlock()

	head, found, err := s.store.Head("manifest")
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	if !found {
		fail(w, store.ErrUnavailable, http.StatusInternalServerError)
		return
	}
	data, _, err := s.store.GetBlob(head.Hash)
	if err != nil {
		fail(w, err, http.StatusInternalServerError)
		return
	}
	labelStale(w, head)
	etag := `"` + head.Hash + `"`
	w.Header().Set("ETag", etag)
	w.Header().Set("Cache-Control", "no-cache")
	if r.Header.Get("If-None-Match") == etag {
		w.WriteHeader(http.StatusNotModified)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	w.Write(data)
}

// seedStore prepares a fresh or older store at startup: each log a
// receive op writes takes its first entry from its kinds' old pointers'
// heads (logkinds.go, seedKept): the disruptions log from the old edits
// pointer, else the dev store's -edits file; the stops log from the old
// stops and stop_requests pointers; the plans log from the old plans
// pointer.
func (s *server) seedStore(editsPath string) {
	s.describeStore()
	s.seedKept(map[string]seedFile{"edit_records": {flag: "edits", path: editsPath}})
}

// describeStore writes the store's description, create only
// (store/describe.go): the layout, the README, this build's kind table,
// and every schema it names, from beside the table (-kinds,
// <dir>/schemas/<hex>.json). A failure is logged: the store stays
// readable as before, and the next boot tries again.
func (s *server) describeStore() {
	path := s.kindsPath
	if path == "" {
		path = defaultKindsPath
	}
	table, err := os.ReadFile(path)
	if err != nil {
		log.Printf("describe the store: %v", err)
		return
	}
	dir := filepath.Join(filepath.Dir(path), "schemas")
	entries, err := os.ReadDir(dir)
	if err != nil {
		log.Printf("describe the store: %v", err)
		return
	}
	schemas := map[string][]byte{}
	for _, e := range entries {
		hex, ok := strings.CutSuffix(e.Name(), ".json")
		if !ok {
			continue
		}
		data, err := os.ReadFile(filepath.Join(dir, e.Name()))
		if err != nil {
			log.Printf("describe the store: %v", err)
			return
		}
		schemas["sha256:"+hex] = data
	}
	if err := s.store.Describe(table, schemas); err != nil {
		log.Printf("describe the store: %v", err)
	}
}
