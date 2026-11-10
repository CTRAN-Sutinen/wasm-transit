# Static deployment

A rider UI deploys with no server, each on its own (D-131): `just
export <ui> OUT feed.zip [streets.geojson]` builds that one UI alone
(`just build-ui <ui>`: `UI=<ui> vite build` → `web/dist-ui/<ui>/` —
only its page and scripts, the capability manifest, its linked
assembly and each of its capabilities alone for the D-88 fallback, the
basemap's files only when its code includes `src/basemap.ts`, and an
`index.html` that opens the page, keeping the query; its service
worker precaches only that page), then runs the server binary's
`-export DIR` (export.go) over it with `-dist`. The export boots as
`just serve` does — the same modules import the feed and the
street layer and derive the walk table — writes the bundle, and exits:
`-dist`'s files, `manifest` (the server's own manifest), every
published document as `blobs/<hash>` and `blobs/<hash>.gz`, and
`live-feeds.json` (`-live-feeds`, the deployment's own: the agency's
feeds, no `proxy` — a static host has no `/rt`). It refuses a
non-empty directory, and `-store` (it exports the feed's import, not a
store's picks: a new schedule is a new export, D-131). The bundle's
paths are the ones the server answers, so the pages are the same
bytes on either (the server itself serves every UI from `web/dist`;
service design runs only there); `go test`'s `TestTheExportIsWhatTheServerServes`
pins the bundle byte for byte against the server.

What a static deployment has not: the edit document (`edits` 404s;
the pages show no detours or cancels), a geocoder (the manifest lists
no service; place search finds stops), the dev relay, and every write.
A page under a path prefix works: the pages and the service worker
read every path relative to themselves.

The host's headers: a content-addressed file (`assets/*-<hash>.*`,
`caps/*-<sha256>.wasm`, `blobs/<hash>` and `blobs/<hash>.gz`) may be
cached forever; the pages, `manifest`, `caps/manifest.json`,
`live-feeds.json`, and `sw.js` must revalidate (`no-cache`) — the
rule the server applies (`staticCacheControl`). A `.gz` blob is
served as its bytes (`application/gzip` or `octet-stream`), never with
`Content-Encoding`; a host that adds one anyway still works (the
modules read a document gzipped or not). An upload adds files and
never deletes blobs: an open page pinned to the previous manifest still
reads its documents, and a rollback is uploading the previous
`manifest`. Pinned by `web/e2e/static-deploy.spec.mjs` on a plain file
server (`web/e2e/static-server.mjs`): ride-now under `/ride/` and the
stop display under `/display/`, two deployments, each carrying only
its own UI.

A serverless deployment may follow the agency's GTFS URL instead of
publishing a network (D-132): `just export-feed-url <ui> OUT URL
[streets.geojson]` (`-export` with `-feed-url`) writes the same bundle,
its manifest naming `feed_url` and no network. Each page fetches the
zip, imports it in its own assembly (network `import`), and checks the
URL once a day: `servedNetworkPin` answers the network it imported
while the zip's bytes are unchanged, and a pin no network holds once
they change, so the host adopts the new import as it adopts a
republish (`importFollowed` in `web/src/engine.ts`). The last zip is
kept in Cache Storage (`gtfs-feed`), so a device boots offline on it.
The feed must answer with CORS. The cost is the import on every
device (C-TRAN: ~2.7 s, a ~187 MB network module — D-68's budgets do
not hold here), and no walk table between stops (it is derived from
the network, which only the page has); a street layer still ships.
Pinned by `web/e2e/static-feed-url.spec.mjs` (boot, a changed zip
adopted at the next daily check and not before, offline on the last
zip) and Go's `TestTheExportNamesAFeedURLAndImportsNothing`.

Operating one
(building, Cloudflare Workers' configuration, updates, rollback) is
`DEPLOYMENTS.md`.

## Why

One entry per load-bearing choice, headed by the code it governs.
**Holds** is what must stay true; change it only with the user, after
reading **Because**.

### A rider UI needs no server (`just export <ui>`, `-export`, `vite.config.js` "ONE UI")
- **Holds:** ride-now and the stop display each deploy as static files, on their own; service design runs only with the server. The bundle's paths are the server's (`manifest`, `blobs/<hash>[.gz]`), so the page code is one.
- **Because:** deployment flexibility is the point: an agency can run the rider apps on any static host, and each UI is its own product with its own release cadence. Reading the server's layout keeps the server a first-class option rather than a fork ([README.md](README.md), "Deployment shapes").
- **Rejected:** one bundle of every UI (ships service design where it cannot run).
- **Decided:** D-131. **Pinned:** `web/e2e/static-deploy.spec.mjs`, `TestTheExportIsWhatTheServerServes`.

### A serverless deployment may follow the GTFS URL (`-feed-url`, `feed_url`, `importFollowed`, `followedPin`)
- **Holds:** with no server, a manifest may name the agency's GTFS URL; each page fetches, imports, and checks it once a day (an hour after a failed check), keeps the last zip in Cache Storage, and adopts a changed zip without a reload. With a server, pages never parse GTFS.
- **Because:** a fully serverless deployment should not need a redeploy for each new feed; the cost (the import on every device, beyond D-68; no walk table between stops) was accepted for that mode only.
- **Rejected:** a scheduled re-export and redeploy; a Cloudflare Worker cron running the import.
- **Decided:** D-132. **Pinned:** `web/e2e/static-feed-url.spec.mjs`, `TestTheExportNamesAFeedURLAndImportsNothing`.
