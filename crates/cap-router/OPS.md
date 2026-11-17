# router

Trip planning (RAPTOR): how do I get from here to there? The plan runs over the network document and the effective day. The effective day is the model's materialized trips, patched with the edits in force. Every day view starts from the same transit-dispatch composition. When a live document is named, the plan uses its predicted times.

The question has three facets:

- Leaving at a time (earliest arrival).
- Arriving by a time (the latest departure). With no deadline, it answers the last bus home.
- The diversion delta: the unpatched day, one materialization apart, with the rider's notice.

Assemble this capability for users who need directions, not only departure times. Documents are hash-addressed. The capability writes nothing.

## plan

Answer the earliest-arrival itinerary from `from` to `to` at a departure time over the materialized day.

Each end is one of two things:

- A stop id (a bare string).
- A place off the network: `{id, name, walks: [{stop_id, walk_secs}]}`. `walks` lists the walks from the place to the stops near it. The streets capability's access op answers them. A place id that names a stop is refused.

For a place, the trip has one of two forms:

- The trip walks from the place to one of its stops, rides, and walks from one of its stops to the place.
- The trip walks straight there, when `direct` arrives as early. `direct` is `{walk_secs, path}`: the walk between the two ends, when one joins them. The streets capability's access op answers it.

The itinerary and its first and last walk legs name the place by its id and name.

The day has these rules:

- The edits apply when `edits_hash` names a registered document.
- Each trip instance runs at its predicted times where the live document named by `live_hash` states them. Elsewhere it runs on schedule.
- The plan never rides a trip that the live document cancels. It never boards or alights at a stop that the live document skips.
- Transfers follow the streets when `walks_hash` names a registered walk table. Otherwise they are straight lines.

A null itinerary means the destination is unreachable within `max_legs`.

The legs use the model's vocabulary:

- The trip instance is addressed as (trip, start_secs).
- A transit leg carries `route_name` (the route's short label) and `route_long_name` (its long name, null when it has none).
- A transit leg carries the headsign that the bus shows at the boarding visit. It is the visit's own headsign, else the trip's, else the final stop's place.
- A transit leg carries `board_delay_secs` and `alight_delay_secs`. Each is how far a predicted time moved from the day's own time. Null means no prediction, so the time is the day's time.

The itinerary also gives the door-to-door times:

- `leave_secs`: when the rider leaves the origin. It is the first ride's boarding minus the walk before it. With no ride, it is `depart_secs`.
- `duration_secs`: `arrival_secs` minus `leave_secs`.
- `leave`: the board's wait rule, from `depart_secs` to `leave_secs`. A wait under a minute is "due". A wait under an hour is whole minutes (rounded down), for example "leave in 7 min". A longer wait is the clock time.

## plan_arrive_by

Answer the latest departure from `from` that reaches `to` by `arrive_by_secs` within `max_legs`. The op uses the same day and the same ends as `plan`: `edits_hash`, `live_hash`, `walks_hash`, stops or places, and `direct`.

With no `arrive_by_secs`, the op answers the latest departure that reaches `to` at all on the service day. This is the last bus home. Times past 86400 are after-midnight trips of the same day.

The answer has these parts:

- `arrive_by_secs`: the deadline the op answered. It is the request's value, else the day's last arrival.
- `itinerary`: the earliest-arrival plan that leaves at that latest departure. It carries `depart_secs`. `leave_secs`, `duration_secs`, and `leave` are as in `plan`. It is null when nothing arrives by the deadline.

## plan_compare

Answer the diversion delta: the same plan over the patched day and over the unpatched day. The ends are stops or places, as in `plan`.

The answer has these parts:

- `active`: the plan over the patched day.
- `baseline`: the plan over the unpatched materialization. It is null when no edit is in force on the date, or when no `edits_hash` is given.
- `notice`: the rider's notice. It is null when nothing changed.

Both plans use the live document's predictions when `live_hash` is given. Both walk the walk table named by `walks_hash`.

The notice appears whenever the diversion changes the trip. It has these fields:

- `kind`: one of these:
  - `unreachable`: there is no active trip, and the baseline has one.
  - `no_trip`: there is no trip within `max_legs` in either world.
  - `later`: the active trip arrives later. The notice carries `delay_secs` and `minutes_lost` (rounded up).
  - `diverted`: the plan rides a trip whose patched stop times differ from its unpatched instance.
- `baseline_arrival_secs`: the unpatched day's arrival, when it has a trip.
- `routes` and `skipped_stops`: the routes and skipped stops of the diverted trips that the plans ride.
- `text`: the notice as one line.

The op is one op whether or not edits exist.

## itinerary_paths

Answer where an itinerary runs, with one path per leg: `{kind: transit | walk, path: [{lat, lon}]}`. Pass the itinerary that `plan` answered. Pass the same `edits_hash`, so the paths use the same effective day that the plan used.

A transit leg's path is the ridden segment only:

- The trip's line is `trip_path`'s line: the authored alignment, with an in-force diversion's traced detour spliced between its kept stops.
- The segment is that line sliced between the board and alight stops, when the trip visits both.
- When the trip does not visit both, the segment is its stops between them.
- The last resort is a straight line.

A walk leg's path runs through the stops that the walk passes.

Given the plan's ends (`from`, `to`) and `direct`, a walk from or to a place follows that place's walk path. A walk between two places follows the path of `direct`. Without the ends, a place draws nothing.

The map draws the paths. It computes none.
