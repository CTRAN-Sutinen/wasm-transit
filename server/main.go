package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"mime"
	"net"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"rustwasm/server/authz"
	"rustwasm/server/builds"
	"rustwasm/server/edits"
	"rustwasm/server/imports"
	"rustwasm/server/kinds"
	"rustwasm/server/logs"
	"rustwasm/server/manifest"
	"rustwasm/server/plans"
	"rustwasm/server/pointers"
	"rustwasm/server/proxy"
	"rustwasm/server/source"
	"rustwasm/server/store"
	"rustwasm/server/wasmhost"
	"rustwasm/server/workorders"
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
	if s.workOrders, woErr = workorders.NewSystem(*f.workOrdersSpec); woErr != nil {
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
		s.gate().SeedPolicy(*f.policyFile)
		s.edits().Compact()
		uiDists, err := builds.ParseUIs(*f.uiDist)
		if err != nil {
			log.Fatalf("%v", err)
		}
		s.uiDists = uiDists
		s.builds().Sync()
		log.Printf("sync store open: %s (serving: creates and reads, never deletes — `store gc` collects orphans)", *f.storeURL)
	}
	if *f.oidcIssuer != "" || *f.oidcJWKS != "" || *f.oidcKey != "" {
		if *f.oidcJWKS != "" && (*f.oidcIssuer == "" || *f.oidcAud == "") {
			log.Fatal("-oidc-jwks-url needs -oidc-issuer and -oidc-audience")
		}
		a, err := authz.Load(ctx, authz.Config{
			Issuer: *f.oidcIssuer, JWKSURL: *f.oidcJWKS, Audience: *f.oidcAud,
			KeyFile: *f.oidcKey, RolesClaim: *f.oidcRoles, NameClaim: *f.oidcName,
			ClientID: *f.oidcClientID, Scope: *f.oidcScope,
		}, inst, *f.policyFile)
		if err != nil {
			log.Fatalf("authz: %v", err)
		}
		s.authz = a
		log.Printf("auth ON: iss=%q aud=%q jwks=%q key=%q roles-claim=%q name-claim=%q (authz module %s)",
			*f.oidcIssuer, *f.oidcAud, *f.oidcJWKS, *f.oidcKey, a.Cfg.RolesClaim, a.Cfg.NameClaim, a.Wasm.Hash())
	} else {
		log.Printf("auth OFF: no -oidc flags — writes are UNAUTHENTICATED (dev only; bind localhost)")
	}
	if *f.geocoderURL != "" {
		if s.geocoder, err = proxy.LoadGeocoder(*f.geocoderURL, *f.geocoderPolicy); err != nil {
			log.Fatalf("geocoder: %v", err)
		}
	}
	if *f.feedPath != "" {
		s.feeder = &imports.FeedSource{Src: source.New(*f.feedPath, "source.feed", "gtfs_feed", s.store), Wasm: *f.wasmPath}
		s.feeder.OnImport = func(feed *imports.Feed) {
			if err := s.recordFeed(context.Background(), feed); err != nil {
				log.Printf("stops: the feed's differences were not recorded: %v", err)
			}
		}
		if _, err := s.feeder.Load(); err != nil {
			log.Fatalf("feed: %v", err)
		}
	}
	if *f.streetsPath != "" {
		s.streets = &imports.StreetsSource{Src: source.New(*f.streetsPath, "source.streets", "street_layer", s.store), Wasm: *f.wasmPath}
		if _, err := s.published().PublishedStreets(); err != nil {
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
		mux.HandleFunc("/rt", (&proxy.Relay{Timeout: s.proxyTimeout}).HandleRelay)
	}
	mux.HandleFunc("/healthz", s.handleHealthz)
	mux.HandleFunc("/readyz", s.handleReadyz)
	mux.HandleFunc("/manifest", s.manifest().HandleManifest)
	mux.HandleFunc("/network", s.published().HandleNetwork)
	mux.HandleFunc("/network/document", s.published().HandleNetworkDocument)
	mux.HandleFunc("/network/boundary", s.published().HandleNetworkBoundary)
	mux.HandleFunc("/streets", s.published().HandleStreets)
	mux.HandleFunc("/walks", s.published().HandleWalks)
	mux.HandleFunc("/edits", s.gate().ProtectWrite(func(*http.Request) string { return "edits" }, s.edits().HandleEdits))
	mux.HandleFunc("/pointers/", s.gate().ProtectWrite(pointers.NameOf, s.pointers().HandlePointers))
	mux.HandleFunc("/logs/", s.gate().ProtectLog(s.kept().HandleLogs))
	mux.HandleFunc("/plans", s.gate().ProtectWrite(func(*http.Request) string { return "plans" }, s.planner().HandlePlans))
	mux.HandleFunc("/plans/retire", s.gate().ProtectWrite(func(*http.Request) string { return "plans" }, s.planner().HandleRetire))
	mux.HandleFunc("/plans/freeze", s.gate().ProtectWrite(func(*http.Request) string { return "plans" }, s.planner().HandleFreeze))
	mux.HandleFunc("/plans/adopt", s.gate().ProtectWrite(func(*http.Request) string { return "plans" }, s.planner().HandleAdopt))
	// Each kind's `post` route is answered by its log's receive op
	// (server/kindroutes). Its write rule is its store kind.
	if t, err := s.kindTable(); err == nil {
		kr := s.kindRoutes()
		for _, k := range t.Kinds {
			if k.Post != nil && k.Receive != "" {
				mux.HandleFunc(k.Post.Path, s.gate().ProtectWrite(kr.Rule, kr.HandleKindPost))
			}
		}
	}
	mux.HandleFunc("/attachments", s.gate().ProtectWrite(func(*http.Request) string { return "attachments" }, s.attachmentRoutes().HandleAttachments))
	mux.HandleFunc("/attachments/", s.gate().ProtectSignedIn(s.attachmentRoutes().HandleAttachmentFile))
	if e := s.workOrderKind(); e != nil {
		mux.HandleFunc(e.WorkOrder.Path, s.gate().ProtectWrite(func(*http.Request) string { return e.Rule() }, s.workOrderRoutes().HandleWorkOrder))
	}
	mux.HandleFunc("/builds", s.gate().ProtectWrite(func(r *http.Request) string { p, _ := builds.PointerOf(r); return p }, s.builds().HandleBuilds))
	mux.HandleFunc("/feed", s.planner().HandleFeed)
	mux.HandleFunc("/permissions", s.gate().RequireAuth(s.gate().HandlePermissions))
	mux.HandleFunc("/blobs/", s.pointers().HandleBlob)
	mux.HandleFunc("/geocode/", s.geocode)
	mux.HandleFunc("/", s.static().HandleStatic)
	return mux
}

type server struct {
	dist      string
	wasm      *WasmHost
	feeder    *imports.FeedSource    // nil when no feed is configured
	streets   *imports.StreetsSource // nil when no street layer is configured
	publisher string
	geocoder  *proxy.Geocoder // nil when no geocoder is configured
	// The GTFS URL an export names for the pages to fetch. A
	// server imports its own feed, so it is "".
	feedURL string

	// proxyTimeout 0 means the relay's default.
	rtProxy      bool
	proxyTimeout time.Duration

	editsPath string // the dev store's file; empty = in memory only

	// nil is the dev store (no -store).
	store         *store.Store
	retentionDays int

	draining atomic.Bool

	// nil means auth is off (dev only).
	authz *authz.Auth

	// long is the long-lived instance (wasm.go). wasm and plans speak to
	// it as cap-sync and cap-plans.
	long          *wasmhost.Held
	longOnce      sync.Once
	plans         *WasmHost
	keptOnce      sync.Once
	keptLogs      *logs.Logs
	editsOnce     sync.Once
	editDoc       *edits.Edits
	pointersOnce  sync.Once
	pointerRoutes *pointers.Pointers
	plannerOnce   sync.Once
	plansRoutes   *plans.Plans
	kindsPath     string
	kindsOnce     sync.Once
	kinds         *kinds.Table
	kindsErr      error
	attachments   store.Backend
	workOrders    workorders.System
	fixedToday    string           // -today: the date publishing reads, for test servers; "" = the host clock
	now           func() time.Time // the host clock (nil = time.Now); tests pin it
	// The agency's zone, from the last published pick. today() reads it
	// when no feed is configured.
	zoneMu sync.Mutex
	zone   string

	manifestOnce sync.Once
	manifests    *manifest.Publisher

	// buildEvery bounds how often -dist is re-listed (0 means the builds'
	// default). serverModules maps role to path.
	buildEvery    time.Duration
	serverModules map[string]string
	// Each UI's bundle commits to its own build.<id> pointer.
	uiDists    []*builds.UI
	buildsOnce sync.Once
	buildSet   *builds.Builds
}

// generated_at is the publish time, not the request time, so a store-mode
// manifest blob is immutable between republishes.
func (s *server) buildManifest() (manifest.Manifest, error) {
	m := manifest.Manifest{
		WasmHash:    s.wasm.Hash(),
		GeneratedAt: time.Now().Unix(),
	}
	if s.feeder != nil {
		feed, err := s.feeder.Load()
		if err != nil {
			return m, fmt.Errorf("feed unavailable: %w", err)
		}
		m.NetworkHash = feed.NetworkHash
		m.BoundaryHash = feed.BoundaryHash
		m.FeedHash = feed.Hash
		if v := feed.Version; v != "" {
			m.FeedVersion = &v
		}
	}
	if s.streets != nil {
		streets, err := s.published().PublishedStreets()
		if err != nil {
			return m, fmt.Errorf("streets unavailable: %w", err)
		}
		m.StreetsHash = streets.Streets.Hash
		m.WalksHash = streets.Walks.Hash
	}
	if s.publisher != "" {
		m.Publisher = &s.publisher
	}
	m.FeedURL = s.feedURL
	if s.authz != nil && s.authz.Cfg.ClientID != "" {
		m.Auth = &manifest.Auth{Issuer: s.authz.Cfg.Issuer, ClientID: s.authz.Cfg.ClientID, Scope: s.authz.Cfg.Scope}
	}
	if loc := s.agencyZone(); loc != nil {
		m.Timezone = loc.String()
	}
	if s.geocoder != nil {
		m.Services = map[string]manifest.Service{"geocoder": s.geocoder.Service()}
	}
	if s.attachments != nil {
		if m.Services == nil {
			m.Services = map[string]manifest.Service{}
		}
		m.Services["attachments"] = manifest.Service{Path: "/attachments"}
	}
	if e := s.workOrderKind(); e != nil && s.workOrders != nil && s.store != nil {
		if m.Services == nil {
			m.Services = map[string]manifest.Service{}
		}
		m.Services["work_orders"] = manifest.Service{Path: e.WorkOrder.Path}
	}
	if s.authz != nil {
		m.AuthzHash = s.authz.Wasm.Hash()
	}
	if s.store != nil {
		if head, found, err := s.store.Head("policy"); err == nil && found {
			m.PolicyHash = head.Hash
		}
		// One lock reads each log's documents and the seq of their fold.
		// The write that carries the seq as read_seq is checked against
		// the records it changes.
		pins, seqs := s.kept().Pins()
		m.Logged = pins
		for name, seq := range seqs {
			if m.LogSeqs == nil {
				m.LogSeqs = map[string]int64{}
			}
			m.LogSeqs[name] = seq
		}
		if head, found, err := s.store.Head(builds.Pointer); err == nil && found {
			m.BuildHash = head.Hash
		}
	}
	return m, nil
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
