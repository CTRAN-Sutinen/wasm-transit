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

These are original-app (`/app.html`) gaps. A generated UI that
reads the op pays the line instead.

- cap-rt `late_trips` — no UI renders the ranked late list.
- network `validate` warnings — the original app shows the issues,
  not the warnings (the service-design UI shows `apply`'s).
- analysis `network_stats`'s `window` and per-route hours — the
  original app's planner screens do not render them.
- The map draws merged walks through their via stops, but no e2e
  pins that drawing.

## Done when

Every line above names its pin (a session in
`docs/QA-FINDINGS.md`'s stream, an e2e spec) and is deleted.
