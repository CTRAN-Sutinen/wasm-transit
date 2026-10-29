# The streets model

Streets are model data, not exogenous files. Walking time, walksheds,
detours, deadheads, and drawing a pattern all depend on the street
network, so it is a hash-addressed document on the bus like the
transit network: a canonical streets document, read from street
layers (the agency's GIS export, TIGER, OSM) by an import at the
boundary, the way the network is read from GTFS. Its identity is
the hash of the model, never of the file it came from. Its
consumers are the detour editor today and pattern drawing in
planning next; every item below is designed for both.

## Scope

The document, its import, and cap-streets' import / load_streets
are on the bus (`docs/ARCHITECTURE.md`, transit-streets and
cap-streets). What is left:

1. **The original app reads the published streets.** The server
   publishes the document (`/streets`, gzipped, the hash its ETag)
   and ride-now registers it by its bytes (`docs/ARCHITECTURE.md`,
   server). The original app's detour editor still imports a layer
   itself — the browser downloads the 99 MB C-TRAN roads GeoJSON
   and cap-streets reads it — and keeps the document in the page
   (`state.svelte.ts`, `installStreets`). Left: it registers
   `/streets` when the server publishes it (its DataSources import
   stays for a layer the server does not have), and persisting a
   document in the store follows
   [sync-persistence.md](sync-persistence.md). A compact encoding
   (delta-coded varints, names in a table: about 3.4 MB before
   compression against the JSON's 4.5 MB gzipped) is a later step
   if the gzipped JSON is not enough.
2. **The bus profile.** Which classes a bus may be drawn on, as an
   op parameter with an op-owned default (walking's is decided and
   built: `docs/ARCHITECTURE.md`, transit-streets). Blocked: D-26.
3. **Walking for riders.** The walk table and trips from any point
   are on the bus (cap-streets `walks` and `access`, the router's
   `load_walks` / `walks_hash` and place ends; D-33), and ride-now
   reads them (the nearby list by walk, D-34). Left:
   - **ride-now walks C-TRAN's streets.** ride-now walks the
     published streets, from the rider's location or a spot chosen
     on the map (`docs/ARCHITECTURE.md`, web section;
     `web/e2e/published-network.spec.mjs`, "ride now walks the
     published streets" and "ride now chooses a trip's end on the
     map"), on the fixture only. Left: a dogfood session on C-TRAN
     with a streets document walking is worth showing on — an
     OSM-sourced one (QAF-85).
   - **Walksheds.** Coverage over the walking graph instead of 400 m
     circles (`docs/ROADMAP.md` 4).
4. **Drawing on streets.** The future consumer the document is
   designed for: a planner draws a new pattern by tracing between
   waypoints along the streets a bus may use, as the detour editor
   traces a detour. The committed alignment is GEOMETRY in the
   network document and never references the streets document or a
   graph position (`crates/AGENTS.md`, Law 1), so a new streets
   import never breaks a plan. Street names name new stops and the
   trace's legs (a route answer that carries the names it follows).
   The service-design UI registers the published document (item 1).
   Blocked: D-26
   (the bus profile).
5. **Planned streets.** A street a plan serves that the data does
   not have. Blocked: D-31.
6. **The geography release.** The basemap and the streets document,
   built from one source snapshot and
   pinned together in the manifest beside `feed_hash`, the
   basemap's glyphs and sprites in it (closes QAF-84). Blocked:
   D-29.

Geocoding is not this unit's: it is an external service the host
calls (D-36; `docs/ARCHITECTURE.md`, server section, `/geocode`).

## Done when

Each item names its pins when it is scoped: a law test in the
owning crate, the dispatch test and runner gate for a new op, an
e2e spec for what a UI reads, and a C-TRAN session (`just soak`,
`caps-run`) for anything touching the real layer's size.
