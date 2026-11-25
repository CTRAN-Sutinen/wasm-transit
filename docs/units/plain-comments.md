# Plain code comments

Rewrite the code comments to the writing rules in `docs/CONVENTIONS.md`,
"Writing" (D-252). One pass does all of it: delete what the code already
says or what has a home elsewhere, restructure the property-test
headers (`docs/TESTING.md`, "Laws first"), replace the words in "Use
these words", and fix the faults in the table.

A guard checks each file against git HEAD. The code without its
comments must not change, and every decision id and finding id in the
old comments must still be in the new ones. A field's doc comment is
its schema description, so each run ends with `just schema-lock`
(`docs/FORMATS.md`).

## Scope, in order

1. **Rust.** cap-abi's `src/` is done. The other crates go in batches.
2. **Go.**
3. **TS and Svelte.** The guard does not parse these files yet.
4. **The developer documents.** Apply "Use these words" and the fault
   table to `AGENTS.md`, `crates/AGENTS.md`, `docs/`, and the `OPS.md`
   files.

## Done when

- Every Rust, Go, TS, and Svelte file has had the pass, and the guard
  passed on each one.
- No developer document uses a word in the "Do not write" column.
