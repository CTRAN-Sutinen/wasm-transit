# runs

The runs: what does each operator do today? The capability cuts a service day's blocks into pieces under the work rules of a labor agreement and, when given, the agency's scheduling constraints, composes pieces into runs, and judges any set of runs. Who composes a run does not matter: the agency, an operator at the bid, or a planner by hand. Every op reads the materialized day over the network document, patched with the in-force edits, as the runcut does. The domain code lives in the shared transit-runs library.

Each request names the network, the date, and the work rules, and optionally the edits, the scheduling constraints, and the streets. Each answer's `warnings` say when the date is outside the work rules' `source` period.

A block leaves from the constraints' first garage and returns to it. The drive between the garage and the block's first or last stop is the shortest way over the streets, at the constraints' `deadhead_kph`. With no garage, no streets, no speed, or no way, the drive is unknown, and a run that needs it says so in `not_judged`. It is never counted as none.

## cut

Cut the day's blocks into runs: `pieces`, then `compose`, in one op.

The answer has these parts:

- `runs`: each run's `id`, its `class` (absent when no class fits), its `pieces` (each with its block, its trips, whether it starts at the pull-out or ends at the pull-in, and there its `garage` and its drive, `deadhead_out_secs` or `deadhead_in_secs`, when known), and `judged`:
  - `report` and `clear`: when the run starts and ends, and where: the garage, or the stop of a relief. The place is absent when the run pulls out or in and no garage is known.
  - `platform_secs`: the time on the vehicle, revenue, deadhead, and layover. `deadhead_secs`: the drives to and from the garage, absent when one is unknown.
  - `timeline`: the run's activities in order, from the first report to the last clear. Allowances come at each event, the drive from the garage before the first trip and back after the last, layover between two trips of a piece, a gap between two pieces, and travel where a travel rule pays it.
  - `spread_secs` and `measures`: each measure the rules define, totaled over the timeline.
  - `paid_secs`: the time paid at the base rate, each gap at its band's share. `premium_secs`: what the premiums add. `guarantee_secs`: what the guarantees add. `pay_secs`: the three together. Pay is a planning measure in seconds, not dollars.
  - `broken`: every limit the run breaks and every break rule it misses, each with its strength and the clause it cites. A run is legal when every broken rule is waivable.
  - `not_judged`: the rules that one run cannot answer, such as a limit over a week, travel with no trip found home, and a drive to or from the garage that is unknown.
- `unblocked`: each trip that runs on the day and that no block carries, as `trip@start_secs`. The cut does not place them.
- `mix`: each mix rule's share of runs or of paid time, against its minimum, target, and maximum. `met` says whether the share is within the minimum and the maximum.
- `mix_cost_secs`: the pay that `compose`'s moves toward the mix added over the least-pay composition. It is negative when a move also saved pay.

Each trip of each block that runs is in exactly one piece of one run.

## pieces

Cut the day's blocks into pieces.

A block that makes one good run stays whole. A good run is legal, in a class that the rules' objective maximizes, and pays no premium or guarantee. Any other block is cut at reliefs into the longest pieces that are legal alone. A relief is after a trip that ends at a relief point. When the scheduling constraints name none, the relief points are the stops where two blocks or more lay over: a block's trip ends there and its next trip starts there or at another stop of the station. Those are most often transit centers and shared terminals. With no such stop, a relief is after any trip.

The answer has `pieces`, earliest first, and `unblocked`, as `cut` has them. `inferred_relief_points` lists the relief points the cut guessed, each with its stop, the number of blocks that lay over there, and their routes. It is absent when the constraints name relief points. Each trip of each block that runs is in exactly one piece.

## compose

Compose pieces into runs, a suggestion. The request names its `pieces`, each by its `block`, its `first_trip`, and its `last_trip`. A trip that the block runs more than once on the day also names its start (`first_start_secs`, `last_start_secs`). A piece that names a block or a trip that does not run on the day is refused. A piece that pulls out or in takes its drive as `pieces` gives it.

A whole block that makes a good run alone stays a run. Each other piece, earliest first, takes the later piece that makes the best run with it, when that run is good or costs less than the two apart. A piece that pairs with none is a run alone. That is the least-pay composition.

When the least-pay composition misses a mix rule, composing moves it toward the mix, cheapest first. A move merges two one-piece runs into one run of a class, or gives a run another class, and the run it makes breaks no rule. Each step takes the move that adds the least pay for each percentage point of the mix it closes. The steps stop when the mix is met or no move closes any of it, and `mix` says which rules are still missed.

Composing the pieces that `pieces` answers gives the runs that `cut` answers. The answer is the set of runs as `judge` reports it, with `mix_cost_secs` as `cut` has it.

## judge

Judge a set of runs that anyone composed, as `cut` judges its own. The request names its `runs`, each with an `id`, its `pieces` named as `compose` names them, and optionally its `class`. It may name the scheduling constraints.

A run with no class takes the class that the cut would give it. A run that does not meet its class's conditions, whose pieces overlap, or whose piece starts or ends away from a relief point that the constraints name breaks a hard rule. A guessed relief point is never held against a run. A class that the work rules do not define is refused.

The answer has these parts:

- `runs`: each run judged, as `cut` has them.
- `mix`: each mix rule over the set, as `cut` has it.
- `doubled`: each trip in more than one run, as `trip@start_secs`.
- `uncovered`: each trip that a block carries on the day and that no run holds, as `trip@start_secs`.
