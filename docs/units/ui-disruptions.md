# UI: running today's service through disruptions

A generated UI for keeping today's service running when it breaks,
and for telling riders about it:
- what is late and where that bus is;
- which disruptions are in force;
- drawing a detour along the streets;
- which trips are covered and which are uncovered;
- the agency's service alerts.

Dispatch enters detours and the alerts that go with them. The
communications staff write alerts from their templates. Customer
service and other teams read all of it. The work starts as one
workspace, so each department knows the others' screens. It splits
into more UIs only when the workspace is clearly overloaded (D-209).

It follows the generated-UI procedure
(`docs/architecture/generated-uis.md`). It is at step 2: no session
has asked these questions as one sequence.

A diversion draft is not persisted yet. When this UI first keeps one,
the draft gains its kind and version tags and a schema lock like every
persisted kind (`docs/FORMATS.md`, "Versions" and "The schema lock"),
and its exemption in cap-abi `tests/formats_inventory_test.rs` goes.

## Questions, in the order asked

1. What is late right now, and where is that bus?
2. Will a late bus make its next trip?
3. Which disruptions are in force today?
4. Where does this detour go: which stops does it skip, and what
   path does it take on the streets?
5. What does the detour do to the trip's path, the boards, and the
   trip plans?
6. Which trips are covered and which are uncovered, and how do I
   cancel or cover one? (D-15: covered means a vehicle on its block AND an operator holding its run; the operator half waits on the runs layer.)
7. Have the others working today seen this change?
8. Which service alerts are in force, which are upcoming, and which
   have ended?
9. Which detours, trip cancels, and stop closures have no alert?
10. How do I tell riders about this, and what will they see?

## Op inventory (draft, before the session)

| # | op(s) | state |
|---|---|---|
| 1 | rt `live`, `late_trips` | op exists; no UI reads `late_trips` yet; a vehicle with no label shows its id (D-10) |
| 2 | runcut `block_trips` (`live_hash`) | op exists: a trip's live delay at its last stop against its next layover |
| 3 | divert `list` | op exists (diversions in `rows`, trip cancels in `cancels`) |
| 4 | divert `draft`, streets `snap` / `route`, divert `propose` | op exists; streets `import` / `snap` / `route` traced route 72's detour along NE 137th Ave on the county roads (2026-11-06) |
| 5 | runcut `trip_path`, board `stop_board`, router `plan_compare` with `edits_hash` | op exists (a diverted trip's line splices the traced detour into its alignment) |
| 6 | divert `cancel`, `list`, `retire`; board `stop_board` / router `plan` with `edits_hash` | cancel (whole trip, one instance or all, one date) exists: boards list it cancelled (D-100), the router never rides it; cover, hold, swap, partial cancel come later (D-45). Covered / uncovered has no op: the vehicle half (a vehicle on the trip's block) needs a rule for what puts a vehicle on a block before it reports a position, the operator half waits on the runs layer (D-15) |
| 7 | sync `sync_edits` | op exists; dogfooded on C-TRAN through a restarted store server (2026-11-05); no page writes one |
| 8 | alerts `list` | op exists; the long-open rule is set by trying it on C-TRAN's alerts (D-207) |
| 9 | alerts `gaps` | op exists (edits and suspended stops) |
| 10 | alerts `fill`, `validate`, `put`, `end`, `flags`; rt `encode_alerts`, `read_alerts`, `live`; board `stop_board` | op exists; the server keeps the alerts (`POST /service-alerts`); `read_alerts` brings the existing app's alerts across |

## Scope beyond the ops

- **Live input**, as in ride-now (`docs/architecture/web.md`): a timer
  refreshes the snapshot; the registries bound themselves, and the
  host names its keep-set (`keep`) when its documents change.
- **The edit document is shared state.** Retire is the tombstone,
  and every next document is cap-divert's answer.
- **Reading needs no role.** Anyone can read the disruptions and the
  alerts. Writing needs the write role, and an alert has no review
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
  draws a detour and writes its alert. A communications writer
  writes an alert from a template. Customer service reads both
  without signing in.
