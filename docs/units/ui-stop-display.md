# UI: the stop display

A generated UI for the screen at a stop: everyone standing there
reads, from across the shelter, which stop this is, what leaves
next on each route and where it is going, how long until it
leaves, and the alerts in their way. A passenger information
display (PID). It follows the generated-UI procedure
(`docs/ARCHITECTURE.md`, "Generated UIs"). The UI is built
(`web/src/uis/stop-display/`, `e2e/ui-stop-display.spec.mjs`) and has
run in the browser on C-TRAN in all three layouts, its groups equal
to the session's below for the same snapshot. What is left is an
alert read on the display, and when a stopped route runs next.

Three layouts, picked on the display and taking the whole screen:
a transit center (a station's bays pooled, each group naming its
bay), a single route (the stop most stops are: one route, drawn
large), and several routes at a street stop (its two or three, each a
band across the screen).
They are layouts over one answer; any layout shows any stop.

## Questions, in the order asked

1. Which stop is this?
2. What leaves next here, on each route and to each destination —
   the next three of each?
3. How long until each one leaves?
4. Is that time live, or the schedule?
5. Which bay does it leave from (a transit center)?
6. Has a route stopped running here today, and when does it run
   next?
7. Is there an alert or a detour at this stop?

## Op inventory

The 2026-10-29 session (08:53 PDT): the reference import
(`2beddcb3…`), C-TRAN's live trip updates (160 trips, 0 unmatched;
live `bfb2259d…`), boards at Fisher's Landing TC (106, a station
pin), Andresen & Van Mall Dr (2676, one route), and Van Mall Dr &
94th Ave (701, five routes).

| # | op(s) | state |
|---|---|---|
| 1 | directory `stops_by_ids` | answered |
| 2 | board `grouped_board` (`per_group`, default 3) | answered (`stop_board` alone would need the whole day asked and grouped by the host: 106 answers 191 rows in 9 groups) |
| 3 | the row's `wait` (due, minutes, or a clock time) | answered on every board row (QAF-81's board half) |
| 4 | the row's `status` / `expected_secs` | answered |
| 5 | the row's `stop_name` on a pooled board | answered |
| 6 | a group with no departures left | "stopped" answered; "when next" has no op, QAF-82 — the display says "No more service today" |
| 7 | the board's `alerts`, a group's `skipped` | answered on the fixture; C-TRAN publishes no alerts feed (its Alerts URL answers 404), so no alert has been read on real data |

## Answer graph

Inputs: `network` (the imported document), `edits` (the sync
server's edit document, GET /edits), `snapshot` (the decoded live
feeds), `clock` (the poll's tick), `stop` / `layout` (the
selection, kept in the page's URL so a screen reopens on its stop).

| node | inputs | frame | on screen |
|---|---|---|---|
| directory `stops` (query) | network, query | the setup's stop picker | the setup |
| directory `stops_by_ids` | network, stop | the header | always |
| rt `live` (+ `load_boundary`) | network, snapshot | the header's live mark | always |
| board `grouped_board` (+ `load_edits`, `load_live`) | network, edits, rt.live, stop, clock | the layout's groups, the alerts slot | always |

The poll (every 30 s) re-reads the edit document and the live
feeds and requests `clock` plus whichever changed — the same timer
input `docs/units/ui-ride-now.md` names.

## Done when

- An alert has been read on the display from a live feed that
  publishes one (C-TRAN's Alerts URL answers 404), and `ui.json`'s
  `session` records it.
- QAF-82 is closed (a stopped group says when it runs next) and the
  display reads it.
- Then this file is deleted.
