# transit-network

The authored network model (planner authority).

Its stops are read by `transit-stops`, which depends on it. The network depends on no stops document.

**Routes, directions, and patterns.**
- A route has directions (`Route.directions`, each named).
- A pattern runs in at most one direction (`Pattern.direction`, validated against its route). Bay and short-turn variants share one direction.
- The model also holds timing profiles, service types, pattern trips, and the runcut.
- A service type is the service identifier. Its calendar rule links it to days.
- `PatternTrips` is one pattern's trips under one service type. It is not a planner's schedule. The timetable a planner reads is a route's direction under one service combination, across its patterns.

**Service combinations** (`service_combinations`, D-55).
- A service combination is a set of service types that run together on the same dates.
- Its name is its members joined with " + ".
- It is shown with the dates it holds on. The dates fold into spans: a new span starts where the next date is more than seven days on (D-94).
- Every date with service is in exactly one combination. Its members are exactly the types with trips that run that date.
- So types that never share a day are never pooled. A dated exception is a combination of its own (tests/combination_law_test).

**Trips.** A trip is a stored record. It has:
- its own id;
- its own departure: `Departs::At` is one start, and `Departs::Every` is a frequency, with one instance per band slot that shares the trip's id and times;
- its own time at every pattern stop (`StopOffset`s from its start). A timepoint stop carries both times. An unscheduled stop materializes by distance between its scheduled neighbours.

A pattern's timing profiles are the planner's tool for producing a new trip's times. They are never the schedule.

**Blocks.**
- A block references trips by id and runs each trip whole. A frequency trip's instances run back to back.
- A block may hold service types that never share a day. The network judges a block day by day.
- The network judges a wait between two adjacent trips of a block against the recovery required there (`required_recovery`, the one rule every reader applies, D-9).
- When the caller holds a work rule, the work rule's requirement wins (`validation_issues_under`).
- Otherwise, for a pair that carries an authored layover, the requirement is the smallest authored layover at that terminal (`recovery_floors`). The terminal is the layover's `at`, else the stop where the trip before ends.
- A layover's own authored length is slack, never the requirement. An imported layover is the whole observed gap, and a later arrival spends slack.

**Alignments.**
- An alignment is a line on the ground and its own record (`Network::alignments`: its id, its points, the distances published along it).
- A pattern runs an alignment by reference (`Pattern::alignment`), so a pattern is its stops along its line.
- Patterns may share one alignment. An alignment that no pattern runs still stands.

**Materialize and the document.**
- `materialize` answers a day's instances of the stored trips.
- The document leads with `"kind": "network"` and `"schema_version"` (2).
- Its chain (`version.rs`, kept forever, wrapped by cap-migrate) brings v1 forward. v1 is any untagged document. This includes documents written before trips were stored: each old departure becomes a trip `{pattern_trips}@{start}`. It also includes documents written before alignments stood alone: a pattern's inline `path` becomes an alignment under the pattern's id.
- `trip_instance` answers one instance without a date. A one-start trip answers at its start. An exact frequency trip answers on its grid. A non-exact frequency trip answers at any start inside a band.
- A trip instance is addressed (trip, start) everywhere.
- A planner's page keeps its plans as one plan set (`PlanSet`, `plan_set.rs`): `"kind": "plan_set"`, `"schema_version"` (1), and each plan's network document by the store's id for it. The page owns and writes the set; the type is its shape, schema, and tag.

**Where a bus is going** is one rule (`signs.rs`, `visit_destination`). The first of these applies:
1. The visit's own headsign. It overrides the trip's headsign at that visit only, as GTFS's stop_headsign does.
2. The trip's headsign.
3. The final stop, named as its place (the parent station, never the bay).

Every board row and itinerary leg reads this rule.

**The GTFS Schedule spec.** The network models everything the GTFS Schedule spec describes, in model vocabulary. Each family has its own module.
- Per pattern stop: its headsign, pickup / drop-off and continuous-stopping rules (`StopRule`), booking rules, and whether its distance was published.
- Per trip: its headsign, public number, and wheelchair / bike / car accommodation.
- Per route: its long name, description, URL, text color, sort order, continuous stopping, operator, fare network, contactless acceptance, and a published vehicle type where the mode does not say it. The short name is `Route::name`, possibly empty. `Route::label` displays it.
- Per stop: its kind (stop or station) and `StopInfo` (code, spoken name, description, URL, fare zone, wheelchair boarding, platform, level, timezone, street access).
- `operators.rs`: operators, the publisher, attributions, translations.
- `places.rs`: station parts (entrances, nodes, boarding areas), levels, pathways, transfer rules.
- `fares.rs`: zone fares and fare products (areas, fare networks, timeframes, rider categories, media, offers, leg / join / transfer rules).
- `demand.rs`: demand-responsive service. It holds location groups, zones (a GeoJSON FeatureCollection), booking rules, and trips that visit stops, groups, or zones within windows.

GTFS is a codec at the boundary, not the model. No GTFS types leak into this crate.

**Transfer points** are authored flags on stops. They compose over the stop's walking group: the stop and its siblings that share a parent, the parent, and the children. Transfers happen across a street, not at one pin.

**Service plans.** Every document is a service plan (`PlanMeta`: id, name, effective_from). Agencies implement a few picks a year at defined dates. An empty identity means unversioned (a fresh import).

**Authoring** (`edit.rs`).
- Typed, atomic `NetworkEdit`s answer the next document. They never mutate the last one.
- Removals refuse while something references the entity.
- There are two levels: entity upserts and removes, and gestures.
- A gesture is one edit per thing a planner does. Each gesture carries its coherence rule:
  - Timing aligns with timepoints.
  - Every trip's times align with its pattern's stops.
  - Layovers sit only between trips.
  - The edit mints ids and reports them.
  - The plan frame merges field-wise.
  - A fork given its period end carries every weekly calendar that still runs in the calendar's final week to that end. Dated service and closed periods stay as they are (D-41, D-112).
  - A redrawn line names whether it moves one pattern or every pattern on its alignment. It reports the patterns it touched (D-11).
  - `add_pattern_through` lays a new pattern through stops picked in order. It uses great-circle distances, makes its ends timepoints, and adds one all-day profile end to end. So a new route is a route, its directions, a pattern each way, and their headways in one change.

**The timetable's gestures** (`timetable_edit.rs`).
- The gestures are headway, span, running time, and stops.
- The planner states a gesture for a route + direction (+ service type). The gesture applies across the route's patterns.
- A trip that stays is retimed and keeps its id, so its blocks keep it.
- A trip that goes leaves its blocks.
- A new trip gets a fresh id and is unassigned.
- A running-time change names its service type.

**Validation** answers two kinds of result.
- Issues gate. A block that names a trip the network does not have is an issue.
- Warnings never gate. Examples are unassigned trips and a pick with no service in its first week.

**Geometry** (`geometry.rs`).
- A pattern's line is the alignment it runs, else its stops joined. This is the model's own contract.
- `entity_focus` answers where an entity is. A stop's focus is its point. The focus of a route, pattern trips, service type, or block is the lines and stops of every pattern it runs.
- `route_lines` puts the network on a map: every route's distinct lines, by route id, narrowed to the service types given.
- `service_keys` returns each key's calendar as authored and the routes and trips it carries. A timetable trip names its key.

**The planner's lists** (`listing.rs`).
- Rows of any kind, narrowed by every filter term (id, name, and a route's long name).
- The stops a rider's typing finds (`find_stops` / `name_match`). The match is exact, then an abbreviation on the sign ("Van" for "vancouver"), then a typo (D-48; the thresholds are D-91).
- The places near a point (`stops_near`). `stops_near_walked` ranks them by the point's walks.
- A route's composition:
  - its patterns and the stops they visit;
  - its pattern trips by service type;
  - its patterns by direction;
  - each direction's variants.
- A variant is a way a rider rides a direction. Its patterns run over the same places in order, and a station's bays count as one place. Variants are named by their ends and by what sets them apart, most trips first.
- Each entry's trip count is its trips' instances, the materialized day's own expansion.
- A stop's serving routes, its record, and its visits. A visit is every pattern position that names the stop, with what the pattern runs under the members asked.
- The planner's timetable (`timetable`): one route, one direction, one service combination.
  - It lists every trip that the combination's members run on the direction's patterns, by start.
  - It has one column per stop in route order. A variant's stops sit where the variant leaves the others.
  - Its rows are `materialize_service`'s trips, never a second derivation. Each row has its own end-to-end running time, `run_secs`.

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### Stop search is fuzzy on the stops' own names (directory `stops`)

- **Holds:** typos and abbreviations match the names the feed gives. There are no authored aliases.
- **Because:** aliases would be a hand-maintained separate dataset. A geocoder covers everyday names where a deployment runs one (D-36).
- **Rejected:** aliases in the network; deferring entirely to a geocoder.
- **Decided:** D-48 (what counts and the order: D-91).
