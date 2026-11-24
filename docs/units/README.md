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
  reads it, and dogfooded on real data (the user workflow it serves,
  as a person doing QA would run it; D-196) are three claims. A unit
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
| [observability-rollout.md](observability-rollout.md) | what production runs and whether it is healthy; a display's automatic revert |
| [perf-harness.md](perf-harness.md) | `just perf`: compare runs, fail on runaway use, every UI and the server measured; the server's memory tuned |
| [web-conformance.md](web-conformance.md) | the generated UIs held to WCAG 2.2 AA and current web practice: the stops and service-change pages under the gate, route text color, a screen reader by hand |

### The agency's record

| unit | what it is |
|---|---|
| [store-durability.md](store-durability.md) | a restore of a C-TRAN-sized store proved |
| [log-boundaries.md](log-boundaries.md) | a log's boundary moved: kinds split, logs merged, a kind moved, each by a seal and a start entry, and undone by another move |
| [storage-model.md](storage-model.md) | the work rules and the policy in logs; a working store for drafts; permissions per change |

### Stops and streets

| unit | what it is |
|---|---|
| [stop-site-and-requests.md](stop-site-and-requests.md) | the stops app: the agency's other data beside a stop's survey, the work order system's real adapter, and the field forms dogfooded |
| [ui-stops.md](ui-stops.md) | a plan's stops and lines in service change: a station's bays, a moved stop's visits re-measured, consolidating stops, the Route stage's map, and a line redrawn along the streets |
| [streets-model.md](streets-model.md) | the streets document: walking on C-TRAN's streets, walksheds, street attributes, the geography release |

### Planning service

| unit | what it is |
|---|---|
| [ui-service-change.md](ui-service-change.md) | changing and publishing a plan's service: the frequency target, the layover ripple, the pattern strip, the rider preview, freezing and adopting |
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
| [ui-disruptions.md](ui-disruptions.md) | running today's service through disruptions and telling riders: late buses, detours drawn and proposed, today's cancels, internal notes, the agency's service alerts and what has none |

### The code the agency owns

| unit | what it is |
|---|---|
| [server-assembly.md](server-assembly.md) | the server links one assembly, as a UI does, with authz and each import in instances of their own |
| [ui-host-inputs.md](ui-host-inputs.md) | a UI host's inputs declared and run by the kit: built, and `just test` passes; left are six failing `just e2e` specs, the findings it closes, and the release gates |
| [kind-registration.md](kind-registration.md) | a document kind declared once: a gate on where a kind's name appears, the server's and the engine's plumbing, and the kind lists in the docs generated; the references between records checked, as of what the writer read |
| [capability-declared-once.md](capability-declared-once.md) | a capability declared once, in its crate: the gate that holds it, and the last docs |
| [fewer-dependencies.md](fewer-dependencies.md) | the S3 and GCS clients on the Go standard library, Azure Blob out, the Rust build trimmed, and an allowlist gate for new dependencies |

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
- **The stops page with no deployment** (deferred by the user
  2026-11-12; 2026-11-13: "in the future we will definitely want to
  build full offline support"): a stops document begun on the device
  from a GTFS zip or a stops file, and saved to a file. Edits made with
  no signal already wait on the device and are run again over the
  deployment's document when it answers, each checked, and the ones
  that no longer fit are listed (D-189).
- **A database beside the logs** — only when one of D-190's two needs
  arrives: data that must be deletable, or throughput that a log
  cannot carry (the bid is the likely first). It is fed from the logs,
  and the logs stay the record.
- **Checkpoints** in long pointer logs, and an incremental `store
  verify` and `gc`, once a real account's listings measure the need
  ([store-durability.md](store-durability.md), item 1).
- **Fuel metering** in the runner, once it may point at untrusted
  registry entries (the wall-clock guard is the stopgap).

The feature direction (what the software becomes) is
`docs/ROADMAP.md`; the real-data findings are
`docs/QA-FINDINGS.md`.
