# QA findings — open

What real-data dogfood sessions (`docs/TESTING.md`, the dogfood
rules) found that is still true: defects, op gaps, contract gaps,
model gaps, data gaps. Open findings only — a finding is DELETED
in the change that fixes it, and that change names its pin (the
test, gate, or session output). Git is the archive.

Format, grouped by theme:

```
- **QAF-N TYPE** — statement.
  Pin: the caps-run ops (or UI steps) that reproduce it.
```

IDs are never reused; the next new finding is **QAF-65**. A
session that re-confirms a finding adds one line under it
(`Re-confirmed: <date>, <what>`) — a finding confirmed three times
is the next unit (`docs/units/`).

## Reference data

- Feed: `testdata/ctran-gtfs.zip` (C-TRAN GTFS, NOT COMMITTED —
  licensed; fetch from c-tran.com), `sha256:aa8dbc62833237d754
  96b6ec04c2a45fe10ca2a957f63d63b51ddaae85a7dc4f`, version
  20260913-20270109. rt payloads from gtfs-rt.c-tran.org.
- Network import: `sha256:2e27c511aaffb63998166c7cd196d714
  d48c8b14f63059b7b70fa52b3fc049bb` (findings from before
  2026-10-27 were seen on `89631f1a…`, the import before per-trip
  run times and the fidelity columns; before that on
  `bceda1be…`, the import that kept direction_id — the same
  service).
- Population layer: `testdata/clark-population.geojson` (`just
  population`), `sha256:a17d4372bef1122fa8152fe0c8b6f240db2a9d40
  b1dbdb4cfb7c6395505a2602`.
- The four proposals the timetable findings use, all on 1-WKDY
  against the import:
  - **A** — 019 every 15 min, 06:50–08:50 out / 07:09–09:09 in
    (`set_headway` ×2).
  - **B** — 009 evening span to 21:05 out / 21:27 in (`set_span`
    ×2).
  - **C** — 006 to Columbia Shores +3 min, 15:00–18:00
    (`adjust_running_time`).
  - **D** — a new stop on 009 outbound; drop stop 2216 from 019
    inbound (`insert_direction_stop`, `remove_direction_stop`).

## Import fidelity

- **QAF-62 DATA GAP** — imported layovers are the whole observed
  gap between two trips, so any later arrival or earlier start
  reads as infeasible: C's +3 min breaks 6 blocks, and A's
  two-minute clockface moves break 3 layovers on 019 (1-WKDY7801,
  7802, 7804). The model has no split between required recovery
  and slack, so "does the cycle still work?" has no real answer.
  Pin: proposals A and C, author `apply` `issues`.
- **QAF-63 DATA GAP** — GTFS direction_id groups branches, not only
  variants: 006 direction 1 holds Fruit Valley (8 min) and VA
  Medical Center (12 min) patterns leaving at the same minutes all
  day; the direction is named "Fruit Valley" by 31 headsigns to 30.
  A planner reads these as two branches.
  Pin: directory `timetable` 006 "1" 1-WKDY.
- **QAF-9 DATA/MODEL** — `transfer_stops: 0` across C-TRAN while
  the station board pools bays through parent-based walking groups.
  Is transfer inference expected to fire on real feeds, or is the
  fixture the only world with transfers?
  Pin: analysis `network_stats` (`transfer_stops`); board at stop
  9021 pooling bays 9021–9027.
- **QAF-48 DATA GAP** — C-TRAN's vehicle labels are empty, so the
  late list names buses by internal id ("CTRAN2269"). The fallback
  is honest; whether that id is what a dispatcher calls the bus is
  an agency question.
  Pin: rt `live` summary `late[0].label`.

## The model

- **QAF-61 MODEL GAP** — running time belongs to the pattern, so a
  weekday change changes every service type that runs it: C also
  breaks 2-SAT601, 3-SUN601, and 4-NYE601/602. Planners usually
  time weekends separately; the model can only do that by copying
  the pattern.
  Pin: proposal C, `apply` `issues` (the SAT, SUN, NYE blocks).
- **QAF-44 MODEL GAP** — service types duplicate rather than
  relate: 4-NYE carries a full copy of 1-WKDY's pattern trips
  (identical starts on all four 006 patterns), so a weekday change
  is mirrored by hand and silently misses New Year's Eve. There is
  no "same as weekday, except" relation.
  Pin: the document's `4-NYE:pat-006-*` pattern trips equal
  `1-WKDY:pat-006-*` start for start.
- **QAF-36 OP GAP** — the edit document is diversions only: no trip
  cancellation, hold, or vehicle swap. A dispatcher's first
  reaction to a 24-minute-late trip (cancel or cover it) has no
  vocabulary. A new edit kind is a sync-document change.
  Pin: `EditRecord` carries one payload, `diversion`; cancelling
  agency trip 755768 cannot be authored.

## Resources and comparison (the planner's core loop)

- **QAF-3 OP GAP** — the fleet summary op exists (`runcut
  fleet_summary`: blocks, peak vehicles and when, platform /
  revenue / layover hours, over the effective day), but the
  PROPOSAL question is still host arithmetic: a proposal's added
  trips are `unassigned_trips`, so "how many more vehicles does it
  need" has no answer — unassigned trips have no block, and a
  vehicle-need is a block assignment the model does not make.
  Per-service-type scoping of the summary is open too.
  Pin: 2026-10-27 `fleet_summary` on the weekday import — 97 blocks,
  77 peak at 16:50, 1003.78 platform h = 803.61 revenue + 200.18
  layover (revenue cross-checks `network_stats` exactly).
- **QAF-20 OP GAP** — the scenario diff is half there: per route
  (analysis `service_type_compare`) and per timetable (directory
  `timetable {compare_hash}`) are answered. Coverage deltas still
  take two `type_coverage` answers and fleet deltas have no op
  (QAF-3).
  Pin: two `type_coverage` calls on base and fork; no op takes two
  network hashes for coverage or fleet.
- **QAF-64 CONTRACT GAP** — a retimed trip reads as removed plus
  added. A's `set_headway` moves 08:11, 08:41, 09:11 two minutes
  onto the clockface, but the timetable comparison answers "3 gone"
  and counts them among the 7 added: it compares by trip identity
  (pattern trips, start), and a retime changes the start. A planner
  reads "3 gone" as a cut. Needs the edit's own account (`apply`
  reporting what it moved) or a match in the comparison.
  Pin: proposal A in the service-design UI (`timetable-delta`).
- **QAF-26 OP GAP** — no recovery view: `block_trips` carries
  `layover_after_secs` and durations, but nothing folds them into
  layover-to-runtime per trip or pattern. (Meaningless until
  QAF-62 separates recovery from slack.)
  Pin: the 347.73 layover hours in QAF-3's pin were summed
  host-side.
## Coverage

- **QAF-18 DATA/MODEL** — point layers bias coverage: a tract's
  population sits at its centroid, so rural tracts read unserved
  where their developed part has service, and the radius swings the
  answer by 20 points (28.9% at 500 m, 49.1% at 800 m). Support
  areal layers, or document the resolution floor.
  Pin: `type_coverage` on the population layer at 500 and 800 m;
  `load_population_layer` answers `points: 115` with no resolution
  guidance.
- **QAF-28 OP + DATA GAP** — coverage is population-shaped: the
  machinery takes any weighted layer (jobs, seniors, zero-car
  households), but the vocabulary is population and no other layer
  exists. Job access is the standard companion.
  Pin: `load_population_layer` is the only layer op.

## Authoring contract

- **QAF-52 CONTRACT GAP** — the edit vocabulary is not discoverable
  through the runner: `caps-run tools author` gives `apply` one
  paragraph and one example for ~50 edit kinds; their shapes are
  only in the generated TS types. Generate per-kind examples the
  way `gen_manifest` does for ops.
  Pin: `caps-run tools author` — no `add_start` example.
- **QAF-53 OP GAP** — the `no_service_in_plan_period` warning names
  the plan by id; a host-minted id is a UUID, so the UI shows
  "2027-01-10, 03862157-…" instead of the plan's name.
  Pin: `apply` `set_plan "Winter 2027" 2027-01-10` → warnings.
- **QAF-54 OP GAP** — no gesture carries a calendar into a pick: a
  fork after the import's calendar end (2027-01-09) runs nothing,
  and the fix is one `set_service_range` per service type (13 on
  C-TRAN).
  Pin: the same Winter 2027 fork.

## Live operations

- **QAF-31 OP GAP** — no live ripple: `block_trips` ripples edits
  through layovers, not rt delay, so "will this late bus make its
  next trip?" is host arithmetic.
  Pin: block 1-WKDY/311002 — trip 755720 at +370 s against a 600 s
  layover before the 078 at 36600 (230 s left, by hand).
- **QAF-50 OP GAP** — a diverted trip's line drops its alignment
  for the WHOLE trip (straight lines between kept stops) and
  ignores the traced detour polyline. Splice: alignment to the
  first anchor, the record's detour, alignment from the last.
  Pin: runcut `trip_path` with an edits hash (10 points).
- **QAF-49 OP GAP** — the live document is 388 KB per snapshot,
  re-minted every poll, and each module keeps every registered
  document. Eviction is `docs/units/module-runtime.md`'s.
  Pin: `JSON.stringify(live).length` on a C-TRAN snapshot.

## Beyond blocks (runs, work rules)

Runcutting stops at blocks + layovers by design (AGENTS.md
non-goals); these are the layer above, logged for when it starts.

- **QAF-15 OP GAP** — no block structure summary: per-block span,
  span distribution, internal gaps / cut candidates.
  Pin: 2026-10-26 by hand — 93 weekday blocks bucket 22 (<4 h) /
  12 / 3 / 15 / 34 (14–18 h) / 7 (>18 h); only 2 have an internal
  gap ≥ 90 min, so every cut is a mid-layover policy cut.
- **QAF-11 OP GAP** — no runs layer: blocks-to-runs cutting does
  not exist, so a priced run menu cannot be produced.
  Pin: 2026-10-26 by hand — 140 runs over 81 blocks + 25 trippers
  under assumed 8 h max platform / 15+15 report-pull / 2 h min
  piece.
- **QAF-12 OP GAP** — no work-rule policy document: pay parameters
  have no home on the bus, so run legality cannot be validated.
  Pin: QAF-11's menu moves entirely with its assumed parameters.
- **QAF-14 OP GAP** — no run view: report/pull time and place,
  platform, pay, pieces, deadhead per run.
  Pin: block 1-WKDY5004 cut by hand into three pieces reporting
  05:25 / 14:12 / 22:23.

## Data the bus does not have

- **QAF-5 DATA GAP** — no demand axis: no ridership or load data,
  so resource questions price cost but not benefit (cut the
  post-22:00 tail?).
  Pin: 10 blocks run past 23:00 and no op says who rides them.
- **QAF-17 DATA GAP CATALOG** — the planning data absent from the
  bus: pay rates and multipliers; relief points; garage locations
  and deadhead costing (derivable via cap-streets once a base
  exists as data); the workforce (headcount, seniority, extra
  board); observed segment runtimes by hour; historical rt
  aggregates; layover facility capacity; OD demand; fares and
  elasticity; fleet by type and spare ratio; battery range; the
  paratransit coupling; overlay drivers (school and college
  calendars behind the 1-WKDY/15, /16, /30, /31 overlays);
  seasonal and event variation.
  Pin: none of these questions can be asked through `caps-run`.
