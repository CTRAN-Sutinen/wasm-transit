# Adjacent systems

The dispatcher's day is out of scope (D-122): sick calls, extra-board
fills, swaps, and which person and vehicle are on a run today.
CAD/AVL, workforce management, and the agency's scheduling system hold
it. The runs and the bid are planned here (`docs/ROADMAP.md` 7), and the
scheduling system is the final authority that a plan goes to. Unlike a
source, a presentation, or an oracle, an adjacent system both reads this
repo's documents and writes into its records. The data is the agency's,
whichever system holds it (`docs/architecture/agency-data.md`). Each
fact has one home. The agency's documents in its store hold the
network, picks, plans, policy, the authored detours and cancels, and
dispatch's internal notes (D-214). The adjacent system's own rows hold
a sick call, an extra-board fill, a swap, and a vehicle assignment. So
whether a trip is covered (a vehicle on its block, an operator on its
run) is the adjacent system's answer, and no op here computes it
(D-212). A cancel that CAD/AVL publishes in GTFS-rt is read as the
feed gives it (D-213). The interface has two directions.

**Out to an adjacent system**

1. **Documents, by hash.** Their shapes are the registry's schemas
   (`registry/schemas/`, indexed by `registry/documents.json`;
   `docs/FORMATS.md` for the at-rest envelope). The manifest is the
   index of what is in force. An adjacent system keeps a document as a
   read-only cache keyed by its hash. It keeps no second home for the
   fact, and it gets domain answers from ops, not from a table.
2. **Standard formats wherever one exists:**
   - the GTFS zip in force (`GET /feed`);
   - GTFS + TODS for the operations plan (D-126);
   - GTFS-rt for the authored cancels and detours. This repo authors
     what GTFS-rt carries, and no op encodes it (D-124, deferred).

   The registry's schemas cover only what the standards cannot say.
3. **References carry their version.** An external reference names the
   document with the entity. Name a pick id with a trip id. Name a trip
   instance (trip, start, service date) with its `network_hash`. A bare
   id can name different things in different picks (D-123).

**In from an adjacent system**

4. **Only as records, through the write path.** Write with
   `POST /edits` or a pointer commit (the contract is in
   `server/openapi.json`). Use the system's own credentials, so the
   log's `writer` names it. The module validates each record, and the
   per-record LWW merge applies. Nothing writes the bucket directly. An
   adjacent system is one more writer under the write contract
   (`docs/architecture/agency-data.md`).
5. **Effects in this repo's vocabulary, never their causes.** A sick
   call with no extra-board fill arrives as a trip cancel. The sick call
   stays in the workforce system.
6. **A pointer back to the source, opaque.** A record has no field for
   the source system's id (D-125). A field added later is stored and
   served for audit joins, and this repo never branches on it. An
   oracle's label follows the same rule.

**The work order system** (D-159, D-165) is reached the other way. The
server calls it through an adapter (`server/workorders.go`), and nothing
else here couples to the system. A stop request tracks the work order it
opens. The adapter opens one for a submitted request and reads its
status back. It writes its answer onto the request through cap-requests
(`record_work_order`, checked as any write) under the `stop_requests`
pointer. The writer is `work-orders:<system>`. The answer is an effect
in this repo's vocabulary, and its reference is opaque.

`-work-orders` names the adapter. When it is unset, the manifest offers
nothing, and a person enters a request's work order by hand. `stub:` is
a stand-in for tests and demos. The real system's adapter is written
from its OpenAPI spec behind the same interface.

## Why

Each entry covers one choice that the design depends on, headed by the
code it governs. **Holds** is what must stay true. Change it only with
the user, after you read **Because**.

### The dispatcher's day is out of scope; the seam is defined (this file's six rules)
- **Holds:** the dispatcher's day lives in CAD/AVL, workforce
  management, and the scheduling system's rows. The network, picks,
  plans, and the authored detours and cancels live in the agency's
  documents. The two meet only at the interface above. Documents go out
  by hash or standard format, and records come in through the write path.
- **Because:** the dispatcher's day is row-shaped data. Many hands write
  it, and people search it by attribute. Those systems already hold it.
  This repo's data is published and acted on as whole documents. One
  home per fact keeps audits answerable.
- **Rejected:** a row store in this repo for daily operations.
- **Decided:** D-122.

### Coverage is the adjacent systems' answer (the disruptions UI's questions, cap-divert's edits)
- **Holds:** no op answers whether a trip is covered, which vehicle is
  on a block today, or whether a late bus makes its next trip of the
  day. The edit document carries effects that riders see (a detour, a
  cancel, later a partial cancel). It carries no cover, hold, or swap.
- **Because:** coverage needs today's vehicle and operator
  assignments. CAD/AVL and the workforce system hold them. A rule here
  that puts a vehicle on a block before it reports a position is a
  second copy of that assignment, and it is wrong when the dispatcher
  moves a bus. runcut `block_trips` answers against the planned block,
  which is right for planning and not for today's assignments.
- **Rejected:** coverage from the planned blocks and the live
  positions, with an operator half from the runs layer (D-15); cover,
  hold, and swap as edit kinds.
- **Decided:** D-212 (it replaces D-15).

### The work order system is behind an adapter, and optional (`server/workorders.go`, `-work-orders`)
- **Holds:** the server reaches the agency's work order system only
  through an adapter. With no adapter configured, nothing is offered,
  and a request's work order is entered by hand. What the system answers
  lands on the request through cap-requests. The write is checked as any
  write, and the answer is shown as given.
- **Because:** the design avoids tight coupling to one vendor's system.
  The agency picks when to wire it, from the system's OpenAPI spec.
- **Rejected:** calling the system's API straight from the pages;
  requiring it for requests to work.
- **Decided:** D-159, D-165.
