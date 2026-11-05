// The assembly-independence gate, broken on a deliberate fixture: a
// capability whose answer depends on what a neighbour computed first
// (a shared entry used as a dependency, not a cache) fails the gate;
// one that only caches passes. `node --test` (just test).

import { test } from "node:test";
import assert from "node:assert/strict";
import { independence } from "./independence.mjs";

/// A fake manifest: two capabilities, one linked assembly.
const manifest = {
  kernel_ops: [{ name: "load_network", example: { request: { op: "load_network", network: {} } } }],
  capabilities: [
    { id: "a", documents: ["network"], op_docs: { warm: { example: { request: { op: "warm" } } } } },
    { id: "b", documents: ["network"], op_docs: { read: { example: { request: { op: "read" } } } } },
  ],
  assemblies: [{ ui: "page", capabilities: ["a", "b"] }],
};

/// Fake instances over one shared state per instance, like one kernel
/// registry: `a.warm` computes an entry; `b.read` answers from it.
function instantiate(dependent) {
  return () => {
    const state = { entry: null };
    const answer = (request) => {
      if (request.op === "load_network") return '{"ok":{"op":"load_network"}}';
      if (request.op === "warm") {
        state.entry = "warmed";
        return '{"ok":"warm"}';
      }
      if (request.op === "read") {
        // A cache: computes the same value when absent. A dependency:
        // answers differently when a neighbour has not run first.
        const value = state.entry ?? (dependent ? "cold" : "warmed");
        return JSON.stringify({ ok: value });
      }
      return '{"error":"unknown op"}';
    };
    return { invoke: answer, invokeBytes: () => '{"error":"no bulk op"}' };
  };
}

test("a shared entry used as a cache passes", () => {
  const { failures, compared } = independence({ manifest, instantiate: instantiate(false) });
  assert.deepEqual(failures, []);
  assert.equal(compared, 4, "each example, in each order");
});

test("a shared entry used as a dependency fails", () => {
  const { failures } = independence({ manifest, instantiate: instantiate(true) });
  assert.ok(failures.some((f) => f.includes("b.read answers differently linked")), failures.join("\n"));
});

test("a gate that compares nothing fails", () => {
  const empty = { ...manifest, capabilities: manifest.capabilities.map((c) => ({ ...c, op_docs: {} })) };
  const { failures } = independence({ manifest: empty, instantiate: instantiate(false) });
  assert.ok(failures.some((f) => f.includes("compared nothing")));
});
