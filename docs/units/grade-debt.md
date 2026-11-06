# Grade debt

Work that is on the bus (module + tests) but is missing a later
grade: no UI reads it, or it has not been dogfooded on real data.
Each line goes when its grade is pinned; the file goes when it is
empty.

## Not read by a UI

On the bus and tested, but no generated UI calls them. A generated
UI that reads the op pays the line instead.

- cap-rt `late_trips` — no UI renders the ranked late list
  ([ui-disruptions.md](ui-disruptions.md) question 1).
- Diversion authoring: cap-divert (`draft`, `list`, `propose`,
  `retire`, `restore`, `sanitize`), cap-streets `snap` / `route` for
  tracing a detour, and writing the edit document (`POST /edits`) —
  no page writes a detour; ride-now and the stop display only read
  them ([ui-disruptions.md](ui-disruptions.md) questions 3–5, 7).
- cap-runcut (`blocks`, `fleet_summary`, `block_trips`,
  `trip_stops`, `trip_path`, `recovery`, `suggest_blocks`) and the
  kernel's `load_work_rules` — no UI shows a day's blocks, a wait's
  recovery, or a suggested blocking, and no page holds a work-rule
  document (the engine has no `work_rules` loader); author `apply`'s
  `work_rules_hash` is unread too ([ui-runcut.md](ui-runcut.md)).
- cap-calendar (`active_services`, `service_dates`) — no UI asks
  which services run on a date.
- analysis `coverage_compare` — no UI asks it until D-43's matching
  rule says which of one plan's combinations answers to which of the
  other's ([ui-service-design.md](ui-service-design.md) question 19,
  [ui-proposal-compare.md](ui-proposal-compare.md) question 5).

## Done when

Every line above names its pin (a session in
`docs/QA-FINDINGS.md`'s stream, an e2e spec) and is deleted.
