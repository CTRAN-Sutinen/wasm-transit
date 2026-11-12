# streets

Where along the streets? Snap a point to the nearest street and route between two positions along the street graph, so a traced detour (or a pattern drawn the same way) is street geometry, never freehand chords — and how long a rider walks along them: between a network's stops (the walk table the router plans transfers over), and from any point to the stops near it (the ends of a trip from anywhere). The streets are a document: import reads a street layer (an agency GIS export, TIGER, OSM — GeoJSON lines) into the canonical streets document, whose hash is the streets' own, not the file's. The domain code lives in the shared transit-streets library; the module writes nothing.

## import

Read a street layer — a GeoJSON FeatureCollection or Feature of LineString/MultiLineString geometries — into the canonical streets document and register it: the answer carries the document (the host keeps it), its streets_hash, and its ways, segments and nodes. Lines that are not streets (a CFCC railroad or water line, an MTFCC outside S1, an OSM railway or waterway) are left out; every street gets a class from its CFCC, MTFCC or OSM highway (unknown when none says), a name (FULLNAME, name, NAME), and its one-way (TIGER OneWay, OSM oneway — a one-way runs first point to last). The same streets in another feature order, digitized the other way, or with other properties are the same document and hash. Errors when the layer holds no street. Bulk-bytes op 0; pass the raw GeoJSON to transit_invoke_bytes.

## walks

The WALK TABLE: every walk between the network's stops along the streets, within max_walk_secs (default 600 — ten minutes) at speed_mps (default 1.3), as a document {streets_hash, network_hash, speed_mps, max_walk_secs, walks: [{from, to, walk_secs}]} in (from, to) order, symmetric — and its walks_hash (the router's load_walks answers the same). A rider walks every street class but motorway, ramp and unknown, both ways along one-way streets; a walk is each stop's walk to its nearest walkable street, the street distance, and the walk on to the other stop, rounded up to the second. A stop with no walkable street nearby walks nowhere. Which streets the document holds is the host's choice of source (walking wants one with real classes and footways).

## access

The walks from a POINT ({lat, lon}: a tapped spot, the rider's location) to the network's stops within max_walk_secs (default 600) at speed_mps (default 1.3) — the walks a stop at the point would have in the walk table — nearest first, each {stop_id, walk_secs, path}: the path the rider walks, from the point to its street, along walkable streets, to the stop. With `to` (another point), direct is the walk from the point straight to it ({walk_secs, path}; null when none is within the limit or no `to` was given). The walks are a router PLACE's walks as they are (plan's {id, name, walks}: planning reads the seconds, itinerary_paths draws the paths), direct is plan's direct, and the walks rank the directory's stops_near. A point with no walkable street nearby walks nowhere (walks empty).

## snap

The nearest street position to (lat, lon) — the projected point plus its graph address (segment, parameter along it), and how far the query point was. Segments are numbered through the document, way by way, so an address holds for one streets hash: a trace commits the geometry, never the address. Null when no street is nearby.

## route

The street geometry between two snapped positions: the shortest path along the graph, as points ending exactly at the target and starting AFTER the origin (traces chain legs — each leg's start is the previous leg's committed end; prepend `from` for a standalone trace), plus length_m: the path's length along the graph from the origin itself — "how far by road?" answered by the router, never a host-side sum. Empty points and null length when the positions sit on disconnected parts of the network — the caller falls back to a straight line. `streets`: the streets the path follows, in order ({class, name?, length_m}; a street is one entry however many segments it has). The BUS PROFILE (D-26): a bus is drawn down any street — the planner judges a grey area — and `to_confirm` names the streets whose class is in `confirm` (default unknown, path, steps, track) for the planner to confirm; `confirm: []` asks nothing. Never a refusal.
