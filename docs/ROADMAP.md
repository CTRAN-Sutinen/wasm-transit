# Roadmap

Where this is going, sequenced by how much each step changes what
the software IS — not by effort. Each item names what must be true
before it starts, and what it unblocks. (A plan, not a promise:
re-sequence as reality argues.)

## The arc

The architecture thesis: **the same documents and contracts
everywhere, assembled per user from the capability bus; local-first;
the document is the authority; the app is a viewer over documents.**
Every item below is chosen because it is *enabled by what already
exists* and moves the thesis from demonstrable to operational.

---

## 1. Make the pick real: publish → serve → resolve

**What.** The Go server serves plan documents and the materialized
feed per service change date. The manifest protocol (already pins
`feed_hash` + the sync module's hash) grows plan documents; every
client resolves "which plan is in force today" through the plans
capability (`resolve` is on the bus) and re-pins automatically.

**Why it is first.** A pick that cannot be published is still just a
document. This turns the service-plan chain into operational
reality: three picks a year, no merged feeds, no day-of scrambling.
It is the feature that makes the architecture something an agency
could run.

**Exists already:** the plan chain (cap-plans), materialization
through the network capability's export, the manifest concept,
plan persistence.
**Unblocks:** 2, 7, and multi-client deployments at all.

## 2. GTFS export: model → zip

**What.** Serialize the derived feed view back to a GTFS zip — the
publish format for the wider ecosystem (apps, aggregators).

**Why.** Closes the loop named at the start of the project: author
in the model, emit GTFS for compatibility. After this, the artifact
is the authority and GTFS is purely a codec — the statement the
whole model was built to make.

**Exists already:** `network_to_gtfs` (the derived view is built;
only zip serialization + file emission are missing).
**Unblocks:** real-world interop; agency adoption that requires GTFS.

## 3. The runcut as a first-class tool

**What.** Cut blocks from schedules: auto-chain feasible trips,
estimate **deadheads via the street graph** (the differentiated
piece — no GTFS toolchain does this), author layovers, and answer
the question every runcut ends in: **peak vehicle requirement** per
service type.

**Why.** Blocks exist but are imported, not cut. This is the
planner's daily work; the deadhead integration (design domain ×
geometry domain, mediated by the host) is also the first real
exercise of the module-composition thesis.

**Exists already:** blocks + layovers in the model, feasibility
checks, the street graph with routing, service-type analysis.
**Unblocks:** duties/rosters (the layer above), fleet planning.

## 4. The operations → planning feedback loop

**What.** Accumulate adherence history from the rt feeds (locally,
or server-side if it earns a role). Answer "which trips are
chronically late," and let the planner import that evidence into
the next pick's timing profiles.

**Why.** Measure → plan → publish → measure. No scheduling product
closes this loop locally-first; it is the most defensible long-term
feature here.

**Exists already:** rt decode, timing profiles, the plan chain.
**Needs:** accumulation storage (IndexedDB first), reporting ops.

## 5. Walkshed coverage (not crow-flies)

**What.** Compute real walksheds from stops through the street
graph (bounded by walk time), and re-run population coverage against
them.

**Why.** The planner's opening question ("what share of the
population does this network serve?") deserves an answer that
survives scrutiny. 400 m circles are a placeholder.

**Exists already:** street graph, population layer, coverage ops
parameterized already.

## 6. Rider wins (filler that pays visibly)

- **ServiceAlerts** — the third rt feed.
- **Live replanning** — rt says you miss the connection → replan;
  the artifact has RAPTOR and the rt decode; they have not met.
- **PWA / offline** — the app is already local-first; installable
  offline trip planning is nearly free.

## 7. The data plane: module composition

**First composition proof:** deadhead estimation (runcut ×
streets) — two modules composed by the host over documents
(composition is documents only — the bus's standing law). The
runcut's growth toward the operational bid (duties, the package
operators bid on) is a future layer, not a Block extension (the
standing non-goal).

---

## Sequencing

**1 → 2** (complete the service-change story; a pick that can't
publish is a document), **3** next (planner's daily work — and
the runcut's growth toward the operational bid), **4** as the
long moat. 5 and 6 slot anywhere. 7's discipline (composition is
documents only) is the bus's standing law; its first proof is the
deadhead composition in 3.
