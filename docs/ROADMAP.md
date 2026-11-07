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
`network_hash` + the zip's `feed_hash` + the sync module's hash)
grows plan documents; every
client resolves "which plan is in force today" through the plans
capability (`resolve` is on the bus) and re-pins automatically.

**Why it is first.** A pick that cannot be published is still just a
document. This turns the service-plan chain into operational
reality: three picks a year, no merged feeds, no day-of scrambling.
It is the feature that makes the architecture something an agency
could run.

**Exists already:** the plan chain (cap-plans), the GTFS publish
path (cap-network `export`: deterministic GTFS zip bytes under their
content hash), plan persistence in the browser, the durable store,
and publish → serve → resolve itself: `POST /plans` stores a pick and
the published plan set, the manifest pins it (`plans_hash`), and the
rider pages load the pick in force.
The planner publishes from the service-design UI (or `caps-run
--post`), retires a pick that has not taken effect (D-75), and the
server serves the pick in force as GTFS (`GET /feed`).
**Needs:** nothing — it runs; a deployment beyond localhost is
`docs/units/deploy-rider-apps.md`.
**Unblocks:** 6, and multi-client deployments at all.

## 2. The runcut as a first-class tool

**What.** Cut blocks from schedules: auto-chain feasible trips,
estimate **deadheads via the street graph** (the differentiated
piece — no GTFS toolchain does this), author layovers, and answer
the question every runcut ends in: **peak vehicle requirement** per
service type (QAF-3).

**Why.** Blocks exist but are imported, not cut. This is the
planner's daily work; the deadhead integration (design domain ×
geometry domain, mediated by the host) is also the first real
exercise of the module-composition thesis.

**Exists already:** blocks + layovers in the model, feasibility
checks judged against the recovery required at each terminal (the
work rules', else the terminal's floor — D-9), the recovery view,
suggested blocking of unassigned trips under the work rules with the
peak it needs (D-46; no deadheads yet), the street graph with
routing, service-type analysis.
**Unblocks:** 7 (runs are cut from blocks), fleet planning.

## 3. The operations → planning feedback loop

**What.** Read operations history as TIDES (D-127) — `stop_visits`
and `trips_performed`, produced by the agency's AVL, APC, and fare
vendors — and answer "which trips are chronically late" and "what
running times did each pattern actually need, by time band," so the
planner can carry that evidence into the next pick's timing profiles.
The history stays with the systems that own it; this repo consumes
TIDES and never produces it (D-128).

**Why.** Measure → plan → publish → measure. No scheduling product
closes this loop locally-first; it is the most defensible long-term
feature here.

**Exists already:** rt decode, timing profiles, the plan chain (a
service date resolves the pick that governed it).
**Needs:** a TIDES import (a source: a span of summaries becomes a
document by hash), the analysis ops over it, and a vendor's TIDES
export to dogfood against.

## 4. Walkshed coverage (not crow-flies)

**What.** Compute real walksheds from stops through the street
graph (bounded by walk time), and re-run layer coverage against
them.

**Why.** The planner's opening question ("what share of the
population does this network serve?") deserves an answer that
survives scrutiny. 400 m circles are a placeholder.

**Exists already:** the streets document and its graph, the walk
table (walking along the streets, between stops), weighted layers
(points and polygons, D-47), coverage ops parameterized already.
**Needs:** a walkshed op over the walking graph
(`docs/units/streets-model.md`), and an OSM-sourced streets document
for walking (QAF-85).

## 5. Rider wins (filler that pays visibly)

- **Live replanning** — rt says you miss the connection → replan.
  The router plans over the live document (`live_hash`); no UI
  re-plans when a new snapshot breaks the rider's connection.
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
modules beside the network (D-16), never a `Block` extension. The
runs model takes TODS's concepts — runs as ordered run events,
deadheads as non-revenue trips, internal locations — and GTFS + TODS
is its interchange with scheduling and CAD/AVL systems (D-126).

**Why.** A pick ends in work operators can bid on. Blocks answer
what the vehicles do; runs answer what the people do, and the bid
is how a pick reaches them.

**Exists already:** blocks + layovers, the block chain with the
layover ripple, the work-rule document (D-50: run rules and pay,
every rule optional), the sync document's LWW-per-record discipline
(a bid is shared state).
**Needs:** 2 (runs are cut from blocks); workforce data (QAF-17);
the runs cut (`docs/units/runs-layer.md`: QAF-11, QAF-12, QAF-14,
QAF-15).
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
