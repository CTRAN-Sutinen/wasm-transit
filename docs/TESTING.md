# Testing expectations

Every change lands with tests at the layer that owns the logic.
`just check` is the full gate; run it before declaring work done.

1. **Unit/property tests in the owning crate** (oracle style for
   laws — e.g. `transit-codec`'s `import_prop_test` holds that the
   inferred document is valid by construction over every spec-valid
   feed and deterministic, and its `codec_prop_test` holds the
   export round trip; `plans_prop_test` holds the service-plan
   chain laws with independent oracles; transit-dispatch's
   `dispatch_laws_test` holds the diversion/ripple laws).
2. **A dispatch test if a new op exists** (native, JSON round-trip).
3. **Extend `crates/transit-wasm/abi-smoke.mjs`** for ABI contract
   changes; regenerate `testdata/corpus.json` (`just wasm` then
   `just corpus`) when response shapes change — the corpus is the
   cross-host byte-identical replay (Rust `golden_test`, Go
   `golden_test.go`). Capability-module changes extend
   `crates/cap-abi/caps-smoke.mjs` the same way (`just test` runs
   it; `just test` and `just e2e` build `caps` first).
4. **A Go round-trip test** if server-visible behavior changes.
5. **Playwright e2e for user-visible flows** (Svelte components
   should not need changes when only the ABI underneath changes).
   The capability bus keeps its own spec (`web/e2e/caps.spec.mjs`:
   assembly, composition, degraded state, migrate/rollback) —
   extend it for bus-visible changes; capability law tests
   (migration laws, validator mutation law, the dispatch
   diversion laws) follow expectation 1.
6. **Dogfood on real data**: capability work touching feeds or
   payload size runs a `caps-run` session against the C-TRAN feed
   before it is done (see the dogfood-QA rules in
   `docs/CAPABILITIES.md`) — fixture-scale gates do not catch
   real-data defects.
