# The server (`server/`)

The server is a Go and wazero host. [hosts.md](hosts.md) holds the
rules it keeps with the other hosts, and how it differs from them. It
is scaffolding: HTTP, the manifest, and proxies. It makes no domain
decision. It reads the feed's `feed_info.txt` version field, which is
one boundary CSV field and so I/O.

The server links one assembly, as each UI does:
`module-server`, which `just caps` writes to `web/pkg/server.wasm`
(`-wasm`; `kodata/pkg/server.wasm` in the image). gen-assemblies links
the capabilities that `server/capabilities.json` lists (the ones the
server calls by name) and each capability the kind list names to check
or receive a kind, so a new kind's capability needs no line in the
list. At boot the server refuses a module that does not link them.
The manifest's `wasm_hash` names the module.

**The code, by concept.** `package main` holds the wiring, the flags,
the mux (`routes()` in `main.go`), the host's clock (`clock.go`), and the
routes of its own concepts (the network, the streets, the pointers, the
builds, the attachments, sign-in). Each other concept is a package:

| Package | Holds |
|---|---|
| `server/logs` | the logs: the fold (`fold.go`), one write through a receive op (`receive.go`), each kind's document at the head (`read.go`), `/logs/` (`routes.go`), and the first entry from the old pointers (`seed.go`) |
| `server/plans` | publishing: `POST /plans`, the freeze, the adoption, the retire, `GET /feed`, and the picks brought forward at boot |
| `server/kindroutes` | each kind's `post` route, which its log receives |
| `server/kinds` | the kind table, read |
| `server/store` | the bucket: blobs, pointers, the logs' entries and checkpoints |
| `server/wasmhost` | the module ABI, and the long-lived instance (`Held`) |
| `server/httpx` | the answers every route shares: a failure, a module's error, a stale head, unknown fields, the writer |

The server registers a kind's routes from the kind table, so a grep of a
route finds no handler. `node tools/caps-run.mjs kind <tag>` prints a
kind's whole path: each route and its Go handler, its log, its ops and
the library functions behind them, its read rights, and its store
layout.

Each call names its capability: a JSON request carries `"cap"`, and a
capability's bulk op carries its index (`wasmhost.Speak`). The server
runs these instances of the one compiled module:

- **The long-lived instance** answers the sync, plans, stops, requests,
  alerts, notes, log, and migrate ops. It holds the head of each kept
  log's documents, each registered once, when its log assembles it. A
  request registers nothing the instance holds. After each request's
  calls, the instance evicts every document that is not a held head (the
  kernel's evict, op 257).
- **authz's own instance** runs when sign-in is on, so a trap in another
  capability never takes the fail-closed check with it.
- **A fresh instance** for each import (the feed, the street layer), each
  publish (a pick's export, with its stops' history), and each pointer
  write's check, closed after, so the memory it used goes with it.

**A refresh.** Wasm memory never shrinks. When the long-lived instance
passes its memory ceiling (256 MB, `wasmhost.MemoryCeiling`), the
server starts a fresh instance of the same compiled module beside it and
registers the held heads in it. Requests go on to the held instance, with
no wait. Before the swap, the server registers each head that moved
during the refresh. The old instance finishes the call it holds, then
closes. A trap replaces an instance the same way, and authz's too. A
refresh starts on memory growth, never on the number of requests.
`server/instances_test.go` pins all of this.

What each capability does for the server:

- cap-sync is the op behind `POST /edits`. In store mode its `receive`
  op writes the disruptions log ("One path for a log", below). It also
  turns the edit document into the log's records and back, and answers
  what a revert writes back.
- cap-authz checks each write when authentication is on. In store mode a
  fresh instance also checks each policy write (`validate`).
- cap-plans' `receive` op writes the plans log. It turns the published
  plan set and the frozen versions into the log's records and back, and
  answers the picks in use and in force. A pick's checks run in the same
  op (`pick`): the network's publish check, and the stops it
  reads on its first date.
- cap-stops' `receive` op writes the stops log: the stops document, the
  stop requests document, the work order system's answer, and the feed's
  differences. It also exports a published pick with its stops' history.
- cap-requests assembles the stop requests document from the stops
  log's records, and answers what to send the work order system
  (`work_order_call`).
- cap-alerts' `receive` op writes the alerts log. The server serves no
  feed of the alerts (D-124).
- cap-notes' `receive` op writes the notes log. It stamps each note with
  the verified writer and the server's clock from the entry's header, so
  a page never names who wrote a note.
- cap-log keeps all five logs (disruptions, stops, plans, alerts, notes):
  the fold, a revert, a closed gap, and where a key landed. cap-sync,
  cap-stops, cap-requests, cap-plans, cap-alerts, and cap-notes turn
  their documents into the log's records and back (`log_records`,
  `log_document`).
- cap-network imports the configured GTFS zip (the kernel's `load_feed`
  and `import`), and canonicalizes and validates each published pick.
- cap-streets imports the configured street layer (`import`), and
  derives the walk table over the street layer and the published network
  (the kernel's `load_network` and `walks`).

Beside the modules, `just caps` writes the kind table
(`web/pkg/kinds.json`, `-kinds`; D-202) and the schemas it names
(`web/pkg/schemas/`). Store mode reads the table at startup and stops
without it. At each boot in store mode the server writes the store's
description, create only: the layout, the README, the table, and the
schemas (D-203; `docs/FORMATS.md`, "The store describes itself").

The server compiles the module's bytes once per process (decoding,
validating, machine code). It makes every instance from the compiled
module in well under a millisecond, so a fresh instance per import or
publish costs only its memory. `docs/CONVENTIONS.md` holds
reentrancy and the Go-host serialization.

**The deployment publishes the network, not the feed.** Browsers never
parse GTFS that a server has. The import runs once per published zip,
on the server. In store mode the record holds its documents, so a later
boot reads them and imports nothing ([storage.md](storage.md), "The
record"). It uses the same module bytes that the browser runs, so the
`network_hash` is the same. On the real C-TRAN feed this makes a large
difference:

- The import takes ~2.7 s and needs a ~187 MB network module on every
  rider's page.
- Registering the network takes ~0.45 s and ~9 to 12 MB per
  registration (from its 680 KB gzip stream). The wasm memory keeps a
  module's high-water mark.
- A page registers the network once, in its linked assembly.

A zip that a person loads by hand (the UIs' file input) still imports in
the browser. So does a serverless deployment that names the agency's
GTFS URL instead of a network
(see [static-deployment.md](static-deployment.md)).

**The deployment publishes the streets the same way.** The server reads
the street layer once (`-streets`). It also derives the walk table
there: every walk between the network's stops, 13 s of work on C-TRAN. A
browser registers the canonical document by its bytes. The kernel's
`load_streets` bulk op parses it in the module, and the page never
parses it. The browser registers the table in the router. On C-TRAN's
Trapeze roads (`sha256:48fe6c89…`):

- The server's startup spends 11.8 s on the import and the table.
- The page fetches 4.5 MB (the document, gzipped) and 43 KB (the table),
  and registers the document in 0.47 s.
- The first `access` builds the walking graph in 0.32 s. Later ones
  answer in ~14 ms.

With `-store <url>` the server keeps the agency's record in object
storage. [storage.md](storage.md) holds the engine, its
backends, the pointers, and the offline tools. `docs/FORMATS.md` holds
the bytes at rest. A serving process opens the store with
`store.OpenServing` and never deletes.

The server's inputs come from their sources. `-feed`,
`-streets` and `-policy` each take one of these:

- an http(s) URL
- `bucket:<key>` (the object `inputs/<key>` in the store's bucket)
- a dev path

The server re-checks a URL or bucket source at most once a minute (a
conditional GET, or a stat). Only bytes that changed re-import. With
`-store`, the server commits each read of the feed and the layer as
`source.feed` and `source.streets`, and the record names its source. If
a source does not answer, the server serves the last read, else that
pointer's head. A process is therefore disposable: a replacement that
has the bucket and the same configuration serves the same manifest.

While the bucket does not answer, the server serves the heads
and blobs it holds (a byte-bounded blob cache, because blobs are
immutable). It labels each such read `X-Store-Stale` and
`X-Store-As-Of`. No age makes a read fail. A read that the server holds
nothing for answers 503. The server refuses every write with 503 and
`Retry-After`, and the client holds its document and resends. The pages
show the edit document's age (kit's `inputLabel`). A bucket call that
takes longer than its timeout (20 s, 2 min for a create) counts as a
bucket that does not answer (`store/degraded.go`).

**The disk cache** (`-store-cache <folder>`, `store/diskcache.go`;
D-224). A copy keeps on its own disk a working copy of what it reads
from the bucket and confirms: each blob and its envelope, each
pointer's and log's last confirmed head with its as-of time, and the
logs' entries and checkpoints. A copy that starts while the bucket does
not answer boots on it. It serves those heads, labelled stale as of
when the bucket last confirmed them. It folds each log from the cached
checkpoint and entries. It refuses every write with 503, as a running
copy does. The first time the bucket answers, the copy lists each
pointer and log again from the start, so a bucket restored to an older
state wins over the cache.

The cache is scoped to its store URL (`<folder>/<scope>/store-url`). It
keeps at most 4 GB of blobs, the least recently used out first. For
each log it keeps the newest two checkpoints and the entries after the
older one, which is what a boot reads. Each file is written whole and
renamed into place. A failed cache write is ignored. Nothing in the
cache is written back to the bucket, and losing it loses nothing.
Without `-store-cache`, such a copy has nothing to serve, and `/readyz`
answers `unavailable`.

**More than one copy.** Any number of copies run on one bucket. Each is
a working copy: it caches heads, and it reads at most 2 s behind
another copy's commit. Two copies that write the same log at the same
time race for the next entry's conditional create. The loser reads the
log again and asks its receive op again (`logs.Receive`), so every write
lands once, in one order. `server/resilience_test.go` pins this with the
fault harness: an answer lost after the bucket kept the write, a copy
stopped in the middle of a write, and a slow bucket.

Without `-store`, the server runs over **the dev store**. The edit
document is in memory, or, with `-edits <path>`, in one flat file on
this machine. There is no history and no publishing: `/plans`,
`/builds`, `/pointers` and the store's log are off. The dev store is
for `-export`, which refuses a store, and for a server that starts
bare. The server logs `DEV STORE` at startup. `server/dev.toml` gives
`just serve`, `just dev` and `just qa` a store in memory instead. A
deployment runs with `-store`.

**Settings** (`config.go`, `flags.go`). Every setting is a flag.
`-config <file>` sets them from a TOML file:

- Each key is a flag's name.
- A `[table]` prefixes its keys with its name and a dash (`[oidc]`
  `issuer` is `-oidc-issuer`).
- A relative path is read from the file's directory.
- A URL or `bucket:<key>` is used as given.

`-config` repeats, and a later file overrides an earlier file. A flag on
the command line overrides every file. A key that names no flag stops
the boot with the file's name. A value that its flag refuses does the
same. Secrets stay out of the files: the store's credentials come from
the environment or the workload's identity, from the sources that
`DEPLOYMENTS-SERVER.md` names ("Prepare the bucket").

`server/dev.toml` holds the development defaults: the fixture feed and
streets, `mem://` store and attachments, the stub work order system,
`-rt-proxy`. It lists every setting, and comments out the rest at their
defaults (`TestDevConfigShowsEverySetting` refuses a flag that it does
not show). `server/local.toml` is gitignored and is a developer's own.
It is laid over `server/dev.toml` (`server/local.example.toml`). `just
serve`, `just dev` and `just qa` read both.

**Running it.** The server stops on SIGTERM or SIGINT in two steps:

1. `/readyz` answers 503 `draining` for `-drain` (default 5 s), so a
   load balancer stops sending requests.
2. The listener closes, and requests in flight get up to 30 s (`serve`
   in `main.go`).

The HTTP server bounds each client (`newHTTPServer`): headers within
10 s, a request body within 5 min, idle keep-alives 2 min, and a
response 10 min.

`just server-image` builds the server as one container image with ko
(no Dockerfile). The image holds the binary on a distroless base. The
server's modules, `web/dist`, and the geocoder policy are in
`server/kodata/`, which ko copies to `$KO_DATA_PATH`. When
`KO_DATA_PATH` is set, every module and file flag defaults to its copy
there, and `-addr` defaults to `:8080`. The image therefore runs with
only its deployment's flags. [DEPLOYMENTS-SERVER.md](../../DEPLOYMENTS-SERVER.md)
is the operator's guide.

The manifest pins the network the server publishes, the zip it came
from, the published plan set (store mode), the streets and walk table,
and the server's module's hash.

## The routes

`server/openapi.json` holds the contract for each route: its method,
parameters, bodies, and status codes. Go tests hold it to the
real server, and a gate holds each change to expand, then contract
([release-artifacts.md](release-artifacts.md), "The HTTP contract").
This section says what the server does behind each route, and gives a
body or an answer only where the behavior needs it.

- `/healthz` reads nothing (`health.go`).
- `/readyz` reads the `edits` head, which the store rechecks at most
  once per 2 s. It answers `degraded` (200) while the bucket does not
  answer and the cached heads serve labelled reads, as a copy that
  booted on its disk cache does. It answers `unavailable` (503) when the
  store holds no head to serve, in memory or in its disk cache. Without
  `-store`, it is `ready` with `store: "none"`.
- `/` serves the pages. With `-store`, the server serves the pinned
  build's files (the `build` pointer's head,
  [release-artifacts.md](release-artifacts.md)), and `-dist` (default
  `web/dist`) for what no build holds, such as the basemap. Without
  `-store`, it serves `-dist`. Both use `http.ServeContent`, so the
  Range requests that PMTiles needs work.
  - The caching and security headers are the rules that the export
    writes as `_headers`
    ([static-deployment.md](static-deployment.md), "A host must send
    these headers"). A file with its content hash in its name is
    `immutable`. Everything else is `no-cache`, so an open page's update
    check meets a new build ([updates-offline.md](updates-offline.md)).
  - The server gzips text and wasm types for a client that sends
    `Accept-Encoding: gzip` (`gzipStatic` in `headers.go`). It never
    compresses a Range request, a HEAD request, or any other type: the
    basemap archive and the `.gz` documents go out as stored. A
    compressed answer has a weak ETag and `Vary: Accept-Encoding`.
- `/builds`: `POST {"build": "<hash>"}` moves the `build` pointer to a
  build that its log holds, as a rollback or a move forward again
  (store mode; `docs/FORMATS.md`, "Builds"). It is a write, protected
  like the others. `POST /builds?ui=<id>` moves one UI's pointer,
  `build.<id>`, instead. `-ui-dist <id>=<dir>,…` names each UI's
  bundle, which the server commits to that pointer as it commits
  `-dist` ([release-artifacts.md](release-artifacts.md), "Each UI at
  its own URL").
- `/rt?url=<encoded>` is a dev relay. The route exists only with
  `-rt-proxy` (`server/dev.toml` turns it on). It fetches an
  http(s) feed (15 s timeout, 32 MB cap) and returns it with CORS
  headers, for a dev config whose feeds send no
  `Access-Control-Allow-Origin`. It relays any URL for any caller, so
  the server binds 127.0.0.1 when it is on. A deployment's pages fetch
  the agency's feeds directly.
- `/geocode/{autocomplete,search,reverse}` proxies the Pelias geocoder
  that `-geocoder` names (`<url>/v1/<op>`). It forwards the user's input
  only (`text`, the focus point, the reverse point), and adds the
  deployment's query policy (`-geocoder-policy`, default
  `server/geocoder.json`). It answers 404 when no geocoder is
  configured. The geocoder is an oracle, never a document on the bus
  ([web.md](web.md), "External services").
  The justfile's `geocoder-build` and `geocoder` recipes run a local
  Pelias.
- `/manifest` holds the deployment's pins (`server/openapi.json` lists
  its fields), and the server omits the optional ones when they are
  empty. These need a word here:
  - `network_hash` and `boundary_hash` name the authoritative network
    and its boundary (the import's numbering).
  - `feed_url` appears in an export that names the feed instead
    The pages import that URL themselves.
  - `timezone` is the agency's zone. The server counts its today in
    this zone, and every page reads its clock in it.
  - `<store kind>_hash` and `log_seqs` pin each log's documents ("One
    path for a log").
  - `auth` says how a page signs in ("Authentication and
    authorization").
  - `services` lists the external services that the server proxies
    (for example `services.geocoder: {path, query}` with `-geocoder`).

  The server serves it `no-cache` with an ETag that names what it pins.
  The ETag is the store's manifest head. Without the store, it is a
  hash of every field but `generated_at`. An open page's poll is
  therefore a 304 while nothing moved.
- `/streets` serves the canonical streets document imported from the
  configured layer (`-streets`), bare (the bytes that `load_streets`
  takes). It is gzipped, and its ETag is the streets hash. The server
  re-imports it when the file changes (mtime-checked). The pages read
  the same document by the manifest's `streets_hash` (`/blobs`, below;
  `servedStreets` in `src/engine.ts`).
- `/walks` serves the walk table over the published streets and network
  (both `-streets` and `-feed`). The server derives it again when either
  changes. It is gzipped, and its ETag is the walks hash. The pages read
  it by the manifest's `walks_hash` (`servedWalks`).
- `/network` serves the import's answer for the configured zip
  (`-feed`) as is: the network, its boundary, the import's notes, and
  the network hash. It is gzipped when the client takes it. Its ETag is
  the network hash, so a client that holds it revalidates for a 304.
  When the zip is republished, the server re-imports it on the next
  request (mtime-checked). The planner's page takes it, because it forks
  the network into a plan (`servedNetworkValue` in `src/engine.ts`).
- `/network/document` and `/network/boundary` split the same answer
  into its two documents, for pages that register them without reading
  them.
  - `/network/document` is the bare network as gzipped bytes
    (`application/gzip`, never `Content-Encoding`). The browser hands
    them to the modules compressed, and kernel op 256 decodes the stream
    as it parses.
  - `/network/boundary` is the boundary as JSON.
  - Both are under the network hash's ETag. The document's bytes are
    the import's canonical serialization, and unzipped they hash to the
    pin.
  - The pages read the same two documents by the manifest's
    `network_hash` and `boundary_hash` (`/blobs`, below;
    `servedNetwork`).
- `/blobs/{hash}` and `/blobs/{hash}.gz`: with or without the store, the
  server serves every document that the manifest pins (the network, its
  boundary, the streets, the walk table) by its hash. It serves the
  JSON, and the same bytes gzipped (`application/gzip`, never
  `Content-Encoding`). The blobs are immutable, and the ETag is the
  path. Every blob's bytes hash to its name. A static host serves this
  same layout ([static-deployment.md](static-deployment.md)), so one
  page code reads either deployment. A store blob of the same hash wins,
  with its envelope in headers (`publishedBlob` in `export.go`). A blob
  whose kind's read class is `restricted` (the policy) is never served:
  404.
- **Authentication and authorization.** Any `-oidc-*` flag
  turns it on. Then the server verifies bearer JWTs:
  - The key is a JWKS (`-oidc-jwks-url`, cached by `kid`, refetched on
    an unknown kid) or a pinned PEM (`-oidc-key`).
  - Only RS256 and ES256 are accepted.
  - `iss` and `aud` must match exactly. `exp` and `nbf` have leeway.

  Reads stay public. Before any write, the server asks the
  cap-authz, in its own instance, over the policy document. A
  write needs a role that the policy grants write on that pointer. The
  The verified `sub` is the store log's `writer`.

  The policy is the store's `policy` pointer. The server seeds it from
  the `-policy` source at first boot. Later changes are
  authenticated CAS commits.

  `-oidc-name-claim` (dotted, default `name`) is the claim that a record
  uses to name the person, for example a survey's surveyor. A
  token without it is named by its `sub`.

  `-oidc-client-id` is the pages' own public client at the provider
  (authorization code with PKCE). `-oidc-scope` is the scopes that a
  page asks for. With the client id, the manifest's `auth` names the
  issuer, the client, and the scopes, and the pages sign in to write
  (`web/src/signin.svelte.ts`).

  With no `-oidc-*` flag, auth is off, and the server logs this loudly.
  Use this for development only. For `just serve` and `just dev`, put
  `policy` and the `[oidc]` table in `server/local.toml`.

  The deployment handles these items:
  - The first JWKS fetch trusts the IdP's HTTPS. Pin its certificate at
    the proxy for more.
  - The reverse proxy throttles repeated 401s.
  - A browser gets and refreshes its IdP token on its own. It presents
    the token on writes only.
- `/edits` is the edit document. `POST` takes a client document and
  answers the LWW-merged result (cap-sync's `sync_edits`). The write
  path clamps client timestamps to the server clock (D-56). `GET` reads
  the server's document. With `-store`, the record is the disruptions
  log ("One path for a log"; `docs/FORMATS.md`, "The logs"):
  - A `POST` is one call to cap-sync's `receive` (`sync sync`): the
    client's document brought forward and read, its clock clamped to
    the entry's, merged with the fold, its drafts submitted, and the
    entry composed. The server creates the entry at the next seq. A
    write that changes nothing writes no entry. A lost race reads the
    log again and asks again. A gap in the log answers 503.
  - `GET` serves the fold: cap-log's `fold` brings the records to the
    log's head at most once per 2 s, and cap-sync's `log_document`
    assembles the document. Every 256 entries the server stores a
    checkpoint. A fresh process starts from the newest checkpoint that
    equals the fold from the one before it.
  - At startup, a store with an `edits` pointer and no log gets the
    log's first entry from the pointer's head (writer `migrate:edits`),
    else from the `-edits` file (writer `migrate:-edits`): the receive
    op's `migrate`. Then the tombstone compaction (D-57) runs as one
    entry (`sync compact`, writer `gc`). It skips a tombstone that a
    held record of another log names, so the server gives it the
    logs as the reference checks read them.
  - A page never writes the log by intent (`POST /logs/disruptions` is
    405): `POST /edits` is the client protocol.
  - An entry the fold cannot apply is quarantined, and a gap stops
    writes. Each logs a line that starts with `ALARM disruptions log`.

  Without `-store`, `-edits <path>` persists the document as one flat
  file across restarts, else it is in memory (the dev store).
- `/logs/disruptions` (store mode) is the log itself. `GET ?since=<seq>`
  answers the entries after a seq, the head, the gaps, the newest
  checkpoint, and the quarantined entries. `POST
  /logs/disruptions/revert` `{"seq"}` writes the entry that reverts one
  entry: cap-log's `revert`, with the records cap-sync's `log_revert`
  writes back, stamped to win the merge (D-193). It answers 409, naming
  the record, when a later entry changed it. `POST
  /logs/disruptions/close-gap` `{"seq", "cause"}` writes the entry that
  closes a gap. With auth on, every `/logs/` call needs the `audit` rule
  on `edits`, because an entry names its writer inside its bytes.
- Store mode (`-store` only) also exposes the generic surface:
  - `GET /pointers/{name}`. For `edits` it answers the disruptions
    log's fold: its hash and the log's seq.
  - `GET /pointers/{name}/log?since=<hash>` is delta sync over the
    append-only log. For `edits` it is the pointer's history before the
    disruptions log.
  - `POST /pointers/{name}` is a compare-and-swap from the head the
    writer read (`from`). A conflict answers 409 with the current head.
    For `edits` the write is one entry of the disruptions log, and
    `from` is the fold's hash. The kind table (`-kinds`; D-202) governs
    every other write: a kind it does not name, or one with its own
    route, answers 400 naming the route. cap-authz `validate` checks the
    policy (422 with its answer), and the server stores its canonical
    bytes.
  - `GET /blobs/{hash}` accepts `Accept: application/gzip`, as
    `/blobs/{hash}.gz` does.
  - `GET /manifest` serves the `manifest` pointer's blob. The server
    commits it again when the published feed, streets, module, or
    services change.

  With auth on, the log endpoint requires a read (an `audit` rule
  implies it). It redacts `writer` unless audit is granted. An agent
  chose this, and the user has not decided it (D-164). `GET
  /permissions` (any valid token) returns the caller's
  effective-permissions projection (cap-authz `effective`). It is the
  only client-readable form of the policy.
- `POST /plans/freeze?purpose=…` (store mode) freezes a version (D-191).
  The body is a network, checked as `POST /plans` checks it, in one
  `plans pick` op with no adoption. The server commits the network to
  `plan.<id>`, then writes the frozen version as one entry. Nothing is
  put in force, and no stops are read.
- `POST /plans/adopt` `{"network_hash"}` (store mode) puts a frozen
  version in force from its date (D-191), as one `plans pick` op with no
  freeze. It refuses a network that no frozen version has
  (`not_frozen`). The pick names the stops log entry it read
  (`stops_seq`).
- `POST /plans` (store mode) publishes a pick (`docs/ROADMAP.md` 1,
  D-74): a freeze and an adoption in one entry of the plans log, with
  the version's purpose from `?purpose=` or `published`. The body is the
  pick's network in any serialization.
  - The whole write is one op of the plans log's receive op: `plans
    pick`. cap-plans registers the network on its kernel load,
    with the stops document beside it.
  - The op first answers what refuses the network as a pick: its issues,
    and a pick with no service in its period. cap-network's
    `publishable` answers the same, because both call transit-network's
    `publish_check`.
  - The op then freezes the pick and adopts it, and answers the entry,
    or why it refuses the pick: the chain, or a pick dated before today.
  - `POST /plans/retire` `{"id"}` retires a pick that has not taken
    effect, as one entry of the plans log (`plans retire`).
  - "Today" is the agency's date: the host clock read in the network's
    zone. The zone is the feed import's (cap-network), else the last
    published pick's, which the kernel's `load_network` answers beside
    the pick's hash before the `pick` op runs. Until a zone is known, it
    uses the host's own zone. `-today` pins it on a test server.
  - The op answers the network's canonical bytes in `made`. The server
    commits them to `plan.<id>`, then writes the plans log's entry. The
    log's fold is the set, and the manifest pins it as `plans_hash`.
    Clients read the set and each pick's network from `/blobs`. `GET
    /logs/plans` answers the entries.
  - The plans log has no revert: a pick is corrected by its own rules. A
    page never writes it by intent (`POST /logs/plans` is 405), because
    a freeze stores its network first.
  - With a stops document, the pick reads it before the freeze.
    It reads each stop's record on its first date (transit-stops
    `read_pick`, the function behind cap-stops `read_pick`), so the
    network that the server stores is the read one, whatever copy the
    client sent. It also warns about the stops it visits that are not in
    service over its period, after the network's own warnings. The op
    derives the period from the published set: from the pick's date
    until the next pick that is not retired. The server does not read
    the set's picks or their status.
  - The server never writes the stops document here.
- `POST /stops` (store mode) writes the stops document:
  `{"from": "<the head read, or empty>", "stops": {...}}`.
  - The stops log's receive op (cap-stops `receive`, the op `stops
    validate`) first names the person who writes as the surveyor of
    every survey that the held document does not hold
    (`attribute_surveys`). The name is never one that the client
    sent. It is the verified token's `-oidc-name-claim` (default
    `name`), else its `sub`. With auth off, it is the caller's
    `X-Writer` label, else `anonymous`.
  - It then validates the document and answers its canonical bytes.
  - The server writes the change as one entry of the stops log, and
    refuses it when `from` is not the document at the head. The manifest
    pins the document as `stops_hash`, and `/blobs` serves it.
  - A stops write changes no pick. A stop's life is its own, and a pick
    only reads the document.
  - The server refuses a raw pointer commit of the stops (400).
  - The first document arrives through this route (a page makes it with
    cap-stops `seed`).
  - The server records each new import of the agency's feed (`-feed`, at
    boot or when its bytes change) in a stops document that the store
    holds: the feed applies, and the planner sees what it did.
    cap-stops `import_events` runs on the feed's first service date, and
    each event names the feed's hash. One entry of the stops log records
    them, and the writer is `import:feed`. The server records nothing
    when the feed differs in nothing (`recordFeed`, `stops.go`).
- `POST /attachments` and `GET /attachments/<hash>` (`-attachments`, a
  bucket of its own: any store URL) carry the photos and drawings that
  surveys and requests hold. The server creates each object
  `attachments/<hex>` only if it is absent, with its media type in the
  object's metadata.
  - A POST needs a write on the policy's `attachments` pointer.
  - A GET needs any signed-in person whenever sign-in is on
    (`protectSignedIn`). It answers with `Cache-Control: private`.
  - The manifest's `services.attachments` offers the route only with a
    bucket.
  - `store gc` never touches this bucket.
- `POST /stop-requests/work-order` (store mode, `-work-orders`; the
  stop requests' `work_order` in the kind table names the route and its
  ops) takes `{"request": "<id>", "action": "open"|"refresh"}`. The work order
  system's adapter ([adjacent-systems.md](adjacent-systems.md))
  opens a work order for the request, or reads its status. cap-requests
  `work_order_call` answers what to send it (the request, or the work
  order the request holds), and the server reads nothing out of the
  document. The stops log's receive op (`requests record_work_order`)
  writes the answer on the request (dated the server's today) over the
  requests at the head when the write runs, as one entry of the stops
  log. The writer is `work-orders:<system>`.
  - A body with a `key` whose entry landed answers the request's work
    order, and the system is not asked again, so an outbox's resend
    never opens a second one.
  - The manifest's `services.work_orders` offers the route only when an
    adapter is configured.
- `POST /stop-requests` (store mode) writes the stop requests document
: `{"from": "<the head read, or empty>", "requests": {...}}`.
  - The stops log's receive op (the op `requests validate`) validates
    it and answers its canonical bytes.
  - The server writes the change as one entry of the stops log, and
    refuses it when `from` is not the document at the head. The manifest
    pins the document as `stop_requests_hash`, and `/blobs` serves it.
  - The server refuses a raw pointer commit of the requests (400).
  - The first request begins the document (a page makes it with
    cap-requests `put`).
- `/logs/stops` (store mode) is the stops log (cap-stops `receive`, "One
  path for a log"; `docs/FORMATS.md`, "The logs"). It holds the stops
  document and the stop requests document, because completing a request
  and adding the stop's event is one transaction.
  - `POST /logs/stops` `{"key", "ops": [{"cap", "op", "args"}],
    "read_seq", "claimed_at_ms"}` is the stops log's intent write
    ([storage.md](storage.md), "The intent write"). It runs each op over
    the documents at the head, in order (stops: `add_event`,
    `correct_event`; requests: `put`, `record_work_order`), names the
    surveyor and validates as `POST /stops` does, and writes what
    changed as one entry. Two writers who change different stops both
    land.
  - A `correct_event` whose `saw` (the event's hash as read) is stale
    conflicts, as does a record that an entry after `read_seq` changed
    and that the write changes too. A key that landed writes nothing.
  - Each document the write writes needs its write rule (`stops`,
    `stop_requests`).
  - `POST /logs/alerts` and `POST /logs/notes` take the same body.
  - `GET /logs/stops?since=`, `POST /logs/stops/revert`, and `POST
    /logs/stops/close-gap` work as the disruptions log's do, and need the
    `audit` rule on `stops`. A revert takes out of the document a record
    the reverted entry created.
  - Every write to the stops or the requests is one entry, whatever
    route sent it. The server folds the log at most once per 2 s and
    assembles each document from its records. It stores a checkpoint
    every 256 entries. A store with `stops` and `stop_requests` pointers
    and no log gets entry 1 from their heads at startup (writer
    `migrate:stops`, the note naming each head). The pointers stay, read
    only.
- **One path for a log** (`server/logs`) covers each log whose
  kinds the kind table names with a receiving capability (`receive`):
  the disruptions, stops, plans, alerts, and notes logs. The server
  reads the head, makes one call, and creates the entry.
  - Each kind's document is its capability's `log_document` over the
    fold's records. The manifest pins it as `<store kind>_hash`
    (`edit_records_hash`, `stops_hash`, `stop_requests_hash`,
    `plans_hash`, `frozen_versions_hash`, `service_alerts_hash`,
    `alert_templates_hash`, `notes_hash`), with the log's seq in
    `log_seqs`. `/blobs` serves each by its hash, and `GET /logs/<log>`
    names them too.
  - A write registers the documents at the head in the receiving
    module, each on its kernel load when the module declares the kind
    (its `about`). Any other goes inline as `documents` (the plans' and
    the edit document). Then the server calls the module's `receive` op
    with the write (the checkpoint, the entry's header, and the entries
    after `read_seq`) and the write's ops. The op runs the ops, checks
    them, and answers the entry and each document it wrote.
  - The server checks the write rule of each document written (the
    kind's first pointer, else its store kind: `edits`, `stops`,
    `service_alerts`), and creates the entry. When another write took
    the seq, it asks again over the new head. Documents the op made (a
    pick's network, in `made`) are stored first.
  - The receive op also judges the write's references (D-233;
    `transit_log::receive`). The server gives it the kind table's kinds,
    each log's records now, and, for a person's write, each log's
    records as of the seq its writer read (`read_seq`, and `read_seqs`
    for the other logs; a log not named, now). It folds the other logs
    from the store (`referenceFolds`, `foldAt`), never from their folds,
    so it takes no other log's lock. A log with no entries is not
    judged.
  - A reference that the write adds to a record missing as of the read
    is `422 {refusals: [], references}`, and nothing is written. A
    kind's own route answers the same body. The server's own writes are
    never refused for it.
  - A reference that the entry touches and that is broken now is a
    warning. `POST /logs/<log>` and each kind's own route answer it as
    `references`, beside the answer, with the referring record, its
    field, the target, the id it names, and the entry. A revert (`POST
    /logs/<log>/revert`) answers the same, through cap-log
    `references`. A check that cannot read a log refuses nothing.
  - Each write also carries its date in the agency's time zone
    (`write.today`, `writeDate`), for the rules that read a dated state:
    the stops log's receive op judges each request's stop by its
    status on that date. `writeDate` reads the zone of the feed already
    loaded and never loads the feed, because the feed's own import
    writes the stops log while the feed's lock is held. A rule's refusal
    and its warning have the same shape as a missing target's, with
    `rule`, the reason in words.
  - The writes that the server makes itself take the same call with no
    write rule: the feed's differences (`stops import_events`, over the
    feed's network, which the server registers first) and the plans'
    writes, whose routes check the `plans` rule first.
  - An empty log gets entry 1 from its kinds' old pointers (the kind
    table's `pointers`) at startup: the receive op's `migrate` records
    their heads as they are (writer `migrate:<log>`).
  - `POST /logs/<log>` refuses a write that a record changed since the
    writer read it, and an op the log does not take. A key that landed
    answers before the ops run.
  - A kind's own route (the kind table's `post`) is one op of the kind's
    checking capability over the request's body: `POST /notes`
    (`write`), `POST /service-alerts` (`validate`), `POST
    /alert-templates` (`validate_templates`), `POST /stops` (`stops
    validate`), and `POST /stop-requests` (`requests validate`). A whole
    document over a head the writer did not read (`from`) is refused. An older service alerts document comes
    forward in the op. A header names the fields that a document
    carries and this build does not know.
  - A log that a page never writes by intent (the kind table's
    `intent: false`: the plans log, the disruptions log) answers `POST
    /logs/<log>` with 405. A log with an LWW kind (the edit document)
    writes back its own records on a revert, through that kind's
    `log_revert` (D-193), and its document is there from the empty log
    on.
- `GET /feed?date=` answers the GTFS zip in force on the date (default
  the server's today).
  - It is cap-network `export` of the published pick in force (cap-plans
    `in_force` answers which pick).
  - With a stops document, cap-stops `export_pick` runs first over the
    pick's period, which the op derives from the published set. A stop
    whose record changes inside the period is then one stop per version.
  - The server keeps the result by the pick's network hash and the stops
    document's.
  - Before the first pick takes effect, it is the configured `-feed` zip
    itself (D-74).
  - The ETag is the zip's hash.

`go test ./...` in `server/` proves two things: the server runs the
exact module bytes that the manifest pins, and it answers what the
browser does.

## Why

Each entry is one important choice, headed by the code it governs.
**Holds** is what must stay true. Change it only with the user, after
reading **Because**.

### No domain logic in Go (`server/*.go`)
- **Holds:** every domain answer comes from a capability module. The Go host does I/O, routing, and proxies only. Logic that the server needs becomes a capability.
- **Because:** the server and a page that have the same documents must answer the same bytes ([web.md](web.md), "Parity"). Go logic is a second implementation to keep in step.

### A write is one op (`server/logs` `Receive`, `server/plans` `writePick`)
- **Holds:** each write the server takes is one op of its log's receive op. A pick's checks (the network's publish check, the stops it reads) run inside cap-plans' `pick`, which links their libraries. Go registers the documents and stores what the op made, and sequences no checks.
- **Because:** checks sequenced in Go are host logic, and the read-seq check (D-233) replays a write's check as one op. The cost is a larger single cap-plans module: it links transit-network and transit-stops.

### The server imports the feed; pages register the network (`network.go`, `feedSource`)
- **Holds:** with a server, pages never parse GTFS. They register the imported network.
- **Because:** C-TRAN's import costs ~2.7 s and a ~187 MB module. Registering the result costs ~0.45 s and ~9–12 MB.
- **Exception:** a serverless deployment may have each page import the agency's GTFS URL ([static-deployment.md](static-deployment.md)).

### One module, fresh per import (`wasm.go`, `importFeed`, `streetsSource.Load`, `exportedPick`)
- **Holds:** the server links one assembly, as each UI does. The long-lived instance holds the logs' heads and refreshes past a memory ceiling. An import, a publish, and a pointer write's check get a fresh instance, closed after. authz keeps its own instance. A boot whose documents the store holds imports nothing.
- **Because:** one host shape for every host ([hosts.md](hosts.md)), and each document registered once in the server too. Wasm memory never shrinks, so closing an instance is the memory hygiene, and the refresh does the same for the instance that lives on. A trap in another capability must not take the fail-closed authz check with it.
- **Rejected:** a module for each capability, with one flag each: eleven flags, a flow that needs two capabilities composed in Go across modules, and each long-lived module holding its own copy of a document.

### Two ways to fetch a published document (`/network/document`, `/network/boundary`, `/streets`, `/walks`, `/blobs/<hash>`)
- **Holds:** both stay. The routes serve the current document at a stable address, pinned by ETag (a 304 while unchanged). `/blobs/<hash>` serves a document by the hash that the manifest names. The pages read this one.
- **Because:** a static host serves the `blobs/` layout too, so one page code reads either deployment. One manifest read names the network and its boundary together. The routes tie them only by matching ETags across two requests.

### Gzip as content, never transport (`/blobs/<hash>.gz`, `Accept: application/gzip`, `/network/document`)
- **Holds:** the server serves a gzipped document as `application/gzip` bytes, never `Content-Encoding: gzip`. `/blobs/<hash>` answers gzip on `Accept: application/gzip`. `/blobs/<hash>.gz` answers it by path. Both stay.
- **Because:** the page hands compressed bytes to the module, which decodes as it parses (kernel op 256). The 9.6 MB network text therefore never exists whole on the page. The `.gz` path exists because a static host cannot negotiate on a header. A host that decompresses anyway still works, because the modules read either form.

### The server compresses its static files and sends the rider pages' policies (`headers.go`, `gzipStatic`)
- **Holds:** the server compresses text and wasm itself, and it sends a strict policy on the two rider pages. Both come from the same rules as the export's `_headers`.
- **Because:** a deployment is right with no load balancer rule. A missing rule would cost a rider three times the bytes with no error. The policy needs the deployment's own origins, and the server and the export know them. The operator would let them drift.
- **Rejected:** a load balancer that compresses. A policy on every page: only the rider pages have been walked under one.
- **Provisional:** D-182 and D-183.

### Inputs from a URL or the bucket (`-feed`, `-streets`, `-policy`, `source.go`)
- **Holds:** each input is a URL, `bucket:<key>`, or a dev path. With `-store`, the server commits each read (`source.feed`, `source.streets`).
- **Because:** a process must be disposable. Its replacement, given the bucket and the same flags, serves the same manifest. Local files tie a deployment to one machine.
- **Rejected:** the bucket only, with a separate import step.

### Stale reads labelled, writes refused (`httpx.Fail`, `httpx.LabelStale`, `X-Store-Stale`)
- **Holds:** while the bucket does not answer, the server serves cached heads and blobs labelled with their age. It refuses every write (503).
- **Because:** blobs are immutable, so only a head can be stale. Reads peak during disruptions, when outages are likeliest. If the server acknowledged a write that the bucket has not taken, the write would be lost with the process. The browser keeps its documents and resends.
- **Rejected:** failing reads past an age. Failing every request.

### Public reads, authenticated writes (`authz.go`, `protectWrite`, `-oidc-*`, `-policy`)
- **Holds:**
  - GETs are public.
  - Writes need a role that the policy grants on that pointer.
  - Tokens verify by JWKS or a pinned key, RS256/ES256 only.
  - The roles' claim path is configurable.
  - The policy is a store document, seeded from a file at first boot and changed by audited commits.
- **Because:** reads publish what riders need. Writes are the shared state. Symmetric or `none` algorithms would let anyone forge tokens. The claim's location is the IdP's. A file re-read per request would leave policy changes unaudited. A store-only policy needs a break-glass bootstrap.

### Attachments live apart and are read only signed in (`attachments.go`, `-attachments`)
- **Holds:** photos and drawings sit in a bucket of their own, and each object is named by its content hash. A record names a file only by that hash, never a URL. The server hands out the bytes to a signed-in person whenever sign-in is on, unlike every other read. Only `POST /attachments` adds a file, behind the write check on `attachments`. `/attachments/<hash>` is behind sign-in alone, so it only reads (405 for any other method).
- **Because:** files are large, and their access and keeping differ from the documents'. A URL would tie every record to one provider and bucket name (the infrastructure must not couple to one store or provider). A hash keeps a record portable and shows a swapped file. A site drawing can show parcels and private property, and a photo can catch people. The deployment is internal-facing, and its attachments are staff's to see.
- **Rejected:** URLs in the records. Attachments among the store's blobs. Public reads.

### A copy boots on its disk cache (`-store-cache`, `store/diskcache.go`)
- **Holds:** a copy keeps the blobs, heads, entries, and checkpoints it read on its own disk. Started while the bucket does not answer, it serves them read-only, labelled stale, and `/readyz` answers `degraded`. The bucket wins the first time it answers. The flag is opt-in.
- **Because:** every copy shares one bucket, and a platform restarts containers on its own schedule. Without a cache, an outage that outlasts one restart leaves every copy with nothing to serve. Everything the cache holds but a head is immutable, so only a head can be wrong, and a head is labelled with its age and checked again from the start. The cache is a working copy, never the record: it writes nothing back, so losing it loses nothing.
- **Rejected:** a cache on by default (a container with no writable volume would fail at boot or lose it on each restart). A distinct `/readyz` answer (a balancer treats it as `degraded` anyway). Serving the cache once the bucket answers.
- **Provisional:** D-224.

### Degraded is ready (`/readyz`, `health.go`)
- **Holds:** `/readyz` stays 200 while the bucket does not answer and cached heads serve. `/healthz` reads nothing.
- **Because:** every instance shares one bucket, so all of them would fail the check at once. A balancer that pulled them all would turn a write outage into a read outage, while the pages read the cached heads fine. Restarting a process cannot fix the bucket.

### The dev store is not a deployment mode (no `-store`)
- **Holds:** without `-store`, the server warns at startup and keeps working. It does not refuse.
- **Because:** with no store there is no history, rollback, or publishing, and edits live on one machine. This is right for a developer and for `-export` (which writes a bundle and exits). It is never right for a server that riders depend on. A warning keeps a bare server to one command.

### Settings are a file whose keys are the flags (`config.go`)
- **Holds:** `-config` sets flags from TOML files. The command line beats a file, and a later file beats an earlier file. An unknown key refuses the boot. The flags themselves stay, every one.
- **Because:** the server grew past thirty flags. A long command line (or an environment variable per flag, passed through each recipe) is hard to read, review, and keep. A file keeps a deployment's settings in one reviewed place and the developer's defaults in the tree. Keys that are the flags' own names mean one vocabulary (`-help` documents both). The flags stay, so the image, the e2e servers, and a one-off override need no file. TOML is used because people edit it and it takes comments. An unknown key is refused because a misspelt setting that the server ignores silently is a deployment that runs on a default nobody chose.

### `/rt` is a dev relay, off by default (`handleProxy`, `-rt-proxy`)
- **Holds:** the route exists only with `-rt-proxy`, which binds localhost. Deployments fetch feeds directly.
- **Because:** it fetches any URL for any caller. Beyond localhost it is an open relay into the server's network. Feeds are fixed at their source (HTTPS with CORS).

### Picks: retired, not removed; never landing in the past (`POST /plans`, `/plans/retire`)
- **Holds:** a retired pick stays in the set marked `retired` and never governs. The server refuses a pick with no service in its period. It refuses a new, restored, or moved pick dated before today. A correction under its own date always publishes.
- **Because:** retirement is the tombstone everywhere (no hard deletes). A pick with no service would leave every rider page with a network that runs nothing on its date. A pick dated in the past would rewrite what the deployment says ran.
- **Rejected:** removal from the set. Publishing with warnings only.
- **Pinned:** `TestRetiringAPick`, `TestPublishRefusalsNameTheirReasons`.
