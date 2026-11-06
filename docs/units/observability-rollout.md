# Observability and staged rollout — see production, and change it a little at a time

Nothing reports what production runs or whether a change hurt it.
Stop displays stay open for weeks with no one watching them, and under
D-63 every display applies a new build at its next poll, so a bad
build reaches every stop at once. Publishing a pick has the same blast
radius: it reaches every rider at midnight on its date. This unit
makes each deployed part report what it runs and whether it is
healthy, and lets a change reach a few before it reaches all, with a
revert that needs no person. [deploy-rider-apps.md](deploy-rider-apps.md)
depends on it for its canary and revert.

## Scope

- **What each part reports** (Blocked: D-83, what leaves which
  device and where it goes):
  - a stop display: a heartbeat with its build, its module or
    assembly hashes, the manifest, network, and plan-set hashes it
    has pinned, its last good poll, its memory against the D-68
    budget, and its errors;
  - the server: structured logs of imports, publishes, store commits
    and conflicts (the store log already audits writes and their
    writers), and failed requests;
  - a rider's phone: nothing until D-83 says otherwise. A rider's
    device is not the agency's to watch.
- **A fleet view.** Which display runs which build, which is stale or
  offline, and which has reset under its memory ceiling: a page for
  the agency, reading the reports. It is a generated UI, so it follows
  the procedure in `docs/ARCHITECTURE.md` ("Generated UIs").
- **Rings** (Blocked: D-84, who is first and what counts as
  healthy). The manifest names a build per cohort: a canary set of
  displays takes build B while the rest keep A, and B is promoted once
  the canary has stayed healthy for the agreed time.
- **Automatic revert.** A display whose health fails after it applies
  a build reverts to the previous retained build by itself
  ([release-artifacts.md](release-artifacts.md) keeps it) and
  reports the revert. A revert never loops: a build that failed is
  not applied again until a newer one replaces it.
- **A pick's first day.** The preview before publishing is the
  planner's ([ui-service-design.md](ui-service-design.md)). Here,
  each display reports which pick it loaded on the pick's date, so the
  agency sees the switch land.

## Order

1. The display heartbeat and the server logs (D-83).
2. The fleet view.
3. Rings and the automatic revert (D-84), which also need
   release-artifacts' retention.

## Done when

- e2e: a stop display's heartbeat reaches the server with its build
  and pins; a display whose poll fails reports it once it is back.
- e2e: with two cohorts, the canary display takes build B while the
  other keeps A; a display that fails its health check after B
  reverts to A by itself and reports the revert.
- A C-TRAN dogfood on a Pi 4 display (with
  [deploy-rider-apps.md](deploy-rider-apps.md)): the fleet view shows
  it across a republish and a pick's effective date.
- D-83 and D-84 are decided and encoded.
