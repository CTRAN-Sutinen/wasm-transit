// The broken fixtures of web/gates/parity.mjs and web/gates/uis.mjs: a
// tree that passes, then one file that breaks each rule.

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { parity, RULES } from "./parity.mjs";
import { uis } from "./uis.mjs";

const UI = JSON.stringify({
  id: "board",
  title: "t",
  summary: "s",
  questions: [],
  components: [],
});
const CLEAN = {
  "web/src/engine.ts": 'import { bus } from "./bus";',
  "web/src/bus.ts": "const w = new Worker(url);",
  "web/src/bus.worker.ts": "WebAssembly.instantiate(b); transit_invoke();",
  "web/src/types/Rt.ts": "stop_updates: X; WebAssembly.Memory;",
  "web/src/components/Board.svelte":
    "<script>let { rows } = $props();</script>",
  "web/src/uis/board/ui.json": UI,
  "web/src/uis/board/host.svelte.ts": 'import { ask } from "../../engine";',
  "web/src/uis/board/Shell.svelte":
    'import Board from "../../components/Board.svelte";',
  "web/board.html": "",
  "web/e2e/ui-board.spec.mjs": "",
};

function check(extra) {
  const root = mkdtempSync(join(tmpdir(), "web-gates-"));
  for (const [path, text] of Object.entries({ ...CLEAN, ...extra })) {
    if (text === null) continue;
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  try {
    return [...parity(root), ...uis(root)];
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("a clean tree passes", () => assert.deepEqual(check({}), []));

const BROKEN = {
  "bus-in-components": {
    "web/src/components/Map.svelte": 'import { ask } from "../engine";',
  },
  "rt-join-in-web": {
    "web/src/uis/board/host.svelte.ts": "const d = snap.stop_updates;",
  },
  "clone-and-mutate": {
    "web/src/plans.svelte.ts": "const n = structuredClone(net);",
  },
  "bus-outside-engine": {
    "web/src/documents.svelte.ts": 'import { bus } from "./bus";',
  },
  "worker-outside-bus": { "web/src/engine.ts": "new Worker(u);" },
  "abi-outside-worker": {
    "web/src/engine.ts": "await WebAssembly.compile(b);",
  },
  "status-test-in-host": {
    "web/src/uis/board/host.svelte.ts":
      'if (plan.status === "retired") hide();',
  },
  "reads-outside-inputs": {
    "web/src/uis/board/host.svelte.ts": 'await fetch("/manifest");',
  },
};

for (const rule of RULES) {
  test(`parity: ${rule.name} fails`, () => {
    const problems = check(BROKEN[rule.name]);
    assert.equal(problems.length, 1, problems.join("\n"));
    assert.ok(problems[0].startsWith(`${rule.name}: `));
  });
}

test("uis: a UI with no page, no spec, or a descriptor missing a field fails", () => {
  assert.match(
    check({ "web/board.html": null }).join(),
    /missing web\/board\.html/,
  );
  assert.match(
    check({ "web/e2e/ui-board.spec.mjs": null }).join(),
    /missing web\/e2e\/ui-board\.spec\.mjs/,
  );
  assert.match(
    check({
      "web/src/uis/board/ui.json": JSON.stringify({ id: "board" }),
    }).join(),
    /missing title/,
  );
  assert.match(
    check({
      "web/src/uis/board/ui.json": UI.replace('"board"', '"other"'),
    }).join(),
    /id other is not its directory's/,
  );
});

test("uis: a UI that imports another UI's files, or a component that imports a host, fails", () => {
  assert.match(
    check({
      "web/src/uis/board/Shell.svelte": 'import x from "../stops/host.svelte";',
    }).join(),
    /imports another generated UI's files/,
  );
  assert.match(
    check({
      "web/src/components/Board.svelte":
        'import h from "../uis/board/host.svelte";',
    }).join(),
    /imports a generated UI's host layer/,
  );
});
