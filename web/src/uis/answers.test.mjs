// The answer graph's laws (src/uis/answers.ts): each node's state, and
// the frame that shows it. A failure belongs to its frame, and every
// other node answers.

import { test } from "node:test";
import assert from "node:assert/strict";
import { answers } from "./answers.ts";

/// A graph over plain nodes. `missing` lists the capabilities that did
/// not assemble.
function graph(nodes, { missing = [] } = {}) {
  const frames = {};
  const errors = [];
  const request = answers(nodes, {
    frames,
    onerror: (e) => errors.push(e.message),
    available: async (cap) => !missing.includes(cap),
  });
  return { frames, errors, request };
}

const ran = () => {
  const log = [];
  return { log, node: (id, more = {}) => ({ id, frame: id, needs: ["x"], run: async () => void log.push(id), ...more }) };
};

test("a node answers, and its frame has no state", async () => {
  const { log, node } = ran();
  const { frames, request } = graph([node("A")]);
  await request("x");
  assert.deepEqual(log, ["A"]);
  assert.deepEqual(frames, {});
});

test("a node whose capability did not assemble is unassembled, and is not run", async () => {
  const { log, node } = ran();
  let cleared = 0;
  const { frames, request } = graph([node("A", { caps: ["board", "rt"], clear: () => cleared++ }), node("B")], { missing: ["rt"] });
  await request("x");
  assert.deepEqual(log, ["B"]);
  assert.deepEqual(frames, { A: { kind: "unassembled", cap: "rt" } });
  assert.equal(cleared, 1);
});

test("a node missing a document it reads is starved, and is not run", async () => {
  const { log, node } = ran();
  let cleared = 0;
  let layer = null;
  const { frames, request } = graph([node("A", { requires: () => (layer ? null : "the layer"), clear: () => cleared++ })]);
  await request("x");
  assert.deepEqual(log, []);
  assert.deepEqual(frames, { A: { kind: "starved", missing: "the layer" } });
  assert.equal(cleared, 1);
  layer = {};
  await request("x");
  assert.deepEqual(log, ["A"]);
  assert.deepEqual(frames, {});
});

test("a node that throws fails its frame alone, and every other node answers", async () => {
  const { log, node } = ran();
  let cleared = 0;
  const { frames, errors, request } = graph([
    node("A", { run: async () => { throw new Error("refused"); }, clear: () => cleared++ }),
    node("B"),
    node("C", { needs: ["A"] }),
  ]);
  await request("x");
  assert.deepEqual(frames, { A: { kind: "failed", message: "refused" } });
  assert.equal(cleared, 1);
  // A dependent still answers: it reads what A left, which is cleared.
  assert.deepEqual(log, ["B", "C"]);
  assert.deepEqual(errors, []);
});

test("a node that keeps its answer on a failure is failed, and is not cleared", async () => {
  let cleared = 0;
  const { frames, request } = graph([
    { id: "A", frame: "A", needs: ["x"], keepOnFailure: true, clear: () => cleared++, run: async () => { throw new Error("late"); } },
  ]);
  await request("x");
  assert.deepEqual(frames, { A: { kind: "failed", message: "late" } });
  assert.equal(cleared, 0);
});

test("the next answer clears a failure", async () => {
  let fail = true;
  const { frames, request } = graph([{ id: "A", frame: "A", needs: ["x"], run: async () => { if (fail) throw new Error("no"); } }]);
  await request("x");
  assert.equal(frames.A.kind, "failed");
  fail = false;
  await request("x");
  assert.deepEqual(frames, {});
});

test("a frame with several nodes shows the first that did not answer", async () => {
  const { frames, request } = graph(
    [
      { id: "A1", frame: "Map", needs: ["x"], run: async () => { throw new Error("one"); } },
      { id: "A2", frame: "Map", needs: ["x"], run: async () => {} },
      { id: "A3", frame: "Map", needs: ["x"], caps: ["router"], run: async () => {} },
    ],
    { missing: ["router"] },
  );
  await request("x");
  assert.deepEqual(frames, { Map: { kind: "failed", message: "one" } });
});

test("retry runs again only the nodes that failed", async () => {
  const { log, node } = ran();
  let fail = true;
  const { frames, request } = graph([
    node("A", { run: async () => { log.push("A"); if (fail) throw new Error("no"); } }),
    node("B"),
  ]);
  await request("x");
  assert.deepEqual(log, ["A", "B"]);
  fail = false;
  await request.retry();
  assert.deepEqual(log, ["A", "B", "A"]);
  assert.deepEqual(frames, {});
});

test("a hidden frame has no state, and its node is cleared", async () => {
  let shown = true;
  let cleared = 0;
  const { frames, request } = graph([
    { id: "A", frame: "A", needs: ["x"], visible: () => shown, clear: () => cleared++, run: async () => { throw new Error("no"); } },
  ]);
  await request("x");
  assert.equal(frames.A.kind, "failed");
  shown = false;
  await request("x");
  assert.deepEqual(frames, {});
  assert.equal(cleared, 2);
});

test("a document read that fails goes to the page's handler, not to a frame", async () => {
  const { frames, errors, request } = graph([{ id: "document.a", frame: "", needs: ["x"], run: async () => { throw new Error("unreadable"); } }]);
  await request("x");
  assert.deepEqual(frames, {});
  assert.deepEqual(errors, ["unreadable"]);
});

test("nodes answer in declared order, and a node reads only nodes before it", async () => {
  const { log, node } = ran();
  const { request } = graph([node("A"), node("B", { needs: ["A"] }), node("C", { needs: ["B"] })]);
  await request("x");
  assert.deepEqual(log, ["A", "B", "C"]);
  assert.throws(() => graph([node("B", { needs: ["A"] }), node("A")]), /before it is declared/);
});

/// A graph whose host keeps input states (src/uis/inputs.ts).
function withInputs(nodes, inputs) {
  const frames = {};
  const request = answers(nodes, { frames, onerror: () => {}, available: async () => true, inputs });
  return { frames, request };
}

test("a node that requires an input whose reads fail, with nothing held, is unread, and is not run", async () => {
  const { log, node } = ran();
  let cleared = 0;
  const inputs = { edits: { kind: "failed", input: "edits", name: "the detours", sinceMs: 5, held: false } };
  const { frames, request } = withInputs([node("A", { inputs: ["edits"], clear: () => cleared++ }), node("B")], inputs);
  await request("x");
  assert.deepEqual(log, ["B"]);
  assert.deepEqual(frames, { A: { kind: "unread", input: "edits", name: "the detours", sinceMs: 5 } });
  assert.equal(cleared, 1);
});

test("a node runs over a held copy of a failed input, and over an input that keeps none", async () => {
  const { log, node } = ran();
  const inputs = {
    edits: { kind: "failed", input: "edits", name: "the detours", sinceMs: 5, held: true },
    stops: { kind: "none", input: "stops", name: "the stops", atMs: 5 },
  };
  const { frames, request } = withInputs([node("A", { inputs: ["edits"] }), node("B", { inputs: ["stops"] })], inputs);
  await request("x");
  assert.deepEqual(log, ["A", "B"]);
  assert.deepEqual(frames, {});
});

test("an unread frame answers once its input does", async () => {
  const { log, node } = ran();
  const inputs = { edits: { kind: "failed", input: "edits", name: "the detours", sinceMs: 5, held: false } };
  const { frames, request } = withInputs([node("A", { inputs: ["edits"] })], inputs);
  await request("x");
  inputs.edits = { kind: "answered", input: "edits", name: "the detours", atMs: 9, asOfMs: null, old: false };
  await request("x");
  assert.deepEqual(log, ["A"]);
  assert.deepEqual(frames, {});
});
