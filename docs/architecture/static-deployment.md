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

The bundle's paths are the ones the server answers. So the pages are
the same bytes on either. The server serves every UI from `web/dist`.
Service design runs only there. `go test`'s
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

Set the host's headers as follows. A content-addressed file may be
cached forever: `assets/*-<hash>.*`, `caps/*-<sha256>.wasm`,
`blobs/<hash>`, and `blobs/<hash>.gz`. Make these files revalidate
(`no-cache`): the pages, `manifest`, `caps/manifest.json`,
`live-feeds.json`, and `sw.js`. The server applies the same rule
(`staticCacheControl`).

Serve a `.gz` blob as its bytes, with `application/gzip` or
`octet-stream`. Never send `Content-Encoding` for it. A host that adds
one anyway still works, because the modules read a document gzipped or
not.

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
~187 MB network module). The budgets (D-68) do not hold here. No walk
table between stops ships, because the walk table derives from the
network, and only the page has the network. A street layer still ships.

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
- **Because:** Deployment flexibility is the point. An agency can run
  the rider apps on any static host. Each UI is its own product, with
  its own release cadence. Reading the server's layout keeps the server
  a first-class option, not a fork ([README.md](README.md), "Deployment
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
- **Because:** A fully serverless deployment should not need a redeploy
  for each new feed. The import on every device goes beyond the budgets
  (D-68), and there is no walk table between stops. The cost was
  accepted for that mode only.
- **Rejected:** a scheduled re-export and redeploy; a Cloudflare Worker
  cron running the import.
- **Decided:** D-132. **Pinned:** `web/e2e/static-feed-url.spec.mjs`,
  `TestTheExportNamesAFeedURLAndImportsNothing`.
