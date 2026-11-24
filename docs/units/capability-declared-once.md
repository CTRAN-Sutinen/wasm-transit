# A capability, declared once

A capability is its crate: the `Request` enum, the dispatch, `OPS.md`,
its worked examples (`src/examples.rs`, behind its `examples` feature),
and its declaration (`capability!`: the kinds it reads, consumes,
speaks, and migrates). The generator (`crates/generator`) is a library
that takes the capabilities as data, and its binary, gen-manifest, is
generated (`assemblies/gen-manifest/`). Nothing hand-written names a
capability (`docs/architecture/bus.md`, "The declaration" and
"gen-manifest").

The goal: a new op changes only its capability's crate. A new
capability is a new crate directory, and nothing else names it. What is
left is the gate that holds this, and the last docs.

## Scope

6. **The line holds.** A gate in `just test` fails when the generator's
   code, cap-abi, or the root `Cargo.toml` names a capability.
7. **The docs.** The remaining places that tell a reader to list a
   capability or an op by hand: the justfile's comments, the header of
   `tools/caps-run.mjs`, `docs/architecture/storage.md`, and
   `docs/FORMATS.md`. The documentation sweep (`docs/CONVENTIONS.md`), `crates/AGENTS.md`,
   `docs/guide/first-change.md`, `docs/TESTING.md`,
   `docs/architecture/bus.md`, `registry.md`, and
   `release-artifacts.md` describe the built state.

Item 6 and kind-registration's item 4 are one gate mechanism (an
allowlist of names in the arch gates), built together. Then item 7 here
and kind-registration's item 3 are one docs sweep.

## Done when

- The generator's code names no capability, and the gate in item 6
  holds it.
- An op added to one capability, with its `OPS.md` section and its
  example, changes files in that crate only. A test capability
  added in a fixture is linked, documented, and given examples with no
  other file changed.
- `just test` passes.
