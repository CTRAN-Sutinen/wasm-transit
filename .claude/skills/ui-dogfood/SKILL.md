---
name: ui-dogfood
description: Walk a generated UI in a real browser on the C-TRAN feed, control by control, with `just qa` and tools/ui-shot.mjs. Use it before you call UI work done, to reproduce a UI bug, or when asked to QA or dogfood a page.
---

# UI dogfood

A dogfood session does what a person doing QA does in one user
workflow. The rules are in `docs/TESTING.md`, "Dogfood QA". Read them
first. A passing spec is not QA: walk every control, and look at the
screenshot after each one.

Before you start, name the user workflow that the session mirrors. If
the work has no workflow of its own, do not dogfood it. Say which tests
pin it instead.

## 1. Start the QA loop

`just qa` serves the pages with hot reload on :5174, over a sync server
on :8091 (logs: `/tmp/transit-qa.log`). The server reads the real C-TRAN
feed (`testdata/ctran-gtfs.zip`) when it is there, else the e2e
fixture, from an in-memory store, so publishing works. The C-TRAN feed
is licensed and not committed. If it is missing, say so: a walk on the
fixture is not a real-data dogfood. The ports are its own. Never take :8080,
because the user often runs `just serve` there.

Run it outside the sandbox, as a memory-capped user unit, so it can be
stopped cleanly and cannot crash the machine:

```
systemd-run --user --unit=qa-loop -p MemoryMax=8G -p MemorySwapMax=0 \
  --working-directory="$PWD" -E PATH="$PATH" -E HOME="$HOME" just qa
```

Wait until :5174 and :8091 both answer (use Monitor with an until-loop,
not a foreground sleep). The first start builds the wasm-fast modules.

While it runs:
- A page edit (Svelte, TS) shows on reload. It needs no build.
- A Rust edit needs `just caps-fast`, then a reload. The server stays up.

## 2. List the controls

For each stage of the UI, list every control and the workflow that each
one serves: bar pickers, tabs, buttons, links, and comboboxes. Do this
before you test, so that no control is skipped.

## 3. Walk the page

Drive the page with `tools/ui-shot.mjs`. Its header documents every
option. The common form:

```
node tools/ui-shot.mjs <ui> --steps <walk.mjs> --size 2560x1300
```

- `--size 2560x1300` and the default dark scheme match the user's
  screen.
- Open a selection by its hash (`--hash '#stage=route&route=025'`).
- A walk script's default export is
  `async (page, { shot, status, settle }) => …`. Put one-off walks in
  the scratchpad. Standing walks live in `qa/<name>/`.
- The browser profile in `.qa/profile` keeps the plans that a walk
  forked. Fork once, then reuse the fork. Use `--fresh` to start
  empty.

Walk on a forked plan. Then:
- Click every control on every stage. Type a real name into each
  combobox.
- After each step, read the screenshot, the status line, and the page
  errors that ui-shot prints. Check that the answer shows where the
  user lands.
- Then walk the paths between stages: a route chosen on one stage and
  opened on another, an issue and the thing it names, a card and its
  view.

## 4. Record what you found

- Fix defects in your own work, and pin each fixed control in the UI's
  e2e spec (`web/e2e/ui-<id>.spec.mjs`).
- Log each real defect or gap that is not yours in
  `docs/QA-FINDINGS.md`, with its pin. Two sources that disagree, shown
  as given, are not a defect.
- A one-off script that the session needed is a finding (a missing op
  or runner feature). Do not ship the script.
- Do not keep a session narrative. Tests and findings are the record.

## 5. Stop

Stop the unit, and check that no server is left:

```
systemctl --user stop qa-loop
pgrep -af "server -addr|go run \.|vite"
```

Stop every server that the task started, also leftovers on other ports.
