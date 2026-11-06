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

IDs are never reused; the next new finding is **QAF-100**. A
session that re-confirms a finding adds one line under it
(`Re-confirmed: <date>, <what>`) — a finding confirmed three times
is the next unit (`docs/units/`). A finding whose fix waits on the
user carries `Blocked: D-N`, the question in `docs/DECISIONS.md`.

## Reference data

- Feed: `testdata/ctran-gtfs.zip` (C-TRAN GTFS, NOT COMMITTED —
  licensed; fetch from c-tran.com), `sha256:aa8dbc62833237d754
  96b6ec04c2a45fe10ca2a957f63d63b51ddaae85a7dc4f`, version
  20260913-20270109. rt payloads: trip updates and vehicle positions
  from gtfs-rt.c-tran.org, service alerts from
  https://gtfs-rt.c-tran.com/Alerts.
- Network import: `sha256:c6847464f1b9f7b882abac3e458d54105d92f698
  e7a903d7d195e26e4bb68b37` (the whole spec: every file round-trips;
  C-TRAN's dated calendars read as weekly rules, D-105; findings
  before 2026-11-06 were seen on `2beddcb3…`, the import that kept them
  dated; findings from
  2026-10-27–28 were seen on `2e27c511…`, the import before stored
  trips; before 2026-10-27 on `89631f1a…`, the
  import before per-trip run times and the fidelity columns; before that on
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

- **QAF-63 DATA GAP** — GTFS direction_id groups branches, not only
  variants: 006 direction 1 holds Fruit Valley (8 min) and VA
  Medical Center (12 min) patterns leaving at the same minutes all
  day; the direction is named "Fruit Valley" by 31 headsigns to 30.
  A planner reads these as two branches.
  Pin: directory `timetable` 006 "1" 1-WKDY.
  Blocked: D-44.
- **QAF-44 MODEL GAP** — service types duplicate rather than
  relate: 4-NYE carries a full copy of 1-WKDY's pattern trips
  (identical starts on all four 006 patterns), so a weekday change
  is mirrored by hand and silently misses New Year's Eve. D-8 keeps
  no "same as, except" relation; the gesture panel's warning (D-95)
  names the types that RUN with the edited one, but 4-NYE never runs
  with 1-WKDY (they share no date), so nothing tells the planner the
  copy exists. What is left: a read that names the types carrying
  copies of the trips a gesture changes.
  Pin: the document's `4-NYE:pat-006-*` pattern trips equal
  `1-WKDY:pat-006-*` start for start.

## Resources and comparison (the planner's core loop)

- **QAF-3 OP GAP** — the fleet summary answers a DATE (`runcut
  fleet_summary`; a proposal's vehicle need is `suggest_blocks`'
  `added_vehicles`), never one service type: a planner asking
  "what does the weekday network need" picks a date and gets
  whatever combination of service types runs on it.
  Pin: 2026-10-27 `fleet_summary` on the weekday import — 97 blocks,
  77 peak at 16:50, 1003.78 platform h = 803.61 revenue + 200.18
  layover (revenue cross-checks `network_stats` exactly).
- **QAF-20 OP GAP** — the scenario diff is half there: per route
  (analysis `service_type_compare`) and per timetable (directory
  `timetable {compare_hash}`) are answered, and analysis
  `coverage_compare` answers the coverage churn (gained and lost,
  served and frequent) between two sides the host names (D-99). What
  is missing is which of one plan's service combinations answers to
  which of the other's when a fork renames or splits a type (D-43's
  matching rule), so no UI asks it yet; fleet deltas have no op
  (QAF-3).
  Pin: the service-design coverage frame and ProposalScore's
  "Coverage gained and lost" say so; no op takes two network hashes
  for fleet.
  Blocked: D-43 (the matching rule).
- **QAF-92 OP GAP** — `stop_gaps` reads one stop, not its walking
  group: asked for a station (99th Street Transit Center, stop 110)
  it answers 0 departures, while `timed_transfers` and
  `hub_connections` pool the same station's bays (1,410 connections
  across bays A–J). The hub view's waits go empty exactly where a
  planner looks first.
  Pin: 2026-10-30 analysis `stop_gaps` 110 on 2026-09-14 — 0
  departures; `hub_connections` 110 — 82 route pairs.

## Coverage

- **QAF-98 MODEL** — nothing pins that adding service never lowers
  the whole-day "frequent" answer. The weekday read 3.2% frequent as
  1-WKDY alone (per service id, import `2beddcb3…`) and 3.1% as the
  combination 1-WKDY + 1-WKDY/31 (import `c6847464…`), though a
  combination only adds trips. Not confirmed as one cause: the two
  reads differ in import and grouping, and the suspected mechanism is
  the whole-day test reading headway over a stop's first-to-last
  span, so an extra early or late departure can stretch it. A law
  (more trips at a stop never make it less frequent, or the test says
  why) decides it.
  Pin: 2026-11-06 analysis `type_coverage` members [1-WKDY, 1-WKDY/31]
  on C-TRAN (`c6847464…`, `clark-population.geojson`, 400 m, 900 s,
  min 2) — frequent 15,882 of 516,959, 82 stops.

## Authoring contract

- **QAF-90 DATA/MODEL** — a frequency gesture's new trips run at the
  pattern's early-morning pace in the evening: `set_headway` on 071
  direction 0, 18:00–21:00 every 15 min, under 1-WKDY adds six trips
  timed by the imported profile (D-3: the earliest trip's run time).
  The trip added at 18:00 runs 20 min, and trip 758184 beside it
  (18:15, retimed from 18:12) runs 28 min. Toward 99th Street the
  trip added at 18:00 arrives at 18:23, before the 17:57 trip that
  left ahead of it (18:26).
  Pin: 2026-10-30 directory `timetable` 071 "0" 1-WKDY on the fork
  (`compare_hash` the import) — trip `1-WKDY:pat-071-0-0@64800`
  64800→66000, trip 758184 65700→67380; "1" — trip
  `1-WKDY:pat-071-1-4@64800` 64800→66180, trip 758239 64620→66360.
  Blocked: D-3 (re-opened), D-54.


## Live operations

- **QAF-49 OP GAP** — ride-now receives the whole live document
  (388 KB per C-TRAN snapshot, re-minted every poll) to read two
  things from it: the vehicles on the map and the alerts banner. Its
  board and router read the document by hash inside the page's
  assembly (it never crosses back), and the stop display declines it
  (`with_document: false`), so this is ride-now's alone: each
  snapshot's answer lands in the engine's answer cache, and the page
  heap climbs until the cache's 256 entries fill (C-TRAN, 2026-11-05:
  21.9 → 25.7 MB across ride-now's 36-poll soak). The vehicles and
  alerts it shows want an op's answer (rt's) instead of the
  document.
  Pin: `JSON.stringify(live).length` on a C-TRAN snapshot; ride-now's
  "Memory over time" table in `just perf`.

## Beyond blocks (runs, work rules)

The runs layer (`docs/ROADMAP.md` 7): runs cut from blocks under
work rules, and the bid over them.

- **QAF-15 OP GAP** — the block outline is answered (runcut
  `fleet_summary` `spans`: each running block's span, trips, and
  longest internal gap), but where a block may be CUT is not: the
  span buckets that matter and the cut candidates are work-rule
  questions (piece lengths, relief points), so they wait on the
  runs layer (QAF-11, QAF-12).
  Pin: 2026-10-26 `fleet_summary` — 93 weekday blocks span 22 (<4 h)
  / 12 / 3 / 15 / 34 (14–18 h) / 7 (>18 h); the longest internal
  gaps are 88, 84, and 70 min (1-WKDY/311013, /311015, /311011),
  none ≥ 90, so every cut is a mid-layover policy cut.
  The runs cut: `docs/units/runs-layer.md`.
- **QAF-11 OP GAP** — no runs cut: blocks-to-runs cutting does
  not exist, so a priced run menu cannot be produced.
  Pin: 2026-10-26 by hand — 140 runs over 81 blocks + 25 trippers
  under assumed 8 h max platform / 15+15 report-pull / 2 h min
  piece.
  The runs cut: `docs/units/runs-layer.md`.
- **QAF-12 OP GAP** — the work rules have a home (the kernel's
  `work_rules` document: platform, spread, report and clear,
  pieces, meals, relief points, pay — D-50), but nothing judges a
  run against them, so run legality cannot be validated.
  Pin: QAF-11's menu moves entirely with its assumed parameters;
  no op takes a run.
  The runs cut: `docs/units/runs-layer.md`.
- **QAF-14 OP GAP** — no run view: report/pull time and place,
  platform, pay, pieces, deadhead per run.
  Pin: block 1-WKDY5004 cut by hand into three pieces reporting
  05:25 / 14:12 / 22:23.
  The runs cut: `docs/units/runs-layer.md`.

## Geography

- **QAF-87 DATA GAP** — the geocoder answers no intersection: the
  local Pelias (`just geocoder-build`, pelias/docker `3dfa07d5`,
  portland-metro) answers addresses, venues, and places, but
  "4th plain & andresen" (autocomplete) and "NE 4th Plain Blvd & NE
  Andresen Rd" (search) answer `[]` — D-27's intersections are not
  in its data. A rider types the corner a stop is named for.
  Pin: `/geocode/autocomplete?text=4th plain %26 andresen` and
  `/geocode/search?text=NE 4th Plain Blvd %26 NE Andresen Rd` →
  no features (2026-10-29).
- **QAF-85 DATA GAP** — C-TRAN's Trapeze roads cannot carry
  walking. The export codes every state route CFCC `A10` — I-5 and
  I-205, and also NE Fourth Plain Rd, W Mill Plain Blvd, NE 117th
  Ave, W Main St (Battle Ground), Lewis River Rd, and the on-ramps
  ("ONTO I-5 NB"): 468 street names — so its class cannot tell a
  freeway from an arterial, and it holds no sidewalk, path, or
  stairs. Read as the spec reads CFCC (D-30), walking refuses every
  A1 street: the stops on NE 117th Ave, NE 162nd Ave, and W Main St
  lose their street walks. The fix is the data, not the model: an
  OSM-sourced streets document for walking (the import reads OSM
  `highway`), the Trapeze roads kept for driving and drawing.
  Pin: streets `walks` on the C-TRAN network (`sha256:2beddcb3…`)
  and roads (`sha256:48fe6c89…`): stops 812 and 852 (NE 117th Ave &
  Padden Pkwy, 205 m apart) have no walk; 46 of the 2,924 stop pairs
  within 300 m straight-line have none. Router `plan` 812 → 852 on
  2026-10-29 at 08:20 walks 158 s in a straight line, and over the
  walk table rides route 007 around, arriving 08:51.

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
