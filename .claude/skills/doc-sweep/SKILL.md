---
name: doc-sweep
description: Run the documentation sweep before reporting any change in this repo as done. Use it at the end of every change that touches code, ops, names, commands, conventions, or tests, and whenever you are about to say work is finished.
---

# The documentation sweep

Work is not done until its documentation is done. This file holds the
checklist. Do not copy it into your report or into another file.

## Steps

1. **Scope the change.** Run `git status --short` and `git diff --stat`.
   Write down, for yourself:
   - each file that the change touched;
   - each name that it added, renamed, or removed (crates, types, ops,
     fields, kinds, recipes, decision ids, unit files, doc headings);
   - each behavior that a reader of the docs would see differently.
2. **Go through the checklist below.** For each item, decide if the
   change touches it. Read the target before you decide. Edit the ones
   that it touches. Write in the present tense, with one home for each
   topic (AGENTS.md, "Keeping the documents correct").
3. **Run the mechanical checks** that apply:
   - Retired names. For each removed or renamed name, search the
     tree. The search must find nothing, except code that reads old
     documents on purpose:

     ```
     rg -n -w '<name>' -g '*.md' -g '*.rs' -g '*.ts' -g '*.svelte' -g '*.mjs' -g justfile -g '!.claude/**'
     ```
   - Moved doc sections. For each heading that moved or changed, find
     the pointers to it, and fix each one:

     ```
     rg -n '"<old heading>"' -g '*.md' -g '*.rs' -g '*.ts' -g justfile
     ```
   - A changed op or `OPS.md`: run `just caps-fast`. The generator
     refuses an `OPS.md` whose `##` sections are not exactly the ops,
     and it runs the worked examples again.
   - A changed field doc comment on a document kind: run
     `just schema-lock`. The doc comment is the schema's description.
   - A changed Rust type that crosses to TypeScript: run `just types`.
     Never edit `web/src/types/` by hand.
   - A new or changed decision: the entry in `docs/decisions/D-N.md`
     and its row in `docs/DECISIONS.md` agree.
4. **Run the gate.** Run `just test` (and `just e2e` for a change that
   the UI shows). A change to prose or comments only needs only the
   gates that read it (doctests, `just caps-fast`, the doc gates). Say
   which checks you ran and which you skipped.
5. **Report.** In your report to the user, list what you updated in
   the sweep. Name each place, not a count. Say what you checked and
   left unchanged only where a reader could expect a change.

Do not commit. Git is the user's.

## The checklist

1. **The docs.**
   - `AGENTS.md`: the rules, and the map when a top-level part is
     added, renamed, or removed.
   - `README.md`: what the project is and how to start it.
   - `crates/AGENTS.md`.
   - Every file under `docs/` whose topic the change touches.
   - `docs/guide/`, when the change alters something that it
     describes. This includes what an app does (a new UI gets a short
     entry in `docs/guide/apps.md`), a step in a walkthrough, a design
     choice or its cost, and a term in the glossary. The guide follows
     ISO 24495-1 (plain language). Use short sentences and everyday
     words, and put every other term in the glossary. Do not put a
     decision number, a finding number, or a code name in its prose
     unless the reader needs it to act.
2. **The open-work records.**
   - Edit a unit down to the work that is left, or delete it with its
     pin.
   - Add each decision that you had to encode as an entry in
     `docs/decisions/`, with its row in `docs/DECISIONS.md`. Mark it
     provisional, with its pins. Mark an entry that the user settled as
     Decided, with the answer.
   - When a decided entry is built, move its reasoning into the Why of
     its topic (`docs/architecture/<topic>.md`), or into the crate
     comment when it governs one crate. The entry keeps the record.
     Never delete a decision entry.
   - Delete fixed findings in `docs/QA-FINDINGS.md`. Reword a finding
     that the change answers in part. Update the reference hashes that
     a dogfood session measured again.
3. **The contract that an agent reads.**
   - The capability's purpose and op descriptions, in its
     `crates/cap-<id>/OPS.md`. The generator refuses a file whose `##`
     sections are not exactly the capability's ops.
   - The worked examples in each capability's `src/examples.rs` (its
     `examples` feature). The generator runs them through the
     capability's dispatch. Generate the manifest again.
   - The justfile: the comment above each recipe that the change
     touches, and the gate comments.
   - The TS bindings (`just types`).
4. **The comments.** A crate's `//!` comment is its only description
   outside the code. It says what the crate answers, the rules that
   other code depends on, and why, in one line each with the decision
   id. It does not restate a signature, a field list, or a walkthrough
   of a function. It names a law test and does not repeat it. Also
   check the module doc comments, the law-test headers, and the web
   glue's comments. A comment that describes a retired type, op,
   field, or address misleads the next reader as much as a stale doc
   does. Comments follow `docs/CONVENTIONS.md`, "Writing".
5. **The retired names.** Search the whole tree for each name that the
   change removed or renamed (step 3 has the command). Also search for
   phrases that describe the old design.
