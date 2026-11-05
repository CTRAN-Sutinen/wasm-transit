# Runtime cost gate

A page registers each document version once, in its linked assembly
(kernel bulk op 256; the engine, `web/src/engine.ts`, registers a
document once per module instance per version); a single-capability
module (runner, server) registers its own. On C-TRAN a scratch Chromium bench measured
5.8–8.4 MB of wasm memory per consumer and ~300 ms per
registration at 4× CPU throttle — the direct parse plus the
canonical re-hash every module repeats for a document the import
already hashed (measured on the main thread; the modules now run in
the page's bus worker). Nothing gates these numbers, so the next
regression is invisible.

## Scope

- A ratchet spec in `web/e2e` over the fixture feed: linear memory
  per instance after registration (each page's assembly), ms per
  registration, the JS
  heap after boot, and the bytes a cold boot transfers, against
  recorded ceilings that may only come down. The ceilings ratchet;
  the absolute budgets the rider apps must meet are D-68's
  ([deploy-rider-apps.md](deploy-rider-apps.md)).
- The same measurement on C-TRAN as a dogfood record (not a gate —
  the feed is not committed).
- Candidates once it is measured, not before. Measured on C-TRAN
  (`just perf`, ride-now "plan a trip leaving now", the network
  registered from its gzip stream): the router's first registration
  is ~450 ms of the step's 729 ms on the bus worker — the streamed
  parse (serde_json 240 ms, most of it the per-byte reader; inflate
  49 ms) and the canonical hash (sha2 79 ms over the
  re-serialization, `canonical_hash` ~65 ms). Skipping the
  re-serialization when the host passes the pinned hash is not a
  verification — the module would trust the pin (D-71: refused, full
  verification stays);
  the served bytes ARE the canonical serialization (a server test
  hashes them to the pin), so a module could verify the pin over the
  bytes it decodes instead of re-serializing. A page's assembly is
  16–18 MB of linear memory on C-TRAN, the one parsed network plus
  what its capabilities derive: a stop-scoped document would replace
  the display's whole-network copy, and documents without shapes
  would shrink every page that never draws.
  cap-author keeps a buffered decoder beside the direct one
  (+77 KiB) because `apply` edits carry network sub-types.
  Splitting the network document: the whole GTFS spec lands in it
  (C-TRAN's is 9.6 MB, shapes and their distances included), so
  alignments, fares, demand-responsive service, and station
  internals are candidates for documents of their own. A split
  alignment document keys lines by alignment id and keeps them
  exact: C-TRAN's shapes carry 14–15-digit coordinates, and nothing
  rounds at import.

## Measured on C-TRAN (2026-11-04)

The streamed registration with opt-level 3 (`PERF_RT=testdata/perf-rt-10m
just perf`): per module that holds the network, linear memory 17 →
8.6–11.9 MB; the stop display 51 → 31.8 MB of wasm and an 18 → 9.7 MB
page heap after 118 polls; ride-now 69 → 45.7 MB of wasm and a ~30 →
22 MB heap after its session. The page no longer parses or
re-encodes the network: the display's boot main thread 394 → 221 ms,
no long task (was two); ride-now's page code at boot 139 → 21 ms
(its main thread is MapLibre's, 520–730 ms run to run). The cost is
worker time: ride-now's "plan a trip leaving now" (the router's
first registration) 504 → 729 ms — the streamed parse (serde_json
113 → 240 ms) and inflate (49 ms).

## Measured on C-TRAN (2026-11-05): the linked assemblies

One module and one network copy per page (D-77), same recording
(`PERF_RT=testdata/perf-rt-10m just perf`; `web/perf-results/`
2026-11-05T23-30-45 and 23-31-29). These are the ratchet's starting
numbers:

| | wasm MB | page heap MB | total vs D-68 | gzip module bytes |
|---|---:|---:|---:|---:|
| stop display, boot | 16.2 (was 29.8 in three modules) | 3.2 | 19.4 | 1.33 MB (2.8 in five) |
| stop display, 118 polls | 16.2 (was 31.8) | 4.8 (was 9.7) | 21.0 of 40 (was 41.5) | |
| ride-now, boot | 16.2 (was 32.0 in five) | 14.2 | 30.4 | 1.52 MB (4.1 in seven) |
| ride-now, after its soak | 17.8 (was 45.7) | 25.7 | 43.5 of 80 (was 71.5) | |
| service-design | | | | 1.52 MB (3.3 in six) |

Every assembly's memory and held documents are flat across the
display's 118 polls and ride-now's soak; no reset fired. Time: the
display's boot main thread 221 → 164 ms and a poll's worker time
28 → 9 ms (p50 main thread per poll 10.5 → 8.5 ms); ride-now's
"plan a trip leaving now" worker time 729 → 355 ms (the network is
already registered; the router derives its graph from it), its
soak's p50 per poll 36 → 28 ms. Boot wall time is noise-bound
(display 1.8 → 1.2 s, ride-now 4.1 → 3.6 s).

## Candidates

Each lands with its before/after in `just perf` (or the ratchet
spec once it exists); the estimates are not results.

- **A faster streamed parse.** Registration decodes the gzip stream
  through serde_json's `IoRead`, a byte at a time through a
  `BufReader<Box<dyn Read>>` (one decoder for both forms). Measured
  in Node on board (opt-level 3): one shared reader path 187 ms and
  2.17 MB; a concrete gzip reader beside the slice decoder 164 ms and
  2.43 MB (a second `Network` decoder). Per-package opt levels
  measured worse on both (2.61 MB, 211 ms — they fight LTO).
  `+simd128` and a `wasm-opt -O3` pass are untried.
- **A smaller canonical document.** `pattern_trips` is 61% of the
  bytes: every stop time an `{"arrival_secs","departure_secs"}`
  object, the two equal on most stops. A compact encoding (a
  document version — `document-versioning.md`) cuts the bytes and
  the parse in every module; `alignments` (31%, shapes) leaves the
  copies of modules that never draw.
- **Stop rebuilding the router's graph per poll.** The router
  rebuilds its RAPTOR graph for every live document (every poll);
  predictions overlaid on a per-day graph would not. (The day every
  board reads is one held derived entry per (network, edits, date):
  `cap_abi::kernel::day`.)
- **String keys in the hot paths.** `memcmp` is the largest single
  bucket in a poll's worker profile (16–22 ms): id lookups through
  `BTreeMap<String, _>` (pattern trips, destinations). Interned or
  indexed ids in the materialized day and the router take it out.
- **Boot.** A page's one assembly is fetched, verified, and
  compiled when its first capability is asked; it is known at load,
  so it could compile streaming before then. The map is ~100 ms of ride-now's boot on the main
  thread and could wait for the first answer.
- The per-page numbers the ratchet starts from are the 2026-11-05
  record above.

## Done when

- The ratchet spec runs in `just e2e` and fails on a raised
  ceiling.
- A C-TRAN measurement is recorded in the change that lands it.
