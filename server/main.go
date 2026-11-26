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

	"rustwasm/server/httpx"
	"rustwasm/server/kinds"
	"rustwasm/server/logs"
	"rustwasm/server/plans"
	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
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
	inst, err := NewWasmHost(ctx, *f.wasmPath)
	if err != nil {
		log.Fatalf("the server's module: %v", err)
	}
	if missing := unlinked(inst); len(missing) > 0 {
		log.Fatalf("the server's module %s links no %s (server/capabilities.json; run `just caps`)", *f.wasmPath, strings.Join(missing, ", "))
	}
	log.Printf("the server's module: %s (%s; %s)", *f.wasmPath, inst.Hash(), strings.Join(inst.Capabilities(), ", "))

	s := &server{
		dist:          *f.dist,
		long:          wasmhost.NewHeld("long-lived", inst),
		publisher:     *f.publisher,
		editsPath:     *f.editsPath,
		retentionDays: *f.retentionDays,
		fixedToday:    *f.today,
		serverModules: map[string]string{"server": *f.wasmPath},
	}
	s.wasm = s.as("sync")
	if *f.attachmentsURL != "" {
		if s.attachments, err = store.OpenBackend(context.Background(), *f.attachmentsURL); err != nil {
			log.Fatalf("attachments: %v", err)
		}
		log.Printf("attachments: %s", *f.attachmentsURL)
	}
	// The work order adapter loads before the store's first manifest,
	// which offers it (services.work_orders).
	var woErr error
	if s.workOrders, woErr = newWorkOrders(*f.workOrdersSpec); woErr != nil {
		log.Fatal(woErr)
	}
	if s.workOrders != nil {
		log.Printf("work orders: the %s adapter", s.workOrders.Name())
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
		s.plans = s.as("plans")
		s.kindsPath = *f.kindsPath
		if _, err := s.kindTable(); err != nil {
			log.Fatalf("store mode: %v", err)
		}
		if missing := s.unhosted(); len(missing) > 0 {
			log.Fatalf("store mode: the kind table names %s, which the server's module does not link (run `just caps`)", strings.Join(missing, ", "))
		}
		s.seedStore(*f.editsPath)
		s.planner().CommitForward()
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
		}, inst, *f.policyFile)
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
		s.feeder = &feedSource{src: newSource(*f.feedPath, "source.feed", "gtfs_feed", s.store), wasm: *f.wasmPath}
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
		s.streets = &streetsSource{src: newSource(*f.streetsPath, "source.streets", "street_layer", s.store), wasm: *f.wasmPath}
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

// In the image, ko copies server/kodata/ under $KO_DATA_PATH.
func dataPath(inImage, inTree string) string {
	if root := os.Getenv("KO_DATA_PATH"); root != "" {
		return filepath.Join(root, inImage)
	}
	return inTree
}

// The response bound is loose on purpose. A phone on a poor signal
// downloads the street data slowly, and a cut-off download starts again.
// The request body bound is long because a build or a feed is large.
func newHTTPServer(h http.Handler) *http.Server {
	return &http.Server{
		Handler:           h,
		ReadHeaderTimeout: 10 * time.Second,
		ReadTimeout:       5 * time.Minute,
		WriteTimeout:      10 * time.Minute,
		IdleTimeout:       2 * time.Minute,
	}
}

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

// /rt is a route only with -rt-proxy. A deployment's pages fetch the
// agency's feeds directly.
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
	mux.HandleFunc("/logs/", s.protectLog(s.kept().HandleLogs))
	mux.HandleFunc("/plans", s.protectWrite(func(*http.Request) string { return "plans" }, s.planner().HandlePlans))
	mux.HandleFunc("/plans/retire", s.protectWrite(func(*http.Request) string { return "plans" }, s.planner().HandleRetire))
	mux.HandleFunc("/plans/freeze", s.protectWrite(func(*http.Request) string { return "plans" }, s.planner().HandleFreeze))
	mux.HandleFunc("/plans/adopt", s.protectWrite(func(*http.Request) string { return "plans" }, s.planner().HandleAdopt))
	// Each kind's `post` route is answered by its log's receive op
	// (server/kindroutes). Its write rule is its store kind.
	if t, err := s.kindTable(); err == nil {
		kr := s.kindRoutes()
		for _, k := range t.Kinds {
			if k.Post != nil && k.Receive != "" {
				mux.HandleFunc(k.Post.Path, s.protectWrite(kr.Rule, kr.HandleKindPost))
			}
		}
	}
	mux.HandleFunc("/attachments", s.protectWrite(func(*http.Request) string { return "attachments" }, s.handleAttachments))
	mux.HandleFunc("/attachments/", s.protectSignedIn(s.handleAttachmentFile))
	if e := s.workOrderKind(); e != nil {
		mux.HandleFunc(e.WorkOrder.Path, s.protectWrite(func(*http.Request) string { return e.Rule() }, s.handleWorkOrder))
	}
	mux.HandleFunc("/builds", s.protectWrite(func(r *http.Request) string { p, _ := buildsPointerOf(r); return p }, s.handleBuilds))
	mux.HandleFunc("/feed", s.planner().HandleFeed)
	mux.HandleFunc("/permissions", s.requireAuth(s.handlePermissions))
	mux.HandleFunc("/blobs/", s.handleBlob)
	mux.HandleFunc("/geocode/", s.handleGeocode)
	mux.HandleFunc("/", s.handleStatic)
	return mux
}

// outbound is http.DefaultTransport as it stands when the package
// starts. Tests replace http.DefaultTransport to check their own requests
// against the HTTP contract, and the server's outgoing requests must not
// pass through that check.
var outbound = http.DefaultTransport

type server struct {
	dist      string
	wasm      *WasmHost
	feeder    *feedSource    // nil when no feed is configured
	streets   *streetsSource // nil when no street layer is configured
	publisher string
	geocoder  *geocoder // nil when no geocoder is configured
	// The GTFS URL an export names for the pages to fetch. A
	// server imports its own feed, so it is "".
	feedURL string

	// proxyTimeout 0 means proxyTimeoutDefault.
	rtProxy      bool
	proxyTimeout time.Duration

	// The edit document is opaque JSON. The module owns its schema.
	editsMu   sync.Mutex
	edits     json.RawMessage // the edit document (current version)
	editsPath string          // the dev store's file; empty = in memory only

	// nil is the dev store (no -store).
	store         *store.Store
	retentionDays int

	draining atomic.Bool

	// nil means auth is off (dev only).
	authz *authzState

	// long is the long-lived instance (wasm.go). wasm and plans speak to
	// it as cap-sync and cap-plans.
	long        *wasmhost.Held
	longOnce    sync.Once
	plans       *WasmHost
	keptOnce    sync.Once
	keptLogs    *logs.Logs
	plannerOnce sync.Once
	plansRoutes *plans.Plans
	kindsPath   string
	kindsOnce   sync.Once
	kinds       *kinds.Table
	kindsErr    error
	attachments store.Backend
	workOrders  workOrderSystem
	fixedToday  string           // -today: the date publishing reads, for test servers; "" = the host clock
	now         func() time.Time // the host clock (nil = time.Now); tests pin it
	// The agency's zone, from the last published pick. today() reads it
	// when no feed is configured.
	zoneMu sync.Mutex
	zone   string

	gzMu    sync.Mutex
	gzCache map[string][]byte

	manifestMu  sync.Mutex
	manifestKey string

	// buildEvery bounds how often -dist is re-listed (0 means
	// buildEveryDefault). serverModules maps role to path.
	builds        buildState
	buildEvery    time.Duration
	serverModules map[string]string
	// Each UI's bundle commits to its own build.<id> pointer.
	uiDists []*uiDist
}

func (s *server) handleStatic(w http.ResponseWriter, r *http.Request) {
	h := w.Header()
	for _, b := range baselineHeaders() {
		h.Set(b.name, b.value)
	}
	if page := strings.TrimPrefix(path.Clean(r.URL.Path), "/"); isRiderPage(page) {
		for _, p := range pageHeaders(page, s.connectOrigins()) {
			h.Set(p.name, p.value)
		}
	}
	gzipStatic(w, r, s.serveStatic)
}

func (s *server) connectOrigins() []string {
	return feedOrigins(s.staticBytes("live-feeds.json"), s.feedURL)
}

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

// http.ServeContent gives the Range requests that PMTiles needs.
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
	// In store mode the pages come from the pinned build (builds.go).
	// -dist supplies what the build lacks, such as the basemap.
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
	http.ServeContent(w, r, info.Name(), info.ModTime(), file)
}

// A file whose name carries its content hash never changes: Vite's
// build output under assets/ (name-<8 chars>.ext) and the
// hash-addressed modules under caps/ (name-<sha256 hex>.wasm).
var (
	viteHashed   = regexp.MustCompile(`^assets/[^/]+-[A-Za-z0-9_-]{8}\.[a-z0-9]+$`)
	moduleHashed = regexp.MustCompile(`^caps/[^/]+-[0-9a-f]{64}\.wasm$`)
)

// Everything that is not content-addressed revalidates on every use, so
// an open page meets the new build on its next load.
func staticCacheControl(name string) string {
	if viteHashed.MatchString(name) || moduleHashed.MatchString(name) {
		return "public, max-age=31536000, immutable"
	}
	return "no-cache"
}

const (
	proxyTimeoutDefault = 15 * time.Second
	proxyMaxBytes       = 32 << 20
)

// handleProxy relays any http(s) URL for any caller, so the server binds
// localhost when -rt-proxy is on.
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

type Manifest struct {
	NetworkHash  string `json:"network_hash"` // the network GET /network serves
	BoundaryHash string `json:"boundary_hash,omitempty"`
	// A serverless deployment that follows the agency's GTFS URL
	// (-export with -feed-url) publishes no network. Each page fetches the
	// zip, imports it, and checks it daily.
	FeedURL     string            `json:"feed_url,omitempty"`
	FeedHash    string            `json:"feed_hash"` // "sha256:<hex>" of the GTFS zip it was imported from
	FeedVersion *string           `json:"feed_version,omitempty"`
	StreetsHash string            `json:"streets_hash,omitempty"` // the streets document GET /streets serves
	WalksHash   string            `json:"walks_hash,omitempty"`   // the walk table GET /walks serves
	WasmHash    string            `json:"wasm_hash"`              // sha256 of the module this server runs
	AuthzHash   string            `json:"authz_hash,omitempty"`   // sha256 of the module authz runs in, when auth is on
	PolicyHash  string            `json:"policy_hash,omitempty"`  // the store's policy pointer head, store mode
	Logged      map[string]string `json:"-"`
	// The seq of each log's fold that this manifest pins. A page sends it
	// with a write as read_seq.
	LogSeqs     map[string]int64 `json:"log_seqs,omitempty"`
	BuildHash   string           `json:"build_hash,omitempty"` // the build the pages are served from (the build pointer head), store mode
	GeneratedAt int64            `json:"generated_at"`
	Publisher   *string          `json:"publisher,omitempty"`
	// A page reads its clock in this zone, never the device's
	// (web/src/clock.ts). Empty while none is known.
	Timezone string `json:"timezone,omitempty"`
	// Absent when sign-in is off or the pages have no client of their own.
	Auth *ManifestAuth `json:"auth,omitempty"`
	// A service that is not listed is off (docs/architecture/web.md,
	// "External services").
	Services map[string]Service `json:"services,omitempty"`
}

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

// Writes need a token and reads stay public.
type ManifestAuth struct {
	Issuer   string `json:"issuer"`
	ClientID string `json:"client_id"`
	Scope    string `json:"scope"`
}

type Service struct {
	Path  string                    `json:"path"`
	Query map[string]map[string]any `json:"query,omitempty"`
}

// generated_at is the publish time, not the request time, so a store-mode
// manifest blob is immutable between republishes.
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
	if e := s.workOrderKind(); e != nil && s.workOrders != nil && s.store != nil {
		if manifest.Services == nil {
			manifest.Services = map[string]Service{}
		}
		manifest.Services["work_orders"] = Service{Path: e.WorkOrder.Path}
	}
	if s.authz != nil {
		manifest.AuthzHash = s.authz.wasm.Hash()
	}
	if s.store != nil {
		if head, found, err := s.store.Head("policy"); err == nil && found {
			manifest.PolicyHash = head.Hash
		}
		// One lock reads each log's documents and the seq of their fold.
		// The write that carries the seq as read_seq is checked against
		// the records it changes.
		pins, seqs := s.kept().Pins()
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

// manifestKeyOf covers everything but the generated_at stamp.
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
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	if s.store == nil {
		// The dev store serves the manifest as built. The ETag names what it
		// pins, so an open page's poll is a 304 while nothing moved.
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
	// In store mode the manifest is the root pointer. Publishing is one
	// CAS on it (docs/FORMATS.md).
	s.manifestMu.Lock()
	key := manifestKeyOf(manifest)
	if key != s.manifestKey {
		body, _ := json.Marshal(manifest)
		env := store.Envelope{Kind: "manifest", SchemaVersion: 1, ProducedBy: "server"}
		for attempt := 0; attempt < 5; attempt++ {
			head, found, err := s.store.Head("manifest")
			if err != nil {
				s.manifestMu.Unlock()
				httpx.Fail(w, err, http.StatusInternalServerError)
				return
			}
			from := ""
			if found {
				from = head.Hash
			}
			_, _, err = s.store.CAS("manifest", from, body, env, "", "server", time.Now().UnixMilli())
			if errors.Is(err, store.ErrUnavailable) {
				break // serve the cached manifest below, labelled
			}
			if err != nil {
				var ce *store.ConflictError
				if errors.As(err, &ce) && attempt < 4 {
					continue
				}
				s.manifestMu.Unlock()
				httpx.Fail(w, err, http.StatusInternalServerError)
				return
			}
			s.manifestKey = key
			break
		}
	}
	s.manifestMu.Unlock()

	head, found, err := s.store.Head("manifest")
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	if !found {
		httpx.Fail(w, store.ErrUnavailable, http.StatusInternalServerError)
		return
	}
	data, _, err := s.store.GetBlob(head.Hash)
	if err != nil {
		httpx.Fail(w, err, http.StatusInternalServerError)
		return
	}
	httpx.LabelStale(w, head)
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

func (s *server) seedStore(editsPath string) {
	s.describeStore()
	s.kept().Seed(map[string]logs.SeedFile{"edit_records": {Flag: "edits", Path: editsPath}})
}

// A failure is logged and the next boot tries again. The schemas sit
// beside the kind table, in <dir>/schemas/<hex>.json.
func (s *server) describeStore() {
	path := s.kindsPath
	if path == "" {
		path = kinds.DefaultPath
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
