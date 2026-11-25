// Tests tools/journal.mjs over fake modules: a journal that replays, and
// one that breaks each rule.

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { gunzipSync } from "node:zlib";
import { join } from "node:path";
import { journalWriter, readJournal, replay, sha256, unpackJournal } from "./journal.mjs";

const HASH = "sha256:" + "a".repeat(64);
const kernelLoads = { load_network: 256 };

// A fake module for one capability. It holds the documents registered
// in it. `count` answers how many it holds, and refuses until it holds
// the network. `scale` lets a candidate answer differently.
function fake(cap, scale = 1) {
  const held = new Set();
  const answer = (request) => {
    if (request.op === "load_network") {
      held.add(HASH);
      return JSON.stringify({ ok: { op: "load_network", network_hash: HASH } });
    }
    if (request.op === "count") {
      if (!held.has(request.network_hash)) {
        return JSON.stringify({ error: { message: `unknown network document ${request.network_hash}: load_network first`, code: "unknown_document", kind: "network document", hash: request.network_hash, load: "load_network" } });
      }
      return JSON.stringify({ ok: { op: "count", n: request.n * scale } });
    }
    return JSON.stringify({ error: { message: "unknown op" } });
  };
  return {
    module: `sha256:${cap}-${scale}`,
    invokeRaw: (bytes) => answer(JSON.parse(bytes.toString("utf8"))),
    invokeBytes: (code, bytes) => (code === 256 ? answer({ op: "load_network" }) : JSON.stringify({ error: { message: "bulk" } })),
  };
}

// Record a journal as a host would: each call through the fake, and
// each outcome recorded. `linked` sends every capability's calls to one
// instance, as a page's assembly does.
function record(dir, calls, { linked = false } = {}) {
  const writer = journalWriter(dir, "run-1");
  const instances = new Map();
  for (const { cap, request } of calls) {
    const key = linked ? "page" : cap;
    if (!instances.has(key)) instances.set(key, fake(cap));
    const payload = Buffer.from(JSON.stringify(linked ? { cap, ...request } : request));
    const outcome = instances.get(key).invokeRaw(payload);
    writer.record({ instance: `run-1/${key}`, cap, module: "sha256:recorded", call: "json", op: request.op, payload, outcome });
  }
  writer.flush();
}

const calls = [
  { cap: "board", request: { op: "load_network", network: { kind: "network" } } },
  { cap: "board", request: { op: "count", network_hash: HASH, n: 2 } },
  { cap: "router", request: { op: "count", network_hash: HASH, n: 3 } },
];

function withDir(fn) {
  const dir = mkdtempSync(join(tmpdir(), "journal-"));
  try {
    fn(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("a linked page's journal replays on single capabilities", () =>
  withDir((dir) => {
    record(dir, calls, { linked: true });
    // The router never registered the network: the page's assembly
    // shared it. Replay registers it in the router's own instance.
    const lines = readJournal(dir);
    assert.equal(lines.length, 3);
    assert.ok(lines.every((l) => l.request.startsWith("sha256:") && l.output.startsWith("sha256:")));
    const report = replay({ dir, kernelLoads, place: (cap) => ({ linked: false, open: () => fake(cap) }) });
    assert.equal(report.same, 3);
    assert.deepEqual(report.differ, []);
    assert.deepEqual(report.cannot, []);
  }));

test("a refusal the host recovers from is not journaled", () =>
  withDir((dir) => {
    record(dir, [{ cap: "board", request: { op: "count", network_hash: HASH, n: 1 } }]);
    assert.equal(readJournal(dir).length, 0);
  }));

test("a candidate that answers differently differs", () =>
  withDir((dir) => {
    record(dir, calls, { linked: true });
    const report = replay({ dir, kernelLoads, place: (cap) => ({ linked: false, open: () => fake(cap, cap === "router" ? 10 : 1) }) });
    assert.deepEqual(
      report.differ.map((d) => [d.cap, d.op]),
      [["router", "count"]],
    );
    assert.deepEqual(report.modules.router.replayed, ["sha256:router-10"]);
  }));

test("a changed output hash differs", () =>
  withDir((dir) => {
    record(dir, calls, { linked: true });
    const file = join(dir, "journal.jsonl");
    const lines = readFileSync(file, "utf8").trim().split("\n").map(JSON.parse);
    lines[1].output = sha256(Buffer.from("something else"));
    writeFileSync(file, lines.map((l) => JSON.stringify(l)).join("\n") + "\n");
    const report = replay({ dir, kernelLoads, place: (cap) => ({ linked: false, open: () => fake(cap) }) });
    assert.deepEqual(report.differ.map((d) => d.index), [1]);
  }));

test("a missing request blob cannot replay", () =>
  withDir((dir) => {
    record(dir, calls, { linked: true });
    const line = readJournal(dir)[2];
    rmSync(join(dir, "blobs", `${line.request.slice(7)}.gz`));
    const report = replay({ dir, kernelLoads, place: (cap) => ({ linked: false, open: () => fake(cap) }) });
    assert.deepEqual(report.cannot.map((c) => c.index), [2]);
  }));

test("a document that never crossed cannot replay", () =>
  withDir((dir) => {
    // The page asked with the network registered, but the journal has
    // no load for it (the host received it some other way).
    const writer = journalWriter(dir, "run-1");
    const module = fake("board");
    module.invokeRaw(Buffer.from(JSON.stringify({ op: "load_network" })));
    const payload = Buffer.from(JSON.stringify({ op: "count", network_hash: HASH, n: 1 }));
    writer.record({ instance: "run-1/board", cap: "board", module: "m", call: "json", op: "count", payload, outcome: module.invokeRaw(payload) });
    writer.flush();
    const report = replay({ dir, kernelLoads, place: (cap) => ({ linked: false, open: () => fake(cap) }) });
    assert.equal(report.cannot.length, 1);
    assert.match(report.cannot[0].why, /never crossed/);
  }));

test("a linked page's journal replays on its own assembly, requests as sent", () =>
  withDir((dir) => {
    record(dir, calls, { linked: true });
    let opened = 0;
    const seen = [];
    const report = replay({
      dir,
      kernelLoads,
      place: () => ({
        linked: true,
        open: () => {
          opened += 1;
          const module = fake("page");
          return { ...module, invokeRaw: (bytes) => (seen.push(JSON.parse(bytes.toString("utf8")).cap), module.invokeRaw(bytes)) };
        },
      }),
    });
    assert.equal(report.same, 3);
    assert.equal(opened, 1);
    assert.deepEqual(seen, ["board", "board", "router"]);
  }));

test("a page's journal file unpacks to a journal that replays", () =>
  withDir((dir) => {
    record(dir, calls, { linked: true });
    const blobs = {};
    for (const line of readJournal(dir)) {
      blobs[line.request] = gunzipSync(readFileSync(join(dir, "blobs", `${line.request.slice(7)}.gz`))).toString("base64");
    }
    const bundle = { journal: readFileSync(join(dir, "journal.jsonl"), "utf8"), blobs };
    const other = mkdtempSync(join(tmpdir(), "journal-"));
    try {
      unpackJournal(other, bundle);
      const report = replay({ dir: other, kernelLoads, place: (cap) => ({ linked: false, open: () => fake(cap) }) });
      assert.equal(report.same, 3);
      bundle.blobs[Object.keys(blobs)[0]] = Buffer.from("tampered").toString("base64");
      assert.throws(() => unpackJournal(other, bundle), /does not hash/);
    } finally {
      rmSync(other, { recursive: true, force: true });
    }
  }));

test("what an instance holds is compared only on the recorded module", () =>
  withDir((dir) => {
    const writer = journalWriter(dir, "run-1");
    const payload = Buffer.from("{}");
    writer.record({ instance: "run-1/page", cap: "board", module: "sha256:page", call: 257, op: "evict", payload, outcome: '{"ok":{"held":["a","b"]}}' });
    writer.flush();
    const held = (module) => ({ module, invokeRaw: () => "", invokeBytes: () => '{"ok":{"held":["a"]}}' });
    const split = replay({ dir, kernelLoads, stateOps: { evict: 257 }, place: () => ({ linked: false, open: () => held("sha256:board") }) });
    assert.equal(split.unchecked, 1);
    assert.deepEqual(split.differ, []);
    const same = replay({ dir, kernelLoads, stateOps: { evict: 257 }, place: () => ({ linked: true, open: () => held("sha256:page") }) });
    assert.equal(same.differ.length, 1);
  }));
