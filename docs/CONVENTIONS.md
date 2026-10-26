# Conventions and gotchas (Rust, wasm, ops)

Frontend-specific conventions live in `docs/WEB.md`. Git is the
user's — see the root AGENTS.md.

- **serde_json must keep the `float_roundtrip` feature** (root
  Cargo.toml workspace dep): the default parser is 1 ULP off on
  some f64s (`-122.7 + 0.005` reproducibly), which broke document
  round-trip equality. Found via a transit-network property test.
- **Add new operations at the transit-ops seam**: extend
  `Request`/`Response` (serde-tagged; JSON form is `{"op": ...}`),
  implement in `dispatch`, expose in each host (wasm.ts method, Go
  helper). Response payloads must be typed serde structs.
- **Model vocabulary is planner vocabulary, not GTFS's**: a
  `ServiceType` IS the service (the GTFS service_id — the day-linked
  design unit); a `PatternSchedule` is one pattern's trips under a
  service type; block trips reference schedules by
  `(schedule, start_secs)`; a `Network` document is a service plan.
  Do not rename these back toward GTFS's calendar/service words.
- **Regenerate the golden corpus through the harness**: `just
  corpus` depends on `just wasm`, so the replay always runs against
  a freshly built artifact — never `node testdata/generate-corpus.mjs`
  by hand against a stale `web/pkg` (a no-op diff means the golden
  test keeps failing on the real change).
- **Diagnostics are measurements; regressions are assertions.**
  Scratch repro specs live in /tmp with their own Playwright config
  (never in web/e2e) and get deleted after use; every bug they
  catch must land as a permanent suite assertion before the work is
  done. If it is worth writing a test, it is worth keeping.
- **Native tests first**: logic tests belong in the crate that owns
  the logic (`transit-ops/tests/dispatch_test.rs` runs the real seam
  without wasm). Property tests use *independent oracles* — see
  `transit-core/tests/prop_test.rs`; proptest regression seeds are
  kept in `*.proptest-regressions`.
- **wazero `memory.Read` returns a view** — copy bytes out before
  freeing the buffer, or the Rust allocator corrupts them (this bit
  us once).
- **The artifact is NOT reentrant** — one module instance, one
  invoke at a time. The Go host serializes the whole alloc→free
  sequence in `WasmHost.call` (net/http is per-goroutine); the
  artifact's internal Mutex only guards its registry map, not its
  heap. `go test -race` is part of `just test`, and
  `server/race_test.go` is what gives it something to watch —
  verify the test still fails if you touch that lock.
- **just 1.59 does not parse `key=value` named args** — recipes use
  positional args.
- GTFS-rt proto is **proto2**: required fields are non-Option in
  prost; nested types live in modules (`gtfs_rt::trip_update::*`).
- A date can have **multiple active service keys**; every
  departure/block row carries its service_id. Don't assume one.
- `#[profile.release]` in the root Cargo.toml is tuned for artifact
  size (opt-level "s", LTO, panic=abort, strip). Measure before
  changing it.
