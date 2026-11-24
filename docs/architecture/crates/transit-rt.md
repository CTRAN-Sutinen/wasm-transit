# transit-rt

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

**The alerts encode** (`encode_alerts`).
- Input: the service alerts document (transit-alerts) and the network, at a moment.
- Output: a GTFS-rt Alerts feed (`FULL_DATASET`). It holds each published alert not yet ended, one entity for each, under the alert's id, in id order. The same document and moment give the same bytes.
- Each text keeps every translation. A direction is the GTFS code that the export writes. A direction with no code is left out and named, never widened to the whole route.
- Decoding the feed and resolving it gives back the authored alerts in force (tests/encode_law_test).
- `entity_reading` reads one authored entity through the same path: the encoder's `informed_entity` (or why the encoder leaves it out), the scopes the reader resolves it to, and what it names in the network's words (a station counts its bays). It equals the round trip of an alert holding only that entity (tests/encode_law_test).

**The alerts read** (`read_alerts`), the inverse of the encode.
- Input: a GTFS-rt Alerts feed and the network.
- Output: the service alerts document. Each alert is published, under its entity id, with every translation (an empty language is none), its periods (the deprecated active period standing in for the communication period), and its entities in the network's ids. A direction maps to the route's model direction by the export's code.
- What cannot be read as written is named, never guessed: an entity (`skipped`) or a whole alert with no entity left or no header (`dropped`).
- Reading back an encoded document gives its published alerts not yet ended (tests/encode_law_test). It is how another app's alerts come across: C-TRAN's own feed reads into the document and writes back as the same alerts in force.

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### An alert's direction is the route's model direction (transit-rt `live_day`, `AlertScope::direction`, transit-codec `gtfs_direction`)

- **Holds:** a GTFS-rt informed entity's `direction_id` names one direction of its route. The read side maps the code to the route's model direction by the rule the export writes (`gtfs_direction` and its inverse `model_direction`). The alert touches only the visits of that direction's patterns. An entity with a direction and no route, or a code the route does not have, names nothing and is counted in `unmatched_alerts`.
- **Because:** the GTFS-rt spec scopes the alert to the trips with that `direction_id`, and a rider in the other direction is not affected. One rule for both the export and the read keeps a code naming the same direction both ways.
- **Rejected:** ignoring the direction, so an alert reaches both directions of the route.
- **Decided:** D-204 (it replaces D-19).
