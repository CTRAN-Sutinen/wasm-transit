# runs

The runs: what does each operator do today? The capability cuts a service day's blocks into pieces under the work rules of a labor agreement and, when given, the agency's scheduling constraints, composes pieces into runs, and judges any set of runs. Who composes a run does not matter: the agency, an operator at the bid, or a planner by hand. Every op reads the materialized day over the network document, patched with the in-force edits, as the runcut does. The domain code lives in the shared transit-runs library.

Each request names the network, the date, and the work rules, and optionally the edits. Each answer's `warnings` say when the date is outside the work rules' `source` period.

## cut

Cut the day's blocks into runs: `pieces`, then `compose`, in one op. The request may name the scheduling constraints.

The answer has these parts:

- `runs`: each run's `id`, its `class` (absent when no class fits), its `pieces` (each with its block, its trips, and whether it starts at the pull-out or ends at the pull-in), and `judged`:
  - `timeline`: the run's activities in order, from the first report to the last clear. Allowances come at each event, layover between two trips of a piece, a gap between two pieces, and travel where a travel rule pays it.
  - `spread_secs` and `measures`: each measure the rules define, totaled over the timeline.
  - `paid_secs`: the time paid at the base rate, each gap at its band's share. `premium_secs`: what the premiums add. `guarantee_secs`: what the guarantees add. `pay_secs`: the three together. Pay is a planning measure in seconds, not dollars.
  - `broken`: every limit the run breaks and every break rule it misses, each with its strength and the clause it cites. A run is legal when every broken rule is waivable.
  - `not_judged`: the rules that one run cannot answer, such as a limit over a week, and travel with no trip found home.
- `unblocked`: each trip that runs on the day and that no block carries, as `trip@start_secs`. The cut does not place them.
- `mix`: each mix rule's share of runs or of paid time, against its minimum, target, and maximum. `met` says whether the share is within the minimum and the maximum.

Each trip of each block that runs is in exactly one piece of one run.

## pieces

Cut the day's blocks into pieces. The request may name the scheduling constraints.

A block that makes one good run stays whole. A good run is legal, in a class that the rules' objective maximizes, and pays no premium or guarantee. Any other block is cut at reliefs into the longest pieces that are legal alone. A relief is after a trip that ends at a relief point, or after any trip when the scheduling constraints name none.

The answer has `pieces`, earliest first, and `unblocked`, as `cut` has them. Each trip of each block that runs is in exactly one piece.

## compose

Compose pieces into runs, a suggestion. The request names its `pieces`, each by its `block`, its `first_trip`, and its `last_trip`. A trip that the block runs more than once on the day also names its start (`first_start_secs`, `last_start_secs`). A piece that names a block or a trip that does not run on the day is refused.

A whole block that makes a good run alone stays a run. Each other piece, earliest first, takes the later piece that makes the best run with it, when that run is good or costs less than the two apart. A piece that pairs with none is a run alone. Composing the pieces that `pieces` answers gives the runs that `cut` answers.

The answer is the set of runs as `judge` reports it.

## judge

Judge a set of runs that anyone composed, as `cut` judges its own. The request names its `runs`, each with an `id`, its `pieces` named as `compose` names them, and optionally its `class`. It may name the scheduling constraints.

A run with no class takes the class that the cut would give it. A run that does not meet its class's conditions, whose pieces overlap, or whose piece starts or ends away from a relief point breaks a hard rule. A class that the work rules do not define is refused.

The answer has these parts:

- `runs`: each run judged, as `cut` has them.
- `mix`: each mix rule over the set, as `cut` has it.
- `doubled`: each trip in more than one run, as `trip@start_secs`.
- `uncovered`: each trip that a block carries on the day and that no run holds, as `trip@start_secs`.
