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
- IDs are never reused; the next new decision is **D-116**.

## Summary

| id | question | state | blocks |
|---|---|---|---|
| D-1 | Is a trip's start its first arrival or its first departure? | encoded | nothing |
| D-2 | What does a running-time change touch besides fixed-start trips? | encoded | nothing |
| D-3 | Which timing profile does the import give a pattern? | encoded; re-opened | QAF-90 |
| D-5 | Does the import keep a block that breaks the spec? | encoded | nothing |
| D-6 | Does "frequent" count terminal arrivals? | decided | nothing |
| D-8 | Do timetable edits act on the whole service family? | decided | nothing |
| D-9 | Where does required recovery time come from? | decided | nothing |
| D-10 | Agency questions on C-TRAN's data | decided | nothing |
| D-11 | Does editing a shared alignment change every pattern on it? | decided | nothing |
| D-12 | What headsign does a new trip get? | encoded | nothing |
| D-14 | Does the proposal comparison collect public responses? | decided | nothing |
| D-15 | What makes a trip "covered"? | decided | the operator half: the runs layer |
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
| D-26 | Which streets can each mode use? | decided | nothing |
| D-27 | What does place search find? | decided | nothing |
| D-28 | Where do stop and address results meet? | encoded | nothing |
| D-29 | What area does a geography release cover? | decided | nothing |
| D-30 | What does the streets import call a street, and what class is it? | encoded | nothing |
| D-31 | Can a plan add a street the data does not have? | decided | nothing |
| D-32 | How does the router walk with no walk table? | encoded | nothing |
| D-33 | How does a trip from a place walk? | encoded | nothing |
| D-34 | Where does a stop no walk reaches go in the nearby list? | encoded | nothing |
| D-35 | Among plans arriving together, where does the rider board and get off? | encoded | nothing |
| D-36 | Is geocoding a document on the bus or a service the host calls? | decided | nothing |
| D-37 | What names a spot dropped on the map? | encoded | nothing |
| D-38 | What may a host do with an external service's answer? | decided | nothing |
| D-39 | Do the live feeds move into the manifest's services? | decided | nothing |
| D-40 | How long does a planned trip take, and from when? | decided | nothing |
| D-41 | What carries a service type's calendar into a fork's period? | decided | nothing |
| D-42 | When a route has stopped for the day, how far ahead does a stop look? | decided | nothing |
| D-43 | What does a coverage comparison report? | encoded | QAF-20's matching rule |
| D-44 | Is a branch of a direction its own thing? | unanswered | QAF-63 |
| D-45 | Which dispatcher edits does the edit document carry beyond the diversion? | decided | nothing |
| D-46 | How many more vehicles does a proposal need? | decided | the work-rule policy document (D-50's shape) |
| D-47 | Which layers does coverage take? | decided | nothing |
| D-48 | Does a stop answer to a name its sign does not carry? | decided | nothing |
| D-49 | Where do the basemap's glyphs and sprites come from? | decided | nothing |
| D-50 | What are the work rules a run is cut under? | decided | nothing |
| D-51 | What does a revenue hour, a revenue mile, and a vehicle cost? | decided | nothing |
| D-52 | How is a route-hour's frequency judged and drawn? | encoded | nothing |
| D-53 | What ridership data does the planner bring? | decided | nothing |
| D-54 | What does a frequency change keep of the trips already there? | unanswered | QAF-90, `ui-service-design` question 16 |
| D-55 | Which members does a service group count on a day? | decided | nothing |
| D-56 | Whose clock is the LWW clock? | encoded | nothing |
| D-57 | How long does a tombstone live? | encoded | nothing |
| D-58 | Which endpoints require a token by default? | decided | nothing |
| D-59 | Which JWT claim carries the roles? | decided | nothing |
| D-60 | How are JWTs verified (JWKS vs pinned key)? | decided | nothing |
| D-61 | Where does the authz policy live and how is it changed? | decided | nothing |
| D-62 | Where does a document's schema version live? | decided | nothing |
| D-63 | What does an open page do when the deployment moves past it? | decided | nothing |
| D-64 | Which devices must the rider apps run well on? | decided | nothing |
| D-65 | Do ride-now and the stop display work offline? | decided | nothing |
| D-66 | How long is a migration kept? | decided | nothing |
| D-67 | Whose feeds does the server's proxy fetch? | decided | nothing |
| D-68 | What are ride-now's and the stop display's performance budgets? | decided | nothing |
| D-69 | What happens to the original app when host calls move into the engine? | decided | nothing |
| D-70 | How do the module runtime's bounds default? | encoded | nothing |
| D-71 | May a module register a document under a pinned hash it did not compute? | decided | nothing |
| D-72 | Is the release profile tuned for size or for speed? | encoded | nothing |
| D-73 | What does a stop's board show for a trip the live feed cancels? | decided | nothing |
| D-74 | How is a pick published, and what runs before the first one? | encoded | nothing |
| D-75 | How does a published pick come back out? | decided | nothing |
| D-76 | What does publishing refuse beyond an invalid chain? | decided | nothing |
| D-77 | How do the capabilities a page uses share one network? | decided | nothing |
| D-78 | Does the Go server host linked assemblies? | decided | nothing |
| D-79 | How is a document version rolled back? | decided | nothing |
| D-80 | Where do retired build artifacts live? | unanswered | retention in `release-artifacts` |
| D-81 | How are op and HTTP contracts versioned? | unanswered | contract versioning in `release-artifacts` |
| D-82 | Where do the store's backups go, and how often? | unanswered | backup in `store-durability` |
| D-83 | What does each deployed device report, and to where? | unanswered | reporting in `observability-rollout` |
| D-84 | Who takes a build first, and what counts as healthy? | unanswered | rings and revert in `observability-rollout` |
| D-85 | Are the generated assembly crates committed? | decided | nothing |
| D-86 | Which op answers what an edit record amounts to on the network? | encoded | nothing |
| D-87 | How does an export name the import numbering it writes back? | encoded | nothing |
| D-88 | What does a page run when its linked assembly does not load? | encoded | nothing |
| D-89 | May a host decline a document an op minted? | encoded | nothing |
| D-90 | What may a rider's device remember to open the map where it will end up? | encoded | nothing |
| D-91 | What does fuzzy stop search accept, and in what order? | encoded | nothing |
| D-92 | When does a plan say "leave in N min"? | encoded | nothing |
| D-94 | Which types make a combination, and how are its dates folded? | encoded | nothing |
| D-95 | Which service type does a gesture change, and what does its warning name? | encoded | nothing |
| D-97 | Where does a layer's weight field and label live? | encoded | nothing |
| D-98 | How finely is a polygon's area sampled? | encoded | nothing |
| D-99 | Who names the two sides of a coverage comparison? | encoded | nothing |
| D-100 | How does a board show a trip the dispatcher cancelled? | encoded | nothing |
| D-101 | What does a trip cancel name, and how does it enter the edit document? | encoded | nothing |
| D-102 | Which capability authors a trip cancel? | encoded | nothing |
| D-103 | Which street classes does the bus profile ask a planner to confirm? | encoded | nothing |
| D-104 | Where does a fork's period end come from? | encoded | nothing |
| D-105 | What does a fork carry when a feed's calendars are all dated? | decided | nothing |
| D-106 | Does a recovery rule judge a trip that continues with no wait? | encoded | nothing |
| D-107 | Which routes may interline when the rules do not say? | encoded | nothing |
| D-108 | What is an hours budget? | unanswered | `ui-service-design` questions 2, 18 (hours against budget) |
| D-109 | What does a suggested blocking change? | encoded | nothing |
| D-110 | Which waits does a recovery requirement judge, and after which trip? | encoded | nothing |
| D-111 | When does a dated service read as a weekly rule? | encoded | nothing |
| D-112 | Which weekly service types does a fork carry? | encoded | nothing |
| D-113 | Where does the sync store live in production? | decided | nothing |
| D-114 | What does the clock answer in the hour before a fall-back day's anchor? | encoded | nothing |
| D-115 | Must a document be readable without the app? | decided | nothing |
| D-116 | Which managed object store does a deployment run on? | unanswered | the deployment runbook in `store-durability` |
| D-117 | Who deletes from the store, and when? | decided | nothing |
| D-118 | Where does a server read its inputs at boot? | decided | nothing |
| D-119 | What does the server serve when the store is unreachable? | decided | nothing |
| D-120 | Does compaction drop a record whose dates have passed? | unanswered | nothing |
| D-121 | Does the store keep the edit document whole per commit, or record by record? | unanswered | the edits layout in `store-durability` |
| D-122 | Is the dispatcher's day in scope? | decided | nothing |
| D-123 | Do trip, block, and run ids survive from one pick to the next? | unanswered | nothing |
| D-124 | Does this repo publish its cancels and detours as GTFS-rt? | decided | nothing |
| D-125 | Does an edit record carry its source system's id? | unanswered | nothing |
| D-126 | Is the runs layer shaped by TODS? | decided | nothing |
| D-127 | Where does operations history come from? | decided | nothing |
| D-128 | Does this repo produce TIDES? | decided | nothing |
| D-129 | Where does the authored GTFS-rt reach riders? | unanswered | publishing in `rt-publish` |

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
Re-opened 2026-10-30 by QAF-90: on 071 an evening trip added by
`set_headway` runs 20 min beside trips that run 28. The user, the
same day: "traffic pattern[s] change throughout the day and layover
needs, reliefs require flexibility. so please don't make a rigid
timetable setup." That points to profiles per time band, which is
the second option. It stays provisional until the user confirms it
and names the band rule.
Update 2026-11-05: the user, on timing profiles: "we need to rethink
timing profiles all together". Neither option stands; it is a
design conversation with D-44, not a choice among these.
Blocks: QAF-90.

### D-11 — Does editing a shared alignment change every pattern on it?
State: Decided.
Answer: 2026-11-05 — the edit names which: the gesture says whether it redraws one pattern or every pattern on the alignment, and reports the patterns it touched. Nothing moves silently.
Built: author `apply` `draw_alignment {pattern, points, redraw: one|all,
id?}`; `one` mints the pattern its own alignment (reported in
`created`), `all` redraws the shared one; the redrawn patterns come
back in `touched`, and their stop distances keep their meters but
stop claiming to be published (they were measured along the old
line). Pins: `crates/transit-network/tests/alignment_edit_law_test.rs`,
`crates/cap-author/tests/dispatch_test.rs`
(`a_redraw_reports_the_patterns_it_touched`). Reading, encoded and
not confirmed by the user: the line `one` leaves behind when no other
pattern ran it stays in the document (an alignment no pattern runs
still stands; no edit removes one yet), so the export writes it as an
unused shape.
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
Blocks: nothing.

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

### D-44 — Is a branch of a direction its own thing?
State: Unanswered.
Context: QAF-63. GTFS direction_id groups branches, not only
variants: C-TRAN's 006 direction 1 holds a Fruit Valley pattern and
a VA Medical Center pattern leaving at the same minutes all day,
and the import puts both in one direction, named by the majority
headsign. A planner reads them as two branches; the timetable shows
one direction with two variants (D-22).
Options:
- The model stays as it is: a planner who wants two branches
  authors them (set_direction + update_pattern). The finding becomes
  documentation.
- The import splits a direction whose patterns end at different
  terminals into one direction per terminal.
- The model names branches inside a direction (a pattern group with
  its own name and timetable), and the timetable can show one branch.
Update 2026-11-05: the user — the model has to handle a branch, so
leaving it as it is is out. GTFS `direction_id` has no meaning at
C-TRAN beyond telling one route's directions apart; it is not
consistent across routes, so nothing may read it as inbound/outbound.
Naming a direction after its majority headsign is a bad pattern. "All
this needs to be rethought" — a design conversation with D-3 (and
D-22, D-12), not a choice among these options.
Blocks: QAF-63.

## Analysis and live

### D-6 — Does "frequent" count terminal arrivals?
State: Decided.
Answer: 2026-11-05 — departures only, for both tests, so a terminal where nobody boards is not frequent.
Context: QAF-78. The whole-day "frequent" test counts every visit
to a stop, arrivals included, so a terminal where nobody boards can
read as frequent. The windowed test counts departures only.
Options: departures only, for both tests (the finding reads this
way); or keep counting arrivals and document the difference.
Pins: `crates/transit-analysis/tests/coverage_law_test.rs`
(`a_terminal_nobody_boards_at_is_never_frequent`),
`crates/transit-analysis/tests/regression_test.rs`
(`service_type_ops_expose_the_design_axis`: frequent_pct 100/180).
Blocks: nothing.

### D-43 — What does a coverage comparison report?
State: Encoded provisionally.
Answer: 2026-11-05 — per representative DATE was the first answer
("for now, I am not 100% sure on this one"). Withdrawn the same day:
asked which dates count as representative, the user said "none",
consistent with D-55 (no date stands in for a combination of service
types). The unit of comparison is the combination of service types
with the dates it holds, gained and lost population reported
separately (served and frequent). Not decided: how a combination in
one plan matches the other's when a fork renames or splits a type
(by id, or by the days it runs, and what a partial overlap does).
Context: QAF-20. Comparing two plans' coverage takes two
type_coverage answers and a host subtraction, and a net change hides
churn: a proposal can serve 2,000 new people and drop 1,900 others
for a net +100.
Options:
- Per service type (matched by id across the two documents): both
  documents' served and frequent shares, and the population gained
  and lost — served and frequent separately.
- The same per representative DATE instead of per service type
  (a fork that renames or splits a type still compares).
- Net change only (the two shares and their difference).
Blocks: the matching rule above for the coverage half of QAF-20.

### D-47 — Which layers does coverage take?
State: Decided.
Answer: 2026-11-05 — a general weighted layer, polygons or points (the
third option, as recommended). The system is not opinionated about how
a planner weights a population layer: a planner imports several
layers, each with a label giving its analytical meaning (population,
jobs, zero-car households, ...). Areal weighting applies to polygon
layers; point layers stay valid, with their resolution stated in the
answer.
Context: QAF-18 and QAF-28. Coverage takes one weighted POINT layer
and calls the weight population. A tract's people sit at its
centroid, so a rural tract reads unserved where its developed part
has service, and the radius swings the answer by 20 points (28.9%
at 500 m, 49.1% at 800 m). Jobs, seniors, and zero-car households
are the usual companions; no such layer exists in the repo.
Options:
- Areal layers: polygons with a weight, served share by the share of
  the area within the radius (block groups or blocks for population).
- Point layers only, at a finer resolution (block centroids), with
  the resolution stated in the answer.
- Either, and the vocabulary becomes "a weighted layer" (any field:
  population, jobs, households).
Pins: `crates/transit-analysis/tests/coverage_law_test.rs` (areal
weighting conserves the layer, serves a polygon by the share of its
area inside the radius, a point layer states its resolution, any
numeric property is a weight), `crates/cap-analysis/tests/dispatch_test.rs`
(the load's `fields`, the answer's `layer` reading), the caps smoke,
`web/e2e/ui-service-design.spec.mjs` (a polygon layer read by `jobs`).
How a layer is read and sampled: D-97, D-98.
Blocks: nothing.

## Service types

### D-8 — Do timetable edits act on the whole service family?
State: Decided.
Answer: 2026-11-05 — gestures stay per service type, and the UI warns when the family has siblings. No "same as, except" relation for now.
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
Pins: `web/src/components/GesturePanel.svelte` (the panel names the
one type it changes; the warning, D-95), `web/e2e/ui-service-design.spec.mjs`
("Changes the trips of"). QAF-65 is deleted; QAF-44 stays open for
what no warning reaches (D-95).
Blocks: nothing.

### D-94 — Which types make a combination, and how are its dates folded?
State: Encoded provisionally.
Context: D-55 makes a service group a combination of the types that
run together, shown with its dates. Three readings were needed to
build it:
- **Members carry trips (encoded).** A type that keys no trip runs
  no service, so it is in no combination (C-TRAN's FARES_NYE, the
  fixture's HOL). Otherwise a fare-only calendar would split every
  combination it overlaps into two rows with the same numbers.
- **Spans (encoded).** The dates are listed whole (`dates`) and folded
  into spans: a new span starts where the next date is more than
  seven days after the last, so a weekly pattern reads as one span
  and a period that comes back later as another. Alternatives: no
  folding (the planner reads the list), or calendar-month periods.
- **Order (encoded).** Most days first, then the first date, then the
  id — the first row is the service that holds most often.
  On C-TRAN the every-other-week overlays (1-WKDY/15, /16) hold on
  dates fourteen days apart, so their combinations read as a list of
  one-date spans (1-WKDY + 1-WKDY/15 + 1-WKDY/31: seven Tuesdays) —
  honest, but a planner may want "every other Tuesday".
- **A dated exception is its own combination** (D-55's reading): it
  falls out of the construction — a date's members are what runs that
  date.
Pins: `crates/transit-network/tests/combination_law_test.rs`.
Blocks: nothing (a different answer changes `service_combinations`
and its laws).

### D-95 — Which service type does a gesture change, and what does its warning name?
State: Encoded provisionally.
Context: D-8 keeps gestures per service type and has the UI warn when
the type has siblings; D-55 replaced the family with combinations, so
"sibling" needed a reading, and a combination of several types needed
one of them picked for the gesture.
- **Encoded:** the gesture panel picks one member of the selected
  group (the first, sorted, by default — C-TRAN's base sorts first)
  and says it changes that type's trips only. A sibling is a type
  sharing a combination with it: the warning names the group's other
  members (their trips stay as they are) and the other groups the
  edited type runs in (the change reaches them too).
- Not covered: a type that COPIES another's trips without ever
  running with it (4-NYE carries 1-WKDY's trips for one day; they
  share no date, so they share no combination) — QAF-44 stays open.
Pins: `web/src/components/GesturePanel.svelte`, `web/e2e/ui-service-design.spec.mjs`.
Blocks: nothing.

### D-41 — What carries a service type's calendar into a fork's period?
State: Decided.
Answer: 2026-11-05 — the fork does it: `set_plan` with a period end extends every weekly service type to the period's end. Reading, not confirmed by the user: the end date is the fork's own period end, and dated exceptions (4-NYE and other holidays) are not carried forward; the planner adds next year's deliberately.
Built: `set_plan {plan, through?}` — a weekly type (a flagged day and
a range) still running in the calendar's final week (D-112) whose last
range ends before `through` ends on it; dated-only
types and every exception stay. Where `through` comes from is D-104.
Pins: `crates/transit-network/tests/plan_period_law_test.rs`,
`crates/cap-author/tests/dispatch_test.rs`
(`changes_chain_by_hash_and_report_the_result`),
`web/e2e/ui-service-design.spec.mjs` ("a fork past the calendar carries
its weekly service through its period"); on C-TRAN, once the import
reads its dated calendars as weekly (D-105), see D-105's pins.
Context: QAF-54. A fork picked after the import's calendar end
(2027-01-09) runs nothing until every service type's date range is
extended by hand — 13 set_service_range edits on C-TRAN. Holiday
types (4-NYE and others) are dated exceptions, not ranges.
Options:
- A gesture, extend_service_ranges {through}: every weekly service
  type whose last range ends before `through` ends at `through`;
  dated-only types are untouched.
- The fork itself (set_plan with a period end) extends every weekly
  type to the period's end.
- No gesture: the warning names the problem, the planner extends by
  hand.
Open inside the first two: which date the period ends on, and
whether next year's holidays (new dated exceptions) are part of it.
Blocks: nothing (QAF-54 is free to start on D-105).

## Service design

### D-51 — What does a revenue hour, a revenue mile, and a vehicle cost?
State: Decided.
Answer: 2026-11-05 — no dollars. "The anchor for a planner is actually
operating/platform hours, not revenue hours. planners generally don't
do $$$, they have an hours budget. they do look at total paid hours
when it comes to run cutting. but the operators also vary in their pay
rates due to seniority, etc. it doesn't make sense to cost in
dollars." So no cost-model document and no rates: a proposal is
measured in platform (operating) hours against an hours budget, with
revenue hours and miles as secondary figures, and total paid hours
come from the runs layer (D-50) at run cutting. What an hours budget
is: D-108.
Context: `docs/units/ui-service-design.md` questions 2 and 18. The
ops answer revenue hours and miles, and the fleet answers peak
vehicles. A proposal's cost in dollars needs the agency's rates, and
the feed carries none. A rate the UI multiplies in is a policy value
in the host.
Options:
- A cost-model document on the bus (rates per revenue hour, per
  revenue mile, per peak vehicle, optionally per service group),
  read by the ops that answer cost.
- Rates as op parameters with no default: the host passes what the
  planner types, and no dollar figure appears until they do.
- No dollars: the UI answers hours, miles, and vehicles only.
Blocks: nothing (the dollar cost line in `ui-service-design` questions 2 and 18 is dropped; the hours line waits on D-108).

### D-108 — What is an hours budget?
State: Unanswered.
Context: D-51 — a planner measures a proposal in platform hours
against an hours budget, not dollars. The ops answer platform,
revenue, and layover hours per day (`fleet_summary`) and per service
type; nothing on the bus holds a budget to measure them against.
Options:
- A number the planner types per service group or period, passed as
  an op parameter (no document; the answer is hours against it).
- A field of the plan's metadata (each pick carries its budget).
- A budget document on the bus (per service group, period, or pick),
  read by hash like the work rules.
Blocks: the hours-against-budget line in `ui-service-design`
questions 2 and 18.

### D-52 — How is a route-hour's frequency judged and drawn?
State: Encoded provisionally — the third option: the grid shades
continuously by the longest gap (full at 10 min or less, none at 75
or more — drawing, not a standard) and prints the longest and
shortest gap in the cell; no boundary judges an hour. Pinned by
`web/src/components/FrequencyGrid.svelte`; the op (`route_frequency`)
answers the gaps and carries no threshold, so a decided standard
becomes an op parameter without changing the answer.
Context: `docs/units/ui-service-design.md` question 4. The frequency
matrix shades each route-hour. The trips vary around a target on
purpose (traffic, layover, reliefs: the user, 2026-10-30), so an
hour has a typical gap, a shortest, and a longest, and no one
number. Which of them the shading reads, and any boundary on it,
is a threshold, so the op owns it, as a parameter with a default.
Coverage already takes `frequent_secs` (default 900).
Options:
- Shade by the longest gap (what a rider can wait), with standards
  as parameters (the frequent-network and base headways).
- Shade by the typical gap, and mark an hour whose longest gap
  breaks the standard.
- No boundaries: continuous shading of the longest gap, with the
  range printed in the cell.
Blocks: nothing (the grid's shading is provisional).

### D-53 — What ridership data does the planner bring?
State: Decided.
Answer: 2026-11-05 — none for now: no ridership document and no
ridership layer. The user: "we would want to build ridership in, but
it is something to address later." The question stays a recorded data
gap, a planned feature and not a rejected one; the document-or-layer
shape (the options above) is decided when it is taken up.
Context: `docs/units/ui-service-design.md` question 10. Productivity
(boardings per revenue hour) is a planner's first measure, and
nothing on the bus carries a boarding. The GTFS-Ride extension and
APC exports carry boardings per stop and per trip. Adherence history
is `docs/ROADMAP.md` 3.
Options:
- A ridership document on the bus (boardings and alightings per trip
  and stop, by date or averaged per service group), imported from
  GTFS-Ride or an APC export.
- A weighted point layer (boardings per stop), read the way coverage
  reads population (D-47).
- None for now: the question stays open as a data gap.
Blocks: nothing (question 10 stays a recorded data gap).

### D-54 — What does a frequency change keep of the trips already there?
State: Unanswered.
Context: QAF-90 and the user's direction of 2026-10-30, against rigid
timetables: a route's trips are adjusted around a target through the
day, for traffic, layover, and reliefs. `set_headway` moves every
one-start trip in the window to the nearest slot of an exact grid,
which erases those adjustments, and times new trips by the pattern's
one profile (D-3).
Options:
- A target with a tolerance: trips whose gaps already fall within
  the tolerance keep their times. Trips are added only where a gap
  exceeds target + tolerance and removed only where gaps fall short
  of target − tolerance, and a new trip goes in the middle of the
  gap it fills.
- Add and remove only: existing trips never move, and new trips
  split the longest gaps until the target is met.
- Keep the exact grid as one option among these, chosen per change.
Open in every option: which running time a new trip takes (D-3),
and whether a hand-nudged trip is ever moved by a later target.
Blocks: QAF-90, the service-design question 16 (the frequency
gesture).

### D-55 — Which members does a service group count on a day?
State: Decided.
Answer: 2026-11-05 — a group is not a base type plus its calendar subsets.
"What you're really seeing is exceptions to regular service. c-tran
typically doesn't have that many, but this current period had a
significant disruption to manage around. what a planner needs to be
able to see is the actual representative combinations and on which
dates they hold. so if they don't overlap on the same day, they
shouldn't be shown together. they aren't *really* the same group." So
a group is a combination of service types that run together on the
same dates, shown with the dates on which it holds; members that never
share a day are never pooled. This is the second option, with the
period boundaries derived from the dates the combinations actually
hold. The user, on the representative day: "i don't know that a
'representative day' is useful" — a combination's numbers describe the
service it runs, and its dates say when it holds; no day stands in for
it. Then, on the code's `representative_date` (the first date a type's
rule runs): "it shouldn't say representative day if it is anchoring
to a specific date. if it just means 'this is WDKY-1 + WKDY/30'..
then yeah that is fine" — a combination named by its members is fine;
a date standing in for it is not (QAF-97). Reading, not confirmed: a
dated exception (a holiday, a disruption day) is its own combination.
Context: QAF-91 and QAF-65. A group pools a base service type and
the types whose calendars are subsets of it. C-TRAN's 1-WKDY/30 and
1-WKDY/31 are both subsets of 1-WKDY, but they run in successive
periods and never share a day, so pooling both on the representative
day counts trips that never run together (2130 against 1774).
Options:
- Count only the members that run on the group's representative day
  (a real day's service). A member that runs in another period shows
  as the group's other period.
- Split a group into periods wherever its members' date ranges
  change, each period with its own representative day and numbers.
- Count every member, as now, and say so in the answer (a total over
  the members' authored service, not a day).
Pins: `crates/transit-network/tests/combination_law_test.rs` (the
partition, never pooled apart, successive overlays as separate
combinations), `crates/transit-analysis/tests/analysis_prop_test.rs`
`service_groups_are_the_combinations` (a combination's numbers equal
`network_stats` and `hourly_trips` on every one of its dates), and the
ops that take `members` (service_type_detail, service_type_compare,
schedule_texture, route_frequency, directory timetable). QAF-88,
QAF-91 and QAF-97 are deleted. Measured 2026-11-06 on the C-TRAN
import `sha256:2beddcb3…` (QA-FINDINGS' reference): analysis
`service_groups` answers eleven combinations — 1-WKDY + 1-WKDY/31
(49 days, 2026-10-12 → 2027-01-08, 1773 trips) and 1-WKDY + 1-WKDY/30
(17 days, 2026-09-14 → 2026-10-09, 1774 trips) instead of one
2130-trip weekday; no row pools /30 with /31; 4-NYE + 4-NYE/31 is one
row (FARES_NYE carries no trips); directory `timetable` 071 "0" on
1-WKDY + 1-WKDY/31 answers 57 trips with no start twice (758141 at
07:12 alone). Coverage answers a combination too (QAF-89, closed
2026-11-06): analysis `type_coverage` and `unserved_points` take
`members` and pool their authored service with no date — on C-TRAN
(import `sha256:c6847464…`, `clark-population.geojson`, radius 400 m,
frequent 900 s, min departures 2) 1-WKDY + 1-WKDY/31 serves 18.6%
(96,152 people, 886 stops) and 3.1% frequently (15,882, 82 stops), its
91 unserved places the remaining 420,807; 1-WKDY alone runs on no date,
so it answers null. Pins: `crates/transit-analysis/tests/coverage_law_test.rs`
(`a_combination_covers_what_each_of_its_dates_covers`),
`analysis_prop_test.rs` (`type_coverage_agrees_with_coverage`,
`unserved_points_match_the_radius_oracle`), `web/e2e/ui-service-design.spec.mjs`
("hubs and coverage answer around a station and a layer").
Blocks: nothing.

### D-127 — Where does operations history come from?
State: Decided.
Answer: 2026-11-06 — from TIDES, read as a source: "works well as
consumption for service design." The history itself is out of scope.
Context: `docs/ROADMAP.md` 3 closes the loop from operations back into
planning, and planned to accumulate adherence history from the rt
feeds itself. TIDES (the Transit ITS Data Exchange Specification,
tides-transit.org) is the standard for that history, as Frictionless
table schemas: event tables (`vehicle_locations`, `passenger_events`,
`fare_transactions`) and summaries built from them (`trips_performed`
per trip and service date, `stop_visits` per trip, stop, and service
date, `station_activities`), joined to GTFS by service date and trip
and stop ids. AVL, APC, and fare vendors produce it. A service date
resolves its pick: no pick lands or moves before today and only one
not yet in effect retires (D-75, D-76), so (service date, trip id)
names one trip. A day's summaries are written once and never change,
so an import is a document like any source. TIDES's boardings are also
the shape D-53's ridership takes when it is taken up.
Options:
- **TIDES as a source; the history stays with the systems that own it
  — decided.** An import turns a span of `stop_visits` and
  `trips_performed` into a document the analysis ops read; service
  design compares observed running times and adherence across picks by
  pattern and time band, not trip id (D-123).
- The repo accumulates its own history from the rt feeds.
Blocks: nothing (`docs/ROADMAP.md` 3 carries the work).

### D-128 — Does this repo produce TIDES?
State: Decided.
Answer: 2026-11-06 — no: "don't produce TIDES."
Context: the one candidate was `trips_performed` rows for this repo's
own cancels, where it is the system of record for a trip that did not
run. Those reach adjacent systems as the edit document (and GTFS-rt if
D-124 says so); the AVL vendor's TIDES records what ran.
Options:
- **Consume only — decided.**
- Produce `trips_performed` for cancels and added trips.
Blocks: nothing.

## Blocks and runcutting

### D-9 — Where does required recovery time come from?
State: Decided.
Answer: 2026-11-05 — a policy document (minutes and/or a percentage of run time, per route or service type) wins; the smallest layover seen at that terminal is the fallback where no policy applies.
Context: QAF-62. Imported layovers are the whole observed gap
between two trips, so any delay reads as infeasible. Separating
required recovery from slack needs a rule the feed does not carry.
Options: a policy document (a minimum in minutes, a percentage of
the run time, or both, per route or per service type); a value
derived from the feed (for example, the smallest layover seen at
that terminal); or both, with the policy winning.
Pins: `crates/transit-network/tests/recovery_law_test.rs` (the floor,
the policy winning both ways), `crates/transit-runs/tests/recovery_view_test.rs`
(short exactly where validation says layover_infeasible). The policy is
the work-rule document's `recovery` (D-50); which waits each
requirement judges is D-110.
Blocks: nothing.

### D-16 — Where does the runs layer live?
State: Encoded provisionally.
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
Pinned by: `docs/ROADMAP.md` 7, `docs/units/ui-runcut.md`,
`crates/transit-runs` (the work-rule document is its own kernel kind,
`work_rules`, read by hash; nothing in the network document carries
operator work).
Blocks: nothing.

### D-46 — How many more vehicles does a proposal need?
State: Decided.
Answer: 2026-11-05 — automatic blocking is the answer, but "blocking
is the beginning of the run cutting process": it is not a naive
standalone op on a proposal. It belongs to the runcut capability and
is tunable by the policy (minimum layover from D-9, interlining across
routes, and the rest), and its blocks are suggestions the planner
adopts or edits, showing the slack they leave. A peak-vehicle estimate
falls out of it. Interlining is set per route pair
(the user, same day), not once per agency.
Context: QAF-3. A proposal's added trips are unassigned (no block),
so fleet_summary cannot say how many more vehicles it needs. A
vehicle need is a block assignment, which the model does not make,
and an automatic one needs a minimum turn time (D-9).
Options:
- An automatic blocking op over a proposal (chain trips terminal to
  terminal under a minimum layover), answering the blocks it would
  make; the planner adopts them or not.
- A peak-vehicle estimate from concurrent trips plus a recovery
  allowance, with no blocks made.
- The planner blocks by hand (add_block_trip) and fleet_summary
  answers; the finding becomes a UI affordance.
Pins: `crates/transit-runs/tests/suggest_law_test.rs` (every
unassigned trip once; each wait at one place, on an interlined pair,
at least the required recovery; the edits adopt into a document that
validates clean with the peak named), runcut `suggest_blocks` (the
`work_rules` gate in the justfile). What a suggestion changes is
D-109; which pairs interline with no rule is D-107.
Blocks: nothing.

### D-50 — What are the work rules a run is cut under?
State: Decided.
Answer: 2026-11-05 — a generic policy document: the common rules
(maximum platform and spread, report and pull, minimum and maximum
piece, meal breaks, relief points, pay), each optional, with an
agency's values as data in it. Nothing is coupled to C-TRAN; the
blocking settings of D-46 sit in the same document.
Context: QAF-11, QAF-12, QAF-14, and the rest of QAF-15. The runs
layer (D-16: beside the network) needs a work-rule policy document,
and its shape is the agency's rules: maximum platform and spread,
report and pull times, minimum and maximum piece, meal breaks,
relief points, pay (guarantee, spread premium, overtime). The
hand-cut menu in QAF-11 assumed 8 h maximum platform, 15 + 15
report and pull, and a 2 h minimum piece.
Options:
- The user supplies C-TRAN's rules (or the labor agreement's
  section), and the policy document is shaped to them.
- A generic policy document (the common rules above, each optional)
  lands first, and C-TRAN's values are data in it.
Pins: `crates/transit-runs/tests/work_rules_test.rs` (every rule
optional, the round trip, the most specific rule, order is not
identity, the refusals); the kernel kind `work_rules` (load 269). The
runs cut that reads the run rules is `docs/units/runs-layer.md`.
Blocks: nothing.

### D-106 — Does a recovery rule judge a trip that continues with no wait?
State: Encoded provisionally.
Context: C-TRAN's import chains many trips at the same stop with
little or no gap (a route that loops, or a trip continuing as the
next): under a flat rule of 5 minutes or 10 % of the run, the base
import reads 292 waits short, every one on the same route at the same
stop, gaps 0–240 s (author `apply` with `work_rules_hash`, 2026-11-06,
network `sha256:2beddcb3…`). Without rules, none (the terminal's floor
is what the data shows).
Options:
- **Every adjacent wait (encoded).** A rule judges every wait it
  applies to; a planner scopes it (a route's own rule of 0 for a loop
  route — the most specific rule answers).
- A through-run is exempt: a wait of 0 s at the same stop on the same
  route is a continuation, not a recovery point.
- A rule field that names its exemptions (through-runs, loops).
Pins: `crates/transit-network/tests/recovery_law_test.rs`
(`a_policy_requirement_wins_over_the_floor`).
Blocks: nothing (a different answer changes `required_recovery`).

### D-107 — Which routes may interline when the rules do not say?
State: Encoded provisionally.
Context: D-46 sets interlining per route pair. A suggested blocking
needs an answer for a pair the rules do not list, and for no rules at
all.
Options:
- **An allow-list (encoded).** A route always follows itself; a
  listed pair (either order) may interline; an unlisted pair never;
  no work rules, no interlining.
- A deny-list: every pair may, unless listed.
- A document-level default with per-pair exceptions.
Pins: `crates/transit-runs/tests/work_rules_test.rs`
(`interlining_is_per_route_pair_either_way`, `every_rule_is_optional`),
`crates/transit-runs/tests/suggest_law_test.rs`
(`routes_interline_only_where_the_rules_allow_it`).
Blocks: nothing.

### D-109 — What does a suggested blocking change?
State: Encoded provisionally.
Context: D-46 makes blocks suggestions a planner adopts or edits. A
suggestion could leave authored blocks alone, extend them, or re-block
the day.
Options:
- **Unassigned trips into new blocks (encoded).** Authored blocks are
  kept as authored; the trips no block carries are chained by start,
  each after the vehicle that leaves it the least slack where it
  starts. A vehicle waits at one place: the same stop or a bay of the
  same station (there are no deadheads yet). A frequency trip is not
  chained (named as unblocked). Adoption is add_block, add_block_trip,
  and set_layover (each wait authored as its gap).
- Also append to an authored block's idle end.
- A full re-blocking of the day, authored blocks included.
Pins: `crates/transit-runs/tests/suggest_law_test.rs`
(`authored_blocks_are_kept`, `a_proposal_names_the_vehicles_it_adds`).
On C-TRAN (2026-10-27), proposal A's 9 added 019 trips suggest 3
blocks and add no vehicles to the 77 peak (16:50): they run in the
morning.
Blocks: nothing (the rest is `docs/units/runs-layer.md` step 5).

### D-110 — Which waits does a recovery requirement judge, and after which trip?
State: Encoded provisionally.
Context: D-9 says where the requirement comes from, not which waits it
judges. A block may pair trips with no authored layover between them
(the import authors none between trips of different service types),
and a percentage needs a run to be a percent of.
Options:
- **(encoded)** The requirement follows the trip BEFORE the wait —
  its route, service type, and run time (the percentage is of that
  run) — at the place the vehicle waits (the layover's `at`, else the
  stop that trip ends at). A work rule judges every pair of adjacent
  trips; the terminal's floor judges only a pair carrying an authored
  layover (a pair without one has no recovery point in the data), and
  a pair without one and with no rule is judged for order only.
- The floor judges every adjacent pair (imported blocks across
  service types would then fail validation).
- The requirement follows the trip after the wait.
Pins: `crates/transit-network/tests/recovery_law_test.rs`
(`a_policy_judges_a_pair_with_no_authored_layover`,
`the_policy_is_asked_about_the_trip_before_and_where_it_waits`).
Blocks: nothing.

### D-126 — Is the runs layer shaped by TODS?
State: Decided.
Answer: 2026-11-06 — yes: the runs layer is built on TODS's concepts,
and GTFS + TODS is its interchange.
Context: TODS (the Transit Operational Data Standard, v2.1.0,
tods-transit.org) extends GTFS with what operating service needs:
supplement files that add, overwrite, or delete GTFS rows by key
(`TODS_delete`) — deadheads and other non-revenue trips, internal
stops and waypoints, crew service ids — and its own files:
`run_events` (a run's trips and other activities in order, keyed by
`service_id`, `run_id`, `event_sequence`; a run id is unique within
its `service_id`), `employee_run_dates`, `vehicles`, and
`vehicle_assignments`. It is internal data, usually unpublished, and
the format scheduling systems hand to CAD/AVL. The runs layer
(`docs/units/runs-layer.md`) is not built yet, so its model can take
TODS's concepts now and its codec stays lossless, as transit-codec is
for GTFS. Law 1 holds: TODS is a serialization at the boundary; the
runs layer's documents are the model. The dated assignment files
(`employee_run_dates`, `vehicle_assignments`) are the dispatcher's
day (D-122): an adjacent system's rows, which a bid's roster may
feed.
Options:
- **TODS's concepts in the runs model, a GTFS + TODS codec at the
  boundary — decided.** An export holds employee ids, so it is served
  only under policy (D-58..D-61), never beside the public `/feed`.
- A runs model of the repo's own, with TODS a later translation.
Blocks: nothing (`docs/units/runs-layer.md` carries the work).

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

### D-40 — How long does a planned trip take, and from when?
State: Decided.
Answer: 2026-11-05 — door to door: the itinerary carries `leave_secs` (the first ride's boarding minus the walk before it) and `duration_secs` (arrival minus leave); "leave in N min" uses the board's `wait` rule (D-24). "That matches what other trip planning services do today."
Context: QAF-81. An itinerary carries depart_secs (the moment the
plan was asked for) and arrival_secs; a walk leg carries only its
seconds, so when the rider must leave is host arithmetic, and so is
"takes 23 min". A board row already says how long until it leaves
(`wait`, D-24).
Options:
- Door to door: `leave_secs` (the first ride's boarding minus the
  walk before it — when the rider must set out) and `duration_secs`
  (arrival minus leave), plus the board's `wait` rule for "leave in
  N min".
- From the moment asked: one `duration_secs` = arrival minus
  depart_secs (it counts the wait before leaving).
- Both readings, named.
Pins: `crates/transit-router/tests/door_to_door_law_test.rs` (leave,
duration, and the wait rule, leave-at and arrive-by over generated
feeds and places); `web/e2e/ui-ride-now.spec.mjs` ("Leave in 7 min",
"11 min"); C-TRAN (import `2beddcb3…`) router `plan` 106 → 110 on
2026-10-29 asked at 08:00 — a 35 s walk to a 08:15 boarding: leave
29665, 4663 s door to door, leave in 14 min. How "leave in" reads on a plan not asked for now is D-92.
Blocks: nothing.

### D-42 — When a route has stopped for the day, how far ahead does a stop look?
State: Decided.
Answer: 2026-11-05 — look ahead day by day for up to a week, each day's in-force edits read and never the live feed; say nothing past a week. "A week sounds good to me."
Context: QAF-82. grouped_board keeps a route and destination whose
last departure has left, with no departures. The display says "No
more service today" where a rider wants "Next: 5:12 am tomorrow" or
"Next: Monday". The next one is on another service day, which the
board does not ask.
Options:
- Look ahead day by day for up to a week (one weekly cycle), with
  each day's in-force edits; say nothing past a week.
- Tomorrow only.
- Until the calendar ends.
Open inside each: whether a later day's answer reads that day's
edits (yes, if they are in the document) and never the live feed.
Pins: `crates/transit-dispatch/tests/next_service_law_test.rs` (the
first later day that serves the group, patched by that day's
diversions; nothing past seven days; unmoved by the live document;
nothing that has already left); `web/e2e/ui-stop-display.spec.mjs`
("Next: Sunday 9:00 am", "Next: Monday 8:20 am"); C-TRAN (import
`2beddcb3…`) board `grouped_board` 106 on 2026-10-29 from 86000 —
nine of ten groups stopped, each with Friday's first departure and
its bay (164 → Portland 18000 at 9008), the tenth (065) with one
departure left and no next. Only a group the
stop is served by today (the board's own groups) is given a next; a
group with nothing within the week reads "No more service this week".
Blocks: nothing.

### D-48 — Does a stop answer to a name its sign does not carry?
State: Decided.
Answer: 2026-11-05 — fuzzy matching on the stop names now (typos, abbreviations like "Van" for "Vancouver"). No authored aliases: they would be a hand-maintained separate dataset. A geocoder stays a possibility down the road where a deployment runs one (D-36).
Context: QAF-70. directory `stops` matches the feed's names, so
"vancouver mall" finds nothing (the sign says "Van Mall"). Where the
server runs a geocoder (D-36), the everyday name finds the place and
the stops near it; without one it has no answer.
Options:
- Stops carry authored aliases in the network (a stop's other
  names), and `stops` matches them.
- The geocoder is the answer; a deployment without one says how
  stops are named (as the UI does now). The finding closes as
  documentation.
- Fuzzy matching on the stop names (typos, abbreviations like
  "Van" for "Vancouver").
Pins: `crates/transit-network/tests/stop_search_law_test.rs`;
`crates/cap-directory/tests/dispatch_test.rs` (a typo finds the
station); `web/e2e/ui-ride-now.spec.mjs` ("mian 2nd"); C-TRAN (import
`2beddcb3…`) directory `stops` "vancouver mall" answers the 21 stops
"van mall" answers (it answered none), and "fishers landng" finds
Fisher's Landing Transit Center and its bays. What counts as an
abbreviation or a typo, and the order, is D-91.
Blocks: nothing.

### D-91 — What does fuzzy stop search accept, and in what order?
State: Encoded provisionally.
Context: D-48 decided fuzzy matching on stop names (typos, a sign's
abbreviations) and left the rules to the code. Too loose and a short
word on many signs answers everything ("NE" for "newport"); too
strict and "vancouver mall" misses "Van Mall".
Options:
- **Three tiers, listed in order (encoded).** Each typed term must be
  answered. Exact: a substring of the name or the id (the picker's
  typing, as before). Abbreviation: a name word of three letters or
  more that begins the term ("Van" → "vancouver"), or a word that is
  a subsequence of the term with its first and last letter ("St" →
  "street", "Blvd" → "boulevard"; "Dr" does not answer "drive").
  Typo: one edit (substitute, add, drop, swap neighbours) of a name
  word or its start from four letters, two from eight; never under
  four. The stop's tier is its worst term's; exact stops list first,
  then abbreviations, then typos, each by name then id.
- One tier: fuzzy matches interleaved by name (the old order).
- A score (edit distance, prefix length) ranking every stop.
Pins: `crates/transit-network/tests/stop_search_law_test.rs`.
Blocks: nothing (a different answer rewrites `name_match` and its laws).

### D-92 — When does a plan say "leave in N min"?
State: Encoded provisionally.
Context: D-40 makes "leave in N min" the board's wait rule. A rule
needs a "from": the op knows only the moment the plan was asked to
leave at (`depart_secs`), not the clock — a plan asked for 6 pm at
noon, or an arrive-by plan (whose `depart_secs` is the origin's
latest moment), has no "now" in it.
Options:
- **From `depart_secs`, shown for a trip leaving now (encoded).** The
  itinerary's `leave` is the wait from `depart_secs` to `leave_secs`
  on every plan; ride-now shows it only when the rider asked to leave
  now, where `depart_secs` is now. Other plans show their clock
  times.
- The request carries the clock (`now_secs`) and `leave` is from it
  on every plan, arrive-by included.
Pins: `crates/transit-router/tests/door_to_door_law_test.rs`;
`web/src/components/Itinerary.svelte` (`now`).
Blocks: nothing.

## Public comparison

### D-14 — Does the proposal comparison collect public responses?
State: Decided.
Answer: 2026-11-05 — read-only. Responses go through the agency's existing channels and the UI links to them; no response document.
Context: the comparison UI reads published proposals. Collecting
comments or votes on them would add a new shared document, synced
through the server, with its own retention and moderation
questions.
Options:
- Read-only. Responses go through the agency's existing channels,
  and the UI links to them.
- A response document on the bus (a new document kind, synced by
  LWW per record like the edit document).
Blocks: nothing.

## Live operations

### D-15 — What makes a trip "covered"?
State: Decided.
Answer: 2026-11-05 — "you need both": a trip is covered only when a vehicle is on its block AND an operator holds its run. A trip missing either is uncovered, and the screen says which half is missing. Cancel (D-45) also marks a trip uncovered.
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
Blocks: the operator half waits on the runs layer and workforce data (`docs/ROADMAP.md` 7); the vehicle half is free to build.

### D-45 — Which dispatcher edits does the edit document carry beyond the diversion?
State: Decided.
Answer: 2026-11-05 — cancel (whole trip) first: a cancelled trip shows on boards as cancelled and the router never rides it. Partial cancel, cover, hold and swap come later.
Context: QAF-36. The edit document holds diversions only; a
dispatcher's first answer to a 24-minute-late trip (cancel it,
cover it, hold a connection, swap the vehicle) has no vocabulary. A
new edit kind is a sync-document change, and each kind needs a
meaning on every reader: does a cancelled trip leave the board or
show as cancelled, does the router skip it, does the live feed's
own cancellation (SCHEDULED/CANCELED in GTFS-rt) agree with it.
D-73 answers the board's reading for the live feed's cancellations.
D-15 ("covered") depends on the answer.
Options:
- Cancel (whole trip) first; a cancelled trip shows on boards as
  cancelled and the router never rides it.
- Cancel and partial cancel (from a stop on).
- Cancel, cover (another block or vehicle takes it), hold, swap —
  the full set, landed together.
Blocks: nothing.
Pins: `crates/transit-sync` (the cancel payload's laws, the merge
semilattice over both payloads), `crates/transit-dispatch/tests/cancel_law_test.rs`
(the day drops exactly the cancelled instances, the base untouched;
boards list them cancelled, never as rows; reports; `cancel_of`),
`crates/cap-router/tests/dispatch_test.rs`
(`a_cancelled_trip_is_never_ridden`), `crates/cap-divert/tests/dispatch_test.rs`
(`a_trip_cancel_walks_the_edit_document`), `web/e2e/ui-ride-now.spec.mjs`
and `web/e2e/ui-stop-display.spec.mjs` (the cancelled trip on the boards).
The shape is D-100..D-102.

### D-100 — How does a board show a trip the dispatcher cancelled?
State: Encoded provisionally.
Context: D-45 says a cancelled trip "shows on boards as cancelled";
D-73 says a trip the LIVE FEED cancels is dropped silently. The two
sources differ: the dispatcher's cancel is the agency's own decision
in its own document, the live feed's is a report.
Options:
- **Listed among the visits the day's edits took away (encoded):**
  the trip is out of the effective day, so it is no row and takes no
  place among a group's next departures; the board's `skipped` lists
  its visit with `cancel` (the cancel's id) beside `diversion`. Ride
  now lists it under "Cancelled"; the stop display's group says "A
  trip was cancelled" (or "Cancelled" when nothing else is left). The
  live feed's own cancellation stays silent (D-73).
- In place among the rows, marked cancelled (it takes one of a
  group's three places).
- Silent, as D-73 (contradicts D-45's "shows as cancelled").
Pins: `cancel_law_test.rs` (`a_cancelled_visit_is_listed_cancelled_never_a_row`,
`a_group_whose_only_trip_is_cancelled_stays_with_it`), the e2e specs
named under D-45.
Blocks: nothing (a different answer moves the visit into the rows).

### D-101 — What does a trip cancel name, and how does it enter the edit document?
State: Encoded provisionally.
Context: D-45 cancels a whole trip. A trip instance is (trip, start);
a frequency trip's instances share one stored trip. A diversion
spans a date range; the edit document has no version tag inside it
yet (`docs/units/document-versioning.md`).
Options:
- **One trip on one service date (encoded):** `{id, trip_id,
  start_secs, date}` — `start_secs` one instance, null every instance
  the trip runs that day; authored only when the trip's service runs
  on the date. A second payload beside `diversion` on the record
  (exactly one present, the absent one omitted so existing documents
  keep their bytes and hashes), under `schema_version` 1 — a module
  built before it drops a cancel record at read and counts it.
- A date range, like a diversion (a trip cancelled for a week).
- A schema bump to v2 with a tagged payload enum (needs the
  document-versioning migration machinery first).
Pins: `crates/transit-sync` (`a_diversion_record_keeps_its_bytes_and_a_cancel_omits_the_diversion`,
`sanitize_keeps_only_records_with_exactly_one_payload`,
`cancels_are_in_force_on_their_one_date_and_never_retired`),
`cancel_law_test.rs` (`a_cancel_is_authored_only_for_an_instance_that_runs_that_day`).
Blocks: nothing.

### D-102 — Which capability authors a trip cancel?
State: Encoded provisionally.
Context: law 3 (one need per module). cap-divert answered "what does
this diversion amount to?"; its list, reports, retire, restore, and
sanitize already range over every record of the edit document.
Options:
- **cap-divert, its question widened to "what does this operator
  edit amount to?" (encoded):** a `cancel` op beside `propose`; `list`
  answers `rows` (diversions) and `cancels`; `reports` validates both.
  The crate keeps its name. The next edit kind (partial cancel,
  cover, hold, swap — D-45's later set) re-judges the cut: a module
  whose ops split into diversion drafting and the edit document's
  envelope is the split trigger.
- A new capability for cancels (its own crate and manifest entry),
  retiring and listing through cap-divert's envelope ops.
- Split now: the edit document's envelope (list, reports, retire,
  restore, sanitize) in one capability, each edit kind's drafting in
  its own.
Pins: `crates/cap-divert/tests/dispatch_test.rs`; the manifest's
divert entry (`cancel` op and worked example).
Blocks: nothing.

### D-73 — What does a stop's board show for a trip the live feed cancels?
State: Decided.
Answer: 2026-11-04 — "Drop it silently" (asked on the stop display
showing a cancelled 34 at Fisher's Landing as leaving in three
minutes). A departure the live document says does not happen — its
trip instance CANCELED or DELETED, or the stop SKIPPED — is no row on
any board: no "Canceled" mark, no note on the group, and it takes no
place among the group's next three. The group stays while the route
still serves the stop that day. The router follows the same rule: it
never rides a cancelled trip, and never boards or alights at a
skipped stop.
Context: the rt decode dropped GTFS-rt's schedule_relationship, so a
cancelled trip reached the board as an ordinary trip with no
prediction and showed at its scheduled time. The live document now
carries `canceled` (trip) and `skipped` (stop), and every reader asks
`LiveDay::runs`.
Options:
- Show it in place, marked Canceled (it uses one of the three slots).
- Drop it, with a note on the group ("A trip was canceled").
- Drop it silently (encoded).
Blocks: nothing.
Pinned by: `crates/transit-rt/tests/cancel_law_test.rs`; law 6 in
`crates/transit-dispatch/tests/grouped_board_prop_test.rs`; law 5 in
`crates/transit-router/tests/live_law_test.rs`.

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
State: Decided.
Answer: 2026-10-29 — walking: "Also refuse unknown" (a rider walks
every class but motorway, ramp, and unknown — strict); a rider walks
a one-way street both ways ("agreed"); a walk is at most ten minutes
by default, at 1.3 m/s, both op parameters. Pins:
`crates/transit-streets/tests/walk_law_test.rs`.
Answer (a bus): 2026-11-05 — a warning, not a refusal: "a planner
needs to use good judgement. if it is a grey area then it is up to them
to confirm whether it is a legal road or not." Drawing a bus on a
street the data does not clearly allow (an unknown class, a path-like
class) warns and draws. Reading, not confirmed by the user: no class
is refused outright for a bus, and the warning names the class.
Built: streets `route` answers `streets` (the streets a path follows:
class, name, meters) and `to_confirm` (those whose class is in the op's
`confirm` parameter, default D-103's); the path never depends on a
class. Pins: `crates/transit-streets/tests/bus_profile_law_test.rs`,
`crates/cap-streets/tests/dispatch_test.rs`
(`a_route_names_its_streets_and_the_ones_to_confirm`); C-TRAN session
2026-11-06 on the Trapeze roads (`sha256:48fe6c89…`, 112,662 ways):
streets `route` from (45.49755, -122.5900) to (45.49755, -122.5750)
draws 1,194 m — SE 71st Ave (secondary) 12 m, SE Powell Blvd (unknown)
1,166 m, SE 85th Ave (local) 16 m — with `to_confirm` SE Powell Blvd.
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
- **Decided (a bus):** every class may be drawn; unknown and
  path-like classes carry a warning the planner confirms.
Blocks: nothing.

### D-27 — What does place search find?
State: Decided.
Answer: 2026-10-29 — "i can run a pelias container for this project
and have it wire to ride-now" … "wire up pelias with the portland
metro example". Search finds stops (directory `stops`) and whatever
the geocoder answers: Pelias's portland-metro project — addresses
(OpenAddresses, TIGER interpolation), streets and venues (OSM),
and places (Who's On First). Which sources is the geocoder's
configuration, not the model's (D-36).
Context: search found stop names only (directory `stops`). A rider
types addresses ("1400 NE 136th Ave"), intersections ("4th Plain
& Andresen"), and places ("PeaceHealth", "WSU Vancouver"). The
Trapeze roads carry TIGER-style address ranges per side
(`L_F_ADD`…`R_T_ADD`) and street names, so addresses and
intersections need no new source; places do (OSM, or the county's
data), and they are the hardest to keep current.
Options: stops only; + intersections; + addresses
(interpolated along the ranges, or the county's address points);
+ places. **Decided:** stops, and every kind the geocoder answers.
Blocks: nothing.

### D-28 — Where do stop and address results meet?
State: Encoded provisionally.
Context: a rider wants one search box. The host may not merge two
answers (`docs/ARCHITECTURE.md`, "Generated UIs", step 6). With the
geocoder a service (D-36), the stops are an op's answer and the
addresses the geocoder's; neither knows the other.
Options:
- **Encoded:** one box, two lists — the directory's stops, then the
  geocoder's addresses and places under their own heading, each in
  the order its source gave; the arrow keys move through both. The
  "no stop is named" hint shows only when neither answered anything.
- Rank them together: the geocoder's candidates go into an op as
  input (like an rt snapshot) and a capability orders both — the
  oracle promoted to a source (D-38).
- Two boxes (stops, addresses).
Blocks: nothing. Pins: `web/e2e/published-network.spec.mjs`, "ride
now searches addresses through the geocoder".

### D-29 — What area does a geography release cover?
State: Decided.
Answer: 2026-11-05 — whatever the agency's GIS exports: "these datasets are inputs to the system", so the system sets no boundary of its own.
Context: a geography release pins the basemap and the streets
document, built from one source snapshot
(`docs/units/streets-model.md`). C-TRAN runs into Portland. The
Trapeze export already reaches Portland (its first feature is NE
21st Ave, Portland); the local basemap is one extract.
Options: the service area plus a buffer (how far?); the counties
served (Clark, plus Multnomah for the Portland trips); whatever
the agency's GIS exports.
Blocks: nothing.

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
State: Decided.
Answer: 2026-11-05 — freehand only for now: a planned street is drawn freehand on the pattern. A streets edit document stays an option if planners need walking or routing on planned streets.
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
Blocks: nothing.

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

### D-36 — Is geocoding a document on the bus or a service the host calls?
State: Decided.
Answer: 2026-10-29 — "basemap tiles are outside the data model, but
streets are a part of it. what about geocoding/reverse geocoding?
this feels like something that should just be a service i call to"
… "i can run a pelias container for this project and have it wire
to ride-now". The geocoder is an external service: the host asks
it, through the server's `/geocode` proxy, and every op takes the
point it answers.
Context: data belongs on the bus when an op's answer depends on it
— the streets do (walks, the walk table); basemap tiles do not.
A geocoder turns text into a point and a point into a name; from
the point on, every op answers the same whichever way the point
came (typed, tapped, located). The gazetteer that was queued in
`docs/units/streets-model.md` (a document read from the street
layers' names and address ranges) would have rebuilt a geocoder —
interpolation, fuzzy matching, ranking, places — inside a
capability.
Options:
- A gazetteer document on the bus: offline, the same answer on
  every host, and a geocoder to build and keep current.
- **Decided:** an external service (Pelias, `just geocoder`). Costs:
  no address search offline or where the server runs no geocoder
  (stop search, the map, and the location still answer); a rider's
  typed addresses reach whoever runs it (the agency's server, here);
  its answers drift as its data does, so nothing durable keeps a
  query — a trip end keeps the point and the name it was shown with.
  Stops stay the network's: the Pelias project's transit importer
  is not run.
Blocks: nothing. Pins: `server/geocode_test.go` (the proxy forwards
the query as-is, answers 404 with no geocoder),
`web/e2e/published-network.spec.mjs` ("ride now searches addresses
through the geocoder").

### D-37 — What names a spot dropped on the map?
State: Encoded provisionally.
Context: a spot tapped on the map (as a trip's end, or to see what
is near it) has a point and no name; the itinerary repeats its
name ("min walk to …"). The geocoder's reverse lookup names a
point; nearby, it may answer a street, an address, a venue, or a
town far off.
Options:
- **Encoded:** the nearest address, venue, or street the geocoder
  answers within 250 m, else "Dropped pin" (and with no geocoder).
  The name rides into the router's place as its label — an echo no
  op branches on (D-38). On the local
  Pelias (2026-10-29) a venue names a spot well at a transit center
  ("Vancouver Mall Transit Center", 2 m) and poorly elsewhere: a
  sign by the I-5 bridge ("Welcome to Washington", 39 m, where the
  street is East Fourth Plain Boulevard, 71 m) and a store
  ("Advance Auto Parts", where NE Andresen Road is 39 m).
- Always "Dropped pin": the itinerary never depends on the geocoder.
- The nearest of anything, at any distance.
- The nearest address or street only: always a street name, never
  a venue's — and never "Vancouver Mall Transit Center".
Blocks: nothing. Pins: `web/e2e/published-network.spec.mjs` ("an
address typed as a trip's end, and a dropped pin named by the
geocoder"; the no-geocoder specs keep "Dropped pin").

### D-38 — What may a host do with an external service's answer?
State: Decided.
Answer: 2026-10-29 — "makes sense to me", to the proposal: external
things are sources (their answer becomes a document on the bus),
presentation (pixels only), or oracles (an input the user could have
given by hand). A host asks an oracle with the user's input only,
renders its answer as given, and turns the pick into the user's
input (a point, the label shown); it never ranks, filters, merges,
thresholds, or joins the answers — answers that must meet model
data promote the oracle to a source. Ops echo an oracle's label,
never branch on it. The query policy is the deployment's (the
server's policy file, applied by its proxy); the manifest's
`services` lists what a deployment offers; nothing durable keeps a
query or an oracle's id. The rule's home is `docs/ARCHITECTURE.md`,
web section, "External services".
Context: the geocoder (D-36) was the first time a host showed an
answer that is not an op's. Its query policy (layers, radius, size)
was chosen in ride-now's TypeScript — a threshold in TypeScript,
which "Parity" forbids — and the page found it by probing for a
404, a third way of discovering a deployment's offer beside fixed
endpoints and `live-feeds.json`.
Options:
- Leave external services to each host's judgment.
- **Decided:** the three kinds and the oracle contract above.
Blocks: nothing. Pins: `server/geocode_test.go` (the page's own
policy is dropped and the deployment's applied; the manifest lists
the service exactly when configured), `web/e2e/published-network.
spec.mjs` ("ride now searches addresses through the geocoder": the
page sends the text and the focus point only), the `--ask` runner
gate in `just test`.

### D-39 — Do the live feeds move into the manifest's services?
State: Decided.
Answer: 2026-11-05 — the feeds stay in the static `web/public/live-feeds.json`. The departure board must run as plain static files on a CDN with a static config document and no sync server, so a page's default feeds never depend on `/manifest`. If `services` ever carries them, it is a static file the same way.
Context: D-38 makes the manifest's `services` the way a deployment
says what it offers. The deployment's GTFS-rt feeds are still named
in a static `web/public/live-feeds.json` beside the page (read by
ride-now and the stop display; the perf harness and its recorder
read the file too), and a rider may set their own. Moving them
moves where a deployment configures its feeds — server flags or a
file the server reads — and the page would read them from
`/manifest`, so a page served without the sync server has no
default feeds.
Options:
- Move them now: `services.live: {trips, vehicles, alerts}` from
  the server's configuration; `live-feeds.json` is retired.
- Wait for a second deployment to need it; `live-feeds.json` stays.
Blocks: nothing.

### D-49 — Where do the basemap's glyphs and sprites come from?
State: Decided.
Answer: 2026-11-05 — "lets do option 2": a recipe fetches them once
into `web/public/basemap-assets/` (gitignored); the style reads them
locally when present and falls back to the network.
Context: With `basemap.pmtiles` served, the style still
fetches its fonts (glyphs) and icons (sprites) from
protomaps.github.io, so a stop display that loses its uplink loses
every map label. Installs and downloads are the user's job.
Options:
- Vendor them into the repo (the Protomaps font PBFs and sprite
  sheets, a few MB, OFL/BSD licensed), served beside the tiles.
- **Decided:** a recipe that fetches them once into web/public (like
  `just geocoder-build`), gitignored; the style reads them locally
  when present and falls back to the network. `just basemap-assets`
  fetches `protomaps/basemaps-assets` at a pinned commit (the light
  sprites and every font stack, ~14 MB, licenses included);
  `pickStyle` probes for `basemap-assets/sprites/v4/light.json`
  beside the tiles. The assets are an install: a checkout without
  them still draws labels from protomaps.github.io, so it needs an
  uplink for them until the recipe has run.
- Keep the network fetch; the finding is documented as a known
  offline limit.
Blocks: nothing. Pins: `web/e2e/basemap-assets.spec.mjs` (installed:
the style's glyph and sprite URLs are the app's own origin and
nothing is asked of protomaps.github.io; absent: the network
fallback).

## Sync and persistence

### D-56 — Whose clock is the LWW clock?
State: Encoded provisionally.
Context: `transit_sync::merge` orders records by `updated_at`, stamped
by the writer — the client's clock for local edits, the server's
`now_ms` only for the submit transition. A device whose clock runs
fast mints a record that beats every honest record until real time
passes it, and skew can resurrect a retired record. Convergence needs
one clock, and the fix must stay idempotent: at-least-once redelivery
of the same document has to converge to the same hash.
Options:
- Client clocks as they are (the old behavior): broken under skew —
  a future-dated record wins indefinitely.
- **Clamp at receive (encoded).** The receive path sets
  `updated_at = min(updated_at, now_ms)` on every incoming record
  before merging; submit still stamps `now_ms`. A future-dated writer
  is treated as written now, client order survives within honest
  skew, and clamping a clamped value is a no-op, so redelivery stays
  idempotent.
- Re-stamp every record with the server clock at receive: strongest,
  but not idempotent — redelivering the same document would mint a
  new hash, breaking the converge-to-same-hash invariant.
Pinned by: `transit-sync` law test
`a_future_dated_record_is_clamped_at_receive`, cap-sync dispatch test
`sync_edits_clamps_the_client_clock`, and the store's clock-skew
session through a restarted server.
Blocks: nothing (the receive path is built on the encoded answer; a
different answer rewrites the pins above).

### D-57 — How long does a tombstone live?
State: Encoded provisionally.
Context: the edit document grows monotonically — a retirement is a
tombstone kept so deletions propagate through sync, so whole-document
sync never shrinks and every new device receives every retired
diversion. Dropping a tombstone can resurrect a record redelivered
after it, so the horizon must exceed the longest offline redelivery.
Options:
- Never compact (the old behavior): the document grows forever.
- **Compaction commit with a horizon (encoded).** The kind's
  `compact_edits` op drops records that are retired AND whose
  `updated_at` is older than `now_ms - horizon_ms`; the host runs it
  as an ordinary CAS'd commit, audited in the store log. Default
  horizon 90 days (`-retention-days`). With D-56's clamp a
  redelivered record can carry at most "now" as its stamp, so a
  resurrection needs a record older than the horizon arriving after
  compaction — accepted and documented.
- Compaction as a derived view (readers skip old tombstones): no
  resurrection risk, but the stored document and the delta log never
  shrink.
Pinned by: `transit-sync` law test
`compact_drops_only_retired_records_past_the_horizon`, cap-sync
`compact_edits` dispatch test, and the store GC test.
Blocks: nothing (compaction is built on the encoded answer; a
different answer rewrites the pins above).

### D-74 — How is a pick published, and what runs before the first one?
State: Encoded provisionally.
Context: docs/ROADMAP.md 1 — a pick that cannot be published is just
a document. The store holds content-addressed blobs under pointers,
and GC keeps only blobs a pointer's log names; the module is the hash
authority for every bus document; the host makes no domain decision.
Encoded (provisional):
- The published plan set is its own document (store kind `plans`,
  `transit_plans::PublishedPlans`, its `schema_version` inside it per
  D-62): every pick's id, name, effective date and network hash, in
  (effective_from, id) order, one entry per id. It lives at the
  `plans` pointer and the manifest pins it as `plans_hash`. Each
  pick's network lives at its own `plan.<id>` pointer, so every
  revision stays referenced and GC never collects a published network.
- `POST /plans` takes the network in any serialization. cap-network
  registers and validates it (a network with issues never publishes)
  and answers its canonical bytes; cap-plans `publish` answers the
  next set, or refuses an unversioned plan, an undated one, or one
  whose date another pick already has. Republishing an id replaces
  the pick: it is the pick's next revision, and its date may move
  (never into the past, D-76); republishing a retired pick restores it
  (D-75). A plan id that cannot name a pointer is refused.
- A page resolves the pick in force on its local date through
  cap-plans `in_force` and loads that network by hash. Before the
  first pick takes effect, and when none is published, it loads the
  server's `-feed` import, and `GET /feed` serves the `-feed` zip the
  same way. A published pick's boundary is the default one: it is an
  authored document, numbered the way its export is.
Options: the encoded design; picks only as pointers with no set
document (every client lists pointers and resolves over them — no
single hash pins what is published); or the set embedding each
network (one blob per publish, every client downloads every pick).
Pinned by: `crates/transit-plans/tests/published_test.rs`, the
cap-plans and cap-network dispatch tests, `server/plans_test.go`
(`TestThePickInForceAsGTFS` pins `/feed`),
`server/reader_test.go`,
`web/e2e/published-picks.spec.mjs`.
Blocks: nothing.

### D-75 — How does a published pick come back out?
State: Decided.
Answer: 2026-11-05 — "Retire (tombstone)", and "Future picks only":
a retired pick stays in the published set marked
`status: "retired"`, never governs, and republishing its id restores
it. Only a pick that has not taken effect (dated after today, the
host's date) is retired; a pick already in force is corrected by
republishing it. Encoded: `transit_plans::retire`, cap-plans
`retire`, `POST /plans/retire`.
Pinned by: `crates/transit-plans/tests/published_test.rs` (retiring,
took effect, restore, not a chain member, `retired_picks_never_resolve`),
the cap-plans dispatch test `retire_answers_the_next_set_or_why_not`,
`TestRetiringAPick` in `server/plans_test.go`, and "a retired pick
never reaches a rider" in `web/e2e/published-picks.spec.mjs`.
Context: a pick published by mistake, or a service change the agency
cancels, stays in the published set; republishing can move its date
but not remove it. Retirement is the tombstone everywhere else
(AGENTS.md: no hard deletes).
Options:
- A retired pick: it stays in the set marked `status: "retired"`,
  never resolves, and a later publish of the same id restores it.
- Removal from the set: the set's log still names it (the store's
  history keeps every revision), but the current set does not.
- Nothing until a later pick supersedes it (the behavior before).
Blocks: nothing.

### D-76 — What does publishing refuse beyond an invalid chain?
State: Decided.
Answer: 2026-11-05 — "No-service + new past dates": a pick that runs
no service in its period is refused (cap-network `publishable`'s
`refusals`; every other warning still publishes), and a pick that is
new, restored, or moved never lands before today, the host's date
(cap-plans `publish`, `pick_dated_in_past`); a pick republished under
its own date — a correction to the pick in force — always publishes.
"Today" is the server's local date (`-today` pins it on a test
server).
Pinned by: `a_pick_with_no_service_in_its_period_is_refused_publication`
(transit-network), the date laws in
`crates/transit-plans/tests/published_test.rs`, the dispatch tests
`publishable_answers_what_refuses_a_pick` (cap-network) and
`retire_answers_the_next_set_or_why_not` (cap-plans), and
`TestPublishRefusalsNameTheirReasons` in `server/plans_test.go`.
Context: QAF-95 (fixed by this answer). cap-network `validate` warnings never gate
(they name what validity lets through), so a pick whose service
types' calendars end before its effective date publishes with a
`no_service_in_plan_period` warning, and on that date every rider
page loads a network that runs nothing. A pick dated in the past also
publishes and governs at once, changing what the deployment says ran
on past dates.
Options:
- Publish with the warnings in the answer (the behavior before).
- Refuse a pick with no service in its period; publish other
  warnings.
- Also refuse a pick dated before today (the server's clock, the way
  D-56 bounds the LWW clock), or before the pick in force.
Blocks: nothing.

### D-82 — Where do the store's backups go, and how often?
State: Unanswered.
Context: the sync store holds the edit document, the policy, the
published picks, and the manifest's history, as objects in a bucket
(D-113). Every object is create-only and the store never overwrites
one, so a copy taken records-first, then the blobs they name, is
consistent, and blobs copy incrementally by key
(`docs/units/store-durability.md`). Where the copies live and how much
loss is acceptable is the agency's infrastructure.
Options:
- The provider's own: bucket versioning plus replication to a second
  bucket in another zone or site (S3 replication, GCS dual-region,
  Azure GRS): no loss window to run, and a bad write
  replicates too — versioning is what undoes it.
- A periodic copy to a second bucket or provider, hourly or daily: a
  copy the primary's credentials cannot touch; at most that interval
  of edits is lost.
- Both: replication for the site, a periodic copy for the account.
Blocks: backup and restore in `docs/units/store-durability.md`.

### D-113 — Where does the sync store live in production?
State: Decided.
Answer: 2026-11-06 — object storage: "I want it to store to S3/Blob/GCS";
"make the design explicit that it needs to use an object storage
backend." The user tests against a Ceph RGW instance (S3-compatible).
2026-11-06 — MinIO is cut ("explicitly cut Minio, minio is dead"),
for dev and production; RustFS is the local dev and test server
("maybe rustfs for local testing dev").
2026-11-06 — no coupling to any one store: "i dont want a tight
coupling to ceph just like a don't want a tight coupling to a POSIX
filesystem. those are only dev conveniences." The targets are the
managed providers (S3, Azure Blob, GCS); the engine assumes only what
all three guarantee — whole-object atomic put, create-if-absent,
strongly consistent read and list, object metadata, delete, modified
time — and Ceph RGW and RustFS are dev endpoints, used only as far as
they honor that contract; neither earns a code path of its own. Which
managed provider a deployment runs on is open (D-116).
Context: the store is a directory on one node. Its guarantees rest on
a local POSIX filesystem: `flock` keeps one writer process, a commit is
a file append, and blobs land by `rename` — so a host crash can tear a
log tail, and network filesystems and object stores provide none of
the three. Object stores write whole objects atomically and offer a
conditional create (S3 `If-None-Match: *`, GCS `ifGenerationMatch=0`,
Azure Blob `If-None-Match: *`), which is enough to hold the pointer
CAS across processes.
Options:
- A local directory hardened against crashes (torn-tail recovery,
  directory fsyncs, an offline verify): one node, one process, the
  disk's failure is the store's.
- **Object storage (S3 and S3-compatible, Azure Blob, GCS) — decided.**
  Blobs, envelopes, and log records become create-only objects; the
  conditional create is the CAS and the lock; several server processes
  may share a bucket; the provider carries durability and replication.
Blocks: nothing (`docs/units/store-durability.md` carries the work).

### D-116 — Which managed object store does a deployment run on?
State: Unanswered.
Context: 2026-11-06, the user: "the decision between S3/Blob/GCS is
open, so i don't want that locked." The engine is provider-neutral
(D-113's contract), and each adapter passes one contract suite, so the
choice is an agency's infrastructure (existing cloud agreements, a
government region, where its IdP lives), not the engine's. The S3
adapter is built first only because the dev endpoints speak the S3
API; that order picks nothing for production.
Options: AWS S3; Azure Blob; GCS — each needs only its adapter passing
the contract suite against a real account.
Blocks: the deployment runbook and the restore drill in
`docs/units/store-durability.md` (they name a provider's replication,
versioning and immutability settings); nothing in the engine.

### D-117 — Who deletes from the store, and when?
State: Decided.
Answer: 2026-11-06 — offline: "intuitively i think GC should be
offline"; after the trade-offs, "yes, keep it offline." Serving
processes never delete; an offline `store gc` with its own
delete-capable credentials collects orphans.
Context: the log is append-only and every record names its blob, so
the only garbage is an orphan — a blob whose commit never landed (a
crash between blob and record, or a lost CAS). A store serving a
moderate-traffic agency makes a handful a month. Online GC today runs
in every serving process (`-gc-interval`); over object storage it can
race a commit: a writer whose create-if-absent finds an old orphan
treats it as durable while another process deletes it, and the record
then names a missing blob. The north star (24/7 operations, moderate
to low traffic, high auditability) weighs: the serving credentials,
the immutability settings the provider offers, and what an auditor
can prove was never removed.
Options:
- Online GC (today): no job to run; every serving process needs
  delete permission; the race above needs a lease or a heal step;
  incompatible with object immutability (S3 Object Lock, Azure
  immutability policies, GCS bucket lock).
- **Offline `store gc` — decided.** A separate job with its own delete-capable
  credentials, run on a schedule or by hand; serving processes hold
  create-and-read only, so a compromised server cannot erase history;
  the race remains if it runs beside live servers, closed by a
  long grace plus the commit re-checking its blob after the record.
- Never delete: orphans stay (a lost CAS's blob is itself evidence of
  an attempted write), `store verify` reports them; the bucket can be
  fully immutable for the agency's retention period; storage grows by
  the orphans only.
Blocks: nothing (`docs/units/store-durability.md` carries the work).

### D-118 — Where does a server read its inputs at boot?
State: Decided.
Answer: 2026-11-06 — "boot should be from the bucket OR configurable
external resources, eg the gtfs feeds."
Context: a server reads its inputs from local files today (`-feed`,
`-streets`, `-policy`, the wasm paths), so a process is not disposable:
a replacement needs the same files and flags, and two processes
configured differently publish different manifests to one store.
Options:
- **The bucket, or a configured external source (a GTFS feed URL, a
  street layer) — decided.** Local files remain a dev convenience.
- The bucket only, with external inputs imported by a separate step.
Blocks: nothing (`docs/units/store-durability.md` carries the work).

### D-119 — What does the server serve when the store is unreachable?
State: Decided.
Answer: 2026-11-06 — "agreed with your assessment": serve the cached
heads, each labelled with its age, refuse writes until the bucket
answers, and set no age past which reads fail. A transit system's
reads peak during disruptions, when an outage is likeliest; a correct
but stale answer, labelled, beats none.
Context: blobs are content-addressed and immutable, so a cached copy is
always correct; only a pointer's head can go stale. The north star is
an agency running 24/7 at moderate to low traffic with high
auditability needs. Auditability rules out acknowledging a write the
bucket has not taken (a server-side queue loses an acknowledged write
with the process); the browser already owns its documents and resends
under LWW, so a refused write is held client-side.
Options:
- **Serve the cached heads, each labelled with its age (seq and
  `at_ms`); refuse writes (503) until the bucket answers — decided.**
- The same, with an age past which reads fail rather than serve stale.
- Fail every request until the bucket answers.
Blocks: nothing (`docs/units/store-durability.md` carries the work).

### D-120 — Does compaction drop a record whose dates have passed?
State: Unanswered.
Context: compaction (D-57) drops only retired records past the
horizon. An accepted diversion whose `end_date` has passed, or a
cancel whose `date` has, stays in the live edit document until someone
retires it, so the document grows with every edit the agency has ever
made. Measured 2026-11-06 through the server's merge: a record averages
1.5 KB (a drawn 120-point detour 4.9 KB, a diversion without one
~0.3 KB, a cancel ~0.2 KB); at 10 edits a day the live document is
5.4 MB after a year and 54 MB after ten, and every client syncs it
whole. The store keeps every past version through the log (each
commit's blob), so dropping a record from the live document loses no
history.
Options:
- Retired only (encoded): the live document holds every unretired
  record ever made; growth is unbounded unless people retire.
- Also drop a record whose dates ended before the horizon: the live
  document holds roughly the last 90 days plus what is in force
  (~1.8 MB at 10 edits a day); the log and its blobs keep the rest.
Blocks: nothing.

### D-121 — Does the store keep the edit document whole per commit, or record by record?
State: Unanswered.
Context: a commit to `edits` stores the whole document as one blob
(`docs/FORMATS.md`), so the bucket grows by the document's size on
every commit — quadratically while the document grows (D-120).
Measured 2026-11-06 (1.5 KB a record, ~3 commits per edit), over ten
years at 10 edits a day: 2.9 TB keeping every record, 190 GB with
D-120's expiry, ~10 GB stored record by record. At 30 edits a day:
26.5 TB, 1.7 TB, ~86 GB. Whole snapshots make "what was in force at
14:02" one GET; record by record makes it a GET of the commit's list
and the records it names.
Options:
- Whole document per commit (today): simplest reader; storage grows
  with document size times commits.
- Each record its own content-addressed blob, the document a list of
  record hashes (git's tree): a commit writes the changed records and
  the list; the module stays the hash authority for both; a new
  `docs/FORMATS.md` layout and a `store verify` rule.
With D-122, detours and cancels are the only row-shaped data this
repo holds, so the answer also settles whether it needs a row store at
all: record by record in the bucket keeps it to one durable service.
Blocks: the edits layout in `docs/units/store-durability.md`.

### D-122 — Is the dispatcher's day in scope?
State: Decided.
Answer: 2026-11-06 — "i would say no, but everything we are building
is adjacent/touches it so the interfaces need to be clear and well
defined."
Context: data that is published and acted on as a whole is a document
(the network, picks, plans, policy, a work bid's package, choices and
roster); data written by many hands at once, in no order the domain
imposes, that accumulates and is searched by attribute, is rows. The
dispatcher's day — sick calls, extra-board fills, swaps, vehicle
assignments — is the row-shaped operational data a transit system
runs on, held by its own systems (CAD/AVL, workforce management).
Options:
- **Out of scope; the adjacent systems meet this one at defined
  interfaces — decided.** This repo holds documents (and detours and
  cancels, D-121); each fact has one system of record, and the
  interfaces say what crosses and in whose vocabulary.
- In scope: a row store joins the repo for daily operations.
Blocks: nothing.

### D-123 — Do trip, block, and run ids survive from one pick to the next?
State: Unanswered.
Context: adjacent systems (D-122) join on this repo's ids — a CAD
system matches a block, a workforce system a run. Within the repo a
fork copies the ids it carries, but a network imported from the
scheduling system's export carries whatever ids that export names, so
the same id in two picks may or may not be the same piece of work. An
external reference names its pick or `network_hash` with the id
(`docs/ARCHITECTURE.md`, "Adjacent systems"), so nothing is ambiguous
today — and a service date names its pick (D-75, D-76: the past's
pick in force never changes), so TODS's and TIDES's references by
service date and id are already unambiguous. The question is what an
adjacent system may assume across a pick change; service design's
comparisons across picks go by pattern and time band (D-127).
Options:
- No promise: ids mean something only within their pick, and an
  adjacent system re-joins at every pick.
- Stable ids: an id names the same work across picks, and the import
  keeps them (the scheduling system's ids, or a mapping kept beside
  the network).
- A published correspondence: each pick carries what its ids replace
  in the pick before it.
Blocks: nothing yet; any adjacent system that joins across picks.

### D-124 — Does this repo publish its cancels and detours as GTFS-rt?
State: Decided.
Answer: 2026-11-06 — yes: "detours and cancels are in the GTFS-RT spec,
it is just that we are providing an additional authoring surface."
The edit document is the authored model; GTFS-rt is its serialization,
as GTFS is the network's.
Context: a trip cancel is a TripUpdate with `CANCELED`; a diversion is
a TripModifications (start and end stop selectors, the detour as a
shape, `propagated_modification_delay` = `added_secs`, its
`service_dates`), with TripUpdates marking the skipped stops `SKIPPED`
for clients that do not read TripModifications. The editing workflow
(draft, submitted, accepted, retired; `rev`) has no GTFS-rt
counterpart: it is the authoring surface, and only accepted records
serialize. TripModifications is experimental in the spec, and a
diversion has no replacement stops yet, which TripModifications
carries. `transit-rt` decodes GTFS-rt and joins it over the network;
no op encodes a feed.
Options:
- **Publish: an op answers the edits in force as GTFS-rt — decided**
  (`docs/units/rt-publish.md`).
- Documents only: adjacent systems read the edit document and do
  their own translation.
Blocks: nothing.

### D-129 — Where does the authored GTFS-rt reach riders?
State: Unanswered.
Context: an agency's CAD/AVL vendor usually publishes its GTFS-rt
already (C-TRAN's at `gtfs-rt.c-tran.org`, with alerts at
`gtfs-rt.c-tran.com/Alerts`). This repo's authored cancels and
detours (D-124) are a second source for the same trips: an authored
cancel and the vendor's prediction can disagree, and no code picks a
winner between sources (`docs/ARCHITECTURE.md`, "Adjacent systems").
Options:
- Its own feed endpoints beside the vendor's: consumers read both
  (TripModifications and Alerts are commonly separate feeds); a cancel
  in two TripUpdates feeds is the consumer's to reconcile.
- Handed to the CAD/AVL system, which folds it into the one feed it
  publishes (an adjacent system's input, D-122).
- Both: this repo's endpoints are the source the CAD/AVL reads, and
  the vendor's feed is what riders' apps read.
Blocks: publishing in `docs/units/rt-publish.md`.

### D-125 — Does an edit record carry its source system's id?
State: Unanswered.
Context: a record an adjacent system writes (a trip cancel after a
sick call) is joined back to its cause during an audit by the source
system's own id. `EditRecord` has no field for one; the log's `writer`
names the system, not the event.
Options:
- An optional opaque field (the source and its id), additive under the
  schema version, stored and served, never branched on.
- None: the adjacent system keeps the mapping from its event to this
  repo's record id.
Blocks: nothing.

## Authentication and authorization

### D-58 — Which endpoints require a token by default?
State: Decided.
Answer: 2026-10-30 — "Public read, auth'd write": GET on /manifest,
/network, /streets, /walks, /blobs, /pointers and /edits stays public
(the rider-facing deployment shape); POST /edits and POST
/pointers/* require a role with write on that pointer. The policy
document can refine per pointer later.
Context: with -store landed, writes are the multi-writer surface; reads
publish documents. A deployment that must not expose reads can put a
read rule behind auth in the policy.
Options: public read / auth'd write (chosen); everything authenticated;
only edits+pointers protected.
Blocks: nothing.

### D-59 — Which JWT claim carries the roles?
State: Decided.
Answer: 2026-10-30 — "Configurable claim path": a `-oidc-roles-claim`
flag (dotted path into the payload, e.g. `roles` or
`realm_access.roles` for Keycloak), default `roles`, value a JSON
array of strings. Works with any IdP.
Context: the policy matches role strings; the claim's location is the
IdP's, so the server must be told where to look rather than hard-code
one vendor's shape.
Options: configurable path (chosen); OAuth2 scope/scp only; fixed
top-level `roles`.
Blocks: nothing.

### D-60 — How are JWTs verified (JWKS vs pinned key)?
State: Decided.
Answer: 2026-10-30 — "Both, JWKS preferred": with `-oidc-jwks-url` set
the server fetches the IdP's JWKS, caches by `kid`, refetches on an
unknown `kid`; else `-oidc-key <pem>` pins one public key. RS256 and
ES256 only — `alg: none` and HS* are rejected (the IdP signs with a
private key; symmetric algs would let anyone mint tokens). iss must
equal `-oidc-issuer`, aud must contain `-oidc-audience`, exp/nbf with
60 s leeway.
Context: authentication is the IdP's; the server only verifies.
Options: JWKS only; pinned key only; both (chosen).
Blocks: nothing.

### D-61 — Where does the authz policy live and how is it changed?
State: Decided.
Answer: 2026-10-30 — "Store pointer, seeded from file": the policy is a
document on the store (kind `policy`), seeded from a `-policy <file>`
flag at first boot when the pointer is absent; later changes are
authenticated CAS commits on the `policy` pointer (audited in the log)
needing a role with write on it. Bootstrap-safe: no chicken-and-egg.
Context: the policy is data, so per the bus laws it is a document
addressed by hash, and cap-authz answers authorize/deny over it
fail-closed (no policy, no valid token, or no matching rule → deny).
Options: store pointer seeded from file (chosen); file re-read per
request (not audited); store pointer only (needs break-glass bootstrap).
Blocks: nothing.

## Versioning and deployment

### D-62 — Where does a document's schema version live?
State: Decided.
Answer: 2026-11-03 — "field inside each document." Every document carries a top-level version field; the hash covers it.
Context: `docs/units/document-versioning.md`. The watchlist and the
policy carry their version inside the document; `edit_records`
carries it only in the store's envelope sidecar, so the copy in
IndexedDB, a POST body, or a file `caps-run` loads has none; the
network carries none anywhere and `read_network` sniffs its shape.
A document's hash is its canonical bytes, so a tag inside is part of
its identity.
Options:
- Inside every document, a top-level version field (recommended):
  every copy anywhere says what it is; two versions of the same
  content never share a hash. Costs: `edit_records` stops being a
  bare array (its own v1 → v2 migration), and every network hash
  changes once.
- In the envelope only: the store and HTTP headers carry it; every
  other copy needs a wrapper or a shape sniff, the way the network
  is read today.
Blocks: `document-versioning` scope 2 (the tag) and the first
migrations.

### D-63 — What does an open page do when the deployment moves past it?
State: Decided.
Answer: 2026-11-03 — "these are truly web apps, so they benefit from proper 'update' semantics." Encoded as the web-app update lifecycle: a new build downloads in the background and waits; a rider's page applies it at the next cold start or navigation, or when the rider accepts "a new version is ready"; a display (nobody to tap) applies it itself at its next poll while the server answers; no page applies it over unsaved device data. Data (a new network_hash) re-pins without a reload.
Context: `docs/units/deploy-rider-apps.md`. A stop display stays
open for weeks; a rider's tab for days. A publish moves either the
data (a new `network_hash`, the same modules) or the code (a module
hash, the kernel, the app build). Data re-pins without a reload in
every option below; the question is code.
Options:
- Riders see "a new version is ready" and it applies at the next
  cold start or navigation; displays reload themselves at the next
  poll while the server answers; neither reloads over unsaved device
  data (recommended).
- Every page reloads as soon as it sees new code.
- Nothing until the person reloads (today) — an old page may meet a
  document it does not speak and goes read-only.
Also open: a scheduled nightly reload for displays as a safety net
(e.g. 03:00, outside service).
Blocks: `deploy-rider-apps` following a publish.

### D-64 — Which devices must the rider apps run well on?
State: Decided.
Answer: 2026-11-03 — the stop displays run on Raspberry Pi 4s. Ride-now targets browsers: "it shouldn't be about 'minimum support' most phones these days can handle a properly optimized page." So ride-now has performance budgets, not a device floor; the numbers are D-68.
Context: `docs/units/runtime-cost-gate.md` ratchets the measured
numbers down but sets no target. The scratch bench measured
5.8–8.4 MB of wasm memory per consumer and ~300 ms per C-TRAN
registration at 4× CPU throttle; ride-now assembles five consumers.
Mobile browsers kill a tab well before a desktop would, and the
stop display's hardware is not named anywhere.
Options: name the floor — a phone (e.g. a mid-range Android three
years old, and the smallest supported iPhone), the display's player
hardware and browser, and a cold-load network (e.g. throttled 4G) —
and the budgets on it (e.g. first board under 3 s cold, the page
under 150 MB).
Blocks: `deploy-rider-apps` budgets (the ceilings' targets) and its
phone dogfood.

### D-65 — Do ride-now and the stop display work offline?
State: Decided.
Answer: 2026-11-03 — "yes." Both pages are installable and work offline: the shell, the modules, and the last network cached; boards answer from the schedule, marked as the schedule.
Context: `docs/units/deploy-rider-apps.md`. The modules and the
network are content-addressed, so a service worker can cache them
forever without staleness; the app shell and `/manifest` are what
would go stale (D-63 governs those). A rider at a stop with no
signal is the common case the project's local-first claim is for;
a display whose uplink drops keeps its in-memory network until it
reloads.
Options:
- Installable and offline at launch: the shell, the modules, and the
  last network cached; boards answer from the schedule, marked as
  the schedule (recommended).
- The HTTP cache only at launch; offline is a later unit.
Blocks: `deploy-rider-apps` offline.

### D-66 — How long is a migration kept?
State: Decided.
Answer: 2026-11-03 — "forever … i imagined they'd just be their own wasm caps that don't take much space to store. you keep migrations for a regular application forever as sql files." Every authored kind's migration chain is kept forever, wrapped as its own capability module (the library crate holds the steps; the capability is assembled only when an older document needs it), so consumers speak only the current version.
Context: `docs/units/document-versioning.md`. Derived documents
(an import, the walk table) re-derive and need no migrations.
Authored ones (edits, plans, a planner's fork, a watchlist) can sit
on an unopened device or in an old file for years. A migration is a
pure function with a frozen fixture, so keeping it costs a test.
Options:
- Forever for authored kinds (recommended).
- A floor (e.g. a year, or N versions): older documents are refused
  with a message naming the last build that reads them.
Blocks: `document-versioning` the support window.

### D-67 — Whose feeds does the server's proxy fetch?
State: Decided.
Answer: 2026-11-03 — "I'd prefer to get rid of the proxy honestly, it is just there for local dev/testing" and "we control the feeds, so we just need to fix those issues properly on the feed side." The `/rt` proxy is removed; the agency's feeds serve HTTPS with CORS and pages fetch them directly.
Context: `docs/units/deploy-rider-apps.md`. `/rt?url=` fetches any
http(s) URL for any caller, with no timeout or size cap — beyond
localhost it is an open relay into the server's network. Ride-now
lets a rider set their own feed URLs, which only work through an
open proxy unless the upstream sends CORS headers. Where the
deployment names its feeds is D-39.
Options:
- Only the deployment's own feeds (recommended): an allowlist from
  its configuration, a timeout and a size cap; a rider's own feed
  works only when its upstream allows CORS.
- The deployment's feeds for anyone, any URL for an authenticated
  role (D-58).
- Any URL, localhost deployments only.
Blocks: `deploy-rider-apps` the proxy.

### D-68 — What are ride-now's and the stop display's performance budgets?
State: Decided.
Answer: 2026-11-03 — "ok" to the proposal below, after asking "why would we be allowing a departure board to tax 2 GB of RAM": budgets are set from what the page needs, never the hardware's headroom — the display at most 40 MB of wasm + JS heap flat across 24 h, ride-now at most 80 MB, with the timing budgets below.
Context: D-64 sets no device floor for ride-now and names the
display hardware (a Raspberry Pi 4). Budgets are what the cost
gate's ratchet must come under; the costs that matter are bytes on
a cold load, main-thread blocking, memory (each consumer module
holds its own parsed network, 5.8–8.4 MB on C-TRAN), and polling a
hidden page (battery and data).
Options (proposed): budgets on what the page controls — wasm
linear memory plus the JS heap — never on what the device has; the
browser's own overhead is outside them. Set from what each page
needs, not from the hardware's headroom: a display assembles three
consumers (directory, rt, board), each with its own parsed network
(5.8–8.4 MB on C-TRAN), plus one live snapshot each once eviction
lands (~388 KB) — about 30 MB.
- The display (measured on a Pi 4): wasm + JS heap at most 40 MB,
  flat across 24 h; a board refresh never drops a frame.
- Ride-now (4× CPU throttle, "Fast 4G"): wasm + JS heap at most
  80 MB (five consumers and a map); first board under 3 s cold and
  under 1 s cached (D-65); no main-thread task over 50 ms after
  first paint; no polling while hidden.
These are starting ceilings: the cost gate only lowers them. The
larger saving is a candidate, not part of this answer: a display
shows one stop, so a stop-scoped derived document (the day's
departures there) would replace three whole-network copies — a new
document kind (`docs/units/runtime-cost-gate.md`, splitting the
network document).
Blocks: `deploy-rider-apps` budgets (the ceilings' targets) and its
dogfood.

### D-69 — What happens to the original app when host calls move into the engine?
State: Decided.
Answer: 2026-11-03 — "Retire it now": delete `/app.html` and its host
layer; anything only it answered becomes a gap recorded in a unit.
Context: `docs/units/host-discipline.md`'s gate (no op call outside
the engine) ends with an empty allowlist. The original app (persona
tabs over one shell) called ops directly from `actions.ts`,
`planner.ts`, and `capdemo.svelte.ts`. It was the only browser UI
that authored a diversion, showed a day's blocks, imported a feed or
street layer in the page, and demonstrated migrate/rollback.
Options: migrate it onto the engine; keep it on the gate's
allowlist until retired; retire it now (chosen).
Blocks: nothing. The gaps it left are lines in
`docs/units/grade-debt.md` ("Not read by a UI"), paid by
`ui-disruptions` and `ui-runcut`; the migrate/rollback pin is
`docs/units/document-versioning.md`'s browser e2e.

### D-70 — How do the module runtime's bounds default?
State: Encoded provisionally.
Context: the module runtime puts eviction in the module
(a kernel op over a keep-set the host names) and leaves the
defaults open: how many ephemeral documents a module keeps with no
host steering, when an instance is replaced for its memory, and
what a host sees when a document it holds was dropped.
Encoded (provisional, change by editing the constants):
- Ephemeral kinds keep their most recent N registrations: rt
  snapshots 8 (a live answer reads at most three per poll), live
  documents 4 (board, router, runcut), router graphs 8 (and only the
  newest live document's). Durable kinds (networks, feeds, edits,
  boundaries, layers, walk tables, streets, watchlists) stay until a
  keep-set drops them.
- The memory ceiling is 128 MiB per instance — twice the largest
  working set measured on C-TRAN (2026-11-06: service-design holding
  1/2/3/5 networks at 24/48/57/66 MB; the rider pages 16–18 MB,
  2026-11-05) — checked before an op
  (never a registration); an instance is replaced only after it has
  answered an op since it was made, and at most once a minute.
- A dropped document is invisible to the host: the engine
  registers it again and asks once more.
Pins: `cap_abi::registry` (its keep-last-N and keep-set
properties), `crates/cap-rt/tests/registry_test.rs`,
`crates/cap-router/tests/registry_test.rs`, and
`e2e/ui-stop-display.spec.mjs` ("a display left polling stays
bounded").
Blocks: nothing.
Update 2026-11-05: the bounds are the kernel's (D-77). Each
derivation bounds its own entries: held days 4, router graphs 8 (and
only the newest live document's), street and walking graphs 4
(`cap_abi::kernel::DAY`, `GRAPH`, `STREET_GRAPH`, `WALK_GRAPH`); the
ceiling is per module instance, so in a linked assembly a reset drops
every capability's state together. Pins add
`cap_abi::kernel` (`a_derived_entry_is_a_cache_bounded_per_derivation`).

### D-71 — May a module register a document under a pinned hash it did not compute?
State: Decided.
Answer: 2026-11-05 — keep full verification in every module. After D-77 a page registers each document once, so the cost is one canonical hash per document per page.
Context: registering the C-TRAN network costs ~300 ms per module on
the bus worker (`docs/units/runtime-cost-gate.md`): the parse
(~126 ms), then the canonical hash — the document re-serialized
(~50–100 ms) and SHA-256 over it (~100 ms). The module is the hash
authority (the bus rules): it derives identity, a host never mints
it. A module cannot VERIFY a pin cheaper than computing it — bytes
that hash to the pin are the pin's bytes, but only re-serializing
proves they are canonical, which is the cost to be saved.
Options:
- Keep full verification in every module (today): sound, ~300 ms per
  registration on C-TRAN.
- Trust a pin for bytes that hash to it: the module skips the
  re-serialization and registers under the pin (saves ~50–100 ms per
  registration; the SHA-256 stays). A host passing non-canonical
  bytes with their own hash would mint an identity no module derived.
- Hash once per page: the first module derives the identity, the
  engine passes it with the same bytes to the rest, which trust it —
  the same trust, saving the whole hash in every later module.
- A strict canonical parser that refuses anything but the canonical
  serialization, so the bytes' SHA-256 IS the identity with no
  re-serialization and no trust — sound, and a parser to keep in
  lockstep with serde's output.
Blocks: nothing (the re-hash candidate in `runtime-cost-gate` is closed).
Update 2026-11-04: modules now register the network from its gzip
stream, and every op answers in struct field order, so the server's
`/network/document` bytes ARE the canonical serialization (a server
test hashes them to the pin). The parse is ~290 ms of the router's
first registration on C-TRAN (serde_json 240, inflate 49), the
canonical hash ~145 ms (sha2 79 over the re-serialization).
Update 2026-11-05: under D-77 a page registers each document once,
in its assembly, so the repeat in every module that this entry weighs
leaves the browser; what remains is one canonical hash per document
per page, and one per single-capability instance elsewhere.

### D-72 — Is the release profile tuned for size or for speed?
State: Encoded provisionally.
Context: registration is the hot path every module runs on every
page, and the network is registered from its gzip stream
(`cap_abi::parse_document`), a per-byte decode that opt-level "s"
leaves un-inlined. Measured in Node on the C-TRAN network
(`docs/units/runtime-cost-gate.md`): board at "s" registers the
gzip stream in 302 ms (1.75 MB module); at 3 in 187 ms (2.17 MB).
The plain-JSON slice parse at "s" was 168 ms and 1.70 MB.
Encoded (provisional): `opt-level = 3` for the whole workspace's
release profile — modules ~25–40% larger (cap-network 3.2 MB,
cap-author 2.8 MB), registration ~40% faster than "s" over the same
stream. Per-package opt levels measured worse on both (they fight
LTO).
Options: keep 3; return to "s" and accept the slower streamed
registration (~2× today's worker time per module); or keep the
plain-JSON slice decoder beside the stream decoder (164 ms, 2.43 MB).
Pins: the profile in the root `Cargo.toml`; `just perf`.

### D-77 — How do the capabilities a page uses share one network?
State: Decided.
Answer: 2026-11-05 — sharing: "i actually prefer this, i don't see
why we would need to have each cap hold a full copy of the network."
On linking per generated UI at build time, with "capabilities never
call capabilities" enforced by crate dependencies, a registry
constructor private to the kernel, forbidden `unsafe`, and an
assembly-independence test: "i agree with what you laid out."
Context: each capability is its own wasm instance, with private
linear memory and zero imports, so every network consumer parses and
holds its own copy (~9–12 MB and ~300 ms each on C-TRAN; the stop
display holds three — QAF-93). Each also ships the same ~550 KB
(gzipped) base. Law 3 said a persona's bundle comes together at wire
time, "never at build time".
Options:
- One module per capability: isolation of faults and memory by
  construction; a network copy and a base per consumer.
- Link a page's capabilities into one module with one kernel-owned
  registry (decided): one copy and one base per page. Costs: the
  capabilities in an assembly share one model version; one
  instance's failure or memory reset is every capability's; the rule
  is enforced by the compiler and gates instead of separate
  memories. Linking is per generated UI at build time first, and per
  user selection at wire time later.
- Wasm threads with a shared, imported memory: lets instances of ONE
  module share memory. Separately compiled modules cannot share a
  heap or a parsed `Network` without linking, so it does not replace
  linking. It breaks zero imports, and needs cross-origin isolation
  (COOP/COEP) and a nightly toolchain with a rebuilt std. It remains
  an option on top of linking, if a measured query needs more cores.
- A compact zero-parse document format: smaller, faster copies,
  still one per module.
Blocks: nothing. The design is `docs/ARCHITECTURE.md`, "Bus rules"; Law 3
in `crates/AGENTS.md` carries the answer (linking assembles bytes,
not surfaces).
Update 2026-11-05: built — the kernel owns the registries and the
loads, `assembly!` emits the exports, each generated UI loads one
linked module, and the four gates run in `just test`
(`docs/ARCHITECTURE.md`, "Bus rules"). Measured on C-TRAN the same
day: the stop display 41.5 → 21.0 MB (wasm 31.8 → 16.2), ride-now's
wasm 45.7 → 17.8 MB (`docs/units/runtime-cost-gate.md`); the server
stays on single-capability modules (D-78).

### D-78 — Does the Go server host linked assemblies?
State: Decided.
Answer: 2026-11-05 — no: the server keeps single-capability modules;
only the browser links. The long-lived modules (sync, plans, authz)
hold no network, so there is nothing to share; every network or
streets consumer is a fresh instance per call, which is the server's
memory hygiene; authz stays in its own instance (a trap or reset in
another capability never takes the fail-closed check with it); and
sync's pinned `wasm_hash` changes only when sync does. Revisit if the
server hosts a long-lived network reader (a server-side board or
router): link those readers together, authz still apart.
Context: the server keeps cap-sync, cap-plans, and cap-authz for its
lifetime, and instantiates cap-network and cap-streets fresh for each
published feed or street layer, closing each after
(`server/network.go`, `server/streets.go`, `server/plans.go`). A
fresh instance hands its memory back, because wasm memory never
shrinks. The long-lived modules hold no network.
Options:
- Keep single-capability modules on the server; only the browser
  links (decided). The per-import instance stays the memory hygiene.
- One assembly for the long-lived modules (sync, plans, authz), the
  per-import instances single: the base ships once; there is no
  network to share.
- One assembly for everything: one compile and one registry; loses
  the fresh instance per import.
Blocks: nothing. Pinned by `server/main.go` (one module flag per
capability) and the `caps` recipe (each `module_<cap>.wasm` copied to
`web/pkg/`).

### D-85 — Are the generated assembly crates committed?
State: Decided.
Answer: 2026-11-05 — "looks like a lot of auto generated
assets/packages aren't gitignored": they are generated, like the TS
bindings, and gitignored.
Context: `gen-assemblies` writes one crate per capability and per
generated UI (`assemblies/module-*`), workspace members by glob. A
committed tree would need a currency gate; an ignored one needs the
directory to exist so the glob resolves (cargo refuses a glob whose
directory is missing, and accepts an empty one).
Options:
- Commit them, with a `--check` gate in `just test`.
- Generate and ignore them (decided): `.gitignore` ignores
  `assemblies/*`, and `assemblies/.gitkeep` keeps the directory; `just caps` regenerates them
  before every build. Cargo.lock carries their entries once
  generated, and a plain `cargo` run in a fresh checkout before `just
  caps` drops them again.
Blocks: nothing. Pinned by `.gitignore`, `assemblies/.gitkeep`, and
the `caps` recipe.

### D-86 — Which op answers what an edit record amounts to on the network?
State: Encoded provisionally.
Context: `load_edits` answered per-record validation reports in the
board, runcut, calendar, and router alike — one answer four times
(D-77). Loads became the kernel's, which
answer an identity, never a domain report.
Options:
- cap-divert `reports` (encoded): the capability that answers the
  edit document's rows; the records travel inline, like `list`.
- A field on each `list` row: one op fewer, but every list read pays
  for validation.
Pins: `crates/cap-divert/tests/dispatch_test.rs`
(`reports_say_what_each_record_amounts_to_on_the_network`), the
divert runner gate in `just test`, caps-smoke's reports.
Blocks: nothing.

### D-87 — How does an export name the import numbering it writes back?
State: Encoded provisionally.
Context: cap-network kept each import's boundary keyed by its
network, and `export` used it implicitly. A capability declares no
state of its own now (the gates, D-77), so the boundary registers in
the kernel under its own hash.
Options:
- `export` takes an optional `boundary_hash` (encoded); `import`
  answers it. Without it, 1..n numbering (an authored network).
- A derived kernel entry from network to boundary: hidden state the
  host never names, and not recomputable from the network.
Pins: `crates/cap-network/tests/dispatch_test.rs`
(`an_export_naming_the_import_boundary_round_trips`), caps-smoke.
Blocks: nothing.

### D-88 — What does a page run when its linked assembly does not load?
State: Encoded provisionally.
Context: a generated UI's page runs its linked assembly (D-77); one
module's failure is every linked capability's.
Options:
- Fall back to each capability's own module (encoded): the page
  degrades per capability, as before linking, at the old memory cost.
- Every capability unavailable: simpler; the page fails whole.
Pins: `web/src/bus.worker.ts` (`place`), the degrade specs in
`e2e/ui-ride-now.spec.mjs` and `e2e/ui-service-design.spec.mjs`
(they block the assembly and one capability's module).
Blocks: nothing.

### D-89 — May a host decline a document an op minted?
State: Encoded provisionally.
Context: QAF-49 — the live document is 388 KB a poll; the stop
display never reads it. In its assembly, cap-rt's `live` registers it
where the board reads it.
Options:
- `live` takes `with_document` (default true; encoded): a host whose
  readers are linked beside rt declines it, and the engine mints it
  again (`remint`) when an instance dropped it.
- Never answer the document: a host in separate modules (caps-run,
  a reader not linked) could not move it.
Pins: `crates/cap-rt/tests/dispatch_test.rs` (the with_document law),
`e2e/ui-stop-display.spec.mjs` ("a display left polling stays
bounded" — the reset remints it).
Blocks: nothing.

### D-90 — What may a rider's device remember to open the map where it will end up?
State: Encoded provisionally.
Context: the ride map opened on the archive's metro-wide view, then
framed the network, then moved to the rider — three views, the first
two's tiles thrown away, and the rider's own tiles requested only
about two seconds in. Opening on the rider needs to know where they
are before the first fix lands (a fix can take seconds) — and a
rider who never shares a location (the browser asks first; the page
only locates on its own where permission is already granted) still
saw the map open on the archive and then move to the network, every
load.
Options:
- **Remember two things on the device (encoded).** (1) The last
  shared fix, `localStorage` `ride-now:last-fix`: the rider's
  location, written when a fix arrives, read at boot, never sent
  anywhere (the map is the only reader; no op, no sync, no server).
  The map opens on it only where tiles cover it, and frames the
  network instead when the stops show it stale (outside them). No fix
  shared, no key. (2) The view the network was last framed on,
  `ride-now:network-view` — the network's, not the rider's: a rider
  with no fix opens on it and the camera does not move; a network
  that moved is framed again (the camera is compared with the frame
  the stops give, and moves only when they differ). A device's first
  load, with neither, still opens on the archive and moves once.
- Remember nothing: a first move to the rider after every fix, the
  throwaway tile waves stay.
- Open on the network (the planner's service area) always: no stored
  location, but a repeat rider still waits for the fix to see their
  street.
Pins: `web/e2e/ride-map-load.spec.mjs` ("a rider who shared a
location before opens on it and the camera never leaves", "the next
load opens on the frame the network was given, and does not move"),
`web/e2e/viewport.spec.mjs` (the start view and the framing rules).
Blocks: nothing (a different answer removes the keys and the `start`
and `framed` props).

### D-97 — Where does a layer's weight field and label live?
State: Encoded provisionally.
Context: D-47 makes coverage take any weighted layer, each with a
label giving its meaning. The kernel registers a layer under the hash
of its bytes; a load hint (the old `pop_field`) was parsed into the
document, so the same bytes loaded with another field answered the
first parse — one file carrying people AND jobs could not be read
both ways.
Options:
- **The question's (encoded).** The layer document is the bytes:
  every Point, Polygon and MultiPolygon feature with its numeric
  properties. `load_layer` answers the fields it can be read by (each
  property that is a JSON number, with its total). Each coverage op
  takes `field` (omitted: the layer's one numeric property; a layer
  with several is an error naming them) and `label` (omitted: the
  field), and answers its `layer` reading. A numeric string is read
  only when named.
- The load's: field and label in the load request, the identity the
  hash of bytes plus reading (the bulk lane would need an envelope).
Pins: `coverage_law_test.rs` (`a_layer_with_several_weights_needs_one_named`,
`a_point_layer_states_its_resolution`), `cap-analysis` dispatch test.
Blocks: nothing.

### D-98 — How finely is a polygon's area sampled?
State: Encoded provisionally.
Context: D-47's areal weighting needs the share of a polygon's area
within a radius of the served stops. An exact polygon ∩ union-of-disks
area is costly; a lattice is exact enough and conserves the weight.
Options:
- **A lattice at parse time (encoded).** Cell centres inside the
  polygon, each an equal share of its weight; the spacing is the
  coarser of 25 m and the side that gives about 4,096 samples over its
  area (a bounding box holds at most 65,536 cells); a polygon smaller
  than a cell is one sample at its outer ring's mean. The answer
  states the coarsest spacing (`sample_spacing_m`). A point layer
  states the side of its average area per point (`point_spacing_m`).
- Exact clipping against circles (no resolution to state, much more
  code and time).
Pins: `coverage_law_test.rs` (`areal_weighting_serves_the_share_of_the_area`:
a 2 km square straddling a 400 m radius is served within 1 point of
π·400²/2000²; `served_and_unserved_conserve_the_layer`).
Blocks: nothing.

### D-99 — Who names the two sides of a coverage comparison?
State: Encoded provisionally.
Context: D-43 makes the unit of a comparison the combination of
service types with the dates it holds, and leaves open how one plan's
combination matches the other's when a fork renames or splits a type.
The churn itself (weight gained and lost) does not depend on that
rule once each side is named.
Options:
- **The host names both (encoded).** analysis `coverage_compare`
  takes `base_members` and `members` (service type ids of each side),
  pools each side over its authored service with no date, and answers
  both sides and the gained and lost weight (served and frequent).
  The op never matches; no UI asks it until the matching rule is
  decided (the service-design and proposal-compare frames say so).
- The op matches combinations itself — waits on D-43's matching rule.
Pins: `coverage_law_test.rs` (`a_comparison_is_gains_and_losses`),
`cap-analysis` dispatch test (`unserved_points_answer_the_where`, the
comparison at the seam), the caps smoke.
Blocks: nothing (the coverage half of QAF-20 waits on D-43's
matching rule, not on this).

### D-103 — Which street classes does the bus profile ask a planner to confirm?
State: Encoded provisionally.
Context: D-26 (a bus) says a bus is drawn down any street, and an
unknown or "path-like" class warns. The import's classes (D-30) are
motorway, primary, secondary, local, ramp, service, track, path,
steps, unknown; which of them are path-like is not said, and D-26's
context also names an alley or a private drive (`service`) as a
street a planner may not want a bus on.
Options:
- **unknown, path, steps, track (encoded)** — the classes a bus
  cannot legally use or the data cannot classify; a service road is
  an ordinary street to draw on.
- The same plus `service` (alleys, private drives, parking aisles).
- unknown alone.
The list is the op's `confirm` parameter, so a planner's host can ask
for another; this is the default. A different answer changes
`transit_streets::bus::CONFIRM` and its law
(`the_bus_profile_asks_to_confirm_the_grey_classes`).
Blocks: nothing.

### D-104 — Where does a fork's period end come from?
State: Encoded provisionally.
Context: D-41 has the fork carry every weekly calendar to "the
period's end", read as the fork's own period end. A pick's period is
derived from the plan set (it ends the day before the next pick), and
at fork time the next pick usually does not exist yet, so the end
cannot be derived when the fork is made.
Options:
- **The planner names it (encoded).** `set_plan` takes `through`; the
  service-design fork form asks "Runs through" (optional). Without
  it, nothing is carried and the no-service warning names the
  problem.
- A default length (a pick's usual four months) when none is named.
- Derive it from the published plan set at publish time, carrying
  calendars then rather than at the fork.
Blocks: nothing.

### D-105 — What does a fork carry when a feed's calendars are all dated?
State: Decided.
Answer: 2026-11-06 — the first option, as recommended: the import reads
a weekly rule out of dated service — a type whose added dates are every
flagged weekday of a span, except a few removed dates, imports as days
+ range + those exceptions — and the export writes it back as the feed
had it. D-41's fork then carries 1-WKDY, 2-SAT and 3-SUN; a closed
period such as 1-WKDY/30 ends where its dates end and is not carried.
How a rule is recognized is D-111; which weekly types a fork carries,
D-112.
Pins: `crates/transit-codec/tests/dated_calendar_law_test.rs` (exact,
recognition, the rule's shape, only dated service read, lossless, the
publisher's form after a carry), `crates/transit-codec/tests/
spec_round_trip_test.rs` (the generator's dated weekly service "3"),
`crates/transit-network/tests/plan_period_law_test.rs`, and the C-TRAN
session that closed QAF-54 (2026-11-06): network `import` of
`testdata/ctran-gtfs.zip` → `sha256:c6847464…`, 1-WKDY Mon–Fri
2026-09-14 → 2027-01-08 with four removed holidays, 3-SUN Sunday with
three added, the /30 types closed in October, /15, /16 and the NYE
types dated; author `apply` `set_plan` "Winter 2027" effective
2027-01-25 through 2027-05-29 carries 1-WKDY, 2-SAT, 3-SUN and the /31
types and warns nothing; network `publishable` → no refusals (without
`through`: `no_service_in_plan_period`); network `export` with the
import's boundary writes calendar.txt (13 rows) and calendar_dates.txt
(255 rows) back unchanged.
Context: QAF-54 and D-41. The fork carries every WEEKLY service type
(a flagged day and a range) to its period end. C-TRAN has none: its
calendar.txt flags no day for any service, and every type runs only on
added dates (1-WKDY: 81 dates, 2026-09-14 → 2027-01-08; 1-WKDY/30
closes 2026-10-09; 4-NYE one date). A Winter 2027 fork through
2027-05-29 carries nothing and is still refused for publishing
(C-TRAN session 2026-11-06: import `sha256:2beddcb3…`, author `apply`
`set_plan` with and without `through` answer the same document,
network `publishable` refusals `no_service_in_plan_period`). Setting
ranges by hand does not help either: with no day flagged a range runs
nothing.
The rhythm is in the dates, only not written as a rule: by weekday,
1-WKDY runs 17 Mondays, Tuesdays and Wednesdays and 15 Thursdays and
Fridays (the gaps are holidays); 2-SAT 17 Saturdays; 3-SUN 17 Sundays
plus three holiday weekdays; 1-WKDY/30 and /31 weekdays in two
successive spans; 1-WKDY/15 and /16 Tuesdays only.
Options:
- **Recommended.** The import reads a weekly rule out of dated service: a type whose
  added dates are every flagged weekday of a span, but for a few
  removed dates, imports as days + range + those exceptions (a codec
  normalization; export writes it back as the feed had it). D-41's
  fork then carries 1-WKDY, 2-SAT, 3-SUN; a closed period such as
  1-WKDY/30 ends where its dates end and is not carried.
- The fork carries dated service by its rhythm: a dated type whose
  dates reach the calendar's last week repeats its weekdays through
  `through` (no import change; the rule is inferred at the fork).
- Nothing is inferred: the planner flags a type's days
  (`toggle_service_day`) and the fork carries it from there; the UI
  says which types are dated.
Blocks: nothing.

### D-111 — When does a dated service read as a weekly rule?
State: Encoded provisionally.
Context: D-105. The import reads a weekly rule out of a service
published as dates alone (no weekday flagged, every row an added date).
C-TRAN: 1-WKDY runs 17 each of Mon–Wed and 15 each of Thu/Fri (holiday
gaps); 2-SAT 17 Saturdays; 3-SUN 17 Sundays plus two Fridays and a
Thursday (holidays running Sunday service); 1-WKDY/15 and /16 every
other Tuesday or so (a school calendar); the New Year's Eve types one
date each.
Options:
- **A share and a floor (encoded).** Over the span of the dates, a
  weekday qualifies when the service runs on at least 4 of them and on
  at least three quarters of that weekday's occurrences
  (`transit_codec::WEEKLY_MIN_DAYS`, `WEEKLY_MIN_SHARE`). The rule is
  those weekdays over one range from the first to the last date on
  them; a skipped one is a removed date, every other date an added one,
  so the rule runs on exactly the feed's dates. C-TRAN reads as
  1-WKDY Mon–Fri, 2-SAT Saturday, 3-SUN Sunday with three added
  holidays, the /30 and /31 periods weekly over their own spans; /15,
  /16 and the one-day types stay dated.
- A higher share (every occurrence but one or two), so a weekday with
  more gaps stays dated.
- Weeks rather than days: a weekday qualifies when it runs in most
  weeks of the span.
A different threshold changes the two constants and their law
(`dated_calendar_law_test`, "Recognition").
Blocks: nothing.

### D-112 — Which weekly service types does a fork carry?
State: Encoded provisionally.
Context: D-41 carries every weekly type to the fork's period end;
D-105 says a closed period (1-WKDY/30, ended 2026-10-09) is not
carried. A rule has to say which types are still running.
Options:
- **Those running in the calendar's final week (encoded).** The
  calendar's end is the latest date any type names (a weekly type's
  last range end, or an added date); a weekly type is carried when its
  last range ends within six days of it. On C-TRAN (end 2027-01-09):
  1-WKDY (01-08), 2-SAT (01-09), 3-SUN (01-03) and the /31 period types
  are carried; the /30 period types are not.
- Those whose last range ends on or after the fork's `effective_from`
  minus a margin.
- Only the types the planner names in the fork.
Blocks: nothing.

### D-114 — What does the clock answer in the hour before a fall-back day's anchor?
State: Encoded provisionally.
Context: GTFS counts a service day's times from noon minus 12 h,
local. On the day the clocks go back that anchor is 01:00 daylight
time, so 00:00–01:00 is before the day's first second; seconds cross
every boundary unsigned (AGENTS.md). The exact answer is the previous
service day at 24:00–25:00, which a board cannot read until it reads
two days (`docs/units/deploy-rider-apps.md`).
Options:
- **The day's first second (encoded):** the date is the calendar
  date, the seconds 0. A board in that hour starts at the day's first
  trips, and their waits read up to an hour short.
- The previous date at 86400 and up: that day's after-midnight trips
  answer, the current day's first trips do not.
Pinned by: `web/src/clock.test.mjs` ("fall back: the hour before the
anchor clamps to the day's start").
Blocks: nothing (the two-day board replaces the clamp).

### D-79 — How is a document version rolled back?
State: Decided.
Answer: 2026-11-06 — "i like option 1." Read one version ahead. Then, on old and archived data: the migration module "doesn't have to be shipped as a whole everywhere, it can be cut into operational components and one for the archive"; audit views showing the original with today's shape on request "sounds good"; and at an upgrade, "only the ones in use" are brought forward. Encoded as:
- A format change ships in two releases (expand, then contract). Release A's modules read vN and vN+1 and write vN; release B writes vN+1. A module reads its version and, for that one release, the next; the extra reader goes in the release after B.
- Rollback goes one release back, to a build that reads everything the store holds. Rolling the data back too (pointers to their pre-upgrade heads) is the emergency procedure only, for when release A itself is bad. It lists the dropped commits from the pointer log, whose vN+1 documents stay readable as plain JSON (D-115) to re-enter by hand.
- A client refused for its version reloads its manifest. A page from before a rollback that posts vN+1 gets the refusal naming both versions and follows the server's manifest.
- Nothing older is rewritten (D-66 stands). At an upgrade only the documents in use come forward: the edit document, the published plan set, the networks of the pick in force and the picks still to come, the policy, the work rules. A retired or expired pick stays in its version until it is used again (a republish), and comes forward then.
- An archive reads two ways. As it was: its bytes and its version's published schema, the default for an audit. Or as it is today: brought forward on read, deterministically, never stored; an audit view offers this where a migration note says something was dropped.
- The migration module is cut by use: operational pieces linked only where their documents arrive (the server's receive, a page's restore), and one archive piece holding every chain.
- A backup restores into a build at least as new as the backup; an older build refuses it, naming both versions.
Context: `docs/units/document-versioning.md`. Rollback is a pointer
moved back, never an inverse migration, so the bytes from before an
upgrade stay addressable. But a newer build that has written vN+1
documents leaves a rolled-back vN build unable to read them (a module
refuses a version it does not speak, and never rewrites it). The
server commits each pointer forward at startup, and the browser moves
each kind forward at restore, so the write happens on the upgrade's
first run.
Options:
- Read one version ahead: a release that will write vN+1 is preceded
  by a release that reads vN+1 (and keeps writing vN), so the build
  before any upgrade can always read what the upgrade writes. Two
  releases per format change.
- Write after it holds: an upgrade reads and serves vN+1 but writes
  vN until it has run cleanly for an agreed time, then commits
  forward. One release; the window delays the new format.
- Roll back the data too: a rollback moves every pointer back to its
  pre-upgrade head; commits made since are replayed through the old
  build or lost. Simplest code; it can lose edits.
Blocks: rollback in `docs/units/document-versioning.md`.

### D-80 — Where do retired build artifacts live?
State: Unanswered.
Context: `docs/units/release-artifacts.md`. Every `just caps` deletes
and rewrites `registry/` and `web/public/caps/`; git keeps the source,
not the bytes, and the bytes cannot yet be rebuilt exactly. Rolling
code back, and expand-then-contract, need every deployed build kept.
Options:
- In the sync store: module, assembly, and app-shell bytes as blobs,
  named by the manifests the `manifest` pointer already logs. One
  store, one backup (D-82), the same verification; the store grows
  by each release's bytes.
- A separate artifact store (a directory or bucket keyed by hash),
  beside the registry: keeps release bytes out of the agency's data.
- Release archives (a tarball per release, with its manifest) kept
  wherever the agency keeps releases.
Blocks: retention in `docs/units/release-artifacts.md`.

### D-81 — How are op and HTTP contracts versioned?
State: Unanswered.
Context: `docs/units/release-artifacts.md`. Op requests and responses
have no version of their own (the kernel version is one number for
the whole ABI), and neither does the server's HTTP surface. The
parts of a deployment ship at different times, so a host can send an
old shape to a newer module or server. Example: cap-plans `publish`
gained a required `today` field on 2026-11-05.
Options:
- Additive only, enforced by a gate: a field is added optional or an
  op is added; nothing changes meaning; removal happens only after no
  deployed host sends the old shape. No version numbers on the wire;
  the generated schemas are diffed against the last published
  registry.
- A version per op (and per endpoint), declared in the manifest; a
  host names the version it speaks; a module answers each version it
  keeps.
- A version prefix on the HTTP surface (`/v1/…`) plus additive-only
  ops.
Blocks: contract versioning in `docs/units/release-artifacts.md`.

### D-83 — What does each deployed device report, and to where?
State: Unanswered.
Context: `docs/units/observability-rollout.md`. Nothing reports what
production runs. Stop displays are the agency's devices; riders'
phones are not, and a rider's location or stops are theirs.
Options:
- Displays and the server only: displays send a heartbeat (builds,
  pins, last good poll, memory, errors) to the server; riders' phones
  send nothing.
- The same, plus anonymous error reports from riders' phones (the
  build and the error, no location, no stops), opt-in or opt-out.
- An external monitoring service as the sink (the server forwards,
  or devices send directly), instead of the agency's own server.
Blocks: reporting in `docs/units/observability-rollout.md`.

### D-84 — Who takes a build first, and what counts as healthy?
State: Unanswered.
Context: `docs/units/observability-rollout.md`. Under D-63 every
display applies a new build at its next poll, so a bad build reaches
every stop at once. Rings need a first cohort and a health rule a
display can check about itself.
Options:
- A named canary set (a few displays the agency chooses, e.g. at the
  transit centers staff pass daily), promoted after a fixed time
  (e.g. a full service day) with no failed health check.
- A percentage of displays, picked by id, widened in steps.
- Health as: polls succeed, the board renders, memory stays under
  the D-68 budget, no reset under the ceiling, for the window.
Blocks: rings and the automatic revert in
`docs/units/observability-rollout.md`.

## Agency data

### D-10 — Agency questions on C-TRAN's data
State: Decided.
Answer: 2026-11-05 — transfers: no caveat for what the feed does not
carry. Station 106 is a transit center, but a transfer point is the
feed's own flag; nothing infers one, and RAPTOR computes transfers
regardless, so `transfer_stops: 0` on C-TRAN is the data, not a defect.
Vehicle names: yes, the id is what C-TRAN calls a bus, but not coupled
to C-TRAN — the rule is generic: with no label, fall back to the id.
Context: two findings wait on facts only the agency has, not on a
design choice.
- QAF-9: should transfer inference fire on C-TRAN (station 106 and
  its bays all read `transfer: false`)? Or does C-TRAN mean its
  stations only as walking groups?
- QAF-48: vehicle labels are empty, so the late list shows ids
  like "CTRAN7003". Is that id what a dispatcher calls the bus? If
  not, what is?
Blocks: nothing. QAF-9 and QAF-48 are deleted; pins: analysis
`network_stats` (`transfer_stops` counts flagged stops where two
routes meet) and rt `live` summary `late[0].label` (the id fallback).

### D-115 — Must a document be readable without the app?
State: Decided.
Answer: 2026-11-06 — "documents should also be open formats that can be simply understood without requiring the app." Encoded as: every persisted document is plain JSON (gzip allowed) or an external open format (a GTFS zip, a GTFS-rt protobuf, GeoJSON), never a private encoding; a document on its own says what it is — its kind and its version inside it (extending D-62); and every kind's schema and a worked example, per version, are published as plain files beside the documentation (`registry/schemas/`, `docs/FORMATS.md`), so a person or another tool reads a document with no module, no wasm, and no running deployment.
Context: `docs/units/document-versioning.md`. Documents leave the
app as files: an export, a `caps-run` load, a store blob, a backup.
The store's blobs already read without wasm (`docs/FORMATS.md`), but
a network or an edit document copied out of IndexedDB names neither
its kind nor its version.
Blocks: nothing; `document-versioning` scope 2 and 6 carry it.
