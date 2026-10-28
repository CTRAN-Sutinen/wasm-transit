# Testing expectations

Every change lands with tests at the layer that owns the logic.
Two runners: `just test` is the host-side gate (no browser) — run
it before declaring any change done; `just e2e` adds the browser
runner, for changes that touch user-visible flows; `just check`
runs both, for changes that touch both sides.

1. **Unit/property tests in the owning crate** (oracle style for
   laws — e.g. `transit-codec`'s `import_prop_test` holds that the
   inferred document is valid by construction over every spec-valid
   feed and deterministic, and its `codec_prop_test` holds the
   export round trip; `transit-plans`'s chain tests hold the
   service-plan chain laws (resolution, windows, validity,
   order-insensitivity); transit-dispatch's `dispatch_laws_test`
   holds the diversion/ripple laws).
2. **A dispatch test if a new op exists** (native, JSON round-trip)
   — the module's `tests/dispatch_test.rs`, plus a flow in
   `crates/cap-abi/caps-smoke.mjs` and a chained runner gate in the
   justfile (`just test` runs both; `just test` and `just e2e`
   build `caps` first). The manifest is generated with REAL
   computed examples — extend `gen_manifest`'s `example_for` so an
   op's docs cannot drift from the module.
3. **A Go round-trip test** if server-visible behavior changes.
4. **Playwright e2e for user-visible flows** (Svelte components
   should not need changes when only the ABI underneath changes).
   The capability bus keeps its own spec (`web/e2e/caps.spec.mjs`:
   assembly, composition, degraded state, migrate/rollback) —
   extend it for bus-visible changes; capability law tests
   (migration laws, validator mutation law, the dispatch
   diversion laws) follow expectation 1.
5. **Web parity ratchets** (`just test`, "WEB GATES"): components
   never import the capability resolver, no host code reads raw
   rt `stop_updates`, and nothing clones a document to mutate it
   — each against an allowlist that only shrinks, all empty
   (`docs/ARCHITECTURE.md`, web section, "Parity"). Logic a view needs
   lands as an op with expectations 1–2, never in TypeScript; a
   unit that empties an allowlist entry removes it in the same
   change.
6. **A generated UI** (`src/uis/<id>/`) lands with
   `web/e2e/ui-<id>.spec.mjs` on the fixture feed, which walks its
   questions end to end, and with a dogfood session (below) in which
   its numbers on the real feed match the runner's. The GENERATED
   UIs gate in `just test` refuses one without its spec, and
   `e2e/picker.spec.mjs` covers its card with no edit.
7. **Dogfood on real data**: capability work touching feeds or
   payload size runs a `caps-run` session against the C-TRAN feed
   before it is done — fixture-scale gates do not catch real-data
   defects.

## Dogfood QA — the agent interface mirrors the human UI

`caps-run` exercises the same capabilities, bytes, and laws the app
runs; a human asking planner questions through it is doing real QA.
The rules this paradigm earned (the founding C-TRAN session found
four real defects fixture gates missed — a 64 KiB output truncation
among them):

- **All capability work is dogfooded on real data before it is
  done.** Anything touching feeds or payload size runs against
  `testdata/ctran-gtfs.zip` (the real C-TRAN GTFS, NOT COMMITTED —
  licensed data; fetch from c-tran.com) through `caps-run`. The
  reference hashes are in `docs/QA-FINDINGS.md`'s header.
- **A one-off script is never tooling — it is a finding.** If a QA
  session needs bespoke glue (a bootstrap script, a summarizer),
  that is a missing op or runner feature; log it in
  `docs/QA-FINDINGS.md`, don't ship the script.
- **The mirror cuts both ways.** A question that is one op via
  the CLI but absent from the UI is a UI gap; a question awkward
  via the CLI is an op gap. Dogfood sessions generate both
  signals; both land in `docs/QA-FINDINGS.md`.
- **Findings are open work, not a log.** A session adds new
  findings (and re-confirmation lines on old ones) to
  `docs/QA-FINDINGS.md`; the change that fixes a finding deletes it
  and names its pin. The session narrative itself is not kept —
  what it proved lands as tests, and what it found lands as
  findings.
