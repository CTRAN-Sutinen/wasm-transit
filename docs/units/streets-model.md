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

1. **Serving the document.** Today a host imports a layer itself:
   the browser downloads the 99 MB C-TRAN roads GeoJSON and
   cap-streets reads it (3.3 s native). The import is once per
   layer, not per client: the document is built once (the runner,
   at deploy) and served with its hash pinned, and a client
   registers it with load_streets. Measured on C-TRAN's Trapeze
   roads (`sha256:48fe6c89…`, 112,662 ways): the document is 22.7
   MB of JSON, 4.7 MB gzipped, against the layer's 99 MB (15.8 MB
   gzipped). The server compresses nothing today (`server/`, every
   route). A compact encoding (delta-coded varints, names in a
   table) is about 3.4 MB before compression; it trades the JSON's
   readability for parse time and memory, and is a later step if
   the gzipped JSON is not enough. The browser keeps the document
   (`state.svelte.ts`), so persisting it in the store follows
   [sync-persistence.md](sync-persistence.md).
2. **The bus profile.** Which classes a bus may be drawn on, as an
   op parameter with an op-owned default (walking's is decided and
   built: `docs/ARCHITECTURE.md`, transit-streets). Blocked: D-26.
3. **Walking for riders.** The walk table is on the bus (cap-streets
   `walks`, the router's `load_walks` / `walks_hash`). Left:
   - **Trips from any point.** A streets op answers the walks from a
     point (a tapped spot, the rider's location) to the stops within
     the limit, each with its street path; the router plans from and
     to those walks (access and egress legs at the ends of an
     itinerary, drawn along their paths).
   - **ride-now walks the streets.** Its host loads a streets
     document and the walk table and names them in every plan, plans
     from the rider's spot, and its nearby list ranks stops by walk
     (closes QAF-83). It waits on item 1 — a page cannot download
     the 99 MB layer — and on an OSM-sourced streets document for
     walking to be worth showing (QAF-85).
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
   The service-design UI loads the document (item 1). Blocked: D-26
   (the bus profile).
5. **Planned streets.** A street a plan serves that the data does
   not have. Blocked: D-31.
6. **The geography release.** The basemap, the streets document,
   and later the gazetteer, built from one source snapshot and
   pinned together in the manifest beside `feed_hash`, the
   basemap's glyphs and sprites in it (closes QAF-84). Blocked:
   D-29.

Queued, not scoped: geocoding (a gazetteer document read from the
same layers' names and address ranges). Blocked: D-27, D-28.

## Done when

Each item names its pins when it is scoped: a law test in the
owning crate, the dispatch test and runner gate for a new op, an
e2e spec for what a UI reads, and a C-TRAN session (`just soak`,
`caps-run`) for anything touching the real layer's size.
