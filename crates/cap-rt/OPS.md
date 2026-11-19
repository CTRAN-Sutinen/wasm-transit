# rt

Realtime decoding and the schedule join. The capability turns a GTFS-rt protobuf payload (trip updates, vehicle positions, service alerts) into the flat view that hosts show. It also answers the dispatcher's joined questions: one trip's predicted times, and the ranked what's-late-now view with the delayed-but-dark flag.

It is the third capability extracted from the monolith. The domain code lives in the shared transit-rt library.

It mints the live document. The live document is the snapshot resolved against the network into the model's vocabulary:

- Delays for each trip instance and stop.
- Vehicles on their trip instances.
- Statuses under the caller's on-time window.
- The service alerts in force, with their informed entities resolved to routes, stops, and trip instances.

An rt trip id is a model trip id. The capability resolves it once, here.

It also writes the agency's service alerts document as an Alerts feed (`encode_alerts`).

Assemble this capability for riders (live boards) and dispatchers (the operation view) alike.

## decode

Decode a GTFS-rt protobuf payload into the overview, and register the snapshot under its content hash. The registration is idempotent: the same bytes are the same snapshot. So `trip_progress` and `late_trips` can address the snapshot.

The overview has these parts:

- Entity counts.
- Trip updates with per-stop delays. A trip update is `canceled` when the trip descriptor's schedule_relationship is CANCELED or DELETED. A stop update is `skipped` when its schedule_relationship is SKIPPED.
- Vehicle positions.
- Service alerts with their cause, effect, severity, text, periods, and informed entities in the feed's ids.

This is bulk-bytes op 0. Garbage bytes give an error outcome, never a trap.

## trip_progress

Answer one trip's joined board: every scheduled stop with the live delay and the predicted time (schedule plus delay). This is the dispatcher's most-asked question in one op.

A stop that the update does not mention carries the last stated delay upstream of it. This is the GTFS-rt propagation rule. Stops before the first update stay null.

The op chooses the trip instance and pairs updates with stops by these rules:

- The instance is the one that the snapshot's update names. For a frequency trip, the update's `start_time` names it.
- Updates pair with stops by the published stop_sequence. `boundary_hash` names the numbering. Without it, the numbering is 1..n.
- An update pairs by stop id only when it names no sequence.

The answer is null when the snapshot has no update for the trip, or when the update names no instance of it.

## late_trips

Answer the ranked what's-late-now view: every trip whose worst stop delay reaches `min_delay_secs`, worst first.

Each trip carries the located vehicle (`vehicle_id`, label, position). The op finds the vehicle in one of two places:

- On the trip itself.
- Failing that, on another trip of its block. `position_trip_id` names the trip that the bus actually runs. A delay propagated to a block's next trip is a late bus, not a dark one.

When no vehicle of the block is visible, the trip carries the delayed-but-dark flag (`has_position` is false).

`vp_hash` names the VehiclePositions snapshot. Agencies serve it as a separate feed (C-TRAN does). Omit it for combined feeds.

## live

Answer the live document: one rt moment resolved into the model's vocabulary.

The inputs are snapshots:

- `rt_hash`: trip updates.
- `vp_hash`: vehicle positions.
- `alerts_hash`: service alerts.

Agencies often serve these separately. At least one is required. The op resolves them against the registered network. An rt trip id is a model trip id. A frequency trip's instance is the update's `start_time`.

The document holds these parts:

- Delays for each trip instance, `(trip, start_secs)`, and for each stop. A delay propagates downstream, with the predicted departure.
- Vehicles on their trips.
- Statuses: `early`, `on_time`, `late`, `very_late`, or `no_prediction`.

Trip and vehicle lateness is the trip's worst stated delay.

`window` is the caller's on-time policy. The document records it, so every reader classifies the same way. The default is:

- Any earliness is early (`early_secs` 0). A bus that leaves before its time is a missed bus.
- Up to 60 s late is on time.
- Past 300 s late is very late.

What does not run:

- A trip instance that the feed cancels (CANCELED or DELETED) is `canceled: true`. Its status is `no_prediction`: it is not late, because it does not run.
- A stop that the feed skips (SKIPPED) is an entry with `skipped: true`. It has no delay of its own. The delay before it carries past it.
- Both fields are absent when false.
- Every reader drops what does not run. The board shows no row, and the router never rides it.

`unmatched_trips` counts the rt trips that cannot be resolved. The op never drops them silently.

`alerts` holds the service alerts in force at the snapshot's moment:

- An alert is in force when the header timestamp is inside a communication period. Else the deprecated active period decides. An alert with no period is always in force.
- The op gathers alerts from every snapshot given, one for each entity id.
- Each informed entity resolves to a scope: `route`, `direction`, `stop`, `trip`, `start_secs`. Every present field must match. A scope with every field absent is the whole agency. A `route_type` names every route published under it. A `direction_id` needs a `route_id`, and resolves to the route's model direction (`direction`) by the code the export writes. Else the entity names nothing.
- `unmatched_alerts` counts an alert when none of its entities names anything in the network.

The response also carries the summary that a status panel reads:

- Vehicles by status.
- The late list, worst first. `late_limit` sets its length (default 15). It lists late buses when positions exist, and late trips (predictions) when they do not.

The live document registers where the op mints it. A module that links the board or the router reads it by `live_hash` at once. A host there may decline the document itself: `with_document: false` answers `live` as null. A module of its own registers the document with the kernel's `load_live`.

## encode_alerts

Write the service alerts document as a GTFS-rt Alerts feed. GTFS-rt is the alerts' serialization, as GTFS is the network's.

The feed is a full dataset (`FULL_DATASET`) at `timestamp`, the host's clock in Unix seconds:

- It holds each published alert that has not ended at `timestamp`, one entity for each, under the alert's id, in id order. A draft or a withdrawn alert is not in it.
- Each text keeps every translation, with its language.
- Each informed entity is written in the network's ids. The whole agency is the network's operator. A direction is the GTFS `direction_id` that the export gives it.
- A direction with no GTFS code cannot be written as the author meant it. The entity is left out and named in `skipped`, and never widened to the whole route. An alert left with no entity is not written.

The same document and timestamp give the same bytes. The answer is the bytes (`feed_b64`) under their content hash (`rt_hash`). The feed is also registered as an rt snapshot under that hash, so `live` with `alerts_hash` reads it at once, as a rider's app would.
