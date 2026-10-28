# UI: riding today

A generated UI for someone taking a trip today: find a stop, see
what leaves next and whether it is running late or early, plan a
trip, know the last bus home, and learn about a detour before it
strands them. It follows the generated-UI procedure
(`docs/ARCHITECTURE.md`, "Generated UIs"). The UI is built
(`web/src/uis/ride-now/`, `e2e/ui-ride-now.spec.mjs`) ahead of its
dogfood session, at the user's direction. It has run in the browser
on C-TRAN with the live trip updates and vehicle positions
(2026-10-28, 16:47 PDT, 68 buses). What is left is step 3's
`caps-run` session and step 8's comparison of the two.

## Questions, in the order asked

1. Which stop is mine, by name or near where I am?
2. What leaves next from it, with real-time predictions?
3. Is my bus on time, late, or early?
4. How do I get from here to there, leaving now?
5. When is the last bus home?
6. Is there a detour or disruption on my trip or at my stop?
7. What leaves next from the stops I watch?

## Op inventory

Pinned on the e2e fixture, and seen answering on C-TRAN in the
browser.

| # | op(s) | state |
|---|---|---|
| 1 | directory `stops` (query), `stops_near` (a point: the rider, or a tapped spot) | answered; a colloquial name answers nothing, QAF-70 |
| 2 | rt `live` → board `stop_board` (`live_hash`) | answered; "leaves in N min" has no op, QAF-81 |
| 3 | the board row's `status` / `delay_secs`; the map's buses by status | answered |
| 4 | router `plan_compare` (`live_hash`, `edits_hash`) → `itinerary_paths` | answered; a trip's duration has no op, QAF-81 |
| 5 | router `plan_arrive_by` (no deadline; or a deadline) | answered |
| 6 | board `stop_board` (`alerts`, `skipped`); router `plan_compare` (`notice`) | skipped + notice answered on the fixture; no alert has been read (no fixture carries one, and no alerts URL is configured) |
| 7 | board `watchlist_board` | schedule only, and a station answers nothing: QAF-79; saved stops do not survive a reload: QAF-80 |

## Answer graph

Inputs: `network` (the imported document), `edits` (the sync
server's edit document, GET /edits), `snapshot` (the decoded live
feeds), `clock` (the poll's tick), `query` / `near` / `stop` /
`departure` / `trip` (the selection), `saved` (the watchlist
document), `view` (which panel is on screen).

| node | inputs | frame | on screen |
|---|---|---|---|
| directory `stops` + `routes` | network | RideMap (and every route badge) | always |
| directory `stops` (query) | network, query | PlaceSearch | always |
| directory `stops_near` | network, near | the home panel's nearby cards | always |
| rt `live` (+ `load_boundary`) | network, snapshot | LiveFeeds, RideMap | always |
| board `stop_board` (+ `load_edits`, `load_live`) | network, edits, rt.live, stop, clock, view | RiderBoard | the stop panel |
| board `stop_board`, once per nearby place | network, edits, rt.live, stops_near, clock, view | StopCard | the home panel |
| board `watchlist_board` (+ `load_watchlist`) | network, edits, saved, clock, view | StopCard | the home panel |
| directory `focus` (route) | network, departure | RideMap | always |
| router `plan_compare` \| `plan_arrive_by` | network, edits, rt.live, trip | TripPlan | always |
| router `itinerary_paths` | router.plan | RideMap | always |

The poll (every 30 s) re-reads the edit document and the live feeds
and requests `clock` plus whichever changed; a trip asked for "now"
is re-asked from the new now. That is a timer input, written into
`docs/units/host-discipline.md` as an engine requirement. Each poll
registers a new live document in rt, board, and router (QAF-49); a
rider-length session is bounded only when
`docs/units/module-runtime.md`'s eviction lands.

## Scope beyond the ops

- **Where edits come from.** Detours reach a rider through the sync
  server (`docs/units/sync-persistence.md`); the UI only reads.
- **Live feeds.** The deployment names them in
  `web/public/live-feeds.json` (C-TRAN's trip updates and vehicle
  positions); a rider may set their own. C-TRAN's service-alerts
  URL is not configured.

## Done when

- The session has run on C-TRAN as one sequence (the reference
  import, a live snapshot with trip updates, vehicle positions, and
  service alerts), each question through `caps-run`, its findings
  logged, and the inventory above re-pinned on real data.
- The UI's numbers on the real feed (`just serve
  testdata/ctran-gtfs.zip`) equal the session's for the same
  snapshot, an alert has been read on a board, and `ui.json`'s
  `session` records it.
- Then this file is deleted, or edited down to the questions still
  blocked (QAF-70, QAF-79, QAF-80, QAF-81 while open).
