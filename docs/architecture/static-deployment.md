# Static deployment

A rider UI deploys with no server, each on its own (D-131). Run
`just export <ui> OUT feed.zip [streets.geojson]` to build that one UI
alone. The recipe first runs `just build-ui <ui>`, which runs
`UI=<ui> vite build` and writes `web/dist-ui/<ui>/`. That folder holds
only the UI's page and scripts, the capability manifest, its linked
assembly, and each of its capabilities alone for the fallback (D-88).
It holds the basemap's files only when the UI's code includes
`src/basemap.ts`. It also holds an `index.html` that opens the page and
keeps the query. Its service worker precaches only that page.

Then `just export` runs the server binary's `-export DIR` (export.go)
over that folder, with `-dist`. The export boots as `just serve` does.
The same modules import the feed and the street layer, and they derive
the walk table. The export writes the bundle and exits. The bundle
holds:

- `-dist`'s files.
- `manifest`, the server's own manifest.
- Every published document as `blobs/<hash>` and `blobs/<hash>.gz`.
- `live-feeds.json` (`-live-feeds`), the deployment's own file. It lists
  the agency's feeds, with no `proxy`. A static host has no `/rt`.

The export refuses a non-empty directory. It also refuses `-store`,
because the export takes the feed's import, not a store's picks. A new
schedule is a new export (D-131).

The bundle's paths are the ones the server answers, so the pages are
the same bytes on either. The server serves every UI from `web/dist`,
and service design runs only there. `go test`'s
`TestTheExportIsWhatTheServerServes` pins the bundle byte for byte
against the server.

A static deployment has none of these:

- The edit document. `edits` 404s, and the pages show no detours or
  cancels.
- A geocoder. The manifest lists no service, and place search finds
  stops.
- The dev relay.
- Every write.

A page under a path prefix works. The pages and the service worker read
every path relative to themselves.

A host must send these headers. The rules are in
`server/headers.go`. The server sends them on its
static files, and the export writes the same rules as `_headers` in the
bundle. Cloudflare Workers and Netlify read that file. A host that does
not read it must send the same headers by hand
(`DEPLOYMENTS.md`, "Headers every host must send").

- **Caching.** A content-addressed file may be cached forever:
  `assets/*-<hash>.*`, `caps/*-<sha256>.wasm`, `blobs/<hash>`, and
  `blobs/<hash>.gz`. These files revalidate (`no-cache`): the pages,
  `manifest`, `caps/manifest.json`, `live-feeds.json`, and `sw.js`.
  The host's default does that, so the file has no rule for them. The
  server applies the same rule (`staticCacheControl`). Without the
  immutable rule, a first visit fetches each large file twice, because
  the service worker's precache cannot reuse an uncached response.
- **Every file** sends `X-Content-Type-Options: nosniff` and
  `Referrer-Policy: strict-origin-when-cross-origin`.
- **Each rider page** (`ride-now.html`, `stop-display.html`) sends a
  `Content-Security-Policy` and a `Permissions-Policy`. The policy lists
  what the page reads: its own origin, the origins in `live-feeds.json`,
  the agency's GTFS URL when the export has one (`-feed-url`), and, for
  the page that draws a map, the two tile hosts (OpenFreeMap and
  Protomaps). The policy allows `'wasm-unsafe-eval'` for the modules and
  nothing else that runs code. It refuses frames (`frame-ancestors
  'none'`). Ride now allows the rider's location for itself. The stop
  display allows none. `index.html` has no policy, because it holds an
  inline script that forwards to the page.
- **Compression.** The server gzips text and wasm types for a client
  that takes gzip. A static host does it itself. Never compress a `.gz`
  blob again, and never send `Content-Encoding` for it. A host that adds
  one anyway still works, because the modules read a document gzipped or
  not.

`web/e2e/headers.spec.mjs` runs each rider page under its policy, from
the server and from the export, and fails on any violation.
`web/e2e/static-server.mjs` applies a bundle's `_headers` as Cloudflare
does. `go test` pins the rules (`server/headers_test.go`).

An upload adds files. It never deletes blobs. An open page pinned to
the previous manifest still reads its documents. A rollback uploads the
previous `manifest`.

The spec `web/e2e/static-deploy.spec.mjs` pins this on a plain file
server (`web/e2e/static-server.mjs`). The server runs ride-now under
`/ride/` and the stop display under `/display/`, as two deployments.
Each deployment carries only its own UI.

A serverless deployment may follow the agency's GTFS URL instead of
publishing a network (D-132). Run `just export-feed-url <ui> OUT URL
[streets.geojson]` (`-export` with `-feed-url`). It writes the same
bundle. Its manifest names `feed_url` and no network.

Each page fetches the zip, and it imports the zip in its own assembly
(network `import`). It checks the URL once a day. `servedNetworkPin`
answers with the network it imported while the zip's bytes stay
unchanged. Once the bytes change, `servedNetworkPin` answers with a pin
that no network holds. The host then adopts the new import, as it
adopts a republish (`importFollowed` in `web/src/engine.ts`). The page
keeps the last zip in Cache Storage (`gtfs-feed`). A device therefore
boots offline on that zip. The feed must answer with CORS.

This mode has costs. Every device imports the feed (C-TRAN: ~2.7 s, a
~187 MB network module). No walk table between stops ships, because the
walk table derives from the network, and only the page has the
network. A street layer still ships.

`web/e2e/static-feed-url.spec.mjs` pins the boot, the daily check, and
the offline use of the last zip. A changed zip is adopted at the next
daily check and not before. Go's
`TestTheExportNamesAFeedURLAndImportsNothing` pins the export.

Operating a static deployment (building, Cloudflare Workers'
configuration, updates, rollback) is covered in `DEPLOYMENTS.md`.

## Why

Each entry covers one choice that the design depends on. Its heading
names the code the choice governs. **Holds** is what must stay true.
Change it only with the user, and only after you read **Because**.

### A rider UI needs no server (`just export <ui>`, `-export`, `vite.config.js` "ONE UI")
- **Holds:** Ride-now and the stop display each deploy as static files,
  on their own. Service design runs only with the server. The bundle's
  paths match the server's (`manifest`, `blobs/<hash>[.gz]`). So the
  page code is one.
- **Because:** an agency can run the rider apps on any static host,
  and each UI is its own product with its own release cadence. Reading
  the server's layout keeps the server a first-class option, not a fork ([README.md](README.md), "Deployment
  shapes").
- **Rejected:** one bundle of every UI (ships service design where it
  cannot run).
- **Decided:** D-131. **Pinned:** `web/e2e/static-deploy.spec.mjs`,
  `TestTheExportIsWhatTheServerServes`.

### A serverless deployment may follow the GTFS URL (`-feed-url`, `feed_url`, `importFollowed`, `followedPin`)
- **Holds:** With no server, a manifest may name the agency's GTFS URL.
  Each page fetches, imports, and checks that URL once a day. After a
  failed check, it checks again an hour later. The page keeps the last
  zip in Cache Storage. It adopts a changed zip without a reload. With a
  server, pages never parse GTFS.
- **Because:** a serverless deployment should not need a redeploy for
  each new feed. The import costs every device time and memory, and
  there is no walk table between stops. Only this mode accepts that
  cost.
- **Rejected:** a scheduled re-export and redeploy; a Cloudflare Worker
  cron running the import.
- **Decided:** D-132. **Pinned:** `web/e2e/static-feed-url.spec.mjs`,
  `TestTheExportNamesAFeedURLAndImportsNothing`.
