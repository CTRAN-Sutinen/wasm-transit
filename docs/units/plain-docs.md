# Plain developer documents

Make the developer documents easy for a new developer to read. Apply
the writing rules in `docs/CONVENTIONS.md`, "Writing" (D-174). Give a
new developer a path from a fresh clone to a first change. Give each
concept one name.

## Scope, in order

1. **Test the first-change guide.** `docs/guide/first-change.md`
   exists. A developer new to the project makes one small change with
   it. Fix the guide where they get stuck.
2. **The rest of the rewrite.** `AGENTS.md`, `crates/AGENTS.md`, every
   file in `docs/architecture/`, `docs/CONVENTIONS.md`,
   `docs/TESTING.md`, `docs/FORMATS.md`, and `docs/ROADMAP.md` follow
   the writing rules. Unit files, `docs/DECISIONS.md` and `docs/decisions/`,
   `docs/QA-FINDINGS.md`, `qa/`, and the `OPS.md` files change when
   they are next edited. Keep every fact.
3. **The names.** These concepts have two names, or one name has two
   meanings:
   - kernel and core;
   - diversion and detour;
   - fork and draft;
   - registry: the published module set, and the kernel's per-kind
     holder;
   - module, assembly, and linked module;
   - bus.

   A proposal goes to the user first. Do not rename anything before the
   user answers.
4. **Code comments.** Comments change when their file is next edited.
   There is no sweep.

## The baseline

Measured on 2026-11-17, before the rewrite. Em dashes per 1,000 words,
and decision numbers in the file. The rewritten files measure 0 em
dashes.

| Document | Em dashes per 1,000 words | Decision numbers |
|---|---|---|
| `AGENTS.md` | 20.6 | 16 |
| `crates/AGENTS.md` | 18.1 | 5 |
| `docs/architecture/bus.md` | 20.6 | 15 |
| `docs/architecture/crates.md` | 14.6 | 101 |
| `docs/architecture/server.md` | 10.8 | 44 |
| `docs/CONVENTIONS.md` | 19.1 | 2 |
| `docs/TESTING.md` | 13.8 | 3 |
| `docs/guide/` (all files) | 0.0 | 1 |

## Done when

- The documents in scope item 2 that change when next edited follow
  the writing rules.
- The user has answered the names proposal, and the code and the
  documents use the chosen names.
- A developer new to the project has made one small change with only
  the first-change guide, and the guide is fixed where they got stuck.
