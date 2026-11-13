package main

import (
	"flag"
	"os"
	"time"
)

// settings are the server's flags. Each is set, in order of precedence, on
// the command line, in a -config file (config.go), or by its default here.
type settings struct {
	configs        configList
	addr           *string
	dist           *string
	wasmPath       *string
	feedPath       *string
	networkWasm    *string
	streetsPath    *string
	streetsWasm    *string
	publisher      *string
	editsPath      *string
	storeURL       *string
	retentionDays  *int
	today          *string
	migrateWasm    *string
	plansWasm      *string
	attachmentsURL *string
	workOrdersSpec *string
	requestsWasm   *string
	stopsWasm      *string
	authzWasm      *string
	oidcIssuer     *string
	oidcJWKS       *string
	oidcAud        *string
	oidcKey        *string
	oidcRoles      *string
	oidcClientID   *string
	oidcScope      *string
	oidcName       *string
	policyFile     *string
	geocoderURL    *string
	rtProxy        *bool
	geocoderPolicy *string
	exportDir      *string
	liveFeeds      *string
	drain          *time.Duration
	feedURL        *string
}

// serverFlags registers the server's flags on fs.
func serverFlags(fs *flag.FlagSet) *settings {
	defaultAddr := "127.0.0.1:8080"
	if os.Getenv("KO_DATA_PATH") != "" {
		defaultAddr = ":8080"
	}
	f := &settings{
		addr:           fs.String("addr", defaultAddr, "listen address (in the image: every interface, :8080; from a source tree: localhost only)"),
		dist:           fs.String("dist", dataPath("dist", "../web/dist"), "directory of static files to serve"),
		wasmPath:       fs.String("wasm", dataPath("pkg/cap_sync.wasm", "../web/pkg/cap_sync.wasm"), "the sync capability module to host and pin"),
		feedPath:       fs.String("feed", "", "the GTFS zip's source (D-118): an http(s) URL, bucket:<key> (the object inputs/<key> in the -store bucket), or a dev path; imported and published as the network (enables /network and the manifest's schedule section). With -store each read is committed as source.feed, and a source that does not answer at boot serves that"),
		networkWasm:    fs.String("network-wasm", dataPath("pkg/cap_network.wasm", "../web/pkg/cap_network.wasm"), "the network capability module that imports the feed"),
		streetsPath:    fs.String("streets", "", "the street layer's source (GeoJSON lines; a URL, bucket:<key>, or a dev path, as -feed): imported and published as the streets document (enables /streets, and with -feed the walk table at /walks); committed as source.streets with -store"),
		streetsWasm:    fs.String("streets-wasm", dataPath("pkg/cap_streets.wasm", "../web/pkg/cap_streets.wasm"), "the streets capability module that imports the layer and derives the walk table"),
		publisher:      fs.String("publisher", "", "manifest provenance (who publishes this schedule)"),
		editsPath:      fs.String("edits", "", "with no -store (the dev store): a file to keep the edit document in across restarts; empty = in memory only. With -store it seeds the edits pointer once"),
		storeURL:       fs.String("store", "", "the durable sync store's URL (docs/FORMATS.md): s3://bucket/prefix[?endpoint=URL&region=R] (S3 or an S3-compatible endpoint; credentials from the AWS SDK's default chain) azblob://container/prefix, gs://bucket/prefix, or mem:// (nothing survives the process); enables /pointers, /blobs, /plans, /builds and store-backed /edits and /manifest. Empty = the dev store: edits in memory (or the -edits file), no history, no publishing — for development and -export only"),
		retentionDays:  fs.Int("retention-days", 90, "tombstone retention horizon (days) for the store's D-57 compaction commits"),
		today:          fs.String("today", "", "pin the server's date (YYYY-MM-DD) for hermetic test servers; empty = the host clock. Publishing reads it: a new pick never lands before today, and only a pick after today is retired (D-75, D-76)"),
		migrateWasm:    fs.String("migrate-wasm", dataPath("pkg/cap_migrate.wasm", defaultMigrateWasm), "the migration capability module (cap-migrate): an older document comes forward at receive, and a store's older heads at startup"),
		plansWasm:      fs.String("plans-wasm", dataPath("pkg/cap_plans.wasm", "../web/pkg/cap_plans.wasm"), "the plan-chain capability module (cap-plans) hosted in store mode: it answers the published plan set POST /plans commits"),
		attachmentsURL: fs.String("attachments", "", "the attachments' own bucket (D-162): any store URL (s3://, gs://, azblob://, mem://); photos and drawings on surveys and requests, read only by the signed-in; unset, none are taken"),
		workOrdersSpec: fs.String("work-orders", "", "the work order system's adapter (D-165): unset, none — a request's work order is entered by hand; stub: (or stub:<name>) a stand-in that opens STUB-<request>, for tests and demos"),
		requestsWasm:   fs.String("requests-wasm", dataPath("pkg/cap_requests.wasm", "../web/pkg/cap_requests.wasm"), "the stop requests capability module (cap-requests) in store mode: it validates every POST /stop-requests (D-159)"),
		stopsWasm:      fs.String("stops-wasm", dataPath("pkg/cap_stops.wasm", "../web/pkg/cap_stops.wasm"), "the stops capability module (cap-stops) in store mode: it validates every POST /stops and refreshes each pick's stops from the stops document"),
		authzWasm:      fs.String("authz-wasm", dataPath("pkg/cap_authz.wasm", "../web/pkg/cap_authz.wasm"), "the authorization capability module (cap-authz) hosted when OIDC is configured"),
		oidcIssuer:     fs.String("oidc-issuer", "", "expected JWT iss (D-60); with any -oidc flag this turns authentication on"),
		oidcJWKS:       fs.String("oidc-jwks-url", "", "the IdP's JWKS URL: keys are fetched, cached by kid, refetched on an unknown kid (rate-limited)"),
		oidcAud:        fs.String("oidc-audience", "", "expected JWT aud (string or array member)"),
		oidcKey:        fs.String("oidc-key", "", "pinned PEM public key for JWT verification (used when -oidc-jwks-url is empty)"),
		oidcRoles:      fs.String("oidc-roles-claim", "roles", "dotted JWT claim path holding the roles string array (D-59)"),
		oidcClientID:   fs.String("oidc-client-id", "", "the pages' own registration at the sign-in provider (a public single-page application client, PKCE): with it, the manifest names the issuer, this client, and -oidc-scope, and the pages sign in to write (D-58 reads stay public)"),
		oidcScope:      fs.String("oidc-scope", "openid profile offline_access", "the scopes a page asks for, its access token's audience the server's -oidc-audience (for Entra: openid profile offline_access api://<server app>/<scope>)"),
		oidcName:       fs.String("oidc-name-claim", "name", "dotted JWT claim path holding the person's name, recorded as a survey's surveyor (D-158): name (Entra's display name), preferred_username or upn (the UPN), email; a token without it records its sub"),
		policyFile:     fs.String("policy", "", "the authorization policy document's source (JSON; a URL, bucket:<key>, or a dev path): seeds the store's policy pointer at first boot (D-61); with no -store it is the policy"),
		geocoderURL:    fs.String("geocoder", "", "base URL of a Pelias geocoder (e.g. http://127.0.0.1:4000): enables /geocode/{autocomplete,search,reverse} and lists it in the manifest's services"),
		rtProxy:        fs.Bool("rt-proxy", false, "serve /rt?url=, a relay to any http(s) feed for pages whose feeds send no CORS header — a DEV convenience (D-67): it relays for any caller, so bind localhost; a deployment's pages fetch the agency's feeds directly"),
		geocoderPolicy: fs.String("geocoder-policy", dataPath("geocoder.json", "geocoder.json"), "the deployment's geocoder query policy (Pelias parameters per op), applied by the proxy"),
		exportDir:      fs.String("export", "", "write the deployment as a static bundle into this directory (absent or empty) and exit: -dist's pages, the manifest, and every published document as blobs/<hash> and blobs/<hash>.gz — the paths this server answers (D-131; export.go)"),
		liveFeeds:      fs.String("live-feeds", "", "with -export: the deployment's live-feeds.json (its agency feeds, no proxy), replacing -dist's dev copy"),
		drain:          fs.Duration("drain", 5*time.Second, "on SIGTERM or SIGINT: how long /readyz answers 503 \"draining\" before the listener closes, so a load balancer stops sending first; then requests in flight get up to 30s to finish"),
		feedURL:        fs.String("feed-url", "", "with -export, instead of -feed: the agency's GTFS zip URL the pages fetch, import, and check daily themselves — a serverless deployment that follows the feed with no redeploy (D-132); the URL must answer with CORS"),
	}
	fs.Var(&f.configs, "config", "a settings file (TOML; config.go): each key a flag's name, a [table] prefixing its keys (oidc.issuer is -oidc-issuer), a relative path read from the file's directory; repeatable, a later file over an earlier, and a flag on the command line over every file")
	return f
}
