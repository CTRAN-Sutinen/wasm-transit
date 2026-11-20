# Every rule in an op

A host routes values between ops unchanged, and applies no domain rule
(D-228; `docs/architecture/hosts.md`, "What a host does"). A capability
is a documented namespace for one need, and its crate may link any
library (D-226). So a question that spans kinds is one op, and no host
composes it from two answers.

An audit on 2026-11-20 found the hosts applying rules. One rule is
written twice, in Go and in TypeScript, so the two hosts can disagree.
The others are in one browser host each, so caps-run and an agent
cannot give the same answer without writing the rule again. This unit
moves each rule into an op, and turns the library edges that the
rules exposed the right way round.

## Scope

1. **What a pick reads of the stops.** One op of cap-stops, over the
   network, the stops document, and the published plans. It answers the
   pick's period (retired picks dropped, this pick put in place), each
   stop's record on the pick's first date, the visit warnings, and the
   export's versions. `transit-stops` links `transit-plans`. It
   replaces:
   - in Go: `windows`, `readPick` (`server/stops.go`), and
     `stopsVersions` (`server/plans.go`);
   - in the engine: `planUntil`, `stopWarnings`, and
     `withStopVersions` (`web/src/engine.ts`);
   - in service change and service design: `syncPlanRows`, which
     filters the published picks and reads each pick's frame for
     `plans.periods` and `plans.validate`.
2. **The other rules in the browser hosts.** Each becomes an op, or a
   field of an answer that the host already asks for:
   - service change `stampAsPick`: an imported feed becomes a pick on
     its first service date, under a new id;
   - service change `createRoute`: stop records copied into
     `upsert_stop` edits, with the ids already taken filtered out;
   - disruptions, the preview stop: the gap's first stop, else the
     trip's first stop;
   - disruptions `newAlert` and `startAlertFor`: what a new alert for a
     gap informs, explains, and does;
   - disruptions `fillTemplate`: the dates and the routes and stops
     without repeats that fill a template;
   - stops `newStop` and `putRequest`: an id must be new before a
     write. The receive path already refuses a repeated id, so the host
     check goes;
   - stops `OPEN_WORK` and `STATUSES`: which requests are open work,
     and the stop statuses, from cap-requests `vocabulary` and cap-stops.
3. **The work order system.** `handleWorkOrder` (`server/workorders.go`)
   reads a request's held work order out of the document in Go. An op
   answers what to send the system for a request. The host makes the
   call. `record_work_order` records the answer, as it does now.
4. **The library edges.**
   - `transit-network` stops depending on `transit-stops`. The join
     between a network stop and a stops record
     (`transit-network/src/stop_record.rs`) moves to `transit-stops`,
     which then depends on `transit-network`.
   - `transit-dispatch` splits in two: the operator's day, and the
     rider's own documents (the watchlist and its chain, the device
     records).
   - The alert vocabulary (cause, effect, severity, period) moves from
     `transit-live` to `transit-alerts`, and `transit-live` depends on
     it.
5. **The docs.** `crates/AGENTS.md`, `docs/architecture/crates.md`,
   `docs/architecture/hosts.md` ("Where a host does not keep these
   rules"), the `OPS.md` of each changed capability, and the worked
   examples.

Routing stays in the hosts. These pass a value unchanged and are not
in scope: a picked group's members, a combination's first date (every
date of a combination runs its members, D-55), the walks the router
takes, a picked itinerary, `late_secs` from the live document, a
service day's span in the agency's zone (the clock), following a
picked bus onto its next trip, and putting a selection back (the
screen).

## Done when

- caps-run gives the same stop warnings and export versions for a
  pick as the server and the service change page, in one test over
  the fixture.
- `just test` finds no `"retired"` or `"published"` status test in
  `server/*.go` or `web/src/engine.ts`, and no `planUntil`.
- Each rule in item 2 has a law test in its library and a dispatch
  test at its op. The host calls the op.
- `arch-gates` shows `transit-network` with no dependency on
  `transit-stops`.
- The `hosts.md` list of rules in hosts is empty.
