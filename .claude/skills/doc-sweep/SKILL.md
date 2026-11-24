---
name: doc-sweep
description: Run the documentation sweep before reporting any change in this repo as done. Use it at the end of every change that touches code, ops, names, commands, conventions, or tests, and whenever you are about to say work is finished.
---

# The documentation sweep

Work is not done until its documentation is done. The checklist is in
`docs/CONVENTIONS.md`, "The documentation sweep". That section is the
home of the checklist. This skill tells you how to run it. Do not copy
the checklist into your report or into another file.

## Steps

1. **Scope the change.** Run `git status --short` and `git diff --stat`.
   Write down, for yourself:
   - each file that the change touched;
   - each name that it added, renamed, or removed (crates, types, ops,
     fields, kinds, recipes, decision ids, unit files, doc headings);
   - each behavior that a reader of the docs would see differently.
2. **Read the checklist.** Read `docs/CONVENTIONS.md`, "The
   documentation sweep", in full. Do not work from memory.
3. **Go through each item.** For each item, decide if the change
   touches it. Read the target before you decide. Edit the ones it
   touches. Write in the present tense, with one home for each topic
   (AGENTS.md, "Keeping the documents correct").
4. **Run the mechanical checks** that apply:
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
   - A changed Rust type that crosses to TypeScript: run `just types`.
     Never edit `web/src/types/` by hand.
   - A new or changed decision: the entry in `docs/decisions/D-N.md`
     and its row in `docs/DECISIONS.md` agree.
5. **Run the gate.** Run `just test` (and `just e2e` for a change the UI
   shows), unless the change is only prose that no gate reads. If you
   skip it, say why.
6. **Report.** In your report to the user, list what you updated in
   the sweep. Name each place, not a count. Say what you checked and
   left unchanged only where a reader could expect a change.

Do not commit. Git is the user's.
