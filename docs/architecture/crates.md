# The crates — the layering, bottom up

## transit-network

The authored network model (planner authority).

**Stops document.** A pick only reads the stops document (D-157).
- `stop_record::read_records` takes each stop's record as the document has it on the pick's first date.
- `visit_warnings` warns of a stop that the pick's patterns visit and that is not in service over the pick's period. The stop can be planned, permitted, built, retired, or not there yet. A suspended stop warns only if it is suspended for the whole period or with no end.

**GTFS out and in carry the stops' history** (D-150, D-151).
- `export_versions` makes a stop whose record changes inside the export's dates into one stop per version. The last version keeps the id. An earlier version takes `<id>@<date>`. The op also splits each pattern that visits the stop by calendar.
- `import_events` records a feed's differences as events dated the feed's first service date. It reads a made id back as a version.
- `import_feed` does the same, and each event names the feed (`from_feed`, its hash).
- `feed_events` returns the events a feed recorded.
- The stops that the import calls unlisted are only those in service or suspended that day. These are the stops a feed should list.
- `seed` begins a stops document from a network's stops.

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

## transit-router

RAPTOR planning over the network model's materialized day. It is pure (no I/O, clock, or calendar) and GTFS-free.

**The world planned over.**
- `RouterNetwork::build(network, day, date, live)` groups the caller's day (plain or effective) into patterns by actual stop sequence. An edited trip rides its own pattern.
- With a live document, each trip instance rides at its predicted times where the document states them. Delays are against the published schedule, clamped so a trip never runs backwards. Elsewhere the instance rides at the day's own times.
- A trip that the live document cancels is not in the world planned over. A stop that the document skips is not a stop of the trip (`LiveDay::runs`).

**Places.**
- A plan's ends (`End`) are stops or places off the network.
- A place (`Place`) has an id that is never a stop's, a name, and its walks to the stops near it.
- From a place, the rounds start at every stop it walks to, each at its walk's time.
- To a place, the rounds end at the best of its stops plus its walk. They can also end at the direct walk between the ends, which wins ties (D-33).
- The itinerary and its first and last walks name the place.

**Leave-at and arrive-by.**
- `earliest_arrival_between` answers leave-at.
- `latest_departure_between` answers arrive-by. The same rounds run backward from the destination. With no deadline, the answer is the service day's last arrival: the last bus home. Its itinerary is the forward plan from that departure.
- `earliest_arrival` and `latest_departure` are the stop-to-stop forms.

**Legs.** A transit leg names:
- its trip instance;
- the route's label and long name;
- the sign at the boarding visit (`visit_destination`);
- how far each predicted time moved (board/alight_delay_secs, null without a prediction).

**Door to door** (D-40). An itinerary is door to door.
- `leave_secs` is the first boarding minus the walk before it. With no ride, it is `depart_secs`.
- `duration_secs` is the arrival minus the leave time.
- `leave` is the board's one wait rule (`transit_dispatch::wait`) from `depart_secs` to `leave_secs`, for example "leave in 7 min".
- `Itinerary::new` is the one place that derives them.

**The notice.** `notice::plan_notice` is the one place that computes a compared plan's notice.
- The notice is structured (no_trip / unreachable / later / diverted, minutes lost, routes, skipped stops) plus a text line.
- It fires whenever the diverted world arrives later, has no trip, or rides a trip instance whose patched times differ from its unpatched times.

**Transfers and walks.**
- Transfers are footpaths: straight lines (stop pairs within 300 m at 1.3 m/s).
- `with_walks` replaces them with a walk table's walks. They are exactly the table's walks, between the network's stops (D-32).
- A plan's legs bound its rides, never its walks. The walk to the first boarding and the walk from the last alighting settle before any round. So a one-leg plan walks to and from its bus.
- With its arrival and rides fixed, a plan then walks the least it can. It boards the first ride, makes each transfer, and leaves the last ride at the stops whose walk is shortest and still connects (D-35).
- `walk_path` draws a walk leg. It goes through the leg's stops. Where the leg walks from or to a place, it goes along the place's walk path. A `PlaceWalk` carries the path that `access` answered. Between two places, it is the direct walk's path.

## transit-codec

The network codec, in both directions, over the whole GTFS Schedule spec.

**The feed layer.**
- `Feed` (src/feed.rs) reads and writes every spec file and column as the exact text the feed carried. It is lossless by construction and writes deterministic zip bytes.
- `import` and `export` map a feed to an authored network document and back.
- The feed layer also answers a feed's summary and its feed_info identity, and base64 for the JSON seam.

**What the import keeps.**
- A feed's trips are the document's trips: the agency's ids, every scheduled time exactly (offsets from the trip's first arrival), frequency trips as frequency trips, and blocks by trip.
- A feed's shapes are the document's alignments: the feed's ids, every point and published distance, including shapes that no trip runs.
- Patterns are stop sequences along an alignment. There is one pattern per (route, direction, stops, exactness flags, shape, and what each stop row says). A stop row says its headsign, pickup / drop-off, continuous stopping, booking rules, and published distance. How long a trip takes is the trip's own.
- A trip with a row that is not a stop at a time (a location group, a zone, a window) is a demand-responsive trip.
- Everything else a feed carries reads into its model home and writes back value for value (src/extras.rs; text and values in src/values.rs).
- A row is exact (the pattern's timepoint) when it carries both times and does not say timepoint=0.

**The `Boundary`** is the import's serialization residue:
- stop_sequence and shape_pt_sequence numbering;
- a frequency trip's stop_times base;
- the id given an agency published without one;
- which file names a route's fare network;
- the services published as dates.

An authored document exports with the default `Boundary`.

**Service ids.**
- Service ids are service types, written back as authored (the calendar row, exceptions as calendar_dates).
- The import keeps every calendar the feed names.
- A service published as dates alone has no weekday flagged and every calendar_dates row an added date. C-TRAN's whole calendar is like this. It imports as the weekly rule its dates read as (D-105).
  - A weekday qualifies when it runs on at least 4 of the dates and on three quarters of that weekday's occurrences in the span (D-111).
  - The import makes one range over the dates on those weekdays, a removed date for each one skipped, and an added date for each other date. The service runs on exactly the feed's dates.
  - When no weekday qualifies, the service stays dated.
- The `Boundary` keeps how the service was written. So the export writes it back as dates: its zero-flag calendar row as written, every running date added. This holds after a fork has carried the service too (tests/dated_calendar_law_test).

**Profiles and layovers.**
- Each pattern gets one all-day timing profile from its earliest trip. This is the production tool.
- Each block layover is the observed gap between consecutive one-start trips of one service type. The planner re-authors from there.

**Directions.**
- GTFS direction_id becomes the route's directions ("0"/"1").
- Each direction is named by the trips' most common headsign, else "To" the commonest terminal.
- The export writes direction_id back and each trip's own headsign. It invents no headsign.

**ImportNotes.**
- The import reports what the model cannot hold yet as an `ImportNote`. It is never silent.
- Examples are stops without coordinates and the trips that serve them, single-stop trips, and a transfer point where fewer than two routes meet.
- A transfer point is the feed's own flag. Nothing infers one, so a feed that never sets the flag counts none (RAPTOR plans transfers regardless).

**The laws.**
- The import is valid by construction over spec-valid feeds (tests/import_prop_test).
- The import keeps the trips (tests/trip_import_law_test).
- `import ∘ export` is a fixpoint (tests/codec_prop_test).
- tests/spec_round_trip_test holds GTFS → network → GTFS against the whole spec, one law per file. The oracle reads the files as raw tables.

transit-codec is a library under cap-network. This is the extraction pattern: crate boundary first, capability wrapper second. The C-TRAN import regressions live in its own test suite (tests/import_regression_test).

## transit-analysis

Planner analysis over an authored network document. It is a library under the cap-analysis module. The planner's regression numbers live in its own test suite (tests/regression_test).

**Service groups and types.**
- `service_groups` returns one row per service combination: its dates and its members' pooled numbers. These numbers are the service on each of its dates. The laws pin this against the date-scoped reads. No date stands in for a group (D-55).
- The per-type overview gives each type's authored service, on no date.
- A combination's detail and schedule texture: the hourly profile, headway gaps, and clockface.
- Route corridors: shared stops and segments.

**Frequency and waits.**
- `route_frequency` is the frequency grid. It gives each route of a service combination hour by hour at its headway stop. It uses the trips' own shortest, longest, and median gaps, never regularized, with the route's numbers.
- `hub_connections` gives a hub's connections by route pair (`timed_transfers` folded).
- `missed_connections` gives a hub's near misses: another route that leaves just before the rider can make it, with that route's next departure (D-133).
- `stop_gaps` gives a stop's waits. A station's waits are its bays' waits, pooled.
- `longest_waits` gives the network's longest waits. It ranks every stop, in an hour window or over the day (D-134).

**The worklist and proposals.**
- `network_findings` is the worklist: frequency holes, long waits, and costly near misses at hubs. Each finding is keyed so that a plan reads it against its baseline as new, moved, resolved, or watched (D-136).
- `day_types` gives a period's day types: the combinations arranged by the weekdays they serve (D-137).
- `plan_compare` gives the whole proposal across them.

**Coverage of a weighted layer** (its `coverage` module, D-47).
- The layer is any GeoJSON points and polygons. The weight is whichever numeric property the question names (`field`, with the planner's `label`).
- `type_coverage` gives the share that a service combination serves and serves frequently.
  - It uses the members' pooled authored service, with no date standing in.
  - On every date the combination holds, it gives that date's coverage.
  - Frequent counts departures only (D-6).
- `unserved_points` gives the unserved where. It is the complement, named. A polygon is named by its unserved part.
- `coverage_compare` gives the churn between two sides that the host names: gained and lost (D-99).
- A polygon's weight spreads over its area on a lattice (D-98). The share of the polygon within the radius is served.
- Every answer states the layer's resolution (point_spacing_m, sample_spacing_m).

## transit-dispatch

The materialized day's views, model-native. It is a library shared by the cap-board, cap-runcut, cap-calendar, and cap-divert modules. The UI reads them here. It holds no GTFS layer (the justfile arch gate enforces this).

**Patching the day.**
- `patch_day` skips or shifts materialized stop times. It acts on the day, never the calendar.
- `cancel_day` takes a cancelled trip instance out of the day. Every other trip stays whole.
- Edit validation runs against the network: a cancel's trip must exist and run the instance it names.

**The board** (`stop_board`).
- A stop with nothing of its own pools its walking group.
- The board is realtime-first over the live document. Rows are ordered by when the bus actually leaves. A visit that the document cancels or skips is no row (D-73).
- Each row has:
  - its expected time, delay, status, and wait (`wait`, the one rule: due, then minutes rounded down under an hour, then the clock time);
  - its route's short and long names;
  - its bay's name and platform code;
  - its destination as the sign says there.
- `skipped` lists the departures that the day's edits took away. Each entry has the diversion that skips it or the cancel that took its trip. A dispatcher's cancel is listed, unlike the live document's (D-100).
- The `Day` holds the unpatched day beside the patched one. It is one materialization: `day` and its key, which the kernel holds.
- The live document's alerts about the board are included, each row naming its own.

**The grouped board** (`grouped_board`).
- It groups the board's rows by route and destination for a stop's display.
- It shows every group that serves the stop all day. A group that has stopped stays, with no departures.
- Each group has its next `per_group` departures and its skipped visits.
- Order is route_sort_order, then id, then destination.
- `with_next_service` gives a stopped group its `next`. This is the first departure of the next seven days, whose patched day carries it (D-42).
- Each day is asked through a closure, so the capability decides where the days come from, never the live document.

**Other day views.**
- The day's blocks list.
- The fleet summary: peak concurrent vehicles and when, platform / revenue / layover hours.
- The service keys that run on a date.
- The authored block chain with the layover ripple. `layover_remaining_secs` goes negative when a connection is eaten.

**The diversion editor** over the network (divert.rs).
- A draft's view has:
  - the route's patterns, with the longest as the default;
  - each visit's role, with the interior skipped unless served;
  - the alignment;
  - the flattened trace and its length;
  - the problems.
- The record that a ready draft becomes.
- The trip cancel that a pick becomes (`cancel_of`: the trip, one instance or all, on a date its service runs).
- The edit document's named rows: diversion rows and cancel rows.

**The board at a moment** (`stop_board_at`, `grouped_board_at`).
- The op gives the date's board. Given the previous date at the same instant, it also gives that date's departures that are still to leave.
- The instant is the clock's seconds and the date's length (23, 24, or 25 h).
- GTFS times past 24:00 are the date the trip started on. At 00:30 Friday, Thursday's 24:45 leaves in 15 minutes.
- Each date reads its own edits and its own live instances. Every row names its date.
- Rows order by how long until each leaves.
- In the hour before a fall-back day's anchor, the wait counts from the true instant, not the clamped 0.

**Where service runs today** (geometry.rs).
- A trip's line over the effective day is the authored alignment.
- For a diverted trip, the line is the alignment to the first kept stop, then the traced detour, then the alignment from the last kept stop. A skip-only diversion keeps the alignment.
- The ridden segment is that line sliced between two stops.

**A rider's own documents** (watchlist.rs, device.rs).
- The watchlist: a rider's saved stops, tagged at v2 (`kind`, `schema_version`). A copy from before the tags names `"version": "vN"`. Its v2 reads as v2, and its v1 comes forward through the chain here (`migrate_to`), each note becoming a label and each dropped color reported. `Versioned::version_in` reads the old marker, so the kernel's refusal names it.
- The device records ride-now keeps in the browser's store: `recent_places` (the last places a rider looked at) and `live_feeds` (the rider's own GTFS-rt URLs). Each is tagged at v1.

## transit-rt

prost types from the official GTFS-rt proto, and the joins over the network.

**The overview view.** It covers delays, vehicles, and each trip descriptor's start_time and start_date.

**The joins over the network.**
- An rt trip id is a model trip id.
- The instance is its start. A frequency trip's start is the descriptor's start_time (`resolve_instance`).
- The service date is the descriptor's start_date. When it is absent, none is stated. When it is unreadable, the update names no instance.

**The progress join.**
- It gives one trip's predicted times and the ranked late view with the delayed-but-dark flag.
- It pairs stop updates by the published stop_sequence (the import's `Boundary`; 1..n without one).
- It pairs by stop id only when an update names no stop_sequence.

**The boundary resolution** (`live_day`).
- Inputs: snapshots (trip updates, vehicle positions, service alerts) plus the network (and its boundary).
- Output: the live document.
- It is the one place that resolves rt trip ids (tests/rt_join_law_test).
- It is the one place that turns an alert's informed entities into routes, stops, and trip instances. An alert is kept when it is in force at the snapshot's moment (tests/alert_test).

## transit-live

The live document: one rt moment in model vocabulary. It is pure data and has no GTFS layer (arch gate). cap-rt mints it and cap-board reads it. No host computes a delay.

**Version 2 holds:**
- Delays per trip instance (trip, service date, start) and stop.
  - The date is as the feed states it. An absent date means the current service day, which is GTFS-rt's reading.
  - `LiveDay::on(date, current)` is one date's view. Every reader asks for its own date.
- Vehicles on their trip instances.
- Every status, classified under the on-time window that the document records. There is one window, never a threshold per view. By default any earliness is early, because a bus that leaves early is a missed bus.
- What does not run: a trip instance that the feed cancels (`CANCELED`/`DELETED`: `canceled`) and a stop that it skips (`SKIPPED`: `skipped`). Every reader asks through one rule, `LiveDay::runs`.
- The service alerts in force. Each alert is a set of scopes: route / stop / trip instance. Fields within a scope are ANDed, scopes are ORed, and all absent means the agency.

The crate also holds the classification, the runs and alert-matching rules, and the status summary.

## transit-version

What every persisted document says about itself (D-115).
- `kind_tag!` declares a kind's tag. A writer always writes it. A reader reads an absent tag as the kind and refuses another kind.
- `Version<N>` is a version field whose absence reads as N.
- `Versioned` is what a reader needs to refuse another version, naming both versions (`cap_abi::parse_versioned` at every kernel load, `cap_abi::Current` on a request field).
- Each owning library declares its kind's tag. The kinds and their versions are in docs/FORMATS.md's "Versions".
- `Extra` is a record's fields that this build does not know (D-167). Every record of an authored kind flattens one in, so a read and a write keep them. A value (a point, a stop's two times, a set of weekdays) is not a record and carries none. `noting_unknown` runs a read and names the unknown fields it met. Every kernel load answers them (`unknown_fields`), and so does each receive op (`validate` for stops and stop requests, `sync_edits` for the client's records).

## transit-sync

The sync document: `EditRecord`, `FeedManifest`, `EditReport`, and the edit document. Every host shares it. cap-sync is the seam that the Go server calls. cap-divert is the authoring seam.

**The edit document** belongs to the dispatcher. It lives in the sync contract, not the GTFS codec. Its records carry exactly one payload: a `Diversion` or a `TripCancel`.
- A `TripCancel` takes a whole trip out of service on one date: one instance or all (D-45, D-101).
- An absent payload is omitted, so a diversion record's bytes predate the cancel.

**Logic.** The crate holds pure validation and merge logic.
- `merge` is a property-tested semilattice.
- `submit` transitions drafts with the clock that the host passes.

**The record envelope** that every writer follows:
- `propose` and `propose_cancel`: rev 1 or bumped.
- `set_retired`: the tombstone and restore.
- `read_document`: a received document in the version this build reads, with its records under the current schema. It drops and counts the unparseable records and any record without exactly one payload.

## transit-plans

The service-plan chain, and the published plan set. cap-plans is the seam. This crate is cross-document logic, the counterpart of transit-sync's cross-record merge.

**The chain.** It works over the plan set's metadata. It is derived, never stored, deterministic, and order-insensitive.
- `resolve`: which pick is in force.
- `periods`: each member's governing window.
- `validate`: the chain invariants.

**The published plan set** (store kind `plans`, `PublishedPlans`).
- It holds every published pick's metadata and network hash.
- It is canonical: (effective_from, id) order, one entry per id.
- `publish` gives the next set, or every reason the pick is refused. The reasons are the chain, or a new, restored, or moved pick dated before today (D-76).
- `retire` retires a pick that has not taken effect. The pick is kept as the tombstone (D-75).
- `in_force` gives the published pick, and so the network, that governs a date. Retired picks never govern.
- `in_use` gives the picks in use on a date: the pick in force, then every published pick dated after it. A pick that stopped governing, or a retired one, is not in use.

## transit-runs

The runs layer, beside the network (D-16).

**The work-rule document** (`WorkRules`, kernel kind `work_rules`; D-50).
- It is generic. Every rule is optional. An agency's values are data.
- Recovery is per route and/or service type. It is minutes and/or a whole percent of the run before the wait, whichever is larger. The most specific rule answers.
- The route pairs that one vehicle may interline (D-46). A route always follows itself. An unlisted pair never interlines.
- The run rules: platform, spread, report and clear, pieces, a meal break, and relief points.
- Pay, in time. It has no dollars (D-51).
- `checked` validates and normalizes the document. The kernel's load runs it, so two orderings are one identity.

**The recovery view** (`recovery`).
- It gives every wait between a block's adjacent running trips on a day, the recovery required there, and the slack above it.
- It folds the waits per pattern into layover-to-runtime.
- The result is short exactly where validation says layover_infeasible.

**Suggested blocking** (`suggest_blocks`).
- The op takes the trips that no block carries, by start.
- Each trip follows the vehicle that leaves it the least slack where it starts. This is the same stop, or a bay of the same station. There are no deadheads yet.
- The pair must be one that the rules interline, and the trip follows after the recovery required.
- The op also gives the peak that the adoption needs against the authored peak.
- It gives the edits (`add_block`, `add_block_trip`, `set_layover`) that adopt the suggestion.

The runs cut itself is open work (`docs/units/runs-layer.md`). The crate is pure. cap-runcut and cap-author are the seams.

## transit-stops

The stops document, beside the plans (D-147, D-153). It depends on no other model crate. cap-stops is the seam. A stop's life is its own, managed apart from the picks, which only read it (D-157).

**The stop.**
- Every stop has a lasting id, kept through a move (D-145).
- Each stop is a list of dated events on the stop's own lifecycle (D-157).
- The lifecycle stages are:
  - planned, permitted, built, and in_service (forward only; a stage may be skipped);
  - suspended (a stop in service that is out of use for a while, back by in_service, or never);
  - moved, changed, and retired (the tombstone).

**What an event sets.**
- The point.
- The details (name, code, station, access, fare zone, and others).
- The amenities (D-154). Each is true, false, or absent for not recorded. The crate uses OpenStreetMap's bus-stop names where one exists and keeps the agency's own.
- The site survey's answers (`site`, D-158). Each question's answer is by name.
  - The core questions (`SITE`) are the Service Stop Worksheet's. The crate checks each answer against its kind: one of a list, a whole number, or text.
  - The agency's own questions are kept and not checked.
- The stop's ids in other systems and its parts' ids (`refs`, D-160: sign, TMS, HP, GIS, shelter, and others, not checked).

**Surveys and attachments.**
- An event that records a survey names its surveyor (`surveyed_by`).
- A stop on a date says when it was last surveyed and by whom.
- Any event carries its photos and drawings (`attachments`, D-162). Each is named by its sha256 content hash with its media type, never a URL.

**Reads.**
- Nothing in the document means today.
- `stop_on` applies a stop's events in order up to a date. A temporary suspended or moved event applies until its `until`.
- `served` asks whether a trip at the stop on a date is served. The time is unread at a day's grain (D-148). Only in service counts as served.
- `on_date` lists every stop that exists.
- `next_events` gives what a stop may become on a date: exactly the lifecycle events that a write would accept.
- `counts_on` gives the stops on a date by status.
- `near` gives the stops on a date nearest a point. It uses great-circle distance, nearest first, ties by id, at most a limit, within a radius where asked, at the statuses asked.

**Writes.**
- Each event has an `id`, unique within its stop (D-166). `identified` gives an id to each event that has none, from the stop and the event's content, so the same event in the same stop gets the same id in any document. Every write identifies before it checks. Another writer's event added above an event does not change which event a correction edits.
- `checked` is what a write must pass. Each refusal names its stop, event, and problem. Two events in one stop with one id are refused.
- `attribute_surveys` names the signed-in writer as the surveyor of every survey that a write adds (the receive path's, D-158).
- A document that passes is canonical as it stands (stops by id, events as written).
- The crate keeps what it does not know through a read and a write, such as another tool's field or an agency's amenity (D-167).
- A correction edits the event that was wrong. A real change adds an event.
- The edits answer the next document:
  - `add_event` places one event by date.
  - `correct_event_by_id` takes out the event that was wrong, named by its id, and places the corrected one by date. The corrected event keeps the id.

## transit-requests

The stop requests document (D-159). It holds the planning team's requests for work at a stop. C-TRAN's Bus Stop Sign & Shelter Installation Worksheet is the source. It depends on no other model crate. cap-requests is the seam.

**A request** is a record by id. It has:
- the job (new, reinstall, relocate_remove, repair_improve, temporary);
- the stop;
- the status: drafted, submitted, ordered (the work order opened), completed, or cancelled (the tombstone);
- the worksheet's fields:
  - who asked and when;
  - the intended routes and direction;
  - the street;
  - the cross street that the offset is measured from, the offset, and the side;
  - city, county, township, range, section, and parcel;
  - the marking;
  - the utility locate (the utilities as free text, the agency's own list);
  - the work order it tracks (system, id as the Control #, link, its status as last read, and when);
  - the completion;
  - a note;
  - its photos and drawing (`attachments`, by hash, D-162).

**Operations.**
- `checked` refuses only what makes a request wrong. Each refusal names its request.
- `put` places one request by id.
- `for_stop` gives a stop's requests.
- `list` gives the work across every stop, at the statuses and of the jobs asked.
- `counts` gives every request by status.

The document is canonical as it stands (requests by id). The crate keeps what it does not know.

A stop's event names the request that it carries out (transit-stops `request`). Nothing checks this across the two documents.

## transit-streets

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

## cap-authz

The authorization capability: may this identity perform this action on this pointer?
- It has one op (`authorize`). It is stateless, like cap-plans.
- The host verifies the IdP's JWT and pushes the policy document on each call.
- The verdict is fail-closed and never depends on rule order. transit-authz is the library, and it is property-tested.
- The policy is a store document (kind `policy`, D-61).

## cap-sync

The sync capability: the converged state of an edit document, with its lifecycle facets. It is stateless.
- `sync_edits` does the LWW merge and the submit transition. The receive path clamps client clocks to the host's (D-56). The answer carries the canonical document bytes and their hash. This is the op behind POST /edits.
- `read_edits` is the receive. It reads the current version only. An older version is cap-migrate's. It does a typed parse, defaults fill what age removed, and it drops and counts malformed records.
- `compact_edits` is the D-57 tombstone-retention commit.

Validation is not here. What a record amounts to on a network is cap-divert's reports: one question, one answer.

## cap-plans

The plan-chain capability: which service plan governs a date? transit-plans is the library. The ops are pure and there are no registries.
- `resolve`, `periods`, and `validate` work over the plan set's metadata.
- `publish`, `retire`, `in_force`, and `in_use` work over the published plan set. `publish`, `retire`, and `in_use` take the host's date.
- The host owns the plan set and pushes its metadata on each call.
- `publish` answers the next set's canonical bytes and hash, which the server stores as given.

## cap-stops

The stops capability: what is a stop on a date? transit-stops and transit-network's `stop_record` are the libraries. The kernel's `stops` kind registers a document only if `validate` would pass it.

**Reads.**
- `stops_on` gives every stop, or those named or at the statuses asked, as on a date. It gives each stop's status with since, until, and note, and the stops counted by status.
- `stops_near` gives the stops on a date nearest a point, each with its distance. It answers the field's "which stop am I at?".
- `stop` gives one stop on a date: its events, oldest first, and what it may become that day.
- `questions` gives what a site survey asks: the core site questions with the kind of answer each takes, and the core amenity names.

**Writes.**
- `validate` gives what a write must pass: every refusal, or the canonical document and its hash, each event with its id. This is the document that the kernel's `load_stops` answers. It names the fields the document carries that this build does not know (`unknown_fields`, D-167).
- `seed` makes a stops document from a network's stops.
- `add_event` places one edit by date.
- `correct_event` corrects an event that was wrong, named by its id (D-166), or takes it out.
- Each of the three is checked as a write is.
- `attribute_surveys` names the write's new surveys for the writer. It runs before `validate`.

**What a pick reads.**
- `read_records` gives each stop's record on the pick's first date.
- `visit_warnings` gives the stops that the pick visits that are not in service over its period.
- `records` gives the record that a plan takes of a stop that the document holds and the plan does not, on the pick date (D-173). The plan copies the record. The document is never written.

**GTFS and the stops' history.**
- `export_versions` and `import_events` carry GTFS out and in with the stops' history. Amenities, site answers, and references are the document's alone.
- `on` defaults to the feed's first service date. A named `feed` is named on each event recorded.
- `feed_events` gives what a feed recorded.

## cap-requests

The stop requests capability: what work has been asked for at a stop, and where is it? transit-requests is the library.
- `validate` gives what a write must pass, or the canonical document and its hash. This is the document that `load_stop_requests` answers.
- `requests_for` gives a stop's requests.
- `list` gives the work across every stop, by status and job, with every request counted by status.
- `put` places one request by id, checked. With no document, the first request begins one.
- `record_work_order` records what the work order system answered, on its request.
- `vocabulary` gives the lists that a request's answers come from.

## cap-author

The authoring capability: what does this network become after these edits? transit-network's edit module is the library.
- `apply` takes typed edits, atomic. It answers the next document under its hash, registered for the next change, with its issues and warnings.
- With `work_rules_hash`, the op judges a block's waits under the work rules' recovery (D-9).
- `created` names the entities that a change made.
- `touched` names the entities that the change altered beyond that, such as the patterns that a redrawn alignment moves.

Every host authors through it, the service-design UI included. Its host sends edits and adopts the answer. It never edits a document.

## cap-divert

The operator-edit authoring capability: what does this operator edit amount to over the network? An operator edit is a diversion or a trip cancel (D-102).

**Ops.**
- `draft` gives a diversion editor's whole view.
- `list` gives the edit document's rows with names resolved: diversions in `rows`, trip cancels in `cancels`.
- `reports` gives what each record amounts to on the network: valid, or what is wrong, and the trip instances it touches. It reports invalid records and never rejects them. A day view patches nothing for them.
- `propose`, `cancel`, `retire`, and `restore` give the edit document after the change. They follow the envelope rules. `cancel` refuses an unknown trip, an instance it never runs, and a date it does not run.

**Contract.**
- Every op takes the edit document whole (`edits`) and answers the next one tagged. So a host persists or posts the answer as given.
- The capability refuses another version, naming both (`cap_abi::Current`, the version check that every kernel load shares).
- Reading a received document is cap-sync's `read_edits`.
- This is a separate need from cap-sync's converge step. The Go server hosts that step without a network.
- The host passes the clock and the record ids.

No browser UI authors an edit yet. Drawing a diversion is `docs/units/ui-disruptions.md`'s question 4. Cancelling a trip is its question 6.

## cap-migrate

The migration capability: what is this older document in the version this build reads?
- `migrate` brings a document forward through every step of its chain:
  - a network (v1, every untagged shape, to v2);
  - an edit document (v1, the bare array, to v2);
  - a watchlist (v1, its notes and colors, to v2; a copy that names
    `"version": "v2"` from before the tags is already v2).
- The chains live in the owning libraries (transit-network, transit-sync, transit-dispatch). They are kept forever (D-66). Every record id is kept and every loss is a note.
- `migrate` answers the new document's canonical bytes and hash.
- `version` answers whether a document needs migration.

Consumers read the current version only and refuse any other at the load (`cap_abi::speaks`). So no consumer carries an old chain.

The capability is hosted where an older document arrives: the Go server's receive and startup, and service design's restore (docs/FORMATS.md, "Versions").

## Every module

Every module is the raw rustc output of an assembly crate with the same four flat exports. There is no wasm-bindgen. Do not reintroduce host glue.

## Why

Each entry covers one choice that other code depends on. The heading names the code it governs.
**Holds** is what must stay true. Change it only with the user, after you read **Because**.

### An alignment edit names its reach (author `draw_alignment`, `redraw: one|all`, `touched`)
- **Holds:** redrawing a pattern's line says whether it redraws that pattern alone (a new alignment) or every pattern on the shared alignment. The edit answers the patterns it touched.
- **Because:** several patterns may run one alignment. A fix meant for one variant must never move the others silently. A fix meant for all must not need one edit per pattern.
- **Rejected:** always redraw the shared alignment; always copy it under a new id.
- **Decided:** D-11.

### Timetable gestures act on one service type (author `apply` gestures, `GesturePanel`)
- **Holds:** a gesture changes the trips of the one service type it names. The UI warns when other types run with it. There is no authored "same as, except" relation.
- **Because:** the reads pool a combination, but its members are separate authored types. A gesture that acts on all of them silently would change trips the planner did not pick.
- **Rejected:** gestures acting on the whole family; a "same as, except" relation (a model change, QAF-44).
- **Decided:** D-8.

### A fork carries weekly service through its period (author `set_plan {through}`)
- **Holds:** a fork with a period end extends every weekly service type that still runs in the calendar's last week to that end. The calendar is read up to the period's end. Dated-only types gain nothing.
- **Holds:** then the fork's calendar is its period. Every range is cut to the pick date through the period end. A range or exception outside the period goes. Every service type stays, even one left with no date.
- **Because:** a fork picked after the import's calendar runs nothing until every type is extended (13 edits on C-TRAN). Holidays are next year's to add deliberately.
- **Because:** dates outside the period never run in the fork, because a pick governs from its date. Keeping them only clutters its calendar. A type with no date still names its trips.
- **Rejected:** a separate `extend_service_ranges` gesture; leaving it to the planner by hand.
- **Decided:** D-41 (where `through` comes from: D-104; which types: D-112; the period as the calendar: D-138).

### Dated calendars import as weekly rules (transit-codec calendars)
- **Holds:** a type whose added dates are every flagged weekday of a span, except for a few removed dates, imports as days + range + exceptions. It exports back exactly as the feed had it.
- **Because:** C-TRAN writes all service as dated additions, so nothing is "weekly" and a fork would carry nothing. The rhythm is in the dates. Reading it at the codec keeps the model's rule explicit and the export lossless.
- **Rejected:** inferring the rhythm at the fork; nothing inferred (the planner flags days).
- **Decided:** D-105 (recognition: D-111).

### "Frequent" counts departures only (transit-analysis coverage)
- **Holds:** both frequency tests count departures, so a terminal where nobody boards is never frequent.
- **Decided:** D-6. **Pinned:** `a_terminal_nobody_boards_at_is_never_frequent`.

### Coverage takes any weighted layer (`load_layer`, `type_coverage`, `unserved_points`)
- **Holds:** a layer is points or polygons with any numeric weight and a planner's label (population, jobs, and others). Polygons are areally weighted. A point layer states its resolution.
- **Because:** a tract's people at its centroid misread rural tracts and swing with the radius. The system is not opinionated about what a planner weights.
- **Rejected:** areal population only; finer point layers only.
- **Decided:** D-47 (reading and sampling: D-97, D-98).

### A service group is a combination that runs together (`service_groups`, `members`)
- **Holds:** a group is the set of service types that run on the same dates, shown with those dates. Types that never share a day are never pooled. No "representative day" stands in for a combination.
- **Because:** pooling a base type with subsets that run in successive periods counted trips that never run together (2,130 against 1,774 on C-TRAN). A planner needs the real combinations and when they hold.
- **Rejected:** counting members on a representative day; splitting a group into periods with representative days; counting every member and saying so.
- **Decided:** D-55.

### Required recovery: the work rules, else the terminal's floor (`recovery`, transit-runs)
- **Holds:** a policy wins. A policy is minutes and/or a share of run time, per route or service type, in the work-rule document's `recovery`. The smallest layover seen at that terminal is the fallback.
- **Because:** an imported layover is the whole observed gap, so without a rule every delay reads infeasible.
- **Decided:** D-9 (which waits it judges: D-110).

### Blocking is a suggestion under the work rules (runcut `suggest_blocks`)
- **Holds:** automatic blocking is the start of the runcut. It lives in the runcut capability and the work rules tune it (recovery, interlining per route pair). Its blocks are suggestions that the planner adopts or edits. The peak vehicle need falls out of them.
- **Because:** a vehicle need is a block assignment. A standalone estimate from concurrent trips would answer with a number that no planner could act on.
- **Rejected:** a peak estimate with no blocks; blocking by hand only.
- **Decided:** D-46 (what a suggestion changes: D-109; interlining with no rule: D-107).

### Work rules are one generic document, every rule optional (`work_rules`, kernel load 269)
- **Holds:** the common rules live in one policy document, each rule optional. The rules are platform, spread, report and pull, pieces, meals, relief points, pay, recovery, and interlining. An agency's values are data.
- **Because:** nothing may couple to one agency's labor agreement.
- **Rejected:** shaping the document to C-TRAN's rules.
- **Decided:** D-50.

### A plan's time is door to door (router `leave_secs`, `duration_secs`)
- **Holds:** an itinerary carries when the rider must leave (the first boarding minus the walk before it) and the trip's length from then. "Leave in N min" uses the board's `wait` rule.
- **Because:** that is what riders compare and what other trip planners show. Host arithmetic over legs would put an answer outside the module.
- **Rejected:** a duration from the moment asked.
- **Decided:** D-40 (on a plan not asked for now: D-92).

### A stopped route looks ahead up to a week (board `grouped_board`, transit-dispatch next service)
- **Holds:** a group with no departures left shows the next departure from the following days. Each day is patched by its own edits and never by the live feed. Nothing shows past seven days.
- **Because:** "Next: Monday 5:12 am" is what a rider at a stopped route needs. One weekly cycle covers every regular pattern.
- **Rejected:** tomorrow only; until the calendar ends.
- **Decided:** D-42.

### A stop's site survey is typed answers beside its amenities (transit-stops `site`, `SITE`; stops `questions`)
- **Holds:** the site survey is C-TRAN's Service Stop Worksheet, filled at the stop on a phone or tablet. It is part of the stop's details. Each question's answer is by name, set by an event like any detail, and the event names its surveyor.
- **Holds:** a core question takes only an answer of its kind (one of its list, a whole number, or text). The agency's own questions are kept and not checked.
- **Holds:** a part's type (sign, post, shelter, kiosk, light, seat, garbage can) is a written answer. Its id is a reference (`refs`), as are the stop's ids in other systems (Sign ID, TMS, HP, GIS). A type and an id sit on the stop. They are not records of their own.
- **Holds:** the survey never asks what a schedule says (the routes, a time point, the previous and next stop). The survey reads these from a schedule that serves the stop. A new stop's intended routes are its request's.
- **Holds:** the answers stay in this document. GTFS has no place for them, so the export and the import leave them alone, and a survey never splits a stop into versions.
- **Holds:** the page asks `questions` for the questions, so a question added here needs no page change.
- **Holds:** the surveyor is the signed-in person. The receive path names the surveyor on every survey that the held document does not hold (`attribute_surveys`, by the server's configured name claim). It never uses a name that the page sent. So a survey's surveyor is as trustworthy as the sign-in.
- **Because:** many answers are a choice from a list or a type, which yes/no amenities cannot hold, and widening amenities would change a decided shape. A map of named answers keeps the agency's own questions next to the core ones, as amenities do. Signs, posts, and shelters get records of their own only if they move between stops with histories of their own. A v1 reader keeps fields it does not know, so the new fields need no new version.
- **Rejected:** typed values inside `amenities`; a separate survey document (the answers are facts about the stop and change by its events); asset records for each part.
- **Decided:** D-158, D-160, D-163 (D-154 for amenities).

### A feed's stops apply, and the planner sees what they did (transit-network `stop_record`, cap-stops `import_events`, `export_versions`, `feed_events`)
- **Holds:** when the server imports the agency's feed, each difference between the feed's stops and the stops document on the feed's first service date is recorded as an event dated that day, naming the feed. A stop the feed does not list keeps its events, and events dated later stand.
- **Holds:** the stops page shows what the feed recorded and the stops in service that it does not list. The planner can fix each on its card. A move that was a survey correction becomes a correction of the earlier point.
- **Holds:** on the way out, a stop whose record changes inside an export's dates is one stops.txt row per version, and the rows share its `stop_code`. The version in force last keeps the id. An earlier version takes `<id>@<its first date>`. A re-import reads those back as versions.
- **Because:** the feed is the agency's published statement and riders get it at once. A bad feed is fixed by a correct one, and a planner must see what a feed did. A feed has no dates for its stops, so its first service date is the only date it states. The rider's stop is its `stop_code`. A `stop_id` is for software.
- **Rejected:** a review step before a feed's differences are recorded; ending an export's window at a stop's change (riders could not plan past it); the record in force on the window's first date (wrong after a construction move).
- **Decided:** D-150, D-151.

### A request for work is its own record, not a stop's event (transit-requests, cap-requests)
- **Holds:** a request for work at a stop is the installation worksheet that the planning team submits. It is a record of its own, in a document of its own, by id, and it changes by replacing it whole.
- **Holds:** a request tracks the work order that the work order system opens (system, id, link, status as last read). The app shows this as given and never decides it.
- **Holds:** a request's fields are the worksheet's. They are free where the worksheet's are free (the utilities, the work order system). The crate refuses only what makes a request wrong.
- **Holds:** a new stop's intended routes and direction are the request's, since no schedule serves the stop yet.
- **Holds:** a stop's event names the request it carries out. Nothing checks this across the two documents.
- **Because:** the sheet is not a work order and not a fact about the stop. It is work with its own life (drafted, submitted, ordered, completed, cancelled). The work order lives in the agency's work order system, which this app will one day call to open one and read its status.
- **Rejected:** events on the stop (it mixes the work into the stop's facts); a built-in list of utilities (one agency's list, not the format's).
- **Decided:** D-159.

### Stop search is fuzzy on the stops' own names (directory `stops`)
- **Holds:** typos and abbreviations match the names the feed gives. There are no authored aliases.
- **Because:** aliases would be a hand-maintained separate dataset. A geocoder covers everyday names where a deployment runs one (D-36).
- **Rejected:** aliases in the network; deferring entirely to a geocoder.
- **Decided:** D-48 (what counts and the order: D-91).

### A cancelled trip is not a row (`TripCancel`, `LiveDay::runs`, the board and the router)
- **Holds:** the edit document carries whole-trip cancels beside diversions. A trip that a dispatcher cancels shows on boards as cancelled and is never ridden.
- **Holds:** a trip that the live feed cancels or deletes, or a stop that it skips, is no row on any board, with no mark and no note. The router never boards or alights there.
- **Because:** cancel is a dispatcher's first answer to a late trip, so it lands first. Partial cancel, cover, hold, and swap each need a meaning on every reader. A live cancellation shown at its scheduled time misled riders. The user chose to drop it silently.
- **Rejected:** the full dispatcher set at once; showing a live cancel marked, or noting it on the group.
- **Decided:** D-45, D-73 (the shape: D-100..D-102).

### Streets: any source, per-mode profiles (transit-streets import, `walks`, `access`, streets `route` `to_confirm`)
- **Holds:** every street op reads a streets document by hash, whatever source filled it.
- **Holds:** walking is strict. It allows every class but motorway, ramp, and unknown. One-way does not bind a rider. The limit is at most ten minutes at 1.3 m/s by default.
- **Holds:** a bus may be drawn on any class. Unknown and path-like classes are named for the planner to confirm.
- **Because:** the source is data, not model. An agency's roads give scheduling parity, and OSM gives sidewalks. Which one a deployment loads is its choice (C-TRAN's roads cannot carry walking, QAF-85). A planner judges a grey-area road. The data cannot.
- **Rejected:** one fixed source for every mode; refusing a bus on unknown classes.
- **Decided:** D-25, D-26 (classes: D-30, D-103).

### The feed's own facts are not inferred (analysis `transfer_stops`, rt `late[].label`)
- **Holds:** a transfer point is the feed's flag and is never inferred (RAPTOR finds transfers regardless). A vehicle with no label shows its id.
- **Because:** these are the agency's facts, and the rules stay generic rather than coupled to one agency.
- **Decided:** D-10.
