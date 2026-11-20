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

1. **The timeline rows in the planning pages.** In service change and
   service design, `syncPlanRows` filters the published picks by
   status, decides which plans on the device count as published, and
   reads each plan's frame for cap-plans `periods`, `validate`, and
   `resolve`. One op of cap-plans answers the timeline over the plans
   on the device and the published set: which are published, the
   picks not yet on the device, the periods, the chain's issues, and
   the pick in force. The pages route the answer. (A pick's period and
   what it reads of the stops are already one op: cap-stops `read_pick`
   and `export_pick`.)
2. **The other rules in the browser hosts.** Each becomes an op, or a
   field of an answer that the host already asks for:
   - service change `createRoute`: stop records copied into
     `upsert_stop` edits, with the ids already taken filtered out.
3. **The docs.** `crates/AGENTS.md`, `docs/architecture/crates.md`,
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

- `just test` finds no `"retired"` or `"published"` status test in
  `web/src/uis/*/host.svelte.ts`.
- Each rule in item 2 has a law test in its library and a dispatch
  test at its op. The host calls the op.
- The `hosts.md` list of rules in hosts is empty.
