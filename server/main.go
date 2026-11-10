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
	"net/http"
	"net/url"
	"os"
	"path"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"time"

	"rustwasm/server/store"
)

func main() {
	addr := flag.String("addr", "127.0.0.1:8080", "listen address")
	dist := flag.String("dist", "../web/dist", "directory of static files to serve")
	wasmPath := flag.String("wasm", "../web/pkg/cap_sync.wasm", "the sync capability module to host and pin")
	feedPath := flag.String("feed", "", "the GTFS zip's source (D-118): an http(s) URL, bucket:<key> (the object inputs/<key> in the -store bucket), or a dev path; imported and published as the network (enables /network and the manifest's schedule section). With -store each read is committed as source.feed, and a source that does not answer at boot serves that")
	networkWasm := flag.String("network-wasm", "../web/pkg/cap_network.wasm", "the network capability module that imports the feed")
	streetsPath := flag.String("streets", "", "the street layer's source (GeoJSON lines; a URL, bucket:<key>, or a dev path, as -feed): imported and published as the streets document (enables /streets, and with -feed the walk table at /walks); committed as source.streets with -store")
	streetsWasm := flag.String("streets-wasm", "../web/pkg/cap_streets.wasm", "the streets capability module that imports the layer and derives the walk table")
	publisher := flag.String("publisher", "", "manifest provenance (who publishes this schedule)")
	editsPath := flag.String("edits", "", "path to persist the edit document (JSON); empty = in-memory only (legacy mode; with -store it seeds the edits pointer once)")
	storeURL := flag.String("store", "", "the durable sync store's URL (docs/FORMATS.md): s3://bucket/prefix[?endpoint=URL&region=R] (S3 or an S3-compatible endpoint; credentials from the AWS SDK's default chain) or mem:// (nothing survives the process); enables /pointers, /blobs and store-backed /edits and /manifest; empty = legacy mode")
	retentionDays := flag.Int("retention-days", 90, "tombstone retention horizon (days) for the store's D-57 compaction commits")
	today := flag.String("today", "", "pin the server's date (YYYY-MM-DD) for hermetic test servers; empty = the host clock. Publishing reads it: a new pick never lands before today, and only a pick after today is retired (D-75, D-76)")
	migrateWasm := flag.String("migrate-wasm", defaultMigrateWasm, "the migration capability module (cap-migrate): an older document comes forward at receive, and a store's older heads at startup")
	plansWasm := flag.String("plans-wasm", "../web/pkg/cap_plans.wasm", "the plan-chain capability module (cap-plans) hosted in store mode: it answers the published plan set POST /plans commits")
	authzWasm := flag.String("authz-wasm", "../web/pkg/cap_authz.wasm", "the authorization capability module (cap-authz) hosted when OIDC is configured")
	oidcIssuer := flag.String("oidc-issuer", "", "expected JWT iss (D-60); with any -oidc flag this turns authentication on")
	oidcJWKS := flag.String("oidc-jwks-url", "", "the IdP's JWKS URL: keys are fetched, cached by kid, refetched on an unknown kid (rate-limited)")
	oidcAud := flag.String("oidc-audience", "", "expected JWT aud (string or array member)")
	oidcKey := flag.String("oidc-key", "", "pinned PEM public key for JWT verification (used when -oidc-jwks-url is empty)")
	oidcRoles := flag.String("oidc-roles-claim", "roles", "dotted JWT claim path holding the roles string array (D-59)")
	policyFile := flag.String("policy", "", "the authorization policy document's source (JSON; a URL, bucket:<key>, or a dev path): seeds the store's policy pointer at first boot (D-61); with no -store it is the policy")
	geocoderURL := flag.String("geocoder", "", "base URL of a Pelias geocoder (e.g. http://127.0.0.1:4000): enables /geocode/{autocomplete,search,reverse} and lists it in the manifest's services")
	rtProxy := flag.Bool("rt-proxy", false, "serve /rt?url=, a relay to any http(s) feed for pages whose feeds send no CORS header — a DEV convenience (D-67): it relays for any caller, so bind localhost; a deployment's pages fetch the agency's feeds directly")
	geocoderPolicy := flag.String("geocoder-policy", "geocoder.json", "the deployment's geocoder query policy (Pelias parameters per op), applied by the proxy")
	exportDir := flag.String("export", "", "write the deployment as a static bundle into this directory (absent or empty) and exit: -dist's pages, the manifest, and every published document as blobs/<hash> and blobs/<hash>.gz — the paths this server answers (D-131; export.go)")
	liveFeeds := flag.String("live-feeds", "", "with -export: the deployment's live-feeds.json (its agency feeds, no proxy), replacing -dist's dev copy")
	flag.Parse()

	mime.AddExtensionType(".wasm", "application/wasm")
	mime.AddExtensionType(".mjs", "text/javascript")
	mime.AddExtensionType(".pmtiles", "application/vnd.pmtiles")

	ctx := context.Background()
	host, err := NewWasmHost(ctx, *wasmPath)
	if err != nil {
		log.Fatalf("wasm host: %v", err)
	}
	log.Printf("sync module loaded: %s (%s)", *wasmPath, host.Hash())

	s := &server{
		dist:          *dist,
		wasm:          host,
		publisher:     *publisher,
		editsPath:     *editsPath,
		retentionDays: *retentionDays,
		fixedToday:    *today,
		migrateWasm:   *migrateWasm,
	}
	if *storeURL != "" {
		st, err := store.OpenServing(*storeURL)
		if err != nil {
			log.Fatalf("store: %v", err)
		}
		s.store = st
		if s.plans, err = NewWasmHost(ctx, *plansWasm); err != nil {
			log.Fatalf("plans module: %v", err)
		}
		s.networkWasm = *networkWasm
		s.seedStore(*editsPath)
		s.commitForward()
		s.seedPolicy(*policyFile)
		s.maybeCompactEdits()
		s.serverModules = map[string]string{"sync": *wasmPath, "network": *networkWasm, "streets": *streetsWasm, "plans": *plansWasm, "migrate": *migrateWasm}
		if *oidcIssuer != "" || *oidcJWKS != "" || *oidcKey != "" {
			s.serverModules["authz"] = *authzWasm
		}
		s.syncBuild()
		log.Printf("sync store open: %s (serving: creates and reads, never deletes — `store gc` collects orphans)", *storeURL)
	}
	if *oidcIssuer != "" || *oidcJWKS != "" || *oidcKey != "" {
		if *oidcJWKS != "" && (*oidcIssuer == "" || *oidcAud == "") {
			log.Fatal("-oidc-jwks-url needs -oidc-issuer and -oidc-audience")
		}
		a, err := loadAuthz(ctx, oidcConfig{
			issuer: *oidcIssuer, jwksURL: *oidcJWKS, audience: *oidcAud,
			keyFile: *oidcKey, rolesClaim: *oidcRoles,
		}, *authzWasm, *policyFile)
		if err != nil {
			log.Fatalf("authz: %v", err)
		}
		s.authz = a
		log.Printf("auth ON: iss=%q aud=%q jwks=%q key=%q roles-claim=%q (authz module %s)",
			*oidcIssuer, *oidcAud, *oidcJWKS, *oidcKey, a.cfg.rolesClaim, a.wasm.Hash())
	} else {
		log.Printf("auth OFF: no -oidc flags — writes are UNAUTHENTICATED (dev only; bind localhost)")
	}
	if *geocoderURL != "" {
		if s.geocoder, err = loadGeocoder(*geocoderURL, *geocoderPolicy); err != nil {
			log.Fatalf("geocoder: %v", err)
		}
	}
	if *feedPath != "" {
		s.feeder = &feedSource{src: newSource(*feedPath, "source.feed", "gtfs_feed", s.store), networkWasm: *networkWasm}
		if _, err := s.feeder.Load(); err != nil {
			log.Fatalf("feed: %v", err)
		}
	}
	if *streetsPath != "" {
		s.streets = &streetsSource{src: newSource(*streetsPath, "source.streets", "street_layer", s.store), streetsWasm: *streetsWasm}
		if _, err := s.publishedStreets(); err != nil {
			log.Fatalf("streets: %v", err)
		}
	}
	if *exportDir != "" {
		if err := s.export(*exportDir, *liveFeeds); err != nil {
			log.Fatalf("export: %v", err)
		}
		return
	}
	s.edits = loadEdits(*editsPath)

	s.rtProxy = *rtProxy
	if s.rtProxy {
		log.Printf("-rt-proxy: /rt relays any http(s) URL — dev only; bind localhost")
	}

	log.Printf("listening on http://%s (dist: %s)", *addr, *dist)
	log.Fatal(http.ListenAndServe(*addr, s.routes()))
}

// routes answers the server's mux. /rt is a route only with -rt-proxy
// (D-67): a deployment's pages fetch the agency's feeds directly.
func (s *server) routes() *http.ServeMux {
	mux := http.NewServeMux()
	if s.rtProxy {
		mux.HandleFunc("/rt", s.handleProxy)
	}
	mux.HandleFunc("/manifest", s.handleManifest)
	mux.HandleFunc("/network", s.handleNetwork)
	mux.HandleFunc("/network/document", s.handleNetworkDocument)
	mux.HandleFunc("/network/boundary", s.handleNetworkBoundary)
	mux.HandleFunc("/streets", s.handleStreets)
	mux.HandleFunc("/walks", s.handleWalks)
	mux.HandleFunc("/edits", s.protectWrite(func(*http.Request) string { return "edits" }, s.handleEdits))
	mux.HandleFunc("/pointers/", s.protectWrite(pointerNameOf, s.handlePointers))
	mux.HandleFunc("/plans", s.protectWrite(func(*http.Request) string { return "plans" }, s.handlePlans))
	mux.HandleFunc("/plans/retire", s.protectWrite(func(*http.Request) string { return "plans" }, s.handleRetire))
	mux.HandleFunc("/builds", s.protectWrite(func(*http.Request) string { return buildPointer }, s.handleBuilds))
	mux.HandleFunc("/feed", s.handleFeed)
	mux.HandleFunc("/permissions", s.requireAuth(s.handlePermissions))
	mux.HandleFunc("/blobs/", s.handleBlob)
	mux.HandleFunc("/geocode/", s.handleGeocode)
	mux.HandleFunc("/", s.handleStatic)
	return mux
}

// server holds the host's few pieces of state.
type server struct {
	dist      string
	wasm      *WasmHost
	feeder    *feedSource    // nil when no feed is configured
	streets   *streetsSource // nil when no street layer is configured
	publisher string
	geocoder  *geocoder // nil when no geocoder is configured

	// The /rt feed proxy (-rt-proxy, a dev convenience — D-67) and its
	// upstream timeout (0 = proxyTimeoutDefault).
	rtProxy      bool
	proxyTimeout time.Duration

	// The edit document. Opaque JSON: the artifact owns the schema
	// and every domain decision about it (merge, submit, validation)
	// — this host only stores and ferries it.
	editsMu   sync.Mutex
	edits     json.RawMessage // the edit document (current version)
	editsPath string          // empty = in-memory only (legacy mode)

	// The durable sync store (store mode, -store): blobs + CAS'd
	// pointers. nil = legacy mode.
	store         *store.Store
	retentionDays int

	// Authorization state (nil = auth off, dev only): the IdP config,
	// the hosted cap-authz module, and the policy source.
	authz *authzState

	// Publishing picks (store mode): the hosted cap-plans module, the
	// cap-network module each publish instantiates, and the lock that
	// makes one publish one step.
	plans       *WasmHost
	networkWasm string
	// The migration capability (cap-migrate), loaded on first use
	// (migrator): an older document comes forward at receive.
	migrateWasm string
	migrateOnce sync.Once
	migrateHost *WasmHost
	migrateErr  error
	plansMu     sync.Mutex
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
}

// handleStatic serves the built frontend. http.ServeContent gives
// Range requests (PMTiles needs them), If-Modified-Since, and content
// types by extension — no hand-rolled 206s.
func (s *server) handleStatic(w http.ResponseWriter, r *http.Request) {
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
	resp, err := http.DefaultClient.Do(req)
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
	NetworkHash string  `json:"network_hash"` // the network GET /network serves
	// The import's boundary (the feed's numbering the live feeds name),
	// a blob of its own: sha256 of its bytes, as every blob is named.
	BoundaryHash string `json:"boundary_hash,omitempty"`
	FeedHash    string  `json:"feed_hash"`    // "sha256:<hex>" of the GTFS zip it was imported from
	FeedVersion *string `json:"feed_version,omitempty"`
	StreetsHash string  `json:"streets_hash,omitempty"` // the streets document GET /streets serves
	WalksHash   string  `json:"walks_hash,omitempty"`   // the walk table GET /walks serves
	WasmHash    string  `json:"wasm_hash"`              // sha256 of the sync module this server runs
	AuthzHash   string  `json:"authz_hash,omitempty"`   // sha256 of the cap-authz module, when auth is on
	PolicyHash  string  `json:"policy_hash,omitempty"`  // the store's policy pointer head, store mode (D-61)
	PlansHash   string  `json:"plans_hash,omitempty"`   // the published plan set (the plans pointer head), store mode
	BuildHash   string  `json:"build_hash,omitempty"`   // the build the pages are served from (the build pointer head), store mode
	GeneratedAt int64   `json:"generated_at"`
	Publisher   *string `json:"publisher,omitempty"`
	// The agency's zone (the network's operator timezone, as today()
	// counts in it): a page reads its clock there, never in the
	// device's zone (web/src/clock.ts). Empty while none is known.
	Timezone string `json:"timezone,omitempty"`
	// The external services this deployment offers, by name — a
	// service it does not list is off, and the frames that need it
	// say so (docs/ARCHITECTURE.md, web section, "External services").
	Services map[string]Service `json:"services,omitempty"`
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
	if loc := s.agencyZone(); loc != nil {
		manifest.Timezone = loc.String()
	}
	if s.geocoder != nil {
		manifest.Services = map[string]Service{"geocoder": s.geocoder.service()}
	}
	if s.authz != nil {
		manifest.AuthzHash = s.authz.wasm.Hash()
	}
	if s.store != nil {
		if head, found, err := s.store.Head("policy"); err == nil && found {
			manifest.PolicyHash = head.Hash
		}
		if head, found, err := s.store.Head("plans"); err == nil && found {
			manifest.PlansHash = head.Hash
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
	sum := sha256.Sum256([]byte(strings.Join([]string{
		m.WasmHash, m.AuthzHash, m.PolicyHash, m.PlansHash, m.BuildHash, m.NetworkHash, m.BoundaryHash, m.FeedHash, orEmpty(m.FeedVersion),
		m.StreetsHash, m.WalksHash, orEmpty(m.Publisher), m.Timezone, string(services),
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
		// Legacy mode: the manifest is ephemeral, served as built; its
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

// seedStore creates the edits pointer when the store is fresh: from the
// legacy -edits file (migrated at receive through the module — canonical
// bytes and hash come from the module, never minted here), else empty.
func (s *server) seedStore(editsPath string) {
	nowMs := time.Now().UnixMilli()
	if _, found, err := s.store.Head("edits"); err == nil && !found {
		raw := []byte(emptyEdits)
		if editsPath != "" {
			if body, err := os.ReadFile(editsPath); err == nil && len(body) > 0 {
				raw = body
			}
		}
		_, document, hash, dropped, err := s.readEdits(raw)
		if err != nil {
			log.Printf("seed edits: read_edits: %v", err)
			return
		}
		if _, _, err := s.store.CAS("edits", "", []byte(document), s.syncEnvelope(), hash, "migrate:-edits", nowMs); err != nil {
			log.Printf("seed edits: %v", err)
			return
		}
		if dropped > 0 {
			log.Printf("seed edits: migrated, %d malformed record(s) dropped", dropped)
		}
	}
}
