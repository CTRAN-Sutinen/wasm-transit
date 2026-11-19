# UI: changing service

service-change (`web/src/uis/service-change/`) is the one UI that
changes a plan and publishes it (D-180). The review of how a network
performs is its own UI ([ui-service-design.md](ui-service-design.md)).
This unit holds the open work for changing a plan.

## Flexibility, not a grid

A real timetable is never a rigid interval. A route has a target
headway, and its trips depart from it on purpose through the day:
for traffic, for the running time of the hour, for layover and
recovery, for reliefs, and for timed connections. The UI treats that
variation as information, never as an error.

- A headway is shown as its target and its observed range (shortest,
  typical, and longest gap), never as one number for the hour.
- Running time is what each trip carries. No all-day figure stands in
  for it, and the import derives no profile (D-3).
- A change states a target and a tolerance, over a range the planner
  enters for that change. Trips already inside the tolerance keep
  their own times. Only the trips the target needs are added or
  removed. Each new trip uses a planner-authored profile that covers
  its departure, else copies the nearest trip's times (QAF-90; D-3,
  D-54).
- Any trip can be nudged on its own. A later target moves a nudged
  trip only if the planner's range includes it.
- No view snaps, rounds, or regularizes an authored time.

## Scope

- **The frequency target, with its preview.** D-54 is decided and not
  built. The gesture panel shows it as unavailable today (QAF-90).
- **A held trip's layover ripple.** A block whose trip arrives after
  its next trip leaves offers to hold that trip. The hold moves the
  rest of the block, and the route's blocks show the ripple. The
  recovery issue a hold can leave names a position in the block, not
  its trips (QAF-101).
- **The pattern strip.** A line diagram of a route's branches, stops,
  and timepoints. A click picks a stop.
- **A pattern drawn along the streets.** The ops exist (author
  `draw_alignment`, streets `route`). The gesture is
  [ui-stops.md](ui-stops.md) item 5.
- **The proposal's hours against a budget.** Blocked: D-108.
- **The pick previewed as a rider will see it.** Before publishing,
  the rider pages open as of the pick's date, over the pick, not over
  what the deployment publishes. Publishing reaches every rider at
  midnight on that date, so this is the planner's last look
  ([observability-rollout.md](observability-rollout.md) watches the
  first day itself).
- **A link to the review.** The plan being changed opens in the
  review UI, so the planner can judge its coverage and corridors.
- **Freezing a version, and adopting one** (D-191). The planner freezes
  the plan with what it is (a proposal for public comment, a board
  packet), sees the frozen versions, and adopts one. The server has it
  (`POST /plans/freeze`, `/plans/adopt`, the `frozen_versions`
  document in `GET /logs/plans`). Today the UI only publishes, which
  freezes and adopts in one step.

## Done when

- Each item above is in the UI, or names its blocker there.
- `e2e/ui-service-change.spec.mjs` covers each one.
- Each is walked on C-TRAN on a forked plan.
