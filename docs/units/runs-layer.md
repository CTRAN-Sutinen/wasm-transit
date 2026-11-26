# The runs layer

Cut blocks into operator runs under an agreement's work rules
(`docs/ROADMAP.md` 7). The layer lives beside the network (D-16): its
documents and library are its own (`transit-runs`), and it reads the
network by hash. The scheduling constraints, required recovery, and
suggested blocking are on the bus (`crates/transit-runs/src/lib.rs`
and cap-runcut). The runs cut is
what is left. It closes QAF-11, QAF-12, QAF-14, and QAF-15.

The model takes TODS's concepts (D-126). A run is an ordered list of
run events: its trips and its other activities (report, deadhead,
break, relief, clear). A run is keyed by service and run id, and a run
id is unique within its service. A deadhead is a non-revenue trip. A
garage or a relief point is an internal location. Nothing here reads
TODS files. That is the codec's job (step 8).

## Two documents

- **The labor rules** (`work_rules`) hold what an agreement binds:
  limits, breaks, pay, guarantees, run classes, and the mix of
  classes. A new agreement, or a new reading of a clause, is a new
  document.
- **The scheduling constraints** (`scheduling_constraints`) hold the
  agency's own choices and places: recovery, interline, relief points,
  the places that count as home for travel time, and the numbering of
  run classes.

Recovery and interline judge blocks before any run exists. Author
`apply`, the recovery view, and `suggest_blocks` read only the
scheduling constraints. The runs cut reads the blocks and both
documents, and names both by hash.

## The labor rules model

The model holds the rules of any operator agreement, and any reading
of one. Agreements change at each negotiation, and a memorandum or a
grievance changes how a clause is read. The code changes only when an
agreement needs a kind of rule that the model cannot express. The set
of rule types is fixed, and a new type is a new schema version. There
is no expression language. Unknown fields are kept and ignored, as in
every kind. The cut's pay is a planning measure for comparing cuts, in
time and not in dollars. It is not payroll.

- **Every number and every measure is data.** An agreement says
  "drive", "work", "platform", or "spread", and agreements mean
  different things by each. A document names its own measures as sets
  of activities, and each rule names the measure that it limits or
  pays. The activities follow TODS's run events: `report`, `clear`,
  `revenue`, `deadhead`, `layover`, `travel`, `standby`, and `gap`.
  Spread runs from the first report to the last clear.
- **Rules are lists.** A document may hold no rule of a type, or many.
  Every limit applies, and every break rule must be met.
- **A run class overrides any rule.** A class is a named bundle that
  replaces or adds rules for the runs in it.
- **Each rule may cite its clause** (`cites`). The document carries its
  period (`source`). A cut for a date outside the period warns. It
  does not refuse.

The rule types:

- **Allowances** add paid time at an event: `pull_out`, `pull_in`,
  `relief_on`, `relief_off`, `piece_start`, or `piece_end`.
- **Gap bands** sort a gap between pieces by its length. Each band pays
  a share of the gap, from 0 to 100 percent.
- **Limits** bound a quantity: the total of a measure, the spread, a
  piece's length, the number of pieces, or a gap's length. A limit is
  a minimum or a maximum, over the run, a rolling 24 hours, or a week.
  It is `hard` or `waivable`. The cut judges only limits over the run.
  The others wait for a roster.
- **Break rules** require time off. Each gives a length, a window, a
  count, whether it is paid, and the work that triggers it. A state
  law's rest and meal breaks are break rules.
- **Premiums** pay minutes at a rate, triggered by a measure past a
  threshold, the spread past a threshold, a time of day, a day of the
  week, or a class. `combine` says where premiums overlap: `highest`
  or `sum`.
- **Guarantees** set a minimum of paid time per run, per report, per
  day, or per week.
- **Travel rules** pay the trip back to a home place: at the day's end
  or between pieces, beyond a distance, ending within a distance of
  home, and timed by the next bus trip, a fixed time, or a distance at
  a speed. With no home places in the scheduling constraints, home is
  where the day began.
- **Run classes and the mix.** A class has an id, the rules that it
  replaces or adds, and the conditions that a run in it meets. The mix
  bounds the share of runs, or of paid hours, in a set of classes,
  with a minimum, a target, and a maximum. The cut reports a missed
  mix. It does not refuse it.
- **The objective** is in the document too: the classes whose runs to
  maximize, and the premium time to keep low.

C-TRAN's agreement with ATU Local 757 for 2022–2025
(`Operator_Labor_Agreement_2022-2025_FINAL.pdf`, at the repo's root)
is a fixture. It shows which rule types the model needs. No code
names one of its values. Each clause that two people could read two
ways becomes two fixtures, and both must load, cut, and pay:

- "drive 12 hours" as a limit on driving, and as a limit on platform
  time (Art. 19 §3);
- no break rule, and Washington's rest and meal breaks added;
- travel time at the day's end only, and between pieces too (Art. 14
  §4).

## Scope, in order

1. **The labor rules model.** The rule types above replace
   `work_rules`' `runs` and `pay`, with `checked` refusing a rule that
   names a measure or a class the document lacks. Property tests over
   generated rules that sample every type: none, many, and overlapping.
   The C-TRAN fixture and its paired readings, in `crates/fixtures`.
2. **The runs cut.** A library function from a day's blocks, the labor
   rules, and the scheduling constraints to runs. Each run is one or
   more pieces of blocks, relieved at relief points, else at a block's
   trip ends. Each run carries its timeline, its pay by the minute, its
   class, and every limit it breaks and every break rule it misses. The
   cut reports the mix, the uncovered work, and the work left over.
   This closes QAF-11 and QAF-15.
3. **The run view.** Report and clear, platform, pay, pieces, and
   deadhead per run (QAF-14). Deadhead needs a garage location as data
   and cap-streets for the distance (QAF-17, `docs/ROADMAP.md` 2 and
   6). Until then, a run's deadhead is reported as unknown, never as
   zero.
4. **Run legality.** Judge a run that a planner cut by hand against
   the rules (QAF-12).
5. **Run edits.** Cut, join, or move a piece. These are edits to the
   runs document, which follows the network's edit discipline.
6. **Suggested blocking, extended.** Today a suggestion keeps
   authored blocks as they are and chains only the trips no block
   carries, into new blocks, with no deadheads (D-109). Still open:
   appending a trip to an authored block's idle end, a full
   re-blocking, chaining frequency trips, and deadheads once a base
   exists (QAF-17).
7. **A page.** The engine loads neither document. No UI holds them,
   shows a wait's recovery (runcut `recovery`), or reads author
   `apply`'s scheduling constraints. This lands with
   `docs/units/ui-runcut.md`.
8. **The GTFS + TODS codec** (D-126): export the network with its
   runs layer as GTFS plus TODS's supplements and `run_events`, and
   import the same, losslessly, in transit-codec beside the GTFS
   codec. The export carries internal data and is served only under
   policy.

Where the runs ops go is a rule 3 question, judged when step 2 lands.
The runcut answers what a vehicle does today, and a run is a person's
work. The runs cut is most likely its own capability over the same
documents.

## Testing

C-TRAN has no TODS, so the tests use generated data. C-TRAN's GTFS has
real blocks (every trip carries a `block_id`), so a real weekday
serves as input for a session.

- Property tests in transit-runs over generated blocks and generated
  rules. Every piece of every block that runs is in exactly one run,
  or listed as uncovered. A run's pay is at least each of its
  guarantees. Under `highest`, no minute is paid above the highest rate
  that applies to it, and under `sum` the rates add. A run that breaks
  a hard limit names it. A measure's total equals the sum of its
  activities. The same input gives the same bytes. A document and its
  reordering are one identity.
- The paired fixtures. Where two readings differ, the runs differ in
  the way that the reading predicts.
- Hand-cut examples: a small day of blocks, cut and paid by hand under
  the fixture, in `crates/fixtures`.
- The codec round-trips generated runs.

## Done when

- The labor rules model, its property tests, and the paired fixtures
  (step 1).
- A runs cut on the bus, with its property tests, its op and runner
  gate, and a C-TRAN session reporting the run count, the class mix,
  and the broken rules for a weekday, with the rules named. QAF-11 and
  QAF-15 are deleted with that pin.
- The run view (QAF-14) and run legality (QAF-12) answered, with
  their findings deleted.
- A UI reads them (`docs/units/ui-runcut.md`).
- Generated runs export as GTFS + TODS and import back to the same
  documents (a codec round-trip property), and the export is refused
  without the policy's grant.
