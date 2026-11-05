# Runtime cost gate

Every network consumer registers each document version itself
(kernel bulk op 256; the engine, `web/src/engine.ts`, registers a
document once per module per version). On C-TRAN a scratch Chromium bench measured
5.8–8.4 MB of wasm memory per consumer and ~300 ms per
registration at 4× CPU throttle — the direct parse plus the
canonical re-hash every module repeats for a document the import
already hashed (measured on the main thread; the modules now run in
the page's bus worker). Nothing gates these numbers, so the next
regression is invisible.

## Scope

- A ratchet spec in `web/e2e` over the fixture feed: linear memory
  per consumer after `load_network`, ms per registration, the JS
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
  verification — the module would trust the pin (Blocked: D-71);
  the served bytes ARE the canonical serialization (a server test
  hashes them to the pin), so a module could verify the pin over the
  bytes it decodes instead of re-serializing. Each module's linear
  memory is ~9–12 MB, the parsed network plus what it derives
  (QAF-93): a stop-scoped document would replace the display's
  whole-network copies, and documents without shapes the copies of
  every module that never draws.
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
- **Stop recomputing the day per call.** Each `stop_board`
  materializes the whole day (~5 ms on C-TRAN); a ride-now home poll
  asks for several boards. A derived registry entry per (network,
  edits, date) — bounded like the router's graphs — answers them
  from one. The router rebuilds its RAPTOR graph for every live
  document (every poll); predictions overlaid on a per-day graph
  would not.
- **String keys in the hot paths.** `memcmp` is the largest single
  bucket in a poll's worker profile (16–22 ms): id lookups through
  `BTreeMap<String, _>` (pattern trips, destinations). Interned or
  indexed ids in the materialized day and the router take it out.
- **The live document's round trip** (QAF-49): cap-rt answers it to
  the page, the page parses it, the engine re-encodes it for the
  board and the router, each parses and hashes it — three SHA-256
  passes and four JSON passes per poll. A document a module produced
  could move to the next module inside the worker as bytes.
- **Boot.** Modules are fetched, verified, and compiled one at a
  time, as each is first asked; the page's capabilities are known
  at load, so they can compile in parallel, streaming. Registration
  is serial on one worker (~0.3 s per module on C-TRAN): every
  module holds its own copy anyway, so a worker per module would
  register in parallel on a multi-core device (the Pi 4 has four)
  at the cost of a JS heap per worker. The map is ~100 ms of
  ride-now's boot on the main thread and could wait for the first
  answer.
- **One copy per page.** The structural lever: the display's three
  modules each hold the network. Capabilities linked into one
  module per assembly (wire-time assembly of bytes) would share one
  registry — a design question for the bus, not a tuning.

## Done when

- The ratchet spec runs in `just e2e` and fails on a raised
  ceiling.
- A C-TRAN measurement is recorded in the change that lands it.
