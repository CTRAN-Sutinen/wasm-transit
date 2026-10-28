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

**Exists already:** the plan chain (cap-plans), the publish path
(cap-network `export`: deterministic GTFS zip bytes under their
content hash), the manifest concept, plan persistence in the
browser.
**Needs:** server-side persistence (`docs/units/sync-persistence.md`).
**Unblocks:** 6, and multi-client deployments at all.

## 2. The runcut as a first-class tool

**What.** Cut blocks from schedules: auto-chain feasible trips,
estimate **deadheads via the street graph** (the differentiated
piece — no GTFS toolchain does this), author layovers, and answer
the question every runcut ends in: **peak vehicle requirement** per
service type (QAF-3; the layover recovery/slack split, QAF-62, is
what makes a cut's feasibility real).

**Why.** Blocks exist but are imported, not cut. This is the
planner's daily work; the deadhead integration (design domain ×
geometry domain, mediated by the host) is also the first real
exercise of the module-composition thesis.

**Exists already:** blocks + layovers in the model, feasibility
checks, the street graph with routing, service-type analysis.
**Unblocks:** 7 (runs are cut from blocks), fleet planning.

## 3. The operations → planning feedback loop

**What.** Accumulate adherence history from the rt feeds (locally,
or server-side if it earns a role). Answer "which trips are
chronically late," and let the planner import that evidence into
the next pick's timing profiles.

**Why.** Measure → plan → publish → measure. No scheduling product
closes this loop locally-first; it is the most defensible long-term
feature here.

**Exists already:** rt decode, timing profiles, the plan chain.
**Needs:** accumulation storage (IndexedDB first), reporting ops.

## 4. Walkshed coverage (not crow-flies)

**What.** Compute real walksheds from stops through the street
graph (bounded by walk time), and re-run population coverage against
them.

**Why.** The planner's opening question ("what share of the
population does this network serve?") deserves an answer that
survives scrutiny. 400 m circles are a placeholder.

**Exists already:** street graph, population layer, coverage ops
parameterized already.

## 5. Rider wins (filler that pays visibly)

- **ServiceAlerts** — the third rt feed.
- **Live replanning** — rt says you miss the connection → replan;
  the artifact has RAPTOR and the rt decode; they have not met.
- **PWA / offline** — the app is already local-first; installable
  offline trip planning is nearly free.

## 6. The data plane: module composition

**First composition proof:** deadhead estimation (runcut ×
streets) — two modules composed by the host over documents
(composition is documents only — the bus's standing law).

## 7. Runs and the bid

**What.** Cut blocks into operator runs under work rules (report
and clear times and places, pieces, platform time, pay), and run
the bid: operators pick runs from the pick's offer in seniority
order. Runs, work rules, and bids are their own documents and
modules beside the network (D-16), never a `Block` extension.

**Why.** A pick ends in work operators can bid on. Blocks answer
what the vehicles do; runs answer what the people do, and the bid
is how a pick reaches them.

**Exists already:** blocks + layovers, the block chain with the
layover ripple, the sync document's LWW-per-record discipline (a
bid is shared state).
**Needs:** 2 (runs are cut from blocks); the work-rule policy
(QAF-12) and workforce data (QAF-17); the open findings QAF-11,
QAF-14, QAF-15.
**Unblocks:** `docs/units/ui-runcut.md`'s runs questions and
`docs/units/ui-work-bid.md`.

---

## Sequencing

**1** first (complete the service-change story; a pick that
can't publish is a document), **2** next (planner's daily work),
**7** after 2 (runs are cut from blocks, and the bid is where a
pick reaches operators), **3** as the long moat. 4 and 5 slot anywhere. 6's discipline (composition is
documents only) is the bus's standing law; its first proof is the
deadhead composition in 2.
