# The plan — decompose the monolith into needs

This is THE plan. Status lives in `docs/CAPS-PLAN.md`; this doc
says where the remaining work goes and in what order. The laws
live in `crates/AGENTS.md`. Nothing here restates status — a
unit's state is read from CAPS-PLAN, never from this file.

## The thesis

**A capability is one NEED, atomically. The persona comes together
at WIRE time — the user's selection over the manifest — never at
build time.** Building caps for users bakes bundles into bytes;
building caps for needs lets every user assemble their own. A
module named for a role is the monolith rebuilt sideways, however
small: the bundle lives in the bytes.

**GTFS is a serialization spec for external data sharing. The
NETWORK is the core data model.** "Network vs feed" is never a
decision, an option, or an open question — model-side
capabilities compose over the network document and its day
views. Presenting feed composition as a choice is Law 1 broken in
prose. A question about stops, routes, patterns, or days is a
MODEL question and answers over the network document — the feed
answers only the feed's own questions (its bytes, its identity,
its metadata).

## The cut — criteria every capability must pass

A capability is a named question over hash-addressed documents.
Each criterion is checkable; a cut that fails one is a bundle or
a duplicate, not a capability:

1. **One question.** The purpose states as a single question. If
   it needs "and", cut at the "and".
2. **Persona-blind.** No role words in the purpose (dispatcher,
   rider, planner, operator). Two personas using disjoint op
   subsets of one module for different ends is a bundle.
3. **One subject.** Every answer describes one subject (a street
   graph, a vehicle's chain, a service calendar). Documents are
   inputs, not subjects — "reads network + edits" bundles
   nothing.
4. **Facets, not surface.** Ops are the one question at other
   granularities or parameterizations (list → detail; stop →
   station → saved set). An op that is not a facet of the
   question does not belong, however related.
5. **Alone-valuable.** Worth assembling by itself; if it only
   feeds another capability, it is that capability's op.
   Capabilities never call capabilities — composition is
   documents only.
6. **No two answers to one question.** Across the whole
   manifest, no two modules answer the same question over the
   same documents (Law 3 at module scope). A duplicate answer is
   deleted with the unit that obsoletes it, never left "for
   compatibility".
7. **Registration is plumbing.** `load_network` / `load_edits`
   are the seam every model-side module repeats; they never
   justify a module and do not count toward its need.
8. **Independently retirable.** The module can die alone — that
   is what makes it a decomposition unit instead of a layer.

## The procedure

Mechanical, and it runs on what exists — no design taste
required, only honesty about the need each op serves:

1. **Inventory** every op the monolith still answers.
2. **Name the need** — one line, no personas — then hold the
   cut against the criteria above.
3. **Name the home** — an existing cap, a new cap (one per
   need), the host's view code, or death (a duplicate, or
   plumbing whose consumers are gone).
4. **Extract mechanically** — the pattern from `crates/AGENTS.md`:
   crate boundary first, capability wrapper second, monolith op
   deletion third. A unit is DONE when the ops are deleted and
   `just check` pins it.

## The table — every remaining monolith op, its need, its home

| monolith op(s) | the need | home | unit |
|---|---|---|---|
| sync_edits | sync the edit document (LWW merge, the sync protocol) | transit-sync is the library; the open question is placement only: cap-sync vs permanent seam resident | U7 |
| plans_resolve, plans_validate, plans_periods | know which service plan governs a date | same standing as transit-sync's merge — the U7 placement carries it | U7 |
| load_feed, drop_feed | the artifact's own registry lifecycle — the sync seam's feed-scoped validation keeps them until U7's placement speaks | rides U7 | — |

## The units

- **U2 the need cut of the day views.** cap-dispatch was a
  persona ("the operator's day") and cap-gtfs answered day
  questions over the feed beside it — the criteria dissolve
  both debts:
  * **cap-board** — *when does service leave this stop?* at a
    stop, a station's pooled bays, or a personal saved set:
    `load_network`, `load_edits`, `departures`,
    `station_departures`, and the WATCHLIST document
    (load / migrate / board) moves here from cap-gtfs — a
    personal multi-stop board is the same question
    parameterized by a personal document.
  * **cap-runcut** — *what does this vehicle do today?* the
    day's blocks list (the index facet) and one block's authored
    chain with the layover ripple (the detail facet):
    `load_network`, `load_edits`, `blocks`, `block_trips`.
    Replaces cap-dispatch on the wire — "dispatch" is a persona
    word.
  * **cap-calendar** — *which service keys run today?*
    `load_network`, `load_edits`, `active_services`, answered
    from the EFFECTIVE day: a key whose every trip is skipped
    or unrepresentable is not running, however its calendar
    reads — the same honesty as the runcut view's empty hole.
  * cap-gtfs keeps only its subject, the FEED document:
    identity, pickers, names (until U3 takes the pickers and
    the names join to the directory). Its day reads (feed-side
    `departures`, `station_departures`, `blocks`, `block_trips`,
    `active_services`) are duplicates and die — criterion 6.
  * transit-core's feed-side day answers (`next_departures`, the
    walking-group pooling, `blocks` / `blocks_on`, `block_trips`,
    `active_services`) lose their last BUS callers — the wire never
    answers them again. The methods themselves die in U8, with the
    substrate they belong to.
  * transit-dispatch stays the shared library under all three
    (off the wire; one library under several capabilities is
    the sanctioned pattern); its framing goes need-first.
- **U3 the network directory + the feed line.** The pickers are
  MODEL questions — answering them over the feed was the old
  plan's shape, and it fails the criteria. **cap-directory**:
  *what exists in the network?* — `load_network` (plumbing)
  plus `stops` (the list, carrying the walking-group structure
  the station fallback keys on), `stops_by_ids` (the names join,
  moved from cap-gtfs — one question, one answer), `routes`,
  and `route_stop_sequence` (the drill-down: one route's stops,
  in order). The UI pickers rewire onto it; the feed line reads
  what cap-gtfs already answers (`load_feed`'s summary,
  `feed_info` — the feed document's identity read, genuinely
  feed-side); cap-gtfs ends the unit as parse + identity alone;
  the four monolith ops die.
- **U4 coverage.** The UI registers population layers through
  cap-analysis directly (`load_population_layer`,
  `type_coverage` — the answers already exist; the duplicate
  monolith op dies); delete `coverage_load`.
- **U5 planning (mechanical — there is no decision).** Planning
  composes over the NETWORK and the effective day, like every
  model-side capability — the router's feed-document input is a
  Stage-B-era shape, not an option to weigh. RAPTOR moves onto
  materialized model trips over the network (+ the day's edits —
  the same transit-dispatch composition), the wrapper answers
  by network_hash, the rider planner rewires, plan_trip /
  plan_trip_compare die. The diversion delta (compare) falls out
  of the same composition: the baseline is the unpatched day,
  already one materialization apart.
- **U6 the map view (mechanical).** The map renders from model
  documents — the network (patterns, stops — the directory's
  subject), the effective day (cap-board / cap-runcut), the edit
  document's detour polylines, the trip map for click-throughs.
  Delete the five geojson ops. `set_diversions` dies here, with
  U5.
- **U7 sync + plans (rides sync persistence).** Placement only:
  cap-sync vs permanent seam residents for `sync_edits` and the
  plans chain — both are cross-document logic over transit-sync,
  the same standing as its merge. The sync-persistence design
  (blobs + pointers, migrate-at-receive, pointer CAS) is the
  standing queue's own unit — and U7 does NOT design it: the
  pointer CAS + reflog mechanism is the host discipline's (the
  section below); U7 places ops, never persistence.
- **U8 the GTFS substrate retires.** transit-core is the GTFS
  serialization spec wearing core's name — the network is the
  core model, and GTFS exists to translate it. The spec's one
  home is the codec: parse, calendar, and the deterministic zip
  write fold into transit-codec; cap-gtfs re-points at the codec
  (the feed document's reads and the spec library sit on the
  same side of the boundary); transit-rt's joins move onto the
  network + the import's trip-id map (their model-side join
  already exists). The crate dies.

**Order:** U2 first — it fixes the module shape every later unit
lands on. U3, U4 independent after it. U5 before U6 (the map's
effective times come from the day; planning's move settles the
document shape). U7 independent. U8 last among the units — U5
frees the router, and rt's join move rides with it.

## The host discipline (after the units)

The units retire the monolith's answers; this retires its
orchestration debt. Documents-only composition moved every
sequenced decision into the host — the host pays that debt as a
designed layer or it rots into glue copied per component.

- **The choke point.** Exactly one piece of host code calls a
  module op — the ENGINE, the capabilities resolver grown an
  execution half. Components request flows by name; they never
  see the ABI, the op surface, or an unverified hash. Enforced
  the way the crate laws are: an arch gate in `just test` fails
  the build on any op call outside the engine.
- **The engine interprets; the flow maps describe.** A flow map
  — which documents, which ops, which order, which renderer,
  which DECLARED degrade — is data in the UI manifest document
  (the wire-time assembly item). The engine carries zero domain
  knowledge (the Go-server rule applied to the browser): if it
  names a departure or a trip, domain logic leaked. Flow maps
  validate at load: every op exists in the manifest, every edge
  matches the generated contracts, every op node carries a
  degrade path — a partial selection that would blank a screen
  fails at assembly, not at runtime. Stale answers are the
  engine's to discard: every flow request carries a generation,
  superseded generations never land.
- **Verified documents by construction.** A query op cannot
  accept a raw hash: the engine's load-and-assert step mints a
  branded `VerifiedDoc`, the op wrappers take the brand, and
  glue has no path to one without the assertion having run —
  `assertSameDocument` holds by construction, not discipline.
- **The Merkle journal.** Every session is a log written at the
  choke point: per op, the input document hashes, the artifact
  hash, the output hashes — hashes only, never payloads (the
  blob store retains, unreachable blobs are collected, and
  ephemeral inputs such as rt snapshots are marked so replay
  degrades honestly — reported, never silent). One artifact,
  three uses: the trace, the answer-cache key, the migration
  witness.
- **The store's two git pieces.** A REFLOG — pointer-move
  history, so migrations and rollbacks are audited facts — and
  pointer CAS, the sync-persistence item's ref-update
  discipline. The journal is the same mechanism at per-device
  scope; a second persistence design is the monolith's ghost
  and is rejected on sight.
- **Replay is the proof procedure.** The runner replays
  journals natively — same artifact bytes, same input hashes,
  re-execute, compare output hashes: the golden corpus
  generalized from the fixture to any session. It is how the
  dogfood debt is PAID: the re-pinning sessions U1–U4 owe are
  journal replays of the C-TRAN session through the new
  modules, not hand-run scripts. Artifact replacement becomes
  re-pin + replay against the candidate — a per-session
  differential check, not a hope.
- **Two gates ride with the work.** No op call outside the
  engine; and determinism enforced in the modules — no clocks,
  randomness, or environment inside a cap crate, hosts pass
  time in (`now_ms` is the only route). Replay-as-migration,
  cross-host equality, and the answer cache are all conditional
  on determinism; the gate makes the fact checked, not assumed.

Order: the journal lands first, in the runner — `caps-run`'s
chained-session format BECOMES the journal format, the
crystallize-on-recurrence rule applied to the tool that already
improvises it. The engine and flow maps crystallize from the
recurring host glue. The reflog and CAS land with sync
persistence. None of it blocks a unit — but U7 places ops and
never persistence: pointer CAS and the reflog are this
section's, carried by the standing queue.

## The finish line

After U7 no domain op remains in the monolith. transit-ops
reduces to the seam's permanent residents (or nothing),
transit-wasm retires, and the Go server hosts the capability
modules directly. The monolith goes away ENTIRELY — that is the
point of the whole plan. U8 follows: the serialization spec's
library folds into the codec, and nothing named "core" holds
GTFS again.
