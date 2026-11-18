# Perf harness

`just perf` shows what a change costs, so a feature can be weighed
against its cost. It also catches resource use that is out of
control. There is no fixed budget and no ratchet (D-187). A number
that goes up is a fact for the person who makes the change, not a
failure. Only runaway use fails.

`just perf` stands (docs/TESTING.md, "Performance baselines"). It
drives ride-now and the stop display on the real feed with recorded
live feeds, and reports time, CPU profiles, memory per module, and a
soak. What is left:

## Scope

- **Compare two runs.** Each run writes `report.json`, and nothing
  reads two of them. Add a comparison: a run against a named
  baseline (a saved run in `web/perf-results/`), step by step. It
  shows the change in wall time, main-thread time, worker time, each
  module's linear memory, the JS heaps, and the bytes transferred.
  The output is a table that a person reads beside the change. It
  never fails on a delta.
- **Fail on runaway use.** In the soak, the run fails when:
  - a module's linear memory or the JS heap keeps growing across the
    later samples, and does not level off;
  - a module resets under its memory ceiling;
  - a module's held documents keep growing.

  These are wrong for every deployment, so they need no number from
  the operator. Write the laws first (docs/TESTING.md, "Laws
  first"): what "levels off" means over the soak's samples. A build
  with a planted leak must fail. The fixture pin of the same shape
  stands: `web/e2e/ui-stop-display.spec.mjs`, "a display left
  polling stays bounded".
- **Bytes transferred.** The probe does not count what a cold boot
  fetches, and what a cached boot fetches again. Add both to each
  step.
- **Every generated UI.** Only ride-now and the stop display have a
  perf spec. Add specs for service-design, service-change, and
  stops, each driving the UI's main task. Each spec has a soak where
  the UI polls.

## Done when

- `just perf` compares a run to a named baseline, and the reports
  include bytes transferred.
- `just perf` fails on runaway use in a soak, and a planted leak
  shows the failure.
- Every generated UI has a perf spec, and a C-TRAN run of each one is
  recorded in the change that lands it.
- docs/TESTING.md, "Performance baselines", describes the comparison
  and the runaway check.
