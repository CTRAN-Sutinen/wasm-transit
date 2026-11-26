# runs

The runs: what does each operator do today? The capability cuts a service day's blocks into operator runs under the work rules of a labor agreement and, when given, the agency's scheduling constraints. It reads the materialized day over the network document, patched with the in-force edits, as the runcut does. The domain code lives in the shared transit-runs library.

## cut

Cut the day's blocks into runs. The request names the network, the date, the work rules, and optionally the edits and the scheduling constraints.

The cut is a heuristic, and it is deterministic. A block that makes one good run stays whole. A good run is legal, in a class that the rules' objective maximizes, and pays no premium or guarantee. Any other block is cut at reliefs into the longest pieces that are legal alone. A relief is after a trip that ends at a relief point, or after any trip when the scheduling constraints name none. Then each piece, earliest first, takes the later piece that makes the best run with it, when that run costs less than the two apart. A piece that pairs with none is a run alone.

The answer has these parts:

- `runs`: each run's `id`, its `class` (absent when no class fits), its `pieces` (each with its block, its trips, and whether it starts at the pull-out or ends at the pull-in), and `judged`:
  - `timeline`: the run's activities in order, from the first report to the last clear. Allowances come at each event, layover between two trips of a piece, a gap between two pieces, and travel where a travel rule pays it.
  - `spread_secs` and `measures`: each measure the rules define, totaled over the timeline.
  - `paid_secs`: the time paid at the base rate, each gap at its band's share. `premium_secs`: what the premiums add. `guarantee_secs`: what the guarantees add. `pay_secs`: the three together. Pay is a planning measure in seconds, not dollars.
  - `broken`: every limit the run breaks and every break rule it misses, each with its strength and the clause it cites. A run is legal when every broken rule is waivable.
  - `not_judged`: the rules that one run cannot answer, such as a limit over a week, and travel with no trip found home.
- `unblocked`: each trip that runs on the day and that no block carries, as `trip@start_secs`. The cut does not place them.
- `mix`: each mix rule's share of runs or of paid time, against its minimum, target, and maximum. `met` says whether the share is within the minimum and the maximum.
- `warnings`: the date is outside the work rules' `source` period.

Each trip of each block that runs is in exactly one piece of one run.
