---
name: ui-qa
description: QA a generated UI on the C-TRAN feed - walk it in a real browser, control by control, with `just qa` and tools/ui-shot.mjs, and check each answer against caps-run on the same server. Use it before you call UI work done, to reproduce a UI bug, or when asked to QA a page.
---

# UI QA

QA checks parity between the browser UI and caps-run, the agents' UI:
for the same question on the same data, both give the same answer. The
rules are in `docs/TESTING.md`, "Dogfooding and QA". Read them first. A
passing spec is not QA: walk every control, look at the screenshot after
each one, and check its answer against caps-run.

Before you start, name the user workflow that the walk serves. If the
work has no UI, it has no QA: dogfood it through caps-run instead, and
say which tests pin it.

## 1. Start the QA loop

`just qa` serves the pages with hot reload on :5174, over a sync server
on :8091 (logs: `/tmp/transit-qa.log`). The server reads the real C-TRAN
feed (`testdata/ctran-gtfs.zip`) when it is there, else the e2e
fixture, from an in-memory store, so publishing works. The C-TRAN feed
is licensed and not committed. If it is missing, say so: a walk on the
fixture is not QA on real data. The ports are its own. Never take :8080,
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

For each stage of the UI, list every control, the workflow that each one
serves, and the op that answers it (`web/src/uis/<id>/ui.json`, `ops`;
`node tools/caps-run.mjs tools <cap> <op>`). Do this before you test, so
that no control is skipped.

## 3. Walk the page

Drive the page with `tools/ui-shot.mjs`. Its header documents every
option. The common form:

```
node tools/ui-shot.mjs <ui> --steps <walk.mjs> --size 2560x1300
```

- `--size 2560x1300` and the default dark scheme match the user's
  screen.
- Open a selection by its hash (`--hash '#stage=route&route=025'`), and
  add `?journal` to the page's URL so the page records its op calls
  (`docs/architecture/web.md`, "The journal").
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

## 4. Check each answer against caps-run

Both UIs read the same server, so both read the same manifest and the
same documents.

- Save the page's journal after the step
  (`await page.evaluate(() => transitJournal())`, written to a file in
  the scratchpad). `node tools/caps-run.mjs replay FILE.json --show`
  lists each op the page asked, with its request. Ask that request
  again through caps-run, and compare the numbers on the screen with
  the line:

  ```
  node tools/caps-run.mjs board --upstream http://127.0.0.1:8091 \
    --as b --op stop_board network_hash='$manifest.network_hash' stop_id=8111 \
    date='$TODAY' from_secs:=… limit:=… --print '$b.board.rows.*.{route,departure_secs}'
  ```

  A screen that shows something other than the op's answer is a UI
  defect. A forked plan's network and the live feeds the page read are
  not on the server. Pass the page's journal (`--journal FILE.json`),
  and the runner registers each document the request names from the
  page's own loads. Name the network by the hash the request names,
  not `$manifest.network_hash`.
- At the end of the walk, save the journal again, and run
  `node tools/caps-run.mjs replay FILE.json --single`. Every call must
  answer the same bytes on caps-run's own modules. A call that differs
  is a parity defect.
- A question that the UI answers and no op answers is a finding: the
  page computes it (`docs/architecture/web.md`, "Parity"). A question
  that caps-run answers and the UI does not show is a finding only when
  the UI's workflow needs it.

## 5. Record what you found

- Fix defects in your own work, and pin each fixed control in the UI's
  e2e spec (`web/e2e/ui-<id>.spec.mjs`).
- Log each real defect or gap that is not yours in
  `docs/QA-FINDINGS.md`, with its pin. Two sources that disagree, shown
  as given, are not a defect.
- A one-off script that the walk needed is a finding (a missing op or
  runner feature). Do not ship the script.
- Do not keep a session narrative. Tests and findings are the record.

## 6. Stop

Stop the unit, and check that no server is left:

```
systemctl --user stop qa-loop
pgrep -af "server -addr|go run \.|vite"
```

Stop every server that the task started, also leftovers on other ports.
