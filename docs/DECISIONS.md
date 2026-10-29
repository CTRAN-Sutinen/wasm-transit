# Decisions

This is the durable record of the questions put to the user and
their answers. When
an agent hits a decision that the user's intent, the GTFS spec,
and the architecture laws leave open, the agent writes it down here
and does not build on an answer it picked (AGENTS.md, "Clarify,
don't assume"). The entry names the work the decision blocks, so
the rest of the work can go ahead while the decision is reviewed.

## The rules

- **One entry per question**, in this format:

  ```
  ### D-N — the question
  State: Unanswered | Encoded provisionally | Decided
  Answer: (Decided only) the user's answer, dated, in their words.
  Context: what the intent, the spec, and the code say, briefly.
  Options: each answer and what it costs; mark the one the code
    encodes (if any).
  Blocks: the units, findings, or laws that cannot proceed or be
    deleted until it is decided. "Nothing" is a valid answer.
  ```

- **State.** *Unanswered* means nothing is built on any answer, and
  the work it blocks waits. *Encoded provisionally* means an agent
  had to pick an answer to land a law. The laws that pin that
  answer are named, and a different decision rewrites them.
- **Blocked work names its decision.** A unit or finding that
  waits on a decision says `Blocked: D-N`. Work that does not name
  one is free to start.
- **A decided entry stays — decisions are a durable record, never
  deleted.** Its state becomes *Decided* and its Answer line records
  what the user said and when. In the same change, the answer goes
  into the law that pins it and the doc that owns its topic
  (`docs/ARCHITECTURE.md`, `docs/units/`, `docs/QA-FINDINGS.md`),
  and the `Blocked: D-N` lines are removed. A later answer that
  reverses one is a new entry naming the entry it replaces; the old
  entry's state says so.
- IDs are never reused; the next new decision is **D-33**.

## Summary

| id | question | state | blocks |
|---|---|---|---|
| D-1 | Is a trip's start its first arrival or its first departure? | encoded | nothing |
| D-2 | What does a running-time change touch besides fixed-start trips? | encoded | nothing |
| D-3 | Which timing profile does the import give a pattern? | encoded | nothing |
| D-5 | Does the import keep a block that breaks the spec? | encoded | nothing |
| D-6 | Does "frequent" count terminal arrivals? | unanswered | QAF-78 |
| D-8 | Do timetable edits act on the whole service family? | unanswered | QAF-65 gestures, QAF-44 |
| D-9 | Where does required recovery time come from? | unanswered | QAF-62, QAF-26 |
| D-10 | Agency questions on C-TRAN's data | unanswered | QAF-9, QAF-48 |
| D-11 | Does editing a shared alignment change every pattern on it? | unanswered | an alignment-authoring edit |
| D-12 | What headsign does a new trip get? | encoded | nothing |
| D-14 | Does the proposal comparison collect public responses? | unanswered | `ui-proposal-compare` question 6 |
| D-15 | What makes a trip "covered"? | unanswered | `ui-disruptions` question 6 |
| D-16 | Where does the runs layer live? | encoded | nothing |
| D-17 | What does "the last bus" mean with no deadline? | encoded | nothing |
| D-18 | How do live delays meet a diverted trip? | encoded | nothing |
| D-19 | Does an alert for one direction reach both? | encoded | nothing |
| D-20 | Which language does an alert speak? | encoded | nothing |
| D-21 | Where is a trip going when nothing signs it? | encoded | nothing |
| D-22 | How is a route's rider variant named and ordered? | encoded | nothing |
| D-23 | In what order does a stop display list its groups? | encoded | nothing |
| D-24 | How does a departure's countdown round? | encoded | nothing |
| D-25 | Which streets are the source of truth? | decided | nothing |
| D-26 | Which streets can each mode use? | decided (walking); unanswered (a bus) | drawing on streets |
| D-27 | What does place search find? | unanswered | geocoding (queued) |
| D-28 | Where do stop and address results meet? | unanswered | geocoding (queued) |
| D-29 | What area does a geography release cover? | unanswered | streets-model geography release |
| D-30 | What does the streets import call a street, and what class is it? | encoded | nothing |
| D-31 | Can a plan add a street the data does not have? | unanswered | streets-model planned streets |
| D-32 | How does the router walk with no walk table? | encoded | nothing |
| D-33 | How does a trip from a place walk? | encoded | nothing |
| D-34 | Where does a stop no walk reaches go in the nearby list? | encoded | nothing |
| D-35 | Among plans arriving together, where does the rider board and get off? | encoded | nothing |

## The trip model

### D-1 — Is a trip's start its first arrival or its first departure?
State: Encoded provisionally.
Context: a stored trip's times are offsets from its start, and the
trip laws require the first stop's arrival offset to be 0.
`frequencies.txt` defines `start_time` as the time the first
vehicle *departs* the first stop. A trip whose first stop has a
dwell (arrival before departure) therefore has two candidate
starts. The feed's own frequency base is kept in
`Boundary::frequency_bases`, so export is lossless under either
answer.
Options:
- **First arrival (encoded).** `RetimeTrip`, `AddTrip`, headway
  bands, and the timetable all read the arrival at the first stop.
- First departure. This matches the spec's frequency wording, and
  a rider's "leaves at". The first arrival then becomes a negative
  offset or is dropped.
Pinned by: `trip_law_test.rs`, and `FirstOffsetNotZero` in
`transit-network` validation (`network_test.rs`).
Blocks: nothing. A change rewrites those laws and the codec's
frequency export.

### D-2 — What does a running-time change touch besides fixed-start trips?
State: Encoded provisionally.
Context: `adjust_running_time` names a route, a direction, a
service type, and a window. It rescales the stored times of each
fixed-start trip that starts in the window, keeping the first
stop's dwell.
Options:
- **Fixed-start trips only (encoded).** The pattern's timing
  profiles and any frequency trips are left alone. A trip added
  later with `add_trip` gets the profile's old times, and a
  frequency trip that runs through the window keeps its old times.
- Also update the profile in the window, so that new trips inherit
  the change.
- Also rescale the frequency trips that overlap the window. A
  frequency trip has one set of times, so one that only partly
  overlaps has to be split into two trips, and the new trip needs
  an id.
Pinned by: `running_time_changes_only_the_named_trips`
(`trip_law_test.rs`) and `timetable_edit_test.rs`.
Blocks: nothing.

### D-3 — Which timing profile does the import give a pattern?
State: Encoded provisionally.
Context: profiles only produce times for new trips; they never
govern stored trips. GTFS has no profiles, so the import has to
derive one.
Options:
- **One all-day profile from the pattern's earliest trip
  (encoded).** A trip added at 17:00 gets the 05:00 run time.
- One profile per time band, taken from clusters of observed run
  times. This is closer to a scheduler's practice, but it needs a
  clustering rule, which is a further decision.
- No profile. `add_trip` on an imported pattern then has to name
  its times or copy them from a neighbouring trip.
Pinned by: `trip_import_law_test.rs` and `codec_prop_test.rs`.
Blocks: nothing.

### D-11 — Does editing a shared alignment change every pattern on it?
State: Unanswered.
Context: patterns run alignments by reference (`Pattern::alignment`),
and several patterns may run one: a feed may give one shape to
trips with different stops. No edit authors an alignment yet. The
first edit that redraws a pattern's line has to say what happens to
the other patterns on the same alignment.
Options:
- The edit changes the alignment, so every pattern on it is
  redrawn. This is one place to fix a line, but a change meant for
  one variant moves the others silently.
- The edit copies the alignment under a new id for the edited
  pattern, and the others keep the old line. This follows "new
  entities get new ids", but a fix meant for all of them must be
  made once per pattern.
- The edit names which it does, and the gesture reports the
  patterns it touched.
Blocks: an alignment-authoring edit (none exists; `UpsertPattern`
can point a pattern at an existing alignment, but no edit adds or
redraws one).

### D-5 — Does the import keep a block that breaks the spec?
State: Encoded provisionally.
Context: GTFS forbids trips in one block that overlap on the same
service day. A real feed can still carry them.
Options:
- **Import as written (encoded).** Validation reports
  `BlockInfeasible`, nothing is dropped, and export reproduces the
  feed.
- Drop or split the offending trips, with an import note. This
  loses data, which conflicts with the lossless law.
Pinned by: the spec-valid generators in `codec_prop_test.rs` and
`import_prop_test.rs` (they never produce such a block, so no law
pins the invalid case).
Blocks: nothing. C-TRAN has no such block.

### D-12 — What headsign does a new trip get?
State: Encoded provisionally.
Context: a trip's headsign is its own (`Trip::headsign`), and the
export writes it as is — a feed trip published without one stays
without one (the whole-spec law forbids inventing it). A trip the
planner creates (`add_trip`, a headway or span gesture) has no
headsign, so an authored network publishes none for it; riders see
no sign text.
Options:
- **None (encoded).** The planner signs trips explicitly; there is
  no edit for it yet.
- Copy the headsign of a trip on the same pattern trips (or the
  pattern), when one exists.
- The direction's name, as the export did before trips carried
  their own headsign.
Pinned by: `spec_round_trip_test.rs` (`trips_survive`) pins only
that nothing is invented on export.
Blocks: nothing.

## Analysis and live

### D-6 — Does "frequent" count terminal arrivals?
State: Unanswered. The current answer is pinned as a finding, not
as a law.
Context: QAF-78. The whole-day "frequent" test counts every visit
to a stop, arrivals included, so a terminal where nobody boards can
read as frequent. The windowed test counts departures only.
Options: departures only, for both tests (the finding reads this
way); or keep counting arrivals and document the difference.
Blocks: the fix for QAF-78, and the `frequent_pct` number in
`transit-analysis` `regression_test`.

## Service types

### D-8 — Do timetable edits act on the whole service family?
State: Unanswered.
Context: QAF-65 and QAF-44. The reads pool a service family (a
base service and the siblings that run on its days, such as 1-WKDY
with /30 and /31), but the timetable gestures key on one service
type. A `set_headway` under 1-WKDY retimes only the base's trips.
Options:
- Gestures act on the family. The trips stay under their own
  service ids.
- Gestures stay per service type, and the UI warns when the family
  has siblings.
- An authored "same as, except" relation between service types
  (QAF-44), which the gestures follow. This is a model change.
Blocks: the remaining QAF-65 work (the gestures) and QAF-44.

## Blocks and runcutting

### D-9 — Where does required recovery time come from?
State: Unanswered.
Context: QAF-62. Imported layovers are the whole observed gap
between two trips, so any delay reads as infeasible. Separating
required recovery from slack needs a rule the feed does not carry.
Options: a policy document (a minimum in minutes, a percentage of
the run time, or both, per route or per service type); a value
derived from the feed (for example, the smallest layover seen at
that terminal); or both, with the policy winning.
Blocks: QAF-62 and QAF-26 (the recovery view is meaningless until
recovery is separated from slack).

### D-16 — Where does the runs layer live?
State: Encoded provisionally (in the docs; no code yet).
Context: runcutting grows into runs and bidding (the user's
decision; `docs/ROADMAP.md` 7). A run is cut from blocks, so the
layer could hang off `Block` or stand beside the network.
Options:
- **Beside the network (encoded).** Runs, the work-rule policy,
  and bids are their own documents and modules, reading the
  network by hash. This follows the laws: the network stays the
  core model, and one need per module.
- A `Block` extension: runs as fields on blocks in the network
  document. Every network consumer then carries operator work.
Pinned by: `docs/ROADMAP.md` 7, `docs/units/ui-runcut.md`.
Blocks: nothing.

## Riding today

### D-17 — What does "the last bus" mean with no deadline?
State: Encoded provisionally.
Context: the last bus home. `plan_arrive_by` without `arrive_by_secs` answers
the latest departure that still reaches the destination. Something
has to stand in for the missing deadline.
Options:
- **The service day's last arrival (encoded).** A pair a rider can
  walk between answers a walk that leaves at that moment minus the
  walk.
- Transit legs only: a walk never counts as the last way home.
Pinned by: transit-router `arrive_by_law_test` (law 4), cap-router
`arrive_by_answers_the_latest_departure`.
Blocks: nothing.

### D-18 — How do live delays meet a diverted trip?
State: Encoded provisionally.
Context: a live plan reads each trip at its predicted
times. The rt feed states delays against the PUBLISHED schedule, and
a diversion's `added_secs` also shifts the trip.
Options:
- **The rt delay wins (encoded).** A stated visit is the published
  time plus the delay, not also shifted by `added_secs`. A visit is
  matched by its scheduled departure, else by stop when the trip
  visits it once. A missing arrival (departure) delay takes the
  other's, and predictions are clamped so a trip never runs
  backwards.
- Both apply: the delay is read against the patched time.
Pinned by: transit-router `live_law_test`, cap-router
`a_compared_plan_reads_live_in_both_worlds`.
Blocks: nothing.

### D-19 — Does an alert for one direction reach both?
State: Encoded provisionally.
Context: a GTFS-rt informed entity may name a `direction_id`. The
model matches patterns, not GTFS directions (the diversion record
keeps `direction_id` for the boundary path only).
Options:
- **Not matched (encoded):** the alert reaches both directions of
  the route. Telling too many riders beats telling none.
- Resolve the GTFS direction to the route's model directions at the
  boundary and scope the alert to them.
Pinned by: transit-rt `alert_test` (`resolve_scope` ignores the
direction).
Blocks: nothing.

### D-20 — Which language does an alert speak?
State: Encoded provisionally.
Context: alert text is a `TranslatedString`. The live document
carries one string per field.
Options:
- **The untagged translation, else the first (encoded).**
- Carry every translation and let the reader's language choose.
  That choice would be a parameter on the board, never host logic.
Pinned by: transit-rt `alert_test`
(`an_alert_decodes_and_resolves_into_model_vocabulary`).
Blocks: nothing.

### D-21 — Where is a trip going when nothing signs it?
State: Encoded provisionally.
Context: a board row's or leg's destination. `visit_destination` reads the visit's headsign, then
the trip's. When neither is set, the rule falls back to the final
stop, and that stop may be a bay ("Van Mall Transit Center: Bay D").
Options:
- **The final stop's parent station, else its own name (encoded).**
- The final stop's own name, bay included.
Pinned by: transit-network `signs_law_test`
(`a_final_bay_is_named_by_its_station`), transit-dispatch
`a_row_is_going_where_its_sign_says`.
Blocks: nothing.

### D-22 — How is a route's rider variant named and ordered?
State: Encoded provisionally.
Context: `route_detail` groups a direction's patterns into the
variants a rider rides: the same places in order, a station's bays
one place (the user's rule — the per-bay patterns at a transit
center read as one route). Real differences stay apart (006's two
branches; 060's time-of-day stops). Two variants can share their
ends, and they need names a rider can tell apart.
Options:
- **Ordered by trips, every service type summed; named "first →
  last", then " via" the first place only it visits against the
  most-trips variant with the same ends, else ", skips" the first
  place of that variant it misses (encoded).**
- Ordered by today's trips, named by when they run ("weekday
  mornings") — needs the service day; a rider reads time of day.
Pinned by: transit-network `listing_test`
(`bay_variants_of_a_direction_read_as_one`).
Blocks: nothing.

### D-23 — In what order does a stop display list its groups?
State: Encoded provisionally.
Context: `grouped_board` answers one group per route and
destination. GTFS `route_sort_order` says routes with lower values
display first; it says nothing about routes without one, or about
one route's several destinations. C-TRAN sets it on every route.
Options:
- **`route_sort_order` ascending, routes without one after them by
  route id, then destination by name (encoded).** Stable: a group
  keeps its place as the day goes on, and an ended group stays put.
- Soonest departure first. The next bus is always on top, but the
  sign reshuffles every poll and a rider loses their line.
- Routes without a sort order first, or by public name instead of id.
Pinned by: `the_groups_list_in_display_order`
(`grouped_board_prop_test.rs`).
Blocks: nothing.

### D-24 — How does a departure's countdown round?
State: Encoded provisionally.
Context: the user set the display's time format: "Due" under a
minute, "N min" up to 59, then the clock time. Whether 3 min 50 s
reads "3 min" or "4 min" was left open. Every board row carries it
(`wait`).
Options:
- **Round down (encoded).** 3 min 50 s reads "3 min" — a rider
  hurries rather than misses a bus (an early departure is a missed
  bus).
- Round to the nearest minute. Closer on average; a rider can read
  "4 min" for a bus 3 min 31 s away.
Pinned by: `every_row_says_how_long_until_it_leaves` and
`the_wait_turns_at_a_minute_and_an_hour`
(`grouped_board_prop_test.rs`).
Blocks: nothing.

## Public comparison

### D-14 — Does the proposal comparison collect public responses?
State: Unanswered.
Context: the comparison UI reads published proposals. Collecting
comments or votes on them would add a new shared document, synced
through the server, with its own retention and moderation
questions.
Options:
- Read-only. Responses go through the agency's existing channels,
  and the UI links to them.
- A response document on the bus (a new document kind, synced by
  LWW per record like the edit document).
Blocks: question 6 of `docs/units/ui-proposal-compare.md`.

## Live operations

### D-15 — What makes a trip "covered"?
State: Unanswered.
Context: the disruptions UI asks which trips are covered and which
are uncovered. The bus knows vehicles only through blocks and live
positions. It holds no operator assignments (QAF-17), and the edit
document cannot cancel or cover a trip (QAF-36).
Options:
- A trip is covered when a vehicle is on its block: from the
  blocks and the live positions, with cancel and cover as new edit
  kinds (QAF-36).
- A trip is covered when an operator holds its run. This needs the
  runs layer (`docs/ROADMAP.md` 7) and workforce data.
Blocks: question 6 of `docs/units/ui-disruptions.md`.

## Geography

### D-25 — Which streets are the source of truth?
State: Decided.
Answer: 2026-10-29 — "it should use the streets document model you
just built. that model can get data from multiple sources." Walking
and every other street op read a streets document by hash; which
source fills it is the host's choice. On C-TRAN's Trapeze roads not
carrying walking: "that is something to be solved in the data, not
in the model or the logic … a problem for me/the user to solve" —
walking wants an OSM-sourced document (QAF-85).
Context: the streets document (`docs/units/streets-model.md`) is
imported from a street layer; the import reads the agency's
Trapeze roads export, TIGER, and OSM GeoJSON alike. C-TRAN's
Trapeze roads (118,860 features) are what the agency's scheduling
system routes on, so deadheads computed over them agree with it.
They carry no sidewalks, paths, or stairs (no CFCC A7x feature),
so walking over them treats every road as walkable and misses
every cut-through. OSM carries footways and crossings, at a
licence cost: the ODbL's share-alike applies to a streets document
derived from OSM and distributed.
Options:
- One source for every mode: the Trapeze roads (agency parity,
  no walking network), or OSM (walking detail, no parity).
- Two documents: the Trapeze roads for driving (drawing, detours,
  deadheads) and OSM for walking (access, transfers, walksheds).
  Every op that takes streets then names which.
- **Decided:** the model takes any source; the source is data.
Blocks: nothing.

### D-26 — Which streets can each mode use?
State: Decided for walking; unanswered for a bus.
Answer: 2026-10-29 — walking: "Also refuse unknown" (a rider walks
every class but motorway, ramp, and unknown — strict); a rider walks
a one-way street both ways ("agreed"); a walk is at most ten minutes
by default, at 1.3 m/s, both op parameters. Pins:
`crates/transit-streets/tests/walk_law_test.rs`.
Context: the import classifies every street (D-30). A rider cannot
walk a motorway or a ramp; a bus cannot be drawn down a path,
stairs, or a 4WD track, and a planner may not want one drawn down an
alley or a private drive. C-TRAN's data holds 1,701 streets with no
class (626 with a null CFCC, among them SE Powell Blvd, and 1,075
unnamed `X10` features), so a rule must also say what an unknown
class allows.
Options: a mode profile per class, owned by the op as a parameter
with a default (a walking profile, a bus profile). The defaults
are the decision: which classes each mode may use, and whether an
unknown class is allowed (permissive) or refused (strict).
- **Decided (walking):** strict — every class but motorway, ramp,
  and unknown; one-way does not bind a rider.
- Open (a bus): which classes a bus may be drawn on, and whether an
  unknown class is allowed.
Blocks: drawing on streets in `docs/units/streets-model.md`.

### D-27 — What does place search find?
State: Unanswered.
Context: search finds stop names only (directory `stops`). A rider
types addresses ("1400 NE 136th Ave"), intersections ("4th Plain
& Andresen"), and places ("PeaceHealth", "WSU Vancouver"). The
Trapeze roads carry TIGER-style address ranges per side
(`L_F_ADD`…`R_T_ADD`) and street names, so addresses and
intersections need no new source; places do (OSM, or the county's
data), and they are the hardest to keep current.
Options: stops only (today); + intersections; + addresses
(interpolated along the ranges, or the county's address points);
+ places.
Blocks: geocoding (queued in `docs/units/streets-model.md`).

### D-28 — Where do stop and address results meet?
State: Unanswered.
Context: a rider wants one search box. The host may not merge two
ops' answers (`docs/ARCHITECTURE.md`, "Generated UIs", step 6), and
a capability that takes both the gazetteer and the network trips
the second-document split trigger (`crates/AGENTS.md`).
Options:
- A places capability whose one question is "which place do you
  mean?", over both documents — the trigger's ops belong together
  here, and the law's text says so.
- The gazetteer imports the network's stops, so one document
  answers; the gazetteer then changes with every plan.
- Two boxes (stops, addresses).
Blocks: geocoding (queued in `docs/units/streets-model.md`).

### D-29 — What area does a geography release cover?
State: Unanswered.
Context: a geography release pins the basemap, the streets
document, and later the gazetteer, built from one source snapshot
(`docs/units/streets-model.md`). C-TRAN runs into Portland. The
Trapeze export already reaches Portland (its first feature is NE
21st Ave, Portland); the local basemap is one extract.
Options: the service area plus a buffer (how far?); the counties
served (Clark, plus Multnomah for the Portland trips); whatever
the agency's GIS exports.
Blocks: the geography release in `docs/units/streets-model.md`.

### D-30 — What does the streets import call a street, and what class is it?
State: Encoded provisionally.
Context: street layers mix roads with other lines. C-TRAN's
Trapeze roads hold 1,673 + 1,388 railroads (CFCC `B10`, `B20`) and
1,160 + 1,898 + 81 water lines (`H10`, `H20`, `H30`: shorelines,
the Washougal River). Before the streets document, every one of
them was welded into the routing graph. The import must say which
lines are streets, and give each a class the walking profile and the bus profile (D-26)
can read. The class field is read from the layer's own scheme:
TIGER's CFCC (Trapeze and older TIGER), TIGER's MTFCC, OSM's
`highway`.
Options:
- **Encoded:** a line whose class field names something other than
  a road is not a street: a CFCC in a non-road category (`B`
  railroads, `C` other transport, `D` landmarks, `E` physical
  features, `F` boundaries, `H` water), an MTFCC outside `S1`, an
  OSM line with no `highway` but a `railway`, `waterway`,
  `natural`, `power`, or `boundary` tag. A line with no class
  field, or with a road class the import does not know (CFCC `X10`
  "not yet classified", a null CFCC, CFCC `A65` ferries, MTFCC
  `S1750`, OSM `construction` and `proposed`), is a street of
  class `unknown`, and the walking and bus profiles (D-26) decide what it
  allows. The class field is read in the order CFCC, MTFCC,
  `highway`; the name in the order `FULLNAME`, `name`, `NAME` (the
  Trapeze `NAME` is the bare street name, "21ST" for NE 21st
  Ave). The classes: `motorway` (CFCC A1, MTFCC S1100, OSM
  motorway), `primary` (A2, S1200, trunk, primary), `secondary`
  (A3, secondary, tertiary), `local` (A4, A60–A62, S1400,
  residential, unclassified, living_street, road), `ramp` (A63,
  S1630, any `*_link`), `service` (A64, A73, A74, S1640, S1730,
  S1740, S1780, service), `track` (A5, S1500, track), `path` (A70,
  A71, S1710, S1820, S1830, footway, path, pedestrian, cycleway,
  bridleway), `steps` (A72, S1720, steps).
- Keep every line, classed `other` — the graph then needs the mode
  profiles (D-26) before it is safe to route on.
Blocks: nothing. Pins: `crates/transit-streets/tests/import_law_test.rs`.

### D-31 — Can a plan add a street the data does not have?
State: Unanswered.
Context: a service change often serves streets that do not exist
yet (a subdivision under construction, a new connector). Drawing a
pattern on streets (`docs/units/streets-model.md`) can only follow
streets the streets document has. A pattern's committed alignment
is geometry and never references the streets document, so a
pattern can always be drawn freehand.
Options:
- No: a planned street is drawn freehand on the pattern.
- A streets edit document beside the plan, applied to the imported
  streets the way network edits answer the next network. The
  streets document's ways then need identities that survive a
  re-import.
Blocks: planned streets in `docs/units/streets-model.md`.

### D-32 — How does the router walk with no walk table?
State: Encoded provisionally.
Context: the router's transfers walk the streets when a plan names
a walk table (`walks_hash`, minted by cap-streets `walks` from a
streets document and the network). A plan with none still needs
transfers: the fixture feeds, a host with no streets, a plan
document drawn where no streets are loaded.
Options:
- **Encoded:** with no walk table the router keeps its straight-line
  transfers (every stop pair within 300 m, at 1.3 m/s) — two answers
  to "how long is this walk" on the bus, one of them crow-flies.
- Require a walk table on every plan: one answer, and every host
  that plans must assemble cap-streets and a streets document.
- No transfers on foot without a walk table.
Blocks: nothing. Pins: `crates/transit-router/tests/walks_law_test.rs`.

### D-33 — How does a trip from a place walk?
State: Encoded provisionally.
Context: a plan's end can be a place off the network (a tapped
spot, the rider's location), named by its walks to the stops near
it: cap-streets `access` answers them along the streets, each
within the walk limit (ten minutes, D-26), and the walk straight to
a second point (the plan's `direct`). The router plans between ends. Three things the
intent leaves open:
- Whether the walk to the first stop may continue on foot to
  another stop before boarding (and the walk from the last stop be
  preceded by one). A stop plan already chains transfer walks.
- When walking straight there arrives as early as riding, which the
  rider is told to do.
- Whether a destination past the walk limit on foot is ever walked
  to directly.
Options:
- **Encoded:** a trip from a place is the place's walk to one of its
  stops, any stop plan (which may begin or end with transfer walks:
  a walk chain is one walk leg), and a stop's walk to the
  destination place — or the direct walk, which wins ties. Every
  single walk is within the limit; a chain of them may total more,
  as a stop plan's chains already do. The direct walk exists only
  within the limit.
- The place's walk is the whole walk at each end (no chain): a
  rider never walks more than the limit to reach the first bus.
- Ties go to riding.
- The direct walk is unbounded: a destination 20 minutes away on
  foot with no bus is still a plan.
Blocks: nothing. Pins:
`crates/transit-router/tests/places_law_test.rs` (laws 1–3 compose
the stop plan with chains; law 6 the tie), `walks_from` in
`crates/transit-streets/tests/walk_law_test.rs` (the limit).

### D-34 — Where does a stop no walk reaches go in the nearby list?
State: Encoded provisionally.
Context: with the point's walks along the streets (cap-streets
`access`, within the ten-minute limit), the directory's
`stops_near` ranks the places near the point by walk. Some places
are near as the crow flies and reached by no walk within the
limit — across a freeway, past the limit, or off every walkable
street.
Options:
- **Encoded:** the places a walk reaches come first, by walk (then
  distance, then id); the rest follow by distance, with no walk
  time. The walks reorder the places and nothing else: the same
  places answer within the radius and the limit.
- Leave the places no walk reaches out: the list says only where
  the rider can walk, and is empty off the streets.
- Rank every place by distance and show the walk where one exists.
Blocks: nothing. Pins:
`crates/transit-network/tests/stops_near_law_test.rs` (laws 6–8).

### D-35 — Among plans arriving together, where does the rider board and get off?
State: Encoded provisionally.
Context: the search answers the earliest arrival within the leg
bound, and its rides; it is indifferent to how far the rider walks.
On C-TRAN (2026-10-29, Jantzen Beach to Van Mall Dr & 72nd at 14:15)
it walked the rider 13 minutes up Broadway to route 032's first stop
at 19th St, when the same bus passes Evergreen & Broadway, a
four-minute walk, three minutes later — the same arrival. The user
called it out: closer stops serve the transfer.
Options:
- **Encoded:** the arrival and the rides are the search's; then the
  first ride is boarded, each transfer made, and the last ride left
  at the stops that walk least while still connecting (a tie keeps
  the search's stop). Each walk is the least for the stops around
  it — not a search for the least walking over every plan with that
  arrival.
- Least total walking over every earliest plan (a second criterion
  in the search itself: costlier, and it can trade a ride for a walk
  the encoded rule never does).
- Fewest minutes waiting instead: board as late on the trip as the
  rider can reach.
Blocks: nothing. Pins:
`crates/transit-router/tests/walk_least_law_test.rs` (laws 1–4).

## Agency data

### D-10 — Agency questions on C-TRAN's data
State: Unanswered.
Context: two findings wait on facts only the agency has, not on a
design choice.
- QAF-9: should transfer inference fire on C-TRAN (station 106 and
  its bays all read `transfer: false`)? Or does C-TRAN mean its
  stations only as walking groups?
- QAF-48: vehicle labels are empty, so the late list shows ids
  like "CTRAN7003". Is that id what a dispatcher calls the bus? If
  not, what is?
Blocks: QAF-9 and QAF-48.
