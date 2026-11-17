# C-TRAN 2045 — service-change sessions

Dogfood sessions of the **service-change** app
(`service-change.html`), each one step a planner would take toward
the long range plan ([README.md](README.md)). A long range plan is
carried out a piece at a time, so each session is one service
change: a fork, a few routes changed, the vehicles and hours checked,
the proposal compared and published. The tables and the current
feed's numbers are already extracted (`scenarios.csv`,
`current-feed.csv`); a session starts from them, not from the PDF.

## Running a session

- A **fresh** `just qa`, capped (`docs/TESTING.md`; its store is in
  memory, so every run starts from the C-TRAN feed alone):
  `systemd-run --user --unit=qa-loop -p MemoryMax=8G --working-directory=$PWD -E PATH="$PATH" -E HOME="$HOME" just qa`.
  If a `qa-loop` is already running for other work, run a second loop
  on other ports instead of restarting it (its own server on a spare
  port with `-store mem://`, vite with `TRANSIT_SERVER` pointing at it
  and its own `--port`), and give `LRP/run.sh` its URLs:
  `BASE=http://127.0.0.1:5177 LRP_SERVER=http://127.0.0.1:8095 LRP/run.sh`.
- `LRP/run.sh` runs the sessions that have walks, in order (`run.sh 1`
  runs one). Each prints PASS/FAIL per check and exits 1 when any
  failed; pictures land in `.qa/lrp/`, and the browser profile is
  `.qa/lrp-profile` (`tools/ui-shot.mjs --profile`), apart from the
  route 99 test's.
- **Branching.** Every session without "chains" forks the plan in
  force (the C-TRAN feed) and stands alone. A session that chains forks
  an earlier session's published pick, in the same server's life.
- The test feed runs through **2027-01-09**; picks fall inside it
  (`walks/lib.mjs` `sundayAfter`). A session run after that date
  needs a newer feed.
- A session without a walk is run by hand in the UI, and gets a walk
  (`walks/sNN-*.mjs`, added to `run.sh`) once it has run.
- Each run adds to its session's **Log** below; a finding gets the next
  QAF id (`docs/QA-FINDINGS.md` keeps the counter) and goes under
  **Findings** here. A fixed finding is deleted with its pin.
- Record each session's weekday, Saturday and Sunday revenue-hour
  change (the Proposal cards). The plan budgets in annual service
  hours (365,900 → 268,000 or 450,000), which no op answers (D-108), so
  the sum across sessions is kept here by hand.

## What the app can do, against what the plan needs

| The plan needs | In service-change today |
|---|---|
| A span moved, either end | Route stage, Span (`set_span`) |
| Faster or slower running time over a band | Route stage, Running time (`adjust_running_time`) |
| A stop added to or dropped from a route | Route stage, Stops (`insert_direction_stop`, `remove_direction_stop`) |
| A new route on existing or new stops | Route stage, New route (new stops come from the Stops app) |
| Calendar: keys, weekdays, dates, holidays | Plan & calendar, Service keys |
| Vehicles for new trips | Vehicles & hours, adopt the suggested blocks (new blocks only: `docs/units/runs-layer.md` item 5) |
| A frequency change on a running route | **No.** Blocked: D-54 (QAF-90) |
| Service on a day the route does not run | **No.** QAF-118 |
| A route renamed or retired | **No.** QAF-120 |
| A route rerouted | **No.** No gesture draws a pattern along the streets (`docs/units/streets-model.md` item 4) |

## The sessions

Status: **pass**, **blocked** (names the finding or decision), or
**open** (not yet run). Plan figures come from `scenarios.csv`; today's
from `current-feed.csv`.

### Growth, a piece at a time

| # | Session | Plan change (Growth, Appendix A) | Status |
|---|---|---|---|
| 1 | Evening span, 9 and 92 | 9 Felida weekdays 6:00 am–10:15 pm (today 6:24 am–6:27 pm); 92 Camas/Washougal to 12:30 am (today 9:56 pm) | **pass** (11/12; QAF-119) — walk `s01` |
| 2 | Felida weekends (chains on 1) | 9 Saturdays and Sundays 6:00 am–10:15 pm, 60 min (today none) | **blocked**, QAF-118 — walk `s02` |
| 3 | Evening span, the rest of the locals | weekday last trips: 6 to 10:15 pm, 7 to 12:45 am, 12 to 10:30, 25 to 10:45, 30 to 10:45, 32 to 11:00, 34 to 12:15 am, 19 to 10:30 | open |
| 4 | Earlier weekends | Saturday and Sunday first trips about 6:00 am on 6, 7, 19, 25, 30, 32, 34 (today 6:23–8:20 am); span only, frequency unchanged | open |
| 5 | Vine late night | Vine-Green and Vine-Red to 1:15 am every day (today 12:40 weekdays, 12:10 weekends); tests trips past 25:00 | open |
| 6 | New route 86, 86th Avenue | 30 min weekdays 6:00 am–10:30 pm, 60 min weekends; on existing stops where the corridor has them | open |
| 7 | New route 94, Lakeshore/Washougal Waterfront | 60 min, 6:00 am–9:15 pm every day; new stops through the Stops app first (as route 99 does) | open |
| 8 | North county: 48 La Center, 49 Battle Ground/Ridgefield, 40 Battle Ground/SCTC | 48 renamed and cut back, two new Limited routes (120 and 90 min) | blocked in part: the rename, QAF-120; 40 and 49 can be built |
| 9 | 76, 78th Street/136th Avenue (Frequent) | new 20-min route in place of 78; with the 76th/78th and 136th Ave corridor projects | blocked in part: retiring 78, QAF-120; 20 min on a new route can be built |
| 10 | Express weekends | 105 and 164 Saturdays and Sundays every 90 min, 6:00 am–11:15 pm | blocked, QAF-118 |
| 11 | One weekend schedule | Growth runs Sunday as Saturday; make 3-SUN carry 2-SAT's service | open (a calendar probe: what the Service keys stage allows) |

### Baseline and capital projects (the next six years)

| # | Session | Plan change | Status |
|---|---|---|---|
| B1 | The Vine on Highway 99 (route 52) | Vancouver Waterfront ↔ WSU Vancouver on Main St and Highway 99, 15 min, 4:20 am–12:30 am weekdays (Reduce and Maintain tables); stations through the Stops app; in place of 071 | blocked in part: retiring 071, QAF-120 |
| B2 | Vine Fourth Plain Extension (route 53) | Van Mall TC ↔ Fisher's Landing TC on Fourth Plain and 162nd, 20 min, 4:45 am–12:30 am every day; in place of 074 | blocked in part: QAF-120 |
| B3 | Chkalov infill station | a westbound Red Line station at Mill Plain & Chkalov: the station through the Stops app, then inserted into Vine-Red westbound | open |
| B4 | Corridor speed | the service-quality projects as running time: SR-503/117th (route 7), I-205 bus-on-shoulder (164, 65, 67), 76th/78th (78); what each saves in vehicles and hours | open |
| B5 | Stop consolidation | the Essential Infrastructure program (ADA): drop closely spaced stops on one corridor, the stops retired in the Stops app | open |

### Reduce, in the order a cut would go

| # | Session | Plan change | Status |
|---|---|---|---|
| R1 | Locals to hourly | 6, 12, 25, 32, 34, 72, 78, 92 from 30 to 60 min weekdays | blocked, D-54 (QAF-90) |
| R2 | Express and SR 14 end | 105, 105X, 164, 190, 041 retired | blocked, QAF-120 |
| R3 | Limited trips cut | 47 to 4 trips, 48 to 8 | blocked: no trip removal in the UI (part of QAF-118's gap) |

## Log

### Session 1 — 2026-11-17

Pick 2026-12-06 (forked from C-TRAN, the feed in force). Walk
`walks/s01-evening-span.mjs`: 11/12.

- 092 to Camas & Washougal: last trip 21:33 → 24:30; three trips
  added at 22:32, 23:31, 24:30. To Fisher's Landing TC: 21:56 →
  24:00, trips at 22:58 and 24:00.
- 009 to 139th St & 10th Ave: 7:00–18:05 → 6:00–22:15 (evening trips
  18:05, 19:07, 20:10, 21:12, 22:15). To 99th Street TC: last trip
  18:27 → 21:45; the 6:00 first trip did not apply (QAF-119).
- The span op adds trips in even steps that land on the typed time
  (`crates/transit-network/src/timetable_edit.rs`, `set_span`), so
  the new evening trips leave the route's clock-face minutes (:05 →
  :07, :10, :12, :15). By design today; a planner typing "22:15" may
  have meant "about then".
- Vehicles & hours: 13 trips on no block; adopting gave 3 new blocks
  (one a single 21-minute trip, route 9's 6:00), 77 peak vehicles
  before and after. The suggestion never extends 9201/9202 into the
  evening (`docs/units/runs-layer.md` item 5).
- Proposal against C-TRAN: weekdays 1,773 → 1,786 trips, revenue hours
  800.9 → 805.8 (+4.9 a weekday, about +1,250 a year over ~255
  weekdays); the Tuesday group the same; Saturday and Sunday unchanged.
  The card notes 4-NYE "0 routes changed": New Year's Eve keeps the old
  span (QAF-44).
- Published "takes effect 2026-12-06"; `GET /feed` runs 234 trips on 9
  and 92 on Friday 2026-12-04 and 247 on Monday 2026-12-07.

### Session 2 — 2026-11-17

Forked session 1's pick, picking 2026-12-20. Walk
`walks/s02-felida-weekends.mjs`: 1/2. Route 9 on Saturday opens with
0 trips and the Edit panel; Span refuses: "route 009 runs no trip in
this direction under 2-SAT — add one first". Nothing on the page adds
one (QAF-118). The session stops there.

## Findings

IDs continue `docs/QA-FINDINGS.md`'s sequence.

- **QAF-118 UI GAP** — Service change cannot start service where a
  route runs no trip: on a day type the route does not run, the Span
  form is offered and refused ("add one first"), and no control adds a
  trip (`add_trip` is on the bus; New route refuses a taken id; the
  Service keys stage moves a route's trips to another key, never
  copies them). The plan's weekend service for 2, 9, 41, 47, 67, 105
  and 164 cannot be built, and no trip can be removed one at a time
  either.
  Pin: session 2, step 2.2.
- **QAF-119 UI COPY** — A span end within half a headway of the
  route's edge trip is dropped (the op's rule), and the status says
  only "Changed. The plan now has 0 issues …": the planner is not told
  the first trip stayed at 6:24.
  Pin: session 1, step 1.3 (009 to 99th Street TC, first trip 6:00).
- **QAF-120 UI GAP** — Service change has no control to rename or
  retire a route: the route's names are set only by New route
  (`upsert_route`), and nothing sends `remove_route` or a retirement.
  Every scenario needs one: the Vine lines replace 071 and 074, Reduce
  retires the express routes, and Growth renames 2, 19, 32, 47 and 48.
  Pin: the Route stage of any route in a fork (its Edit panel offers
  span, running time, one trip and stops only).
