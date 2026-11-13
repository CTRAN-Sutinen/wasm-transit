# Adjacent systems

The systems that run the dispatcher's day — CAD/AVL, workforce
management, the agency's scheduling system — are out of scope (D-122)
and touch everything here. Unlike a source, a presentation, or an
oracle, an adjacent system both reads this repo's documents and writes
into its records. The data is the agency's, whichever system holds it
(`docs/architecture/agency-data.md`); each fact has one home: the
agency's documents in its store (the network, picks, plans, policy, the
authored detours and cancels), or the adjacent system's own rows (a sick
call, an extra-board fill, a swap, a vehicle assignment). The seam:

**Out to an adjacent system**

1. **Documents, by hash.** Their shapes are the registry's schemas
   (`registry/schemas/`, indexed by `registry/documents.json`;
   `docs/FORMATS.md` for the at-rest envelope), and the manifest is the
   index of what is in force. An adjacent system keeps a document as a
   read-only cache keyed by its hash — never a second home for the fact,
   and never a table it queries for domain answers: those are ops.
2. **Standard formats wherever one exists:** the GTFS zip in force
   (`GET /feed`); GTFS + TODS for the operations plan (D-126); GTFS-rt
   for the authored cancels and detours — this repo is an authoring
   surface for what GTFS-rt carries (D-124, deferred: no op encodes it). The registry's schemas cover only what
   the standards cannot say.
3. **References carry their version.** An external reference names
   the document with the entity: a pick id with a trip id, a trip
   instance (trip, start, service date) with its `network_hash`. A bare
   id can name different things in different picks (D-123).

**In from an adjacent system**

4. **Only as records, through the receive path** — `POST /edits` or a
   pointer commit (`docs/FORMATS.md`, "The HTTP surface"), under the
   system's own credentials, so the log's `writer` names it; the module
   validates and the per-record LWW merge applies. Nothing writes the
   bucket directly: an adjacent system is one more writer under the
   receive contract (`docs/architecture/agency-data.md`).
5. **Effects in this repo's vocabulary, never their causes.** A sick
   call with no extra-board fill arrives as a trip cancel; the sick call
   stays in the workforce system.
6. **A pointer back to the source, opaque.** A record has no field for
   the source system's id yet (D-125); once it does, this repo stores
   and serves it for audit joins and never branches on it — the rule
   an oracle's label follows.

**The work order system** (D-159, D-165) is reached the other way: the
server calls it through an adapter (`server/workorders.go`), so nothing
here couples to it. A stop request tracks the work order it opens; the
adapter opens one for a submitted request and reads its status back,
and what it answers is written onto the request through cap-requests
(`record_work_order`, checked as any write) under the `stop_requests`
pointer, the writer `work-orders:<system>` — an effect in this repo's
vocabulary, its reference opaque. `-work-orders` names the adapter:
unset, none is configured, the manifest offers nothing, and a request's
work order is entered by hand; `stub:` is a stand-in for tests and
demos. The real system's adapter is written from its OpenAPI spec
behind the same interface.

## Why

One entry per load-bearing choice, headed by the code it governs.
**Holds** is what must stay true; change it only with the user, after
reading **Because**.

### The dispatcher's day is out of scope; the seam is defined (this file's six rules)
- **Holds:** the dispatcher's day lives in CAD/AVL, workforce management, and the scheduling system's rows; the network, picks, plans, and the authored detours and cancels live in the agency's documents. They meet only at the seam above: documents out by hash or standard format, records in through the receive path.
- **Because:** the dispatcher's day (sick calls, fills, swaps, assignments) is row-shaped data written by many hands and searched by attribute, which those systems already hold; this repo's data is published and acted on as whole documents. One home per fact keeps audits answerable.
- **Rejected:** a row store in this repo for daily operations.
- **Decided:** D-122.

### The work order system is behind an adapter, and optional (`server/workorders.go`, `-work-orders`)
- **Holds:** the server reaches the agency's work order system only through an adapter; with none configured, nothing is offered and a request's work order is entered by hand. What the system answers lands on the request through cap-requests, checked as any write, shown as given.
- **Because:** no tight coupling to one vendor's system; the agency picks when to wire it, from the system's OpenAPI spec.
- **Rejected:** calling the system's API straight from the pages; requiring it for requests to work.
- **Decided:** D-159, D-165.
