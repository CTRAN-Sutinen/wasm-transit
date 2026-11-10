# Observability and revert — see production, and undo a bad build

Nothing reports what production runs or whether a change hurt it.
Stop displays stay open for weeks with no one watching them, and under
D-63 every display applies a new build at its next poll, so a bad
build reaches every stop at once. Publishing a pick has the same blast
radius: it reaches every rider at midnight on its date. This unit
makes each deployed part report what it runs and whether it is
healthy, and gives a display a revert that needs no person.
[deploy-rider-apps.md](deploy-rider-apps.md) depends on it for its
heartbeat and revert. Staged rollout (canary rings) is not this
unit's (D-84: not needed now; `docs/units/README.md`, "Queued, not
yet scoped").

## Scope

- **What each part reports** (D-83). Reporting is optional: the
  heartbeat is a service the manifest's `services` lists, and a
  static-only deployment lists none, so its displays send nothing.
  A display's identity is its URL's `display=<id>` parameter, set
  when the screen is installed (D-130); one without it sends no
  heartbeat. The heartbeat takes no token: it changes nothing served,
  and is rate-limited and bounded in size.
  The server keeps each display's latest heartbeat in memory and in
  its structured logs; only transitions (a build applied, a revert,
  offline and back) are durable.
  - a stop display: a heartbeat with its build, its module or
    assembly hashes, the manifest, network, and plan-set hashes it
    has pinned, its last good poll, its memory against the D-68
    budget, and its errors;
  - the server: structured logs of imports, publishes, store commits
    and conflicts (the store log already audits writes and their
    writers), and failed requests;
  - a rider's phone: nothing (D-83). A rider's device is not the
    agency's to watch.
- **A fleet view.** Which display runs which build, which is stale or
  offline, and which has reset under its memory ceiling: a page for
  the agency, reading the reports. It is a generated UI, so it follows
  the procedure in `docs/architecture/generated-uis.md`.
- **Automatic revert** (D-84). A display judges its own health; it
  needs no server and no heartbeat, so a static-only deployment has
  it too. Health counts only failures the build causes: its assembly
  does not load, an op refuses, the board does not render, memory
  passes the D-68 budget or the display resets under its ceiling, an
  uncaught error. A fetch that fails (the network, the server, or a
  feed down) is reported, never a cause to revert. Within the
  probation window after it applies a build (one service day,
  across its rollover), a display whose health fails reverts to the
  previous build it holds ([release-artifacts.md](release-artifacts.md)
  keeps it in the store; the display's cache keeps it offline) and
  reports the revert when it reports. Past the window, a failure is
  reported, not reverted. A revert never loops: a build that failed
  is not applied again until a newer one replaces it.
- **A pick's first day.** The preview before publishing is the
  planner's ([ui-service-design.md](ui-service-design.md)). Here,
  each display reports which pick it loaded on the pick's date, so the
  agency sees the switch land.

## Order

1. The display heartbeat and the server logs (D-83, D-130).
2. The fleet view.
3. The automatic revert (D-84).

## Done when

- e2e: a stop display's heartbeat reaches the server with its build
  and pins; a display whose poll fails reports it once it is back.
- e2e: a deployment whose manifest lists no heartbeat service, or a
  display with no `display` parameter, runs with no report sent.
- e2e: a display that fails its health check inside the probation
  window after build B reverts to A by itself, does not apply B
  again, and reports the revert; a failed feed fetch does not revert
  it; a failure past the window is reported, not reverted.
- A C-TRAN dogfood on a Pi 4 display (with
  [deploy-rider-apps.md](deploy-rider-apps.md)): the fleet view shows
  it across a republish and a pick's effective date.
