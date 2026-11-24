# transit-streets

The streets model. Open work: `docs/units/streets-model.md`.

**`model.rs`: the streets document.**
- `Streets` holds ways: points as [lat, lon] microdegrees, one-way, class, and name.
- The document is canonical, so its hash is the streets' own, not a file's. The same streets are one document in another feature order, digitized the other way, drawn twice, or carrying other properties.
- Canonical form: two-ways in canonical direction, a one-way running first point to last, ways sorted and unique.
- `validate` holds a stored document to this form.

**`geojson.rs`: the boundary.**
- A street layer is an agency GIS export (such as C-TRAN's Trapeze roads), TIGER, or OSM, as GeoJSON lines. The crate reads it into the document.
- `CFCC`, `MTFCC`, or OSM `highway` classify each street. They also leave out lines that are not streets, such as railroads and water (D-30).
- `FULLNAME` / `name` / `NAME` name the street.
- TIGER `OneWay` and OSM `oneway` set its direction.
- A malformed vertex breaks a line.
- No other code reads a layer's properties.

**`lib.rs`: the routing graph** built from the document.
- Segments are numbered way by way.
- The address that a snapped position names is valid for one streets hash. So a trace commits geometry, never an address. A network never depends on a streets document.
- Snap finds the nearest street position.
- Route finds the shortest path. It is one-way aware. Turn restrictions are a non-goal until a restriction table ships in the data.
- The path names the streets it follows in order (class, name, meters along each).

**`bus.rs`: the bus profile** (D-26).
- A bus is drawn along every class and is never refused.
- Unknown classes and the path-like classes (path, steps, track) are the streets that a planner confirms.

**`walking.rs`: how long a rider walks.**
- The walking profile is every class but motorway, ramp, and unknown. Riders walk both ways along one-way streets.
- A walk has three parts: each stop's walk to its nearest walkable street, the street distance, and the walk on to the other stop.
- The default speed is 1.3 m/s. The default limit is ten minutes. Walks are symmetric by construction.
- The walk table (`WalkTable`) is a document derived from a streets document and a network. It holds every walk between the network's stops. It names both hashes, the speed, and the limit. The router plans transfers over it.
- A point walks as a stop there would (`walks_from`). The op gives the point's walks to the stops within the limit, nearest first. Each walk has the path the rider walks: the point, along walkable streets, the stop. These are a router place's walks and the ends of a trip from anywhere.
- Walking quality is the streets document's. It needs a source with real classes and footways (OSM), not one whose classes cannot tell a freeway from an arterial (QAF-85).

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### Streets: any source, per-mode profiles (transit-streets import, `walks`, `access`, streets `route` `to_confirm`)

- **Holds:** every street op reads a streets document by hash, whatever source filled it.
- **Holds:** walking is strict. It allows every class but motorway, ramp, and unknown. One-way does not bind a rider. The limit is at most ten minutes at 1.3 m/s by default.
- **Holds:** a bus may be drawn on any class. Unknown and path-like classes are named for the planner to confirm.
- **Because:** the source is data, not model. An agency's roads give scheduling parity, and OSM gives sidewalks. Which one a deployment loads is its choice (C-TRAN's roads cannot carry walking, QAF-85). A planner judges a grey-area road. The data cannot.
- **Rejected:** one fixed source for every mode; refusing a bus on unknown classes.
- **Decided:** D-25, D-26 (classes: D-30, D-103).
