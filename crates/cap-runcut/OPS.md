# runcut

The runcut: what does this vehicle do today? The capability answers with the day's blocks list and with one block's authored chain. The blocks list shows which vehicles are out, with that day's running trip counts. The chain shows the layover ripple: a diverted trip that ends later eats into the vehicle's next layover. So does a trip that the live document predicts ends late. Blocks and layovers are model state that GTFS never had.

Blocking is where cutting the work begins. So the same question also covers two more views:

- what a vehicle's waits require under the scheduling constraints (the recovery view);
- which vehicles the trips that no block carries need (the suggested blocking, adopted through author `apply`).

The capability reads the materialized day over the network document, patched with the in-force edits. The domain code lives in the shared transit-dispatch and transit-runs libraries.

## blocks

List the day's blocks: every authored block with at least one trip running on the date, with that day's running trip counts.

A block whose every trip is off is a vehicle still in the garage. The list does not offer it. With edits in force, skipped trips leave the count. A block whose every trip is skipped leaves the list.

## fleet_summary

Summarize the day's fleet in one answer: the running blocks, the peak concurrent vehicles, the first time the peak is reached, and the platform, revenue, and layover hours. This one answer replaces one `block_trips` call for each block plus host-side concurrency glue.

A vehicle is out from its block's first running start to its last running arrival. The spans are half-open, so a bus that arrives as another departs is one vehicle, not two.

The hours and the peak are these:

- Platform hours are the fleet's out hours.
- Revenue hours are the trips' own runtimes.
- Layover hours are the rest.
- The peak is the dispatcher's fleet-need number.

`spans` holds each running block's outline, in block order. It is the structure behind the totals, and it shows where a block might be cut. Each outline holds these fields:

- the block's trips;
- the first start and the last end;
- `span_secs`;
- `longest_gap_secs`: the longest internal gap, which is the wait between one trip's end and the next trip's start. It is null for a one-trip block.
- `longest_gap_at_secs`: where that gap starts.

## block_trips

List one block's chain on a service date. The chain is the authored runcut, because the model's blocks carry layovers that GTFS never had. The op materializes the chain for the day and applies the in-force edits.

A diverted trip that ends later eats into the vehicle's next layover. With `live_hash`, a trip that the live document predicts ends late does the same. These fields give the lateness:

- `live_end_delay_secs`: the trip's delay at its last stop (the arrival, else the departure) against the published time.
- A stated delay wins over the edits' added time.
- `layover_remaining_secs`: the authored layover minus that lateness. It is negative when the connection is eaten. It answers the question: will this late bus make its next trip?

Each row's `layover_health` classifies the connection:

- `overlap`: the remaining layover is negative, and the next trip starts late.
- `tight`: the remaining layover is under `tight_layover_secs` (default 300).
- `ok`: the connection holds.

Trips of the chain that do not run on the date stay in view, marked. The answer carries the `block_id` that it is for. When you send many `block_trips` requests, group the results by the answer, never by request order. An unknown block answers empty, never an error.

## trip_stops

List one trip instance's stops on a service date. The instance is `(trip, start_secs)`. This is the day's trip detail, and it is effective: a diverted trip answers its patched sequence.

The trip's line is `trip_path`. A trip that does not run answers empty, never an error.

## trip_path

Draw one trip instance's line on a service date, as `[{lat, lon}]`. The instance is `(trip, start_secs)`. The line is over the effective day.

The line is the pattern's authored alignment. A trip that a diversion in force reroutes draws three parts:

- the alignment to the first kept stop;
- the diversion's traced detour;
- the alignment from the last kept stop.

A skip-only diversion keeps the alignment. When the pattern has no alignment, the line is its patched stops joined in order around the detour. A trip that does not run answers an empty path, never an error.

## recovery

List every wait between a block's adjacent running trips on a service date. This is the layover-to-runtime view.

Each wait holds these fields:

- The trip before the wait: the block index `after_trip`, `trip`, `route`, `pattern`, `start_secs`, and `run_secs`.
- `terminal`: where the vehicle waits. It is the layover's `at`, else the stop that the trip ends at.
- `gap_secs`: the time from the arrival to the next start.
- The recovery required there, and `required_by`, which says what requires it:
  - `policy`: the scheduling constraints' most specific recovery rule. The request carries `scheduling_constraints_hash`.
  - `terminal`: the smallest authored layover at that terminal, for a pair that the block authors a layover for.
  - null: nothing is required.
- `slack_secs`: `gap_secs` less the required recovery. It is negative when the wait is short. It is exactly validation's `layover_infeasible` under the same rules.
- `recovery_pct`: the gap as a whole percent of the run.

`patterns` folds the waits for each pattern of the trip before. Each entry holds the number of waits, the run and gap totals, `recovery_pct`, `short`, and `min_slack_secs`.

## suggest_blocks

Suggest blocks for the trips that run on a service date and that no block carries. Authored blocks are kept as authored.

The op chains the trips in order of start. Each trip follows the vehicle that leaves it the least slack, among the vehicles that wait where the trip starts. A vehicle waits there when it is at the same stop, or at a bay of the same station. There are no deadheads yet. The vehicle must also meet these rules:

- It ran the same route, or a route pair that the scheduling constraints interline.
- It has had the recovery required. That is the scheduling constraints' recovery, else the terminal's floor. When neither applies, nothing is required.

When no vehicle fits, the trip takes a new vehicle.

The answer has these fields:

- `blocks`: `[{block_id, trips}]`. The `block_id` is fresh (`auto-N`). Each trip is `{trip, route, start_secs, end_secs, from_stop, to_stop, wait}`. The `wait` is `{gap_secs, required_secs, required_by, slack_secs}`, or null.
- `unblocked`: the trips that the op cannot place (frequency trips).
- `authored_peak_vehicles`: the fleet summary's peak.
- `peak_vehicles` and `peak_at_secs`: the peak with the suggestion adopted.
- `added_vehicles`: what a proposal's unassigned trips add.
- `edits`: `add_block`, `add_block_trip`, and `set_layover` (each wait as its gap). Author `apply` adopts them. The next document's `fleet_summary` peak is the answer's peak.
