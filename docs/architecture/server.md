# The server (`server/`)

Go + wazero host: scaffolding only (HTTP, manifest, proxies). It must
never make a domain decision; the host reads the feed's own
feed_info.txt version field (one boundary CSV field — I/O, not
domain). It instantiates the single-capability modules it needs
(it links no assemblies — D-78: nothing it holds long is a network,
and authz keeps its own instance), all rebuilt by
`just caps` into web/pkg/: cap-sync (the -wasm flag, cap_sync.wasm),
the op behind POST /edits, held for the server's lifetime; cap-authz
(-authz-wasm) when authentication is on; cap-plans (-plans-wasm) in
store mode, the op behind POST /plans; cap-stops (-stops-wasm) in
store mode, one instance made on first use and emptied after each
(the kernel's `evict`) — the validation behind a stops write, and each
pick's stops refreshed from the stops document; cap-network
(the -network-wasm flag, cap_network.wasm), which imports the
configured GTFS zip (the kernel's `load_feed` + `import`) — and canonicalizes and
validates each published pick — in a fresh instance
that is closed afterwards, so its working memory goes with
it; and cap-streets (the -streets-wasm flag, cap_streets.wasm),
which imports the configured street layer (`import`) and derives
the walk table over it and the published network (the kernel's
`load_network` + `walks`) in a fresh instance, closed the same way. Each module's
bytes are compiled once per process (decoding, validating, machine
code: 1–3 s for cap-network) and every instance is made from that
compiled module in well under a millisecond, so a fresh instance per
import or publish costs only its memory. The manifest pins the network it publishes, the zip
it was imported from, the published plan set (store mode), the
streets and walk table it publishes, and the sync module's hash. (Reentrancy and the Go-host serialization live in
`docs/CONVENTIONS.md`.)

**The deployment publishes the network, not the feed.** Browsers
never parse GTFS a server has: the import runs once per published
zip, on the server, through the module the browser runs (same
bytes, same `network_hash`), and every client registers the
answer. On the real C-TRAN feed that is the difference between
~2.7 s of import and a ~187 MB network module on every rider's
page, and ~0.45 s and ~9–12 MB per registration of the network
(from its 680 KB gzip stream; wasm memory keeps a module's
high-water mark) — once per page, in its linked assembly. A zip loaded by hand
(the UIs' file input) still imports in the browser, and so does a
serverless deployment that names the agency's GTFS URL instead of a
network (D-132; [static-deployment.md](static-deployment.md)).

**The deployment publishes the streets the same way.** The street
layer is read once, on the server (`-streets`), and the walk table
— every walk between the network's stops, 13 s of work on C-TRAN —
is derived there too: a browser registers the canonical document
by its BYTES (the kernel's `load_streets` bulk op parses it in the
module; the page never parses it) and the table in the router. On
C-TRAN's Trapeze roads (`sha256:48fe6c89…`) the server's startup
spends 11.8 s on the import and the table; the page fetches 4.5 MB
(the document, gzipped) and 43 KB (the table), and registers the
document in 0.47 s; the first `access` builds the walking graph in
0.32 s, later ones answer in ~14 ms.

With `-store <url>` the server keeps a content-addressed store in
object storage (`docs/FORMATS.md`; D-113): `s3://bucket/prefix`
(S3 or an S3-compatible endpoint, `?endpoint=` naming it),
`azblob://container/prefix`, `gs://bucket/prefix`, or `mem://` (gone
with the process). The engine (`server/store/`) runs over one small
backend interface (`backend.go`: get, stat, create-if-absent, list
from a key, delete) and assumes only its contract — no append, rename,
fsync, or lock; the in-memory backend and the S3, Azure Blob and GCS
adapters (`s3.go`, `azure.go`, `gcs.go`) all pass one contract suite
(`contract_test.go`; `just store-contract` runs it and every engine
law against local SeaweedFS, Azurite and fake-gcs-server). Immutable
blobs are `blobs/<hex>` objects with the typed envelope as metadata,
and each commit is one record object `pointers/<name>/<seq>.json` —
the pointer is its highest record, and a commit is a blob create plus
a conditional create of record N+1, which is the CAS on the `from`
hash, then a re-read of its blob. Several server processes may share
one bucket; each caches heads and reads at most 2 s behind another's
commit. The `manifest` pointer is the root (publishing is one CAS on
it); the `edits` pointer is the multi-writer LWW document; the `plans`
pointer is the published plan set, and each pick's network is its own
`plan.<id>` pointer (D-74). The model (`store.tla`, `just spec`)
checks processes sharing a bucket with the offline collector beside
them.

A serving process opens the store with `store.OpenServing`: it
creates, reads and lists, and never deletes (D-117). The offline tools
are `server/cmd/store` (`just store …`): `gc` collects orphans with
its own delete-capable credentials, `verify` reads a whole store and
exits non-zero on the first break, and `copy` carries a directory
store an older build wrote into a bucket and verifies it.

The server's inputs come from their sources (D-118): `-feed`,
`-streets` and `-policy` each take an http(s) URL, `bucket:<key>` (the
object `inputs/<key>` in the store's bucket), or a dev path. A URL or
bucket source is re-checked at most once a minute (a conditional GET;
a stat), and only bytes that changed re-import. With `-store` each read
of the feed and the layer is committed as `source.feed` /
`source.streets`, the record naming its source; a source that does
not answer serves the last read, else that pointer's head — so a
process is disposable: a replacement given the bucket and the same
configuration serves the same manifest.

While the bucket does not answer (D-119) the server serves the heads
and blobs it holds (a byte-bounded blob cache; blobs are immutable),
labels each such read `X-Store-Stale` / `X-Store-As-Of`, and refuses
every write with 503 and `Retry-After`; the rider pages show the edit
document's age (kit's `staleLabel`).

Without `-store` the server runs over **the dev store**: the edit
document in memory (or, with `-edits <path>`, one flat file on this
machine), no history, and no publishing — `/plans`, `/builds`,
`/pointers` and the store's log are off. It is for development
(`just serve`, `just dev`) and for `-export`, which refuses a store;
the server logs `DEV STORE` at startup. A deployment runs with
`-store`.

**Running it.** The server stops on SIGTERM or SIGINT in two steps:
`/readyz` answers 503 `draining` for `-drain` (default 5 s), so a load
balancer stops sending, then the listener closes and requests in
flight get up to 30 s (`serve` in `main.go`). The HTTP server bounds
each client (`newHTTPServer`): headers within 10 s, a request body
within 5 min, idle keep-alives 2 min, a response 10 min. `just
server-image` builds the server as one container image with ko (no
Dockerfile): the binary on a distroless base, with the server's
modules, `web/dist`, and the geocoder policy in `server/kodata/`,
which ko copies to `$KO_DATA_PATH`. When `KO_DATA_PATH` is set, every
module and file flag defaults to its copy there and `-addr` to `:8080`,
so the image runs with only its deployment's flags. The operator's
guide is [DEPLOYMENTS-SERVER.md](../../DEPLOYMENTS-SERVER.md).

Its HTTP surface:

- `/healthz` — liveness: 200 `ok` while the process answers HTTP. It
  reads nothing (`health.go`).
- `/readyz` — readiness, JSON `{status, store, store_as_of_ms,
  error}`: 200 `ready`; 200 `degraded` while the bucket does not answer
  and the cached heads serve labelled reads; 503 `unavailable` when
  the store holds no head to serve; 503 `draining` once shutdown has
  begun. Without `-store` it is `ready` with `store: "none"`. It reads
  the `edits` head, which the store rechecks at most once per 2 s.
- `/` — the pages: with `-store`, the pinned build's files (the
  `build` pointer's head, [release-artifacts.md](release-artifacts.md)), and `-dist` (default
  `web/dist`) for what no build holds (the basemap); without it,
  `-dist` (stdlib `http.ServeContent` either way, so the Range
  requests PMTiles needs work). A file whose name carries its content
  hash (Vite's `assets/<name>-<hash>.<ext>`, the modules'
  `caps/<name>-<sha256>.wasm`) is served `immutable`; everything else
  (the pages, `caps/manifest.json`, `live-feeds.json`, the service
  worker `sw.js`) `no-cache`, so an open page's update check meets a
  new build ([updates-offline.md](updates-offline.md)).
- `/builds` — `POST {"build": "<hash>"}` moves the `build` pointer to a
  build its log holds: a rollback, or forward again (store mode;
  docs/FORMATS.md, "Builds"). A write, protected like the others.
- `/rt?url=<encoded>` — a DEV relay, a route only with `-rt-proxy`
  (`just serve` and `just dev` pass it; D-67): it fetches an http(s)
  feed (15 s timeout, 32 MB cap) and returns it with CORS headers, for
  a dev config whose feeds send no `Access-Control-Allow-Origin`. It
  relays any URL for any caller, so the server binds 127.0.0.1 when
  it is on. A deployment's pages fetch the agency's feeds directly.
- `/geocode/{autocomplete,search,reverse}` — a proxy to the Pelias
  geocoder named by `-geocoder` (`<url>/v1/<op>`): it forwards the
  user's input only (`text`, the focus point, the reverse point) and
  adds the deployment's query policy (`-geocoder-policy`, default
  `server/geocoder.json`); 404 when none is configured. The geocoder
  is an oracle, never a document on the bus (D-36, D-38;
  [web.md](web.md), "External services"). `just geocoder-build` builds a local Pelias
  (`github.com/pelias/docker`'s portland-metro project, in the
  gitignored `.pelias/`, its transit importer not run — stops are
  the network's), `just geocoder` starts it on :4000, and
  `GEOCODER=http://127.0.0.1:4000 just serve …` (or `just dev`)
  proxies it.
- `/manifest` — `{network_hash, boundary_hash, feed_url, feed_hash, feed_version,
  plans_hash, stops_hash, streets_hash, walks_hash, wasm_hash, authz_hash,
  policy_hash, build_hash, generated_at, publisher, timezone, services}`
  (the optional ones omitted when empty): which network is
  authoritative and its boundary (the import's numbering, named by the
  sha256 of its bytes) — or, in an export that names the feed instead
  (D-132), the GTFS URL the pages import themselves (`feed_url`) — the
  GTFS zip it was imported from, the published plan
  set, the build the pages are served from (store mode), which
  sync module bytes the server runs, who says so, the agency's zone
  (the one the server counts its today in, and every page reads its
  clock in), and the external services it proxies
  (`services.geocoder: {path, query}` with `-geocoder`). Served
  `no-cache` with an ETag naming what it pins (the store's manifest
  head; without the store, a hash of every field but `generated_at`),
  so an open page's poll is a 304 while nothing moved.
- `/streets` — the canonical streets document imported from the
  configured layer (`-streets`), bare (the bytes `load_streets`
  takes); gzipped, its ETag the streets hash, re-imported when the
  file changes (mtime-checked). The pages read the same document by
  the manifest's `streets_hash` (`/blobs`, below; `servedStreets` in
  `src/engine.ts`).
- `/walks` — the walk table over the published streets and network
  (both `-streets` and `-feed`), derived again when either changes;
  gzipped, its ETag the walks hash; the pages read it by the
  manifest's `walks_hash` (`servedWalks`).
- `/network` — the import's answer for the configured zip
  (`-feed`), served as-is: `{network, boundary, notes,
  network_hash}`. Gzipped when the client takes it; its ETag is the
  network hash, so a client holding it revalidates for a 304.
  Republishing the zip is re-imported on the next request
  (mtime-checked). The planner's page takes it (it forks the
  network into a plan — `servedNetworkValue` in `src/engine.ts`).
- `/network/document` and `/network/boundary` — the same answer's
  two documents, split off for pages that register them without
  reading them: the bare network as gzipped bytes
  (`application/gzip`, never `Content-Encoding`, so the browser
  hands them to the modules compressed — kernel op 256 decodes the
  stream as it parses), and the boundary as JSON, both under the
  network hash's ETag. The document's bytes are the import's
  canonical serialization (unzipped, they hash to the pin). The
  pages read the same two documents by the manifest's `network_hash`
  and `boundary_hash` (`/blobs`, below; `servedNetwork`).
- `/blobs/{hash}` and `/blobs/{hash}.gz` — with or without the store,
  every document the manifest pins (the network, its boundary, the
  streets, the walk table) by its hash: its JSON, and the same bytes
  gzipped (`application/gzip`, never `Content-Encoding`). Immutable,
  the ETag the path. Every blob's bytes hash to its name. This is the
  layout a static host serves too ([static-deployment.md](static-deployment.md)), so
  one page code reads either deployment; a store blob of the same hash
  wins, with its envelope (`publishedBlob` in `export.go`).
- **Authentication/authorization** (D-58..D-61): with any `-oidc-*`
  flag set, the server verifies bearer JWTs — JWKS (`-oidc-jwks-url`,
  cached by `kid`, refetched on unknown kid) or a pinned PEM
  (`-oidc-key`), RS256/ES256 only, iss/aud exact, exp/nbf with leeway —
  and asks the hosted cap-authz module (`-authz-wasm`) over the policy
  document before any write: reads stay public (D-58), writes
  (`POST /edits`, `POST /pointers/*`, `POST /plans` — write on
  `plans`) need a role the policy grants write on that pointer. 401 bad/missing token, 403 denied; the
  verified `sub` is the store log's `writer`. The policy is the store's
  `policy` pointer, seeded from the `-policy` source (a URL,
  `bucket:<key>`, or a dev path) at first boot (D-61);
  later changes are authenticated CAS commits. No `-oidc-*` = auth off,
  logged loudly (dev only). `STORE`/`OIDC_*`/`POLICY` env vars pass the
  flags through `just serve`/`just dev`. What the server leaves to the
  deployment: the first JWKS fetch trusts the IdP's HTTPS (pin its
  certificate at the proxy for more), throttling repeated 401s is the
  reverse proxy's, and a browser gets and refreshes its IdP token on
  its own, presenting it on writes only.
- `/edits` — the edit document: `POST` a client document, receive
  the LWW-merged result (cap-sync's `sync_edits`, with the receive
  path clamping client timestamps to the server clock, D-56); `GET`
  reads the server's. With `-store` this is the `edit_records`
  pointer, CAS'd through the store (a CAS conflict retries against
  the re-read document); without it, `-edits <path>` persists it as
  one flat file across restarts, else in memory (the dev store).
- Store mode also exposes the generic surface (`-store` only):
  `GET /pointers/{name}`, `GET /pointers/{name}/log?since=<hash>`
  (delta sync over the append-only log), `POST /pointers/{name}`
  (`{from, …}` → 200 + new head, 409 + current on conflict), and
  `GET /blobs/{hash}` (raw bytes, ETag the hash, envelope in headers;
  gzipped as `application/gzip` for `Accept: application/gzip` or at
  `/blobs/{hash}.gz` — except
  kind `policy` blobs, which are never served: 404). `GET
  /manifest` serves the `manifest` pointer's blob, re-committed when the
  published feed, streets, module, or services change. With auth on, the
  log endpoint requires a read (an `audit` rule implies it) and redacts
  `writer` unless audit is granted; `GET /permissions` (any valid token)
  returns the caller's effective-permissions projection (cap-authz
  `effective`) — the only client-readable form of the policy.
- `POST /plans` (store mode) — publish a pick (docs/ROADMAP.md 1,
  D-74): the body is the pick's network in any serialization. A fresh
  cap-network registers it and answers what refuses it (`publishable`:
  its issues, and a pick with no service in its period — 422 with
  `issues` and `refusals`) and its canonical bytes and plan frame;
  cap-plans `publish` answers the next published set or why the pick
  is refused (422: the chain, or a pick dated before today, D-76).
  `POST /plans/retire` `{"id"}` retires a pick that has not taken
  effect (D-75). "Today" is the agency's date: the host clock read in
  the network's zone, as cap-network answers it beside the hash (the
  feed's import, else the last published pick's `document`) — the
  host's own zone only until one is known; `-today` pins it on a test
  server. The server commits the network to
  `plan.<id>`, then the set to `plans`, and the manifest pins it as
  `plans_hash`. Clients read the set and each pick's network from
  `/blobs`. With a stops document, the pick is refreshed from it
  first (cap-stops `refresh`, D-156), for its window — from its date
  until the next pick's, as cap-plans `periods` derives it — so the
  network stored is the refreshed one, whatever copy was sent.
- `POST /stops` (store mode) — write the stops document (D-153):
  `{"from": "<the head read, or empty>", "stops": {...}}`. A fresh
  cap-stops validates it (422 with `refusals`) and answers its
  canonical bytes, committed to the `stops` pointer by CAS (409 with
  `current` when another writer moved it); the manifest pins it as
  `stops_hash`. Then every pick in force or to come is refreshed from
  it, and each whose network changed is republished (`plan.<id>`, then
  the set), answered as `republished`. A raw pointer commit of the
  stops is refused (400). With no stops document a pick publishes as
  sent; the first one arrives through this route (a page makes it with
  cap-stops `seed`).
- `GET /feed?date=` — the GTFS zip in force on the date (default the
  server's today): cap-network `export` of the published pick in
  force (cap-plans `in_force` answers which), kept by network hash;
  before the first pick takes effect, the configured `-feed` zip
  itself (D-74); 404 when neither runs. ETag the zip's hash.

`go test ./...` in `server/` proves the server runs the exact
module bytes the manifest pins and answers what the browser does.


## Why

One entry per load-bearing choice, headed by the code it governs.
**Holds** is what must stay true; change it only with the user, after
reading **Because**.

### No domain logic in Go (`server/*.go`)
- **Holds:** every domain answer comes from a capability module; the Go host does I/O, routing, and proxies only. Logic the server needs becomes a capability.
- **Because:** the server and a page given the same documents must answer the same bytes ([web.md](web.md), "Parity"); Go logic would be a second implementation to keep in step.

### The server imports the feed; pages register the network (`network.go`, `feedSource`)
- **Holds:** with a server, pages never parse GTFS; they register the imported network.
- **Because:** C-TRAN's import costs ~2.7 s and a ~187 MB module; registering the result costs ~0.45 s and ~9–12 MB (D-68 budgets).
- **Exception:** a serverless deployment may have each page import the agency's GTFS URL (D-132; [static-deployment.md](static-deployment.md)).

### Single-capability modules, fresh per import (`NewWasmHost`, `importFeed`, `streetsSource.Load`)
- **Holds:** the server links no assemblies; cap-network and cap-streets get a fresh instance per import, closed after; authz keeps its own instance.
- **Because:** wasm memory never shrinks, so closing the instance is the memory hygiene; the long-lived modules hold no network, so linking saves nothing; a trap elsewhere must not take the fail-closed authz check with it.
- **Rejected:** one assembly for the long-lived modules; one for everything (loses the fresh instance). Revisit if the server hosts a long-lived network reader.
- **Decided:** D-78.

### Two ways to fetch a published document (`/network/document`, `/network/boundary`, `/streets`, `/walks`, `/blobs/<hash>`)
- **Holds:** both stay. The routes serve the current document at a stable address, pinned by ETag (a 304 while unchanged). `/blobs/<hash>` serves a document by the hash the manifest names; the pages read this one.
- **Because:** a static host serves the `blobs/` layout too, so one page code reads either deployment; and one manifest read names the network and its boundary together, where the routes tie them only by matching ETags across two requests.

### Gzip as content, never transport (`/blobs/<hash>.gz`, `Accept: application/gzip`, `/network/document`)
- **Holds:** a gzipped document is served as `application/gzip` bytes, never `Content-Encoding: gzip`. `/blobs/<hash>` answers gzip on `Accept: application/gzip`; `/blobs/<hash>.gz` answers it by path. Both stay.
- **Because:** the page hands compressed bytes to the module, which decodes as it parses (kernel op 256), so the 9.6 MB network text never exists whole on the page. The `.gz` path exists because a static host cannot negotiate on a header. A host that decompresses anyway still works: the modules read either form.

### Object storage, create-only (`server/store/`, `backend.go`, `store.OpenServing`)
- **Holds:** the engine assumes only whole-object writes and a conditional create of an absent key — no append, rename, fsync, or lock. Serving processes never delete; `store gc` runs offline with its own credentials.
- **Because:** the store must survive losing a node, and S3, Azure Blob, and GCS all provide exactly that contract. A server that cannot delete cannot erase history, and the bucket can carry the provider's immutability settings.
- **Rejected:** a crash-hardened directory store (one node, its disk's failure the store's); online GC in every server (races a commit over object storage; needs delete rights everywhere); never deleting (kept as an option for immutable buckets).
- **Decided:** D-113, D-117.

### Inputs from a URL or the bucket (`-feed`, `-streets`, `-policy`, `source.go`)
- **Holds:** each input is a URL, `bucket:<key>`, or a dev path; with `-store` each read is committed (`source.feed`, `source.streets`).
- **Because:** a process must be disposable: its replacement, given the bucket and the same flags, serves the same manifest. Local files tie a deployment to one machine.
- **Rejected:** the bucket only, with a separate import step.
- **Decided:** D-118.

### Stale reads labelled, writes refused (`degraded.go`, `X-Store-Stale`)
- **Holds:** while the bucket does not answer, cached heads and blobs are served labelled with their age; every write is refused (503).
- **Because:** blobs are immutable, so only a head can be stale; reads peak during disruptions, when outages are likeliest. Acknowledging a write the bucket has not taken would lose it with the process; the browser keeps its documents and resends.
- **Rejected:** failing reads past an age; failing every request.
- **Decided:** D-119.

### Public reads, authenticated writes (`authz.go`, `protectWrite`, `-oidc-*`, `-policy`)
- **Holds:** GETs are public; writes need a role the policy grants on that pointer; tokens verify by JWKS or a pinned key, RS256/ES256 only; the roles' claim path is configurable; the policy is a store document seeded from a file at first boot and changed by audited commits.
- **Because:** reads publish what riders need; writes are the shared state. Symmetric or `none` algorithms would let anyone mint tokens. The claim's location is the IdP's. A file re-read per request would leave policy changes unaudited; a store-only policy needs a break-glass bootstrap.
- **Decided:** D-58, D-59, D-60, D-61.

### Degraded is ready (`/readyz`, `health.go`)
- **Holds:** `/readyz` stays 200 while the bucket does not answer and cached heads serve; `/healthz` reads nothing.
- **Because:** every instance shares one bucket, so all of them would fail the check at once; a balancer that pulled them all would turn a write outage into a read outage, while the pages read the cached heads fine (D-119). Restarting a process cannot fix the bucket.

### The dev store is not a deployment mode (no `-store`)
- **Holds:** without `-store` the server warns at startup and keeps working; it does not refuse.
- **Because:** with no store there is no history, rollback, or publishing, and edits live on one machine — right for a developer and for `-export` (which writes a bundle and exits), never for a server riders depend on. A warning keeps `just serve` one command.

### `/rt` is a dev relay, off by default (`handleProxy`, `-rt-proxy`)
- **Holds:** the route exists only with `-rt-proxy`, which binds localhost; deployments fetch feeds directly.
- **Because:** it fetches any URL for any caller — beyond localhost an open relay into the server's network. Feeds are fixed at their source (HTTPS with CORS).
- **Decided:** D-67.

### Picks: retired, not removed; never landing in the past (`POST /plans`, `/plans/retire`)
- **Holds:** a retired pick stays in the set marked `retired` and never governs; a pick with no service in its period is refused; a new, restored, or moved pick dated before today is refused; a correction under its own date always publishes.
- **Because:** retirement is the tombstone everywhere (no hard deletes). A no-service pick would leave every rider page with a network that runs nothing on its date; a past-dated pick would rewrite what the deployment says ran.
- **Rejected:** removal from the set; publishing with warnings only.
- **Decided:** D-75, D-76. **Pinned:** `TestRetiringAPick`, `TestPublishRefusalsNameTheirReasons`.
