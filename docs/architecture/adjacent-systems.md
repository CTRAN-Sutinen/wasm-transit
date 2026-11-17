# Adjacent systems

The systems that run the dispatcher's day are out of scope (D-122).
They are CAD/AVL, workforce management, and the agency's scheduling
system. Each one touches everything here. Unlike a source, a
presentation, or an oracle, an adjacent system both reads this repo's
documents and writes into its records. The data is the agency's,
whichever system holds it (`docs/architecture/agency-data.md`). Each
fact has one home. The agency's documents in its store hold the
network, picks, plans, policy, and the authored detours and cancels.
The adjacent system's own rows hold a sick call, an extra-board fill, a
swap, and a vehicle assignment. The seam:

**Out to an adjacent system**

1. **Documents, by hash.** Their shapes are the registry's schemas
   (`registry/schemas/`, indexed by `registry/documents.json`;
   `docs/FORMATS.md` for the at-rest envelope). The manifest is the
   index of what is in force. An adjacent system keeps a document as a
   read-only cache keyed by its hash. It never keeps a second home for
   the fact. It never queries a table for domain answers. Those are ops.
2. **Standard formats wherever one exists:**
   - the GTFS zip in force (`GET /feed`);
   - GTFS + TODS for the operations plan (D-126);
   - GTFS-rt for the authored cancels and detours. This repo is an
     authoring surface for what GTFS-rt carries, but no op encodes it
     yet (D-124, deferred).

   The registry's schemas cover only what the standards cannot say.
3. **References carry their version.** An external reference names the
   document with the entity. Name a pick id with a trip id. Name a trip
   instance (trip, start, service date) with its `network_hash`. A bare
   id can name different things in different picks (D-123).

**In from an adjacent system**

4. **Only as records, through the receive path.** Write with
   `POST /edits` or a pointer commit (`docs/FORMATS.md`, "The HTTP
   surface"). Use the system's own credentials, so the log's `writer`
   names it. The module validates each record, and the per-record LWW
   merge applies. Nothing writes the bucket directly. An adjacent system
   is one more writer under the receive contract
   (`docs/architecture/agency-data.md`).
5. **Effects in this repo's vocabulary, never their causes.** A sick
   call with no extra-board fill arrives as a trip cancel. The sick call
   stays in the workforce system.
6. **A pointer back to the source, opaque.** A record has no field for
   the source system's id yet (D-125). When it gets one, this repo
   stores and serves it for audit joins. This repo never branches on it.
   The rule is the one an oracle's label follows.

**The work order system** (D-159, D-165) is reached the other way. The
server calls it through an adapter (`server/workorders.go`). Nothing
here couples to the system. A stop request tracks the work order it
opens. The adapter opens one for a submitted request and reads its
status back. The adapter writes its answer onto the request through
cap-requests (`record_work_order`, checked as any write) under the
`stop_requests` pointer. The writer is `work-orders:<system>`. This is
an effect in this repo's vocabulary, and its reference is opaque.

`-work-orders` names the adapter. When it is unset, no system is
configured. The manifest offers nothing, and a person enters a request's
work order by hand. `stub:` is a stand-in for tests and demos. The real
system's adapter is written from its OpenAPI spec behind the same
interface.

## Why

Each entry covers one choice that the design depends on. Its heading
names the code the choice governs. **Holds** is what must stay true.
Change it only with the user, and only after you read **Because**.

### The dispatcher's day is out of scope; the seam is defined (this file's six rules)
- **Holds:** the dispatcher's day lives in CAD/AVL, workforce
  management, and the scheduling system's rows. The network, picks,
  plans, and the authored detours and cancels live in the agency's
  documents. The two meet only at the seam above. Documents go out by
  hash or standard format, and records come in through the receive path.
- **Because:** the dispatcher's day (sick calls, fills, swaps,
  assignments) is row-shaped data. Many hands write it, and people
  search it by attribute. Those systems already hold it. This repo's
  data is published and acted on as whole documents. One home per fact
  keeps audits answerable.
- **Rejected:** a row store in this repo for daily operations.
- **Decided:** D-122.

### The work order system is behind an adapter, and optional (`server/workorders.go`, `-work-orders`)
- **Holds:** the server reaches the agency's work order system only
  through an adapter. With no adapter configured, nothing is offered,
  and a request's work order is entered by hand. What the system answers
  lands on the request through cap-requests. The write is checked as any
  write, and the answer is shown as given.
- **Because:** no tight coupling to one vendor's system. The agency
  picks when to wire it, from the system's OpenAPI spec.
- **Rejected:** calling the system's API straight from the pages;
  requiring it for requests to work.
- **Decided:** D-159, D-165.
