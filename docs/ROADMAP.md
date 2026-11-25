# Roadmap

This file shows where the project is going. It sequences the steps by
how much each one changes what the software is. It does not sequence
them by effort. Each item names what must be true before it starts, and
what it unblocks. This is a plan, not a promise. Re-sequence it as the
facts change.

## The arc

The architecture thesis has four parts:

- **The same documents and contracts everywhere, assembled for each UI
  from the capability bus.**
- **Local-first.**
- **The document is the authority.**
- **The app is a viewer over documents.**

Every item below is chosen because it is *enabled by what already
exists* and moves the thesis from demonstrable to operational.

---

## 1. Make the pick real: publish → serve → resolve

**What.** The Go server serves plan documents and the materialized
feed per service change date. The manifest protocol already pins
`network_hash`, the zip's `feed_hash`, and the sync module's hash. It
grows plan documents. Every client resolves "which plan is in force
today" through the plans capability (`resolve` is on the bus). Each
client then re-pins automatically.

**Why it is first.** A pick that cannot be published is still just a
document. This turns the service-plan chain into operational reality:
three picks a year, no merged feeds, no scrambling on the day of
service. It is the feature that makes the architecture something an
agency could run.

**Exists already:**

- The plan chain (cap-plans).
- The GTFS publish path (cap-network `export`): deterministic GTFS zip
  bytes under their content hash.
- Plan persistence in the browser.
- The durable store.
- Publish → serve → resolve itself. `POST /plans` stores a pick and the
  published plan set. The manifest pins it (`plans_hash`). The rider
  pages load the pick in force.

The planner publishes from the service-design UI (or `caps-run --post`).
The planner retires a pick that has not taken effect. The server
serves the pick in force as GTFS (`GET /feed`).

**Needs:** nothing. It runs. A deployment beyond localhost follows
`DEPLOYMENTS.md` and `DEPLOYMENTS-SERVER.md`.

**Unblocks:** 6, and multi-client deployments at all.

## 2. The runcut as a first-class tool

**What.** The tool does these things:

- It cuts blocks from schedules.
- It auto-chains feasible trips.
- It estimates **deadheads via the street graph**. This is the
  differentiated piece: no GTFS toolchain does this.
- It authors layovers.
- It answers the question every runcut ends in: **peak vehicle
  requirement** per service type (QAF-3).

**Why.** Blocks exist but are imported, not cut. This is the planner's
daily work. The deadhead integration (design domain × geometry domain,
mediated by the host) is also the first real exercise of the
module-composition thesis.

**Exists already:**

- Blocks and layovers in the model.
- Feasibility checks that judge the recovery required at each terminal.
  The work rules set that recovery. Without a work rule, the terminal's
  floor sets it.
- The recovery view.
- Suggested blocking of unassigned trips under the work rules, with the
  peak it needs (no deadheads yet).
- The street graph with routing.
- Service-type analysis.

**Unblocks:** 7 (runs are cut from blocks), fleet planning.

## 3. The operations → planning feedback loop

**What.** Read operations history as TIDES: `stop_visits` and
`trips_performed`, produced by the agency's AVL, APC, and fare vendors
(D-127). Answer "which trips are chronically late" and "what running
times did each pattern actually need, by time band." The planner can
carry that evidence into the next pick's timing profiles. The history
stays with the systems that own it. This repo consumes TIDES and never
produces it.

**Why.** Measure → plan → publish → measure. No scheduling product
closes this loop while it runs local-first. It is the most defensible
long-term feature here.

**Exists already:** rt decode, timing profiles, the plan chain (a
service date resolves the pick that governed it).

**Needs:**

- A TIDES import (a source: a span of summaries becomes a document by
  hash).
- The analysis ops over it.
- A vendor's TIDES export to dogfood against.

## 4. Walkshed coverage (not crow-flies)

**What.** Compute real walksheds from stops through the street graph,
bounded by walk time. Re-run layer coverage against them.

**Why.** The planner's opening question ("what share of the population
does this network serve?") deserves an answer that survives scrutiny.
400 m circles are a placeholder.

**Exists already:**

- The streets document and its graph.
- The walk table (walking along the streets, between stops).
- Weighted layers for points and polygons.
- Coverage ops, already parameterized.

**Needs:** a walkshed op over the walking graph
(`docs/units/streets-model.md`), and an OSM-sourced streets document for
walking (QAF-85).

## 5. Rider wins (filler that pays visibly)

- **Live replanning:** when rt says the rider misses the connection, the
  app replans. The router plans over the live document (`live_hash`). No
  UI re-plans when a new snapshot breaks the rider's connection.
- **PWA / offline:** every generated UI installs and works offline, and
  a staff page's writes wait on the device until the server answers
  (`docs/architecture/updates-offline.md`).

## 6. The data plane: module composition

**First composition proof:** deadhead estimation (runcut × streets). The
host composes two modules over documents. Composition is documents only.
This is the bus's standing law.

## 7. Runs and the bid

**What.** Cut blocks into operator runs under work rules. The work rules
cover report and clear times and places, pieces, platform time, and pay.
Run the bid: operators pick runs from the pick's offer in seniority
order. Runs, work rules, and bids are their own documents and modules,
beside the network (D-16). They are never an extension of `Block`. The
runs model takes TODS's concepts: runs as ordered run events, deadheads
as non-revenue trips, and internal locations. GTFS and TODS form its
interchange with scheduling and CAD/AVL systems (D-126).

**Why.** A pick ends in work operators can bid on. Blocks answer what the
vehicles do. Runs answer what the people do. The bid is how a pick
reaches them.

**Exists already:**

- Blocks and layovers, and the block chain with the layover ripple.
- The work-rule document: run rules and pay, every rule optional.
- The sync document's LWW-per-record discipline (a bid is shared state).

**Needs:**

- 2, since runs are cut from blocks.
- Workforce data (QAF-17).
- The runs cut. It is the work in `docs/units/runs-layer.md`
  (QAF-11, QAF-12, QAF-14, QAF-15).

**Unblocks:** `docs/units/ui-runcut.md`'s runs questions and the bid
(queued in `docs/units/README.md`).

---

## Sequencing

- **1** first. It completes the service-change story. A pick that cannot
  publish is only a document.
- **2** next. It is the planner's daily work.
- **7** after 2. Runs are cut from blocks. The bid is where a pick
  reaches operators.
- **3** as the long-term item.
- 4 and 5 fit anywhere.
- 6's discipline is the bus's standing law: composition is documents
  only. Its first proof is the deadhead composition in 2.
