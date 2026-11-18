# Units — the open work

One file per unit of open work. This directory is the only place
status lives: if a unit's file exists, the unit is open; when it
lands, the change that lands it DELETES the file. Git is the
archive — nothing here records what is finished.

## The rules

- **A unit file says what is still true of the work, never its
  history.** Goal (one paragraph), scope (what it touches), the
  order inside it if any, and **Done when** — the pins that must
  exist (tests, gates, e2e specs, a dogfood session on real data)
  before the file is deleted.
- **Grades are not merged.** On the bus (module + tests), the UI
  reads it, and dogfooded on real data are three claims. A unit
  whose work is on the bus but not read by a UI or not dogfooded
  stays open, edited down to the missing grade. An op that no UI
  plans to read is not open work.
- **Deleting a unit is the DONE claim.** The agent that deletes a
  file reports the pins that prove it in the same change. A
  partial landing edits the file down to what is left.
- **What a landed unit built is described in the present tense in
  the architecture docs** (`docs/architecture/`,
  `crates/AGENTS.md`, `docs/CONVENTIONS.md`), in the same change.
- **A finding becomes a unit only when it is picked up**: the unit
  names the `QAF-N` it closes; the finding is deleted from
  `docs/QA-FINDINGS.md` when the unit lands.
- New ops follow the extraction pattern (`crates/AGENTS.md`):
  library first, capability wrapper second.
- **A unit waiting on the user names the decision**: `Blocked:
  D-N` against the part it blocks. The question itself lives only
  in `docs/DECISIONS.md`; the rest of the unit proceeds.

## Open units

Grouped by what the work is about. Each group's units are separate
work; a group is only a way to find them.

### The rider apps in production

| unit | what it is |
|---|---|
| [deploy-rider-apps.md](deploy-rider-apps.md) | ride-now and the stop display beyond localhost: the units it waits on, the agency's feed fixes (HTTPS, CORS, caching, start_date), a real deployment checked, and the Pi 4 and phone dogfood |
| [observability-rollout.md](observability-rollout.md) | what production runs and whether it is healthy; a display's automatic revert |
| [perf-harness.md](perf-harness.md) | `just perf`: compare runs, fail on runaway use, every UI measured |
| [web-conformance.md](web-conformance.md) | the generated UIs held to WCAG 2.2 AA and current web practice: the stops and service-change pages under the gate, route text color, a screen reader by hand |

### The agency's record

| unit | what it is |
|---|---|
| [document-versioning.md](document-versioning.md) | every persisted document versioned, migrated forward at receive, rolled back under one rule; the schema lock and the published spec |
| [store-durability.md](store-durability.md) | the sync store's contract run on each provider's real account, backup and restore (D-82) |
| [storage-model.md](storage-model.md) | a layout per shape of data: collections record by record, the kind table and governed writes, a self-describing bucket, projections, permissions per change |

### Stops and streets

| unit | what it is |
|---|---|
| [stop-site-and-requests.md](stop-site-and-requests.md) | the stops app: the agency's other data beside a stop's survey, the work order system's real adapter, and the field forms dogfooded |
| [ui-stops.md](ui-stops.md) | a plan's stops and lines in service change: a station's bays, a moved stop's visits re-measured, consolidating stops, the Route stage's map, and a line redrawn along the streets |
| [streets-model.md](streets-model.md) | the streets document: walking on C-TRAN's streets, walksheds, street attributes, the geography release |

### Planning service

| unit | what it is |
|---|---|
| [ui-service-change.md](ui-service-change.md) | changing and publishing a plan's service: the frequency target, the layover ripple, the pattern strip, the rider preview |
| [ui-service-design.md](ui-service-design.md) | reviewing how a plan's network performs (coverage, corridors, waits), read-only; the edit views come out |
| [ui-proposal-compare.md](ui-proposal-compare.md) | the public comparing service proposals |

### Blocks and runs

| unit | what it is |
|---|---|
| [ui-runcut.md](ui-runcut.md) | the blocking UI: a plan's blocks, its peak vehicles, block edits; it reads the runs when they land |
| [runs-layer.md](runs-layer.md) | the runs cut under the work rules; the run view, legality, and run edits; suggested blocking, extended; the GTFS + TODS codec |

### Today's service

| unit | what it is |
|---|---|
| [ui-disruptions.md](ui-disruptions.md) | running today's service through disruptions: late buses, detours drawn and proposed, cancels, covered trips |

### The documents

| unit | what it is |
|---|---|
| [plain-docs.md](plain-docs.md) | the developer documents rewritten to the writing rules, the first-change guide tested, and one name for each concept |

## Queued, not yet scoped

Ideas with a direction but no unit yet. Scope one into a file
before starting it.

- **Registry search** — a derived artifact (purpose + op docs +
  worked examples embedded; ranking carries evidence): an agent
  browses the catalog and assembles its own harness.
- **caps-run harness as data** — when a session improvisation
  recurs, crystallize it; until then improvisations are findings.
  The session journal (`docs/architecture/registry.md`, "The
  journal") already records each session's calls.
- **Law 3 re-cuts** — a split costs no network copy now (a page's
  capabilities share one registry): cap-analysis into service
  numbers, headways, coverage, and connections; cap-directory into
  the directory (lists and details), the timetable, and focus, with
  `stops_near` joining `access` in walking and `route_stop_sequence`
  deleted (no host calls it); cap-streets into the street graph and
  walking. Where the service-group relation and `route_overlap`
  belong goes to the user when this is scoped.
- **Staged rollout (canary rings)** — not needed now (D-84). The
  manifest names a build per cohort; a canary set takes build B while
  the rest keep A, promoted once it has stayed healthy. Promotion on
  reported health needs an authenticated heartbeat (D-130's takes no
  token) and a server, so a static-only deployment has none.
- **The bid** — a generated UI for operators to bid on a pick's runs
  in seniority order (`docs/ROADMAP.md` 7). It starts once the runs
  cut lands ([runs-layer.md](runs-layer.md)). It needs workforce and
  seniority data (QAF-17), and a bid is a new shared document.
- **The stops page offline** (deferred by the user 2026-11-12;
  2026-11-13: "in the future we will definitely want to build full
  offline support"): a copy of the stops document on the device that
  every edit lands in first, begun from the deployment's document, the
  network it serves, a GTFS zip, or a stops file; saved to a file;
  sent when the store answers; several stops surveyed with no signal
  and sent later. Open: when the deployment's document moved on while
  edits waited (replay each edit, checked, and list the ones that no
  longer fit, recommended; or choose one of the two documents).
- **Event tables** — high-volume append data (TIDES, AVL, APC,
  heartbeat history) as Parquet with an open table format (Apache
  Iceberg) on the same bucket, committed through a pointer.
  When a workload asks.
- **A transactional engine in front of one collection** — only where
  rules across records under sustained contention outrun the CAS and
  its retry (the bid is the likely first). It writes through to the
  collection before it acknowledges, and passes storage-model's
  rebuild drill.
- **Checkpoints** in long pointer logs, and an incremental `store
  verify` and `gc`, once a real account's listings measure the need
  ([store-durability.md](store-durability.md), item 1).
- **Fuel metering** in the runner, once it may point at untrusted
  registry entries (the wall-clock guard is the stopgap).

The feature direction (what the software becomes) is
`docs/ROADMAP.md`; the real-data findings are
`docs/QA-FINDINGS.md`.
