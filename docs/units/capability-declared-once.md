# A capability, declared once

A capability is its crate: the `Request` enum, the dispatch, `OPS.md`,
and its worked examples (`src/examples.rs`, behind its `examples`
feature). The generator (`crates/generator`) runs each example through
the capability's own dispatch and records the answer. Copies are left:

- `MODULES` in `crates/generator/src/main.rs` repeats each
  capability's ops, the documents it reads, and the kinds it consumes,
  speaks, and migrates. `capability!` already declares the documents,
  and caps-smoke checks that the two agree.
- The generator names each capability by hand: a dependency in its
  `Cargo.toml`, and an arm in `run_examples`.
- `crates/cap-abi/Cargo.toml` lists each capability as a
  dev-dependency, for `real_output_test.rs` and `real_data_soak.rs`.

The goal: a new op changes only its capability's crate. A new
capability is a new crate directory, and nothing else names it.

## Scope

1. **The declaration.** `capability!` takes what `MODULES` holds: the
   kinds the capability consumes, speaks, and migrates, and its
   `OPS.md`. The op list has one source, the `Request` enum. The
   `OPS.md` sections and the examples are checked against it. Reading
   it adds no dependency (`fewer-dependencies.md`, the allowlist).
   caps-smoke's check of a module's `about` stays. It now compares one
   declaration with its build.
4. **The generator.** `crates/generator` becomes a library that
   takes the capabilities as data: each one's declaration, examples,
   and lanes. gen-assemblies writes the binary, as it writes each
   `module-<cap>`: a crate in `assemblies/` that links every capability
   with `examples` and calls the library. So the generator's code names
   no capability, and a fresh clone builds, because nothing hand-written
   depends on a generated crate. The manifest, registry, and kind-table
   code stays in the generator library, not in cap-abi: it reads every
   kind's schema, and the kernel depends on no domain library (D-239).
   The kernel ops' examples and the document kinds' examples stay with
   the kernel and the kind list (`kind-registration.md`).
5. **The lists.** The two cap-abi tests that run a capability
   (`real_output_test.rs`, every capability; `real_data_soak.rs`,
   cap-network and cap-plans) move to the generator crate (item 4),
   which links them all. Then cap-abi's dev-dependencies name no
   capability.
6. **The line holds.** A gate in `just test` fails when gen-manifest's
   code, cap-abi, or the root `Cargo.toml` names a capability.
7. **The docs.** Where a doc tells a reader to edit gen_manifest.rs or
   `MODULES`, it names the capability's crate: the AGENTS.md sweep
   (step 3), `crates/AGENTS.md` ("Where new things go"),
   `docs/guide/first-change.md`, `docs/TESTING.md`,
   `docs/architecture/bus.md` and `registry.md` (where the manifest
   comes from), `release-artifacts.md`, `storage.md`,
   `docs/FORMATS.md`, the justfile's comments, and the header of
   `tools/caps-run.mjs`.

## Order with kind-registration

kind-registration also edits the generator, in the kernel ops and the
document kinds. The two run in this order, in one session, one
build at a time:

1. Items 1, 4, and 5 here, together, then kind-registration's item 1
   (D-239).
2. Item 6 here and kind-registration's item 4 are one gate mechanism
   (an allowlist of names in the arch gates), built together. Then
   item 7 here and kind-registration's item 3 are one docs sweep.

## Done when

- The manifest and the registry that `just caps` writes are byte for
  byte the same before and after, except for an answer that dispatch
  gives and the old library call did not. Each such difference is
  named in the change.
- gen-manifest's code names no capability, and the gate in item 6
  holds it.
- An op added to one capability, with its `OPS.md` section and its
  example, changes files in that crate only. A test capability
  added in a fixture is linked, documented, and given examples with no
  other file changed.
- `just test` passes.
