# The streets model

Streets are model data, not exogenous files. The streets document is
on the bus like the network: an import reads it from street layers
(the agency's GIS export, TIGER, OSM), and its identity is the hash of
the model, never of the file it came from
(`docs/architecture/crates/transit-streets.md` and cap-streets). The
server publishes it and its walk table, the store keeps both
(`docs/FORMATS.md`), and ride-now walks them
(`docs/architecture/generated-uis.md`, ride-now).

Drawing on the streets is not this unit's. A pattern's line drawn
along the streets is [ui-stops.md](ui-stops.md) item 5. A detour
traced along them is [ui-disruptions.md](ui-disruptions.md)
question 4.

## Scope

1. **Walking on C-TRAN's streets.** Ride-now walks the published
   streets from the rider's location or a spot chosen on the map, on
   the fixture only (`web/e2e/published-network.spec.mjs`, "ride now
   walks the published streets" and "ride now chooses a trip's end on
   the map"). The agency's road layer cannot carry walking (QAF-85).
   Left: a dogfood session on C-TRAN with a streets document worth
   walking on, such as an OSM-sourced one.
2. **Walksheds.** Coverage over the walking graph instead of 400 m
   circles (`docs/ROADMAP.md` 4): a walkshed op over the walking
   graph, and layer coverage read against it.
3. **Street attributes.** The import keeps a way's class, name, and
   one-way. The stops page wants the speed limit and lanes beside a
   stop's survey ([stop-site-and-requests.md](stop-site-and-requests.md)
   item 1). The import reads them where the layer has them.
4. **The geography release.** The basemap and the streets document,
   built from one source snapshot and pinned together in the manifest
   beside `feed_hash`. D-29: the area is whatever the agency's GIS
   exports.

Geocoding is not this unit's: it is an external service the host
calls (D-36; `docs/architecture/server.md`, `/geocode`).

## Done when

Each item names its pins when it is scoped: a law test in the
owning crate, the dispatch test and runner gate for a new op, an
e2e spec for what a UI reads, and a C-TRAN session (`just soak`,
`caps-run`) for anything touching the real layer's size.
