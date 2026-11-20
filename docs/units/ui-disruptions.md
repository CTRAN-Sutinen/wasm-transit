# UI: running today's service through disruptions

A generated UI for keeping today's service running when it breaks,
and for telling riders about it:
- what is late and where that bus is;
- which disruptions are in force;
- drawing a detour along the streets;
- which trips are cancelled today;
- the agency's service alerts;
- dispatch's internal notes, in place of email.

Vehicle assignments and trip coverage are not here. CAD/AVL holds
them (D-122, D-212).

Dispatch enters detours and the alerts that go with them. The
communications staff write alerts from their templates. Customer
service and other teams read all of it. The work starts as one
workspace, so each department knows the others' screens. It splits
into more UIs only when the workspace is clearly overloaded (D-209).

It follows the generated-UI procedure
(`docs/architecture/generated-uis.md`). It is at step 8. The page is
built and walked on C-TRAN (`src/uis/disruptions/`, its spec
`e2e/ui-disruptions.spec.mjs`): the map, Status, Alerts, Dispatch, an
opened disruption with its notes, the alert form, and its rider
preview. Left for the form: an alert that applies to a direction, a
trip picked by hand, or a stretch of stops along a route; the service
window (`impact`); and "Created" and "Last edited" from the alerts
log. The layout is the design canvas (one map, with views a
person picks: Status, Alerts, Dispatch; the trip and alert panels
open over the map).

A diversion draft is not persisted yet. When this UI first keeps one,
the draft gains its kind and version tags and a schema lock like every
persisted kind (`docs/FORMATS.md`, "Versions" and "The schema lock"),
and its exemption in cap-abi `tests/formats_inventory_test.rs` goes.

## Questions, in the order asked

1. What is late right now, and where is that bus?
2. What do I tell a rider who asks about this route or stop?
3. Which disruptions are in force today?
4. Where does this detour go: which stops does it skip, and what
   path does it take on the streets?
5. What does the detour do to the trip's path, the boards, and the
   trip plans?
6. Which trips are cancelled today, and why? The live feed's cancels
   and the edit document's cancels are listed as each source gives
   them (D-213).
7. What has dispatch noted about this trip, detour, alert, or stop,
   and what was noted today (D-214)?
8. Have the others working today seen this change or this note?
9. Which service alerts are in force, which are upcoming, and which
   have ended?
10. Which detours, trip cancels, and stop closures have no alert?
11. How do I tell riders about this, and what will they see?

## Op inventory (C-TRAN session, 2026-11-19)

The session: `caps-run --session dis` on `testdata/ctran-gtfs.zip`
(network `e99a1cc1…`), C-TRAN's live feeds fetched at 13:31 on
2026-11-19 (63 vehicles; no cancels at that moment), and
`testdata/perf-rt-10m/020-*.pb` (2026-11-04) for the feed's cancels.
The second session, after the new ops, ran the same snapshots:
`late_trips` names each late trip as riders do ("009 1:20 from NE 139th
St & 10th Ave → 99th Street Transit Center", the bus near NW 139th St
1300 Block); disruptions `today` on 2026-11-04 lists 21 routes (the
Vine-Green's three feed cancels, each with its next trip, then the late
routes, in C-TRAN's sort order) and `gaps` its 8 feed cancels; a note on
one cancel comes back as its `latest_note`; `route` filters to one
route.

| # | op(s) | state |
|---|---|---|
| 1 | rt `live`, `late_trips` | dogfooded: 25 late of 63 at 13:31, ranked; a bus on its block's next trip is found there. Each row carries the route's name, the trip's start, first stop, and headsign, and the nearest stop (D-219) |
| 2 | disruptions `today` (`route`, `stop`) | op exists (D-218): a route's or a stop's disruptions as one answer, with the names a rider uses, the alerts, and the latest note. Not yet dogfooded on C-TRAN |
| 3 | disruptions `today` | op exists (D-218): every source, each as its document gives it, by route |
| 4 | divert `draft`, streets `snap` / `route`, divert `propose` | op exists; streets `import` / `snap` / `route` traced route 72's detour along NE 137th Ave on the county roads (2026-11-06). Not designed yet: the layout has no detour screen |
| 5 | runcut `trip_path`, board `stop_board`, router `plan_compare` with `edits_hash` | op exists (a diverted trip's line splices the traced detour into its alignment) |
| 6 | rt `live` (`canceled: true`), divert `list` (`cancels`) | dogfooded: the 2026-11-04 snapshot holds 8 cancelled instances, each with its date and start. The list across both sources, with names, is disruptions `today`. divert `cancel` and `retire` stay for an agency with no CAD (D-213). Partial cancel comes later (D-45) |
| 7 | notes `on`, `between`; `POST /notes` | dogfooded: a note written on a C-TRAN feed cancel (2026-11-04) is that disruption's `latest_note` in `today`. The server stamps the writer (D-214, D-217) |
| 8 | sync `sync_edits`; the notes log | sync op exists; dogfooded on C-TRAN through a restarted store server (2026-11-05); no page writes one. The manifest pins `notes_hash`, so a page sees a new note when it reads the manifest again |
| 9 | rt `read_alerts`, alerts `list` | dogfooded: C-TRAN's 16 alerts read with nothing skipped; 15 in force, 1 ended. 14 of the 15 have no end, open 9 to 323 days (QAF-126) |
| 10 | disruptions `gaps` | op exists (D-218): edits, the feed's cancels, and suspended stops. An alert explains a feed cancel by its trip instance (D-216) |
| 11 | alerts `fill`, `validate`, `put`, `end`, `flags`; rt `encode_alerts`, `live`; board `stop_board` | ops exist; the server keeps the alerts (`POST /service-alerts`). `encode_alerts` registers the feed it writes, so `live` reads it by `rt_hash` |

## Answer graph

The host's nodes (procedure step 5). The inputs: the manifest's
documents (network, edits, stops, service alerts, alert templates,
notes), the live snapshots (trip updates and vehicle positions, polled
while the page is visible), the clock (the agency's date and Unix
seconds), and the selection (the view, the search, the selected item,
and the alert form).

| node | op(s) | inputs | frame | on screen |
|---|---|---|---|---|
| live | rt `live` | network, snapshots, the on-time window | map (vehicles) | always; each poll |
| late | rt `late_trips` (`min_delay_secs`: the live document's late window) | network, snapshots, live | Dispatch "Late now" | Dispatch view |
| status | disruptions `today` (`route` or `stop` from the search) | network, edits, live, stops, service alerts, notes, date, now | Status cards | Status view |
| day | disruptions `today` | the same, no filter | Dispatch detours and cancels; map (detours, cancels, closures) | Dispatch view; map |
| gaps | disruptions `gaps` | the same | Alerts "No alert" | Alerts view |
| alerts | alerts `list` | service alerts, now | Alerts list | Alerts view |
| notes-day | notes `between` | notes, the day's start and end | Dispatch notes | Dispatch view |
| notes-on | notes `on` | notes, the selected item | the detail panel's notes | a detail panel |
| path | runcut `trip_stops`, `trip_path` | network, the selected trip, as scheduled | the panel's stops, map (the trip's line) | a trip's detail panel |
| templates | alerts `templates` | alert templates | the form's template menu | the alert form |
| fill | alerts `fill` | alert templates, network, the template, the form's routes, stops, and dates | the form's text | the alert form |
| flags | alerts `flags` | the form's alert (alerts `put` over the head), network | the form's notices | the alert form |
| preview | alerts `put`, rt `encode_alerts`, rt `live` (`alerts_hash`), board `stop_board` | the form's alert, network, the preview stop | the rider preview | the alert form |
| search | directory `entities` (routes), `stops` | network, the search text | the search results | Status view |

The preview's ops run in order over one another's answers (the put's
document, then the feed's `rt_hash`, then the `live_hash`). A write is
I/O, not a node: `POST /notes` and `POST /service-alerts`.

## Scope beyond the ops

- **Live input**, as in ride-now (`docs/architecture/web.md`): a timer
  refreshes the snapshot; the registries bound themselves, and the
  host names its keep-set (`keep`) when its documents change.
- **The edit document is shared state.** Retire is the tombstone,
  and every next document is cap-divert's answer.
- **Notes replace dispatch's email** (D-214). A note names a trip
  instance, a detour or cancel record, an alert, or a stop. Anyone
  can read notes, and they never reach riders. The workspace lists
  today's notes, newest first. The UI says that notes are kept.
- **Reading needs no role.** Anyone can read the disruptions, the
  notes, and the alerts. Writing needs the write role, and an alert has no review
  step (D-207).
- **The alerts list is the agency's one list** (D-207): in force,
  upcoming, and ended. Long-open alerts are visible in it; the rule
  for that is set by trying it on C-TRAN's alerts.
- **The gap list** is beside it. Each gap starts an alert with its
  effect, routes, stops, and dates filled in.
- **The alert form** keeps the layout that communications staff know
  from the app it replaces: the template menu, the start and end
  dates and times, "Starts now", "Unknown end", and "End alert".
  - Where an alert applies is one field that shows each scope as a
    line ("Route 31 at Broadway & 25th"). It takes stops on the map,
    a stretch of stops along a route, a direction, a trip, and the
    whole agency.
  - The form adds severity, the link, the translations, and an
    optional window for when service is affected.
  - "Created" and "Last edited" name who and when, from the log.
- **The alert preview** runs rt `encode_alerts`, then `live` with the
  feed's hash, then `stop_board`, and shows the real board with the
  alert on it. The page computes nothing. The page downloads the `.pb`
  file.
- **With no server**, the page reads and previews only. It writes
  nothing.

## Done when

- The session has run, and the inventory names an op or a finding
  for every question.
- The answer graph table is in this file.
- The UI lands per step 8 with `e2e/ui-disruptions.spec.mjs`, and
  this file is deleted, or edited down to its blocked questions.
- A dogfood session on C-TRAN walks every control. A dispatcher
  draws a detour, writes its alert, and notes a cancelled trip. A
  communications writer writes an alert from a template. Customer
  service reads all three without signing in.
