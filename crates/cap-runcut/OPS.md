# runcut

The runcut: what does this vehicle do today — the day's blocks list (which vehicles are out, with that day's running trip counts) and one block's authored chain with the layover ripple (a diverted trip ending later, or a trip the live document predicts ends late, eats into the vehicle's next layover). Blocks and layovers are model state GTFS never had. Blocking is where cutting the work begins (D-46), so the same question covers what a vehicle's waits require under the WORK RULES (the recovery view) and which vehicles the trips no block carries need (the suggested blocking, adopted through author `apply`). The materialized day over the NETWORK document, patched with the in-force edits; the domain code lives in the shared transit-dispatch and transit-runs libraries.

## blocks

The day's blocks: every authored block with at least one trip running on the date, with that day's running trip counts — a block whose every trip is off is a vehicle still in the garage, not offered. With edits in force, skipped trips leave the count (and a block whose every trip is skipped leaves the list).

## fleet_summary

The day's fleet in ONE answer: running blocks, the peak concurrent vehicles and the first time the peak is reached, and the platform / revenue / layover hours — the fold that used to take one block_trips call per block plus host-side concurrency glue. A vehicle is out from its block's first running start to its last running arrival (half-open spans: a bus arriving as another departs is one vehicle, not two). Platform = the fleet's out hours, revenue = the trips' own runtimes, layover = the rest; the peak is the dispatcher's fleet-need number. spans: each running block's outline in block order — its trips, first start, last end, span_secs, and its longest internal gap (longest_gap_secs, the wait between one trip's end and the next's start, from longest_gap_at_secs; null for a one-trip block) — the structure behind the totals and where a block might be cut.

## block_trips

One block's chain on a service date: the AUTHORED runcut (the model's blocks carry layovers GTFS never had), materialized for the day, with the in-force edits applied — a diverted trip ending later eats into the vehicle's next layover, and with live_hash so does a trip the live document predicts ends late: live_end_delay_secs is its delay at its last stop (arrival, else departure) against the published time, and a stated delay wins over the edits' added time. layover_remaining_secs is the authored layover minus that lateness, negative when the connection is eaten — will this late bus make its next trip? Each row's layover_health classifies the connection: overlap (negative — the next trip starts late), tight (under tight_layover_secs, default 300), ok. Trips of the chain that do not run on the date stay in view, marked. The answer carries the block_id it is for (many block_trips results group by the answer, never by request order). Unknown blocks answer empty, never error.

## trip_stops

One trip instance's stops ((trip, start_secs)) on a service date — the day's trip detail, EFFECTIVE (a diverted trip answers its patched sequence). The trip's line is trip_path. A trip that does not run answers empty, never an error.

## trip_path

One trip instance's LINE ((trip, start_secs)) on a service date ([{lat, lon}]), over the effective day: its pattern's authored alignment; a trip a diversion in force reroutes draws the alignment to the first kept stop, the diversion's traced detour, and the alignment from the last kept stop (a skip-only diversion keeps the alignment; with no alignment, its patched stops joined in order around the detour). A trip that does not run answers an empty path, never an error.

## recovery

Every wait between a block's adjacent running trips on a service date (layover-to-runtime): the trip before it (block index after_trip, trip, route, pattern, start_secs, run_secs), where the vehicle waits (terminal: the layover's `at`, else the stop the trip ends at), gap_secs (arrival to the next start), the recovery required there and by what (required_by: policy — the work rules' most specific rule, with work_rules_hash; else terminal — the smallest authored layover at that terminal, for a pair the block authors a layover for; null when nothing is required), slack_secs (gap less required, negative when short — exactly validation's layover_infeasible under the same rules), and recovery_pct (gap as a whole percent of the run). patterns folds the waits per pattern of the trip before: waits, run and gap totals, recovery_pct, short, min_slack_secs.

## suggest_blocks

The trips running on a service date that no block carries, chained into SUGGESTED blocks (D-46) — authored blocks are kept as authored. By start, each trip follows the vehicle that leaves it the least slack among those waiting where it starts (the same stop, or a bay of the same station — there are no deadheads yet), on the same route or a route pair the work rules interline, after the recovery required (the rules', else the terminal's floor; nothing where neither applies); else it takes a new vehicle. blocks: [{block_id (fresh: auto-N), trips: [{trip, route, start_secs, end_secs, from_stop, to_stop, wait: {gap_secs, required_secs, required_by, slack_secs} | null}]}]; unblocked: the trips it cannot place (frequency trips); authored_peak_vehicles (the fleet summary's peak), peak_vehicles and peak_at_secs with the suggestion adopted, added_vehicles (what a proposal's unassigned trips add); edits: add_block, add_block_trip, set_layover (each wait as its gap) — author `apply` adopts them, and the next document's fleet_summary peak is the answer's.
