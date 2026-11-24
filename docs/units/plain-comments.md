# Plain code comments

Rewrite the code comments to the writing rules in `docs/CONVENTIONS.md`,
"Writing" (D-174), and make them shorter. A guard checks each file
against git HEAD: the code without its comments must not change, and
every decision number, finding number, backticked name, path, test
name, and number in the old comments must still be in the new ones.

## Scope, in order

1. **The style pass.** Remove dash asides, words in capitals for
   emphasis, history, and decision numbers inside sentences. Wrap
   comments at 80 columns. Rust is done. Go is next, then TS and
   Svelte. A field's doc comment is its schema description, so the
   pass ends with `just schema-lock` (docs/FORMATS.md).
2. **The law-test headers.** Bring each header to the shape in
   `docs/TESTING.md`, "Laws first": what it pins, where the intent
   lives, and one line for each law. Drop API notes, process notes,
   and text that restates a decision file or an `OPS.md` section. A
   blanket word cut was tried and dropped: on comments already in the
   plain style it saved about 2%.

## Done when

- Every Rust, Go, TS, and Svelte file has had both passes, and the
  guard passed on each one.
