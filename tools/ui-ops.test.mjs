// The UI OPS gate's fixtures (tools/ui-ops.mjs): a host that passes,
// and one that breaks each rule.

import { test } from "node:test";
import assert from "node:assert/strict";
import { checkUi } from "./ui-ops.mjs";

const base = { id: "fixture", ops: ["board.stop_board", "directory.stops"], kernelOps: ["about", "load_network"] };

test("a host whose asks ui.json lists passes", () => {
  const source = `
    if (await available("board")) await ask("board", "stop_board", { network_hash: n });
    await ask("directory", "about", { network_hash: n });
    await ask(
      "directory",
      "stops",
      { network_hash: n },
    );
    const x = task("other", "thing"); // not an ask
    engine.ask("rt", "live"); // a method, not the engine's ask
  `;
  assert.deepEqual(checkUi({ ...base, source }), []);
});

test("an op that ui.json does not list fails", () => {
  const source = `await ask("board", "grouped_board", {});`;
  const problems = checkUi({ ...base, source });
  assert.equal(problems.length, 1);
  assert.match(problems[0], /board\.grouped_board/);
});

test("a capability with no listed op fails", () => {
  const source = `if (await available("router")) {}\nawait together("rt", "board");`;
  const problems = checkUi({ ...base, source });
  assert.deepEqual(
    problems.map((p) => p.replace(/^.*capability (\w+).*$/, "$1")),
    ["router", "rt"],
  );
});

test("an op of a capability fetched alone passes when fetched_ops lists it", () => {
  const source = `if (await available("migrate")) await ask("migrate", "migrate", {});`;
  assert.equal(checkUi({ ...base, source }).length, 2, "neither list names it");
  assert.deepEqual(checkUi({ ...base, fetchedOps: ["migrate.migrate"], source }), []);
});

test("an ask that is not a string literal fails", () => {
  const source = `const op = "stop_board";\nawait ask("board", op, {});`;
  const problems = checkUi({ ...base, source });
  assert.equal(problems.length, 1);
  assert.match(problems[0], /line 2 .*not a string literal/);
});
