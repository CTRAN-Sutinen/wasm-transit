# C-TRAN 2045 — the long range plan, as dogfood material

`C-TRAN_2045-Selected Pages.pdf` is 20 pages from the C-TRAN 2045
Public Review Draft (October 2026): the Vision Network, the three
financial scenarios with their maps, the per-route service tables
(Appendix A), and the capital project list (Appendix B). This folder
holds that excerpt, what was extracted from it so no session redoes
the work, and the service-change sessions built on it
([SESSIONS.md](SESSIONS.md)).

## What is here

| File | What it is |
|---|---|
| `C-TRAN_2045-Selected Pages.pdf` | the source excerpt |
| `scenarios.csv` | Appendix A transcribed: every route × scenario × day type (234 rows) |
| `capital-projects.csv` | Appendix B transcribed: 39 projects, cost and phase |
| `current-feed.csv` | the test feed (`testdata/ctran-gtfs.zip`) measured in the same shape as `scenarios.csv` |
| `current-feed.mjs` | the script that writes `current-feed.csv` (`node LRP/current-feed.mjs > LRP/current-feed.csv`) |
| `SESSIONS.md` | the scenario sessions: the backlog, how to run one, each run's log, the findings |
| `run.sh`, `walks/` | the sessions that have walk scripts (`tools/ui-shot.mjs` steps) |

## The excerpt, page by page

| PDF page | Plan page | Content |
|---|---|---|
| 1 | 36 | "A Vision for 2045": the Vision Network (34 routes, 7 frequent corridors at 20 min or better) and its seven themes |
| 2 | 37 | Figure 17, the Vision Network map |
| 3 | 38 | Figure 18, service types by land-use context (BRT 10–15 min, frequent 15–20, local 30–60) |
| 4 | 39 | Constraining the vision: the three scenarios, the shared assumptions |
| 5 | 40 | Reduce Service, in words |
| 6 | 41 | Figure 19, Reduce Service map (routes eliminated, span and frequency cut) |
| 7 | 42 | Maintain Service, in words |
| 8 | 43 | Figure 20, Maintain Service map |
| 9 | 44 | System Growth, in words |
| 10 | 45 | Figure 21, Growth map (new routes, improved frequency/span/days) |
| 11–12 | 86–87 | Appendix A, Reduce Service table (weekday; Saturday and Sunday) |
| 13–14 | 88–89 | Appendix A, Maintain Service table |
| 15–16 | 90–91 | Appendix A, Growth table |
| 17–20 | 92–95 | Appendix B, capital project list |

To look at a map again, render its page:
`pdftoppm -f 10 -l 10 -r 150 -png "LRP/C-TRAN_2045-Selected Pages.pdf" $TMPDIR/growth`
(the route lines are only readable at 150 dpi or more, and only in
part: the maps give corridors, not stop lists). To re-read a table:
`pdftotext -layout -f 15 -l 16 "LRP/C-TRAN_2045-Selected Pages.pdf" -`.

## The scenarios in brief

All three assume the current service and taxing boundary, ADA
paratransit (C-VAN) growing 2% a year, the Regional Transportation
Plan's projects (the Interstate Bridge Replacement and MAX Yellow Line
extension on every map), and the Baseline capital projects: **The Vine
on Highway 99** (route 52) and **the Fourth Plain extension** (route 53).
The plan counts service in **annual fixed-route service hours**.

- **Reduce Service** (sales tax stays at 0.7%). Hours peak at 365,900
  in 2029, then fall 27% to 268,000 by 2031. Most locals drop from 30
  to 60 minutes (6, 12, 25, 32, 34, 72, 78, 92), Parkrose 65 from 15 to
  30, the Vine to 15 minutes; 47 falls to 4 trips and 48 to 8. No
  express routes (the map marks 105, 105X, 164, 190 eliminated), no SR
  14 (41). The Current is held flat.
- **Maintain Service** (0.9%). Holds 2030 service, 365,900 hours. It is
  today's network with the two Vine lines in place of 71 and 74, a new
  **101 Vancouver to Portland** express, and minor schedule changes.
  `current-feed.csv` matches its table almost route for route.
- **System Growth** (0.9%, then 1.1% around 2040, which needs a county
  of 700,000 and a city of 75,000 under RCW 82.14.045). Phase 1
  preserves 2030 service; Phase 2 (after 2040) grows to 450,000 hours.
  Locals run to about 10:30 pm to 12:45 am, every local gets Saturday
  and Sunday service from about 6:00 am, weekends become one schedule (the
  Saturday and Sunday columns are identical), and the network is
  restructured: new routes 40, 49, 76, 79, 86, 88, 94; 7 and 76 become
  Frequent (20 min); 72 and 78 give way to 79 and 76; several routes are
  renamed (2 Lincoln → Fruit Valley, 32 → Waterfront/Andresen, 47 →
  Yacolt, 48 → La Center, 19 Salmon Creek → 119th Street).

The routes in today's feed that no scenario table lists: 031 Hazel
Dell, 060 Delta Park Regional, 071 Highway 99 and 074 East Fourth
Plain/162nd (the Vine lines replace them), 106 (105X), 500 and 899.
019 Salmon Creek is in the Growth table only (as 119th Street). Where
the plan does not say what happens to a route, a session asks; it does
not guess.

## Reading the tables against the feed

- `scenarios.csv` keeps the plan's words: times as printed, a peak
  frequency in minutes, or a trip count in `note` ("8 trips") where the
  plan gives one; `no service` rows for a day the route does not run.
  Weekend rows carry the weekday row's name and type. Quirks of the
  source: page 90's table is titled "MAINTAIN SERVICE SCENARIO" but is
  the Growth table; Technology Systems & Equipment has no phase.
- `current-feed.csv` is measured, not the plan's method (its header
  comment says how): the first and last trip start, 60 ÷ the most
  trips one direction starts in a clock hour, the most blocks on the
  route at once, and every trip that day. Its dates are Wed 2026-11-18,
  Sat 2026-11-21, Sun 2026-11-22.
- The plan's "peak frequency" reads as the base frequency more often
  than the measured peak: today's 92 is 20 minutes at its peak but the
  plan's 30, and its trip count (62) is today's to the trip. Compare
  trip counts and spans first; treat frequency as a range.
- Route ids: the plan writes 2, the feed 002; the Vine is 50/51 in the
  plan and Vine-Green/Vine-Red (route ids 050/051) in the feed.
