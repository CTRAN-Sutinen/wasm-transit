# transit-analysis

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

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

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
- **Holds:** a combination is named by its members, with its dates. Any rule that groups, labels, or ranks combinations ("weekday", "Saturday", "regular", "exception") is the user's call. Record it as an unanswered decision before any code, never as a provisional encoding.
- **Because:** pooling a base type with subsets that run in successive periods counted trips that never run together (2,130 against 1,774 on C-TRAN). A planner needs the real combinations and when they hold.
- **Rejected:** counting members on a representative day; splitting a group into periods with representative days; counting every member and saying so.
- **Decided:** D-55.

### The feed's own facts are not inferred (analysis `transfer_stops`, rt `late[].label`)

- **Holds:** a transfer point is the feed's flag and is never inferred (RAPTOR finds transfers regardless). A vehicle with no label shows its id.
- **Because:** these are the agency's facts, and the rules stay generic rather than coupled to one agency.
- **Decided:** D-10.
