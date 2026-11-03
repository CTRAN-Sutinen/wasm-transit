# Grade debt

Work that is on the bus (module + tests) but is missing a later
grade: no UI reads it, or it has not been dogfooded on real data.
Each line goes when its grade is pinned; the file goes when it is
empty.

## Not dogfooded on real data

- **cap-streets** (load_streets / snap / route). The real-data
  path is the county soak (`just soak`, COUNTY_GEOJSON), which
  needs the licensed county export. Done when a soak session runs
  and its findings are logged.
- **cap-sync and cap-plans** — no real-data session has synced an
  edit document or resolved a real plan chain through the modules.
  Rides with [sync-persistence.md](sync-persistence.md).

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
  `trip_stops`, `trip_path`) — no UI shows a day's blocks
  ([ui-runcut.md](ui-runcut.md)).
- cap-calendar (`active_services`, `service_dates`) — no UI asks
  which services run on a date.
- network `validate` warnings — service-design shows the issues
  and `apply`'s warnings, not `validate`'s.
- analysis `network_stats` — no UI renders it (the window, the
  per-route hours).
- The map draws merged walks through their via stops, but no e2e
  pins that drawing.

## Done when

Every line above names its pin (a session in
`docs/QA-FINDINGS.md`'s stream, an e2e spec) and is deleted.
