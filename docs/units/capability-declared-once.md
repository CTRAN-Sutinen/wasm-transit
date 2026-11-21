# A capability, declared once

A capability is its crate: the `Request` enum, the dispatch, `OPS.md`,
and the worked examples. gen-manifest holds a second copy of each
capability. Measured on 2026-11-20:

- `MODULES` in `crates/cap-abi/src/bin/gen_manifest.rs` repeats each
  capability's ops, the documents it reads, and the kinds it consumes,
  speaks, and migrates. `capability!` already declares the documents,
  and caps-smoke checks that the two agree.
- `example_for`, about 2,000 lines with 168 arms, computes every op's
  worked example with library calls. `real_output_test.rs` then runs
  each example's request through dispatch to check the same answer.
- Of the last 223 commits, 96 changed gen_manifest.rs. Most of them
  changed `MODULES` and `example_for` together.
- `crates/cap-abi/Cargo.toml` lists each capability as a
  dev-dependency. The root `Cargo.toml` takes its members by class
  (`crates/cap-*`, `crates/transit-*`) and names none.

The goal: a new op changes only its capability's crate. A new
capability is a new crate directory, and nothing else names it.

Every capability depends on cap-abi. So gen-manifest, a binary inside
cap-abi, cannot link a capability, and `MODULES` is the copy that this
cycle forces. The generator moves to a crate that links every
capability.

## Scope

1. **The declaration.** `capability!` takes what `MODULES` holds: the
   kinds the capability consumes, speaks, and migrates, and its
   `OPS.md`. The op list has one source, the `Request` enum. The
   `OPS.md` sections and the examples are checked against it. Reading
   it adds no dependency (`fewer-dependencies.md`, the allowlist).
   caps-smoke's check of a module's `about` stays. It now compares one
   declaration with its build.
2. **The examples, in their crates.** Each capability gives its
   examples behind an `examples` feature. An example is the documents
   to load and the request. The generator runs the request through the
   capability's own dispatch and records the answer. So an example
   cannot differ from what dispatch answers, and the check in
   `real_output_test.rs` becomes part of the generator. The per-kind
   examples (`kinds_for`, author `apply`) move the same way. An example
   that is not "documents, then a request" gets a hook in its own
   crate. No module build turns the feature on.
3. **The fixtures.** The example contexts (`NetworkExampleContext`,
   `StreetsExampleContext`, `RtExampleContext`), the `fixture_*`
   functions, and the fixed clocks move out of gen_manifest.rs to a
   place that a capability's `examples` feature reaches: a fixtures
   crate of its own, in a new class (D-235). Only an `examples` feature
   or a dev-dependency may name it, no assembly's closure may hold it,
   and the arch gates hold both. cap-abi's optional domain
   dependencies for gen-manifest go with it. One proposal to the user
   covers this crate and kind-registration's item 1 (the kernel's
   domain dependencies), with what it costs the gates and the module
   sizes.
4. **The generator.** gen-assemblies writes the generator crate from
   the capability directories, as it writes each `module-<cap>`. The
   crate links cap-abi and every capability with `examples`. The
   manifest, registry, and kind-table code stays a library in cap-abi,
   and the generator's `main` only names the capabilities. The kernel
   ops' examples and the document kinds' examples stay with the kernel
   and the kind list (`kind-registration.md`). The arch gates learn
   the generator's class.
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

kind-registration also edits gen_manifest.rs, in the kernel ops and
the document kinds. The two run in this order, in one session, one
build at a time:

1. kind-registration's logs are all on the one server path. A new
   receive op's example goes in `example_for` until item 2 here moves
   them all. The root `Cargo.toml` already takes its members by class.
   Item 1 lands with item 4: until the generator links the
   capabilities, a declaration in `capability!` would be a third copy
   beside `MODULES`. Item 5 lands with item 4 too.
2. The direction is decided: the fixtures crate (D-235), and the
   kernel's loading trait (D-239).
3. Items 3, 2, and 4 of this unit, then kind-registration's item 1.
4. Item 6 here and kind-registration's item 4 are one gate mechanism
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
