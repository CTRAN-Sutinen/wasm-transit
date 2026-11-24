# transit-runs

The runs layer, beside the network (D-16).

**The work-rule document** (`WorkRules`, kernel kind `work_rules`; D-50).
- It is generic. Every rule is optional. An agency's values are data.
- Recovery is per route and/or service type. It is minutes and/or a whole percent of the run before the wait, whichever is larger. The most specific rule answers.
- The route pairs that one vehicle may interline (D-46). A route always follows itself. An unlisted pair never interlines.
- The run rules: platform, spread, report and clear, pieces, a meal break, and relief points.
- Pay, in time. It has no dollars (D-51).
- `checked` validates and normalizes the document. The kernel's load runs it, so two orderings are one identity.

**The recovery view** (`recovery`).
- It gives every wait between a block's adjacent running trips on a day, the recovery required there, and the slack above it.
- It folds the waits per pattern into layover-to-runtime.
- The result is short exactly where validation says layover_infeasible.

**Suggested blocking** (`suggest_blocks`).
- The op takes the trips that no block carries, by start.
- Each trip follows the vehicle that leaves it the least slack where it starts. This is the same stop, or a bay of the same station. There are no deadheads yet.
- The pair must be one that the rules interline, and the trip follows after the recovery required.
- The op also gives the peak that the adoption needs against the authored peak.
- It gives the edits (`add_block`, `add_block_trip`, `set_layover`) that adopt the suggestion.

The runs cut itself is open work (`docs/units/runs-layer.md`). The crate is pure. cap-runcut and cap-author are the seams.

## Why

Each entry covers one choice that other code depends on. The heading
names the code it governs. **Holds** is what must stay true. Change it
only with the user, after you read **Because**.

### Required recovery: the work rules, else the terminal's floor (`recovery`, transit-runs)

- **Holds:** a policy wins. A policy is minutes and/or a share of run time, per route or service type, in the work-rule document's `recovery`. The smallest layover seen at that terminal is the fallback.
- **Because:** an imported layover is the whole observed gap, so without a rule every delay reads infeasible.
- **Decided:** D-9 (which waits it judges: D-110).

### Blocking is a suggestion under the work rules (runcut `suggest_blocks`)

- **Holds:** automatic blocking is the start of the runcut. It lives in the runcut capability and the work rules tune it (recovery, interlining per route pair). Its blocks are suggestions that the planner adopts or edits. The peak vehicle need falls out of them.
- **Because:** a vehicle need is a block assignment. A standalone estimate from concurrent trips would answer with a number that no planner could act on.
- **Rejected:** a peak estimate with no blocks; blocking by hand only.
- **Decided:** D-46 (what a suggestion changes: D-109; interlining with no rule: D-107).

### Work rules are one generic document, every rule optional (`work_rules`, kernel load 269)

- **Holds:** the common rules live in one policy document, each rule optional. The rules are platform, spread, report and pull, pieces, meals, relief points, pay, recovery, and interlining. An agency's values are data.
- **Because:** nothing may couple to one agency's labor agreement.
- **Rejected:** shaping the document to C-TRAN's rules.
- **Decided:** D-50.
