# The streets model

Streets are model data, not exogenous files. Walking time, walksheds,
detours, deadheads, and drawing a pattern all depend on the street
network, so it is a hash-addressed document on the bus like the
transit network: a canonical streets document, read from street
layers (the agency's GIS export, TIGER, OSM) by an import at the
boundary, the way the network is read from GTFS. Its identity is
the hash of the model, never of the file it came from. Its
consumers are walking (ride-now) today, and drawing a detour
([ui-disruptions.md](ui-disruptions.md)) and a pattern in planning
next; every item below is designed for both drawings.

## Scope

The document, its import, and cap-streets' import / the kernel's load_streets
are on the bus (`docs/architecture/crates.md`, transit-streets and
cap-streets). What is left:

1. **Serving the streets.** The server publishes the document
   (`/streets`, gzipped, the hash its ETag) and ride-now registers
   it by its bytes (`docs/architecture/server.md`); no page imports
   a street layer itself, and a UI that draws on streets (the
   detour, [ui-disruptions.md](ui-disruptions.md)) registers the
   published document the same way. Left: persisting the document
   in the store follows the store's formats (`docs/FORMATS.md`).
   A compact encoding (delta-coded varints, names in a table: about
   3.4 MB before compression against the JSON's 4.5 MB gzipped) is a
   later step if the gzipped JSON is not enough.
2. **The bus profile, read by a UI.** On the bus: a route names the
   streets it follows and, under the bus profile (D-26: every class
   drawn, never refused), the streets to confirm (cap-streets `route`
   `streets` / `to_confirm`, `confirm` the op's parameter;
   `crates/transit-streets/tests/bus_profile_law_test.rs`; on C-TRAN's
   roads a route along SE Powell Blvd, which the layer leaves
   unclassed, draws and names it to confirm — D-26's pins). Left: the
   drawing gesture of item 4 shows `to_confirm` for the planner to
   confirm before it commits.
3. **Walking for riders.** The walk table and trips from any point
   are on the bus (cap-streets `walks` and `access`, the router's
   `walks_hash` (the kernel's `load_walks`) and place ends; D-33), and ride-now
   reads them (the nearby list by walk, D-34). Left:
   - **ride-now walks C-TRAN's streets.** ride-now walks the
     published streets, from the rider's location or a spot chosen
     on the map (`docs/architecture/web.md`;
     `web/e2e/published-network.spec.mjs`, "ride now walks the
     published streets" and "ride now chooses a trip's end on the
     map"), on the fixture only. Left: a dogfood session on C-TRAN
     with a streets document walking is worth showing on — an
     OSM-sourced one (QAF-85).
   - **Walksheds.** Coverage over the walking graph instead of 400 m
     circles (`docs/ROADMAP.md` 4).
4. **Drawing on streets.** A planner draws a pattern's line by
   tracing between waypoints along the streets (cap-streets `snap` /
   `route`), the way a dispatcher traces a detour, and commits it with
   author `draw_alignment` (D-11: the edit names whether it redraws one
   pattern or every pattern on the alignment, and reports the patterns
   it touched). The committed alignment is GEOMETRY in the network
   document and never references the streets document or a graph
   position (`crates/AGENTS.md`, Law 1), so a new streets import never
   breaks a plan. The route answer names the streets each leg follows.
   Left: the gesture, built as the service-change Route stage's map
   ([ui-stops.md](ui-stops.md) item 5: registering the published
   document, item 1; the trace; the confirmation of item 2; the
   commit and its `touched` patterns shown), its e2e spec, and street
   names naming new stops.
5. **Planned streets.** A street a plan serves that the data does
   not have. D-31: freehand only for now.
6. **The geography release.** The basemap and the streets document,
   built from one source snapshot and
   pinned together in the manifest beside `feed_hash`. D-29: the
   area is whatever the agency's GIS exports.

Geocoding is not this unit's: it is an external service the host
calls (D-36; `docs/architecture/server.md`, `/geocode`).

## Done when

Each item names its pins when it is scoped: a law test in the
owning crate, the dispatch test and runner gate for a new op, an
e2e spec for what a UI reads, and a C-TRAN session (`just soak`,
`caps-run`) for anything touching the real layer's size.
