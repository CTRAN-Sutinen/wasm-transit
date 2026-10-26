# The capability bus

Horizontal capability modules assembled per user from a manifest —
the thesis is "ship capabilities to match the user," so capability
bytes differ per user while the DOCUMENT and the contracts stay
identical everywhere. The work queue and per-item status live in
`docs/CAPS-PLAN.md` — that doc is the authority for what is done
and what is not. The two architecture laws (network is the core
model; nobody reinvents the monolith inside a module) live in
`crates/AGENTS.md` and are enforced by arch gates in the justfile
`test` recipe.

## The modules

```
cap-abi         The seam crate: the flat C ABI as a macro (every
                capability module is the same four exports), the
                capability-manifest contract (speaks/migrates/
                consumes + agent context), the ASSEMBLY VALIDATOR
                (compose-time proof: closure, version coherence,
                migration-path reachability — total and property-
                tested; gen-manifest self-validates before writing),
                and gen-manifest (the capability set as code; the
                manifest diff IS the reviewable assembly change).
                Also caps-smoke.mjs: Node ABI smoke + the law that
                every advertised op is documented (undocumented ops
                fail the build — agent context cannot drift).
cap-gtfs        The gtfs capability: GTFS zip → content-hash feed
                registry, departures/stops queries, the dispatch
                reads (active_services, blocks, block_trips,
                feed_info, stops_by_ids), station_departures (the
                walking-group station board — a station pin's bays
                pooled, rows tagged), and the watchlist document
                (v1/v2) with its v1→v2 migration — pure,
                deterministic, loss REPORTED (MigrationNotes, never
                silent; identity-preservation is a property law —
                migrations never touch the sync key space).
                CHARTER CLOSED: parse, export, primitive boundary
                queries — nothing more goes "in GTFS."
cap-router      The router capability: RAPTOR over a feed document
                gtfs produced — a pure-function capability; it
                writes nothing.
cap-analysis    The analysis capability: planner numbers (stats,
                service types, coverage) over hash-keyed network and
                population-layer registries. The domain code lives
                in the shared transit-analysis library — the first
                extraction where a monolith op family is DELETED
                (the planner UI has no monolith fallback for it).
                Consumes network documents from cap-network (the
                registry edge the manifest carries).
cap-network     The network codec capability: load_feed (zip bytes →
                content-hash feed registry), import (the planner's
                bootstrap — the derived document registers itself
                and its hash is in the response), load_document,
                validate (the authoring feedback loop), export (the
                publish path — deterministic GTFS zip bytes under
                their content hash, base64 across the JSON seam).
                The domain code lives in the shared transit-codec
                library — the monolith keeps no copy.
cap-rt          The realtime capability: decode — a GTFS-rt protobuf
                payload (bulk-bytes op 0, the ephemeral bytes an
                agency serves every few seconds) into the overview
                view (delays per trip/stop, live vehicles), each
                snapshot registered under its content hash; and the
                SCHEDULE JOIN the dispatcher actually asks for:
                load_feed (bulk op 1, the SAME feed identity
                cap-gtfs returns — the manifest's rt → gtfs edge),
                trip_progress (one trip's scheduled/delay/predicted
                board), late_trips (ranked, with the DELAYED-BUT-
                DARK flag and vp_hash for agencies that serve the
                two feeds separately — C-TRAN does). The monolith
                keeps no rt copy.
cap-dispatch    The dispatch capability: the operator view over the
                NETWORK document (load_network — the same identity
                cap-network's import produces) plus the OPERATOR
                EDIT document (load_edits — the dispatcher's own,
                validated per record against the network, invalid
                records REPORTED never rejected). departures /
                station_departures / block_trips answer over ONE
                MATERIALIZED DAY (materialize(net, date), patched
                with the in-force edits — no derived registry, no
                GTFS expansion, no effective cache; a day is
                computed per call and a day is cheap). The chain is
                the AUTHORED runcut with the layover ripple. The
                domain code lives in the shared transit-dispatch
                library; the module holds no GTFS layer (arch
                gate). Model-native responses.
```

## Bus rules (load-bearing)

- Capabilities compose through DOCUMENTS addressed by content hash,
  never module-to-module — the host loads the same bytes into each
  module and ASSERTS the returned hashes agree (`assertSameDocument`
  is the law).
- The MODULE is the hash authority — hosts never mint document
  identity (JS JSON.stringify differs from the module's canonical
  serde bytes in float formatting; query with the hash the module
  returned).
- One Request enum per capability is the whole surface (only
  planners write networks — readers have no authoring ops by
  construction).
- `#[no_mangle]` ABI exports collide if one binary links two cap
  crates (tests assert against the hash formula instead); cap
  crates need `crate-type = ["cdylib", "rlib"]` for native tests to
  link.
- Do not let a capability grow a private document format — shared
  data crosses modules as kernel documents addressed by content hash
  only.

## The registry and the runner

The registry (`registry/`, published by `just registry`): immutable
blobs + one evidence record per capability (contracts, agent
context, which gates ran at which commit) + a pointer. It is the
artifact agents and humans pull SANDBOXED COMPUTE from —
`tools/caps-run.mjs` verifies blob bytes against the pinned hash and
runs ops behind a worker-termination wall-clock guard (`--timeout`).
Registry records and the browser manifest are BOTH generated by
`cargo run -p cap-abi --bin gen-manifest` — one source of contracts,
two surfaces. `just test` gates the runner itself (the fixture feed
hash is stable, so registry/fixture drift fails the gate).

## Dogfood QA — the agent interface mirrors the human UI

`caps-run` exercises the same capabilities, bytes, and laws the app
runs; a human asking planner questions through it is doing real QA.
The rules this paradigm earned (the founding C-TRAN session found
four real defects fixture gates missed — a 64 KiB output truncation
among them):

- **All capability work is dogfooded on real data before it is
  done.** Anything touching feeds or payload size runs against
  `testdata/ctran-gtfs.zip` (the real C-TRAN GTFS, NOT COMMITTED —
  licensed data; fetch from c-tran.com) through `caps-run`.
- **A one-off script is never tooling — it is a finding.** If a QA
  session needs bespoke glue (a bootstrap script, a summarizer),
  that is a missing op or runner feature; route it into
  `docs/CAPS-PLAN.md`, don't ship the script.
- **The mirror cuts both ways.** A question that is one op via the
  CLI but absent from the UI is a UI gap; a question awkward via
  the CLI is an op gap. Dogfood sessions generate both signals;
  both land in the plan doc.
