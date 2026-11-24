# transit-router

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

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### A plan's time is door to door (router `leave_secs`, `duration_secs`)

- **Holds:** an itinerary carries when the rider must leave (the first boarding minus the walk before it) and the trip's length from then. "Leave in N min" uses the board's `wait` rule.
- **Because:** that is what riders compare and what other trip planners show. Host arithmetic over legs would put an answer outside the module.
- **Rejected:** a duration from the moment asked.
- **Decided:** D-40 (on a plan not asked for now: D-92).
