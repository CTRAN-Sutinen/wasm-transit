# Decisions — open

This is the only record of the questions waiting on the user. When
an agent hits a decision that the user's intent, the GTFS spec,
and the architecture laws leave open, the agent writes it down here
and does not build on an answer it picked (AGENTS.md, "Clarify,
don't assume"). The entry names the work the decision blocks, so
the rest of the work can go ahead while the decision is reviewed.

## The rules

- **One entry per question**, in this format:

  ```
  ### D-N — the question
  State: Unanswered | Encoded provisionally
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
- **A decided entry is deleted.** In the same change, the answer
  goes into the law that pins it and the doc that owns its topic
  (`docs/ARCHITECTURE.md`, `docs/units/`, `docs/QA-FINDINGS.md`),
  and the `Blocked: D-N` lines are removed. Git is the archive.
- IDs are never reused; the next new decision is **D-11**.

## Summary

| id | question | state | blocks |
|---|---|---|---|
| D-1 | Is a trip's start its first arrival or its first departure? | encoded | nothing |
| D-2 | What does a running-time change touch besides fixed-start trips? | encoded | nothing |
| D-3 | Which timing profile does the import give a pattern? | encoded | nothing |
| D-4 | Is a shape part of a pattern's identity? | encoded | geometry split (U8), QAF-76 |
| D-5 | Does the import keep a block that breaks the spec? | encoded | nothing |
| D-6 | Does "frequent" count terminal arrivals? | unanswered | QAF-78 |
| D-7 | How early is too early? | unanswered | QAF-71 |
| D-8 | Do timetable edits act on the whole service family? | unanswered | QAF-65 gestures, QAF-44 |
| D-9 | Where does required recovery time come from? | unanswered | QAF-62, QAF-26 |
| D-10 | Agency questions on C-TRAN's data | unanswered | QAF-9, QAF-48 |

## The trip model (U8)

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

### D-4 — Is a shape part of a pattern's identity?
State: Encoded provisionally.
Context: the import groups trips into patterns by (route,
direction, stops, timepoint flags, shape). Two trips with the same
stops and different `shape_id`s become two patterns.
Options:
- **Yes (encoded).** The path lives on the pattern, and the export
  writes one shape per pattern.
- No. The pattern is its stops, and the shape becomes a per-trip
  reference. This adds a field to the trip, and the boundary keeps
  the shape ids.
Pinned by: `trip_import_law_test.rs`, and the counts in
`import_regression_test.rs` and the caps smoke (a trip with a
shape and one without are different patterns).
Blocks: the geometry split in `docs/units/u8-gtfs-substrate.md`
(alignments stop being keyed by pattern). Also QAF-76, where 012
direction 1 answers 16 patterns with identical stops, so whether
that grouping is a defect depends on this answer.

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

### D-7 — How early is too early?
State: Unanswered.
Context: QAF-71. The default `OnTimeWindow` has
`early_secs: None`, so a vehicle is never early. Early is not on
time, but the threshold is a policy number.
Options: 0 s (any earliness is early); a grace period (for example
59 s, so the minute shown on a sign still matches); or no default,
so the host must pass a policy.
Blocks: the fix for QAF-71 (`transit-live` and its laws).

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
