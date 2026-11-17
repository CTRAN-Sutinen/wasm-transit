# streets

Where along the streets? The capability snaps a point to the nearest street. It routes between two positions along the street graph, so a traced detour (or a pattern drawn the same way) is street geometry, never freehand chords. It also answers how long a rider walks along the streets:

- between a network's stops: the walk table that the router plans transfers over;
- from any point to the stops near it: the ends of a trip from anywhere.

The streets are a document. `import` reads a street layer into the canonical streets document. A street layer is an agency GIS export, TIGER, or OSM, as GeoJSON lines. The document's hash is the streets' own hash, not the file's hash. The domain code lives in the shared transit-streets library. The module writes nothing.

## import

Read a street layer into the canonical streets document, and register it. The layer is a GeoJSON FeatureCollection or Feature of LineString or MultiLineString geometries.

The answer carries these fields:

- the document, which the host keeps;
- `streets_hash`;
- the counts of its ways, segments, and nodes.

The op leaves out lines that are not streets. These are a CFCC railroad or water line, an MTFCC outside S1, and an OSM railway or waterway.

Every street gets these properties:

- A class, from its CFCC, MTFCC, or OSM highway. The class is `unknown` when none of them says.
- A name, from the property `FULLNAME`, `name`, or `NAME`.
- Its one-way, from TIGER OneWay or OSM oneway. A one-way runs from the first point to the last.

The same streets give the same document and hash when they come in another feature order, digitized the other way, or with other properties. The op returns an error when the layer holds no street.

This is a bulk-bytes op, op 0. Pass the raw GeoJSON to `transit_invoke_bytes`.

## walks

Build the walk table: every walk between the network's stops along the streets, within `max_walk_secs` (default 600, which is ten minutes) at `speed_mps` (default 1.3).

The answer is a document `{streets_hash, network_hash, speed_mps, max_walk_secs, walks: [{from, to, walk_secs}]}`. The walks are in `(from, to)` order, and the table is symmetric. The answer also carries its `walks_hash`. The router's `load_walks` answers the same.

A rider walks every street class except motorway, ramp, and unknown. A rider walks both ways along one-way streets. A walk has three parts:

- each stop's walk to its nearest walkable street;
- the street distance;
- the walk on to the other stop.

The walk time is rounded up to the second. A stop with no walkable street nearby walks nowhere.

The host chooses which streets the document holds, by its choice of source. Walking needs a source with real classes and footways.

## access

Find the walks from a point `{lat, lon}` to the network's stops within `max_walk_secs` (default 600) at `speed_mps` (default 1.3). The point is a tapped spot or the rider's location. The walks are the walks that a stop at the point would have in the walk table.

The answer lists the walks nearest first. Each walk is `{stop_id, walk_secs, path}`. The `path` is the path the rider walks: from the point to its street, along walkable streets, to the stop.

With `to` (another point), `direct` is the walk from the point straight to it, as `{walk_secs, path}`. It is null when no walk is within the limit or when the request gave no `to`.

The answer relates to other ops in these ways:

- The walks are a router place's walks as they are: plan's `{id, name, walks}`. Planning reads the seconds. `itinerary_paths` draws the paths.
- `direct` is plan's `direct`.
- The walks rank the directory's `stops_near`.

A point with no walkable street nearby walks nowhere, and `walks` is empty.

## snap

Find the nearest street position to `(lat, lon)`. The answer holds these fields:

- the projected point;
- its graph address: the segment, and the parameter along it;
- how far the query point was.

Segments are numbered through the document, way by way. So an address holds for one streets hash. A trace commits the geometry, never the address. The answer is null when no street is nearby.

## route

Find the street geometry between two snapped positions. It is the shortest path along the graph, as points. The points end exactly at the target and start after the origin. Traces chain legs, and each leg's start is the previous leg's committed end. For a standalone trace, prepend `from`.

The answer holds these fields:

- `length_m`: the path's length along the graph from the origin itself. This answers "how far by road?". The router answers it, never a host-side sum.
- `streets`: the streets that the path follows, in order, as `{class, name?, length_m}`. A street is one entry however many segments it has.
- `to_confirm`: the streets whose class is in `confirm`, for the planner to confirm.

The points are empty and `length_m` is null when the positions sit on disconnected parts of the network. The caller then falls back to a straight line.

The op uses the bus profile. A bus is drawn down any street, and the planner judges a grey area. `confirm` defaults to `unknown`, `path`, `steps`, and `track`. `confirm: []` asks nothing. The op never refuses.
