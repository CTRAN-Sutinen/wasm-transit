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
- **The server.** `just perf` measures the server too: its peak and
  settled memory, and its startup time, on the C-TRAN feed and the
  county roads (`server/dev.toml`). Measured 2026-11-18 by hand:

  | Run | Peak | Settled | Ready |
  | --- | --- | --- | --- |
  | as it runs today | 4.0–4.9 GB | 4.9 GB, then 2.8 GB after about 2 minutes | 21 s |
  | `GOMEMLIMIT=1GiB` | 3.4 GB | 0.72 GB | 21 s |
  | no street layer | 1.2 GB | not sampled after the first collection | under 10 s |

  A publish takes about 0.5 s and adds almost nothing. Go's live heap
  after startup is 589 MB. What is left to do:
  - **Settled memory.** Go returns freed memory to the system slowly,
    so the server holds about 4 times its live heap for minutes. The
    server sets a soft memory limit of its own (`debug.SetMemoryLimit`,
    from the container's limit when it has one), and the measurement
    shows the settled size fall.
  - **The street import's peak.** Importing the 99 MB county roads
    GeoJSON holds the whole parsed file in module memory at once, so
    the import peaks at 3.4–4.9 GB. The import reads the layer as a
    stream, and the measurement shows the peak fall.
  - **The live heap.** What holds the 589 MB (compiled modules, held
    documents, caches), named from a profile, and each part either
    justified in `docs/architecture/server.md` or cut.
  - **The guide.** `DEPLOYMENTS-SERVER.md` gives the container size
    from the latest measurement.

## Done when

- `just perf` compares a run to a named baseline, and the reports
  include bytes transferred.
- `just perf` fails on runaway use in a soak, and a planted leak
  shows the failure.
- Every generated UI has a perf spec, and a C-TRAN run of each one is
  recorded in the change that lands it.
- docs/TESTING.md, "Performance baselines", describes the comparison
  and the runaway check.
- `just perf` measures the server's peak and settled memory and its
  startup time, and the server's numbers after the work above are
  recorded in the change that lands it.
