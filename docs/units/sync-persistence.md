# Sync persistence

The durable sync store and what rides on it: the edit document, the
policy, the manifest root, and the published plan set
(`docs/ARCHITECTURE.md`, server section; `docs/FORMATS.md`). What is
left is publishing from the planner's side, taking a pick back, the
pick in force as GTFS, and the device journal.

## What remains

- **Publishing from the planner** (QAF-94): `POST /plans` is the only
  way a pick reaches the store. The service-design UI has no publish
  gesture, and `caps-run` has no step that sends an `apply` answer's
  network to the server, so a session hand-builds the POST.
- **Taking a published pick back** (Blocked: D-75): a pick stays in
  the published set until it is republished. Withdrawing one needs an
  answer to how a pick comes back out (no hard deletes).
- **What publish refuses** (Blocked: D-76): today a pick that runs no
  service in its period publishes with a `no_service_in_plan_period`
  warning (QAF-95), and a pick dated in the past publishes and governs
  at once.
- **The pick in force as GTFS** (docs/ROADMAP.md 1): serve the
  materialized feed per service change date — cap-network `export` of
  the pick in force — for consumers that read GTFS, not the network.
- **Per-device journal unification** ([host-discipline.md](host-discipline.md)):
  the browser journal reuses the envelope + hash-chained log shape, so
  a device journal replays against the server store.

## Done when (the remaining pins)

- A planner publishes a pick from the service-design UI (an e2e spec
  that posts it and a rider page that loads it), and a `caps-run`
  session publishes one without glue.
- D-75 and D-76 are decided and their laws pin the answers (cap-plans
  dispatch tests, `server/plans_test.go`).
- A Go round trip serves the pick in force as a GTFS zip whose bytes
  equal cap-network `export` of that pick.
- A device journal replays against a store (host-discipline's journal
  pins).

Pinned already (regression gates, not re-listed as remaining):
`go test -race ./server/store/` covers CAS conflicts, log replay,
idempotent redelivery (same hash AND seq), crash-between-blob-and-
append, pointer names that cannot leave the store, and the periodic
orphan sweep racing commits (`TestSweepCollectsAgedOrphansWhileOpen`);
`go test ./server/` covers delta sync, the `/blobs` ETag/envelope and
gzip headers, the D-56 clamp through the receive path, edits
surviving a restart, publishing and resolving picks through a
restarted server (`plans_test.go`), and a plain-Go reader that
validates the store's plan set, pick networks and edit document — and
every registry worked example — against the generated schemas
(`reader_test.go`); `just spec` model-checks `server/store/store.tla`
(TLC 2.19, 2026-10-30: `store.cfg` 1207 states / 214 distinct,
`store-exhaustive.cfg` 72756 / 8750, no invariant violations);
`cargo test` pins the transit-sync laws, the published-set laws
(`crates/transit-plans/tests/published_test.rs`) and the cap-sync,
cap-plans and cap-network seams; `e2e/published-picks.spec.mjs` pins
the rider pages loading the pick in force. A C-TRAN session
(2026-11-05) published two picks to a store-mode server, synced an
edit document, restarted it, and resolved the chain through cap-plans
(QAF-94, QAF-95 are its findings).
