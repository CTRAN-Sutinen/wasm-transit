// The inputs' laws (src/uis/inputs.ts): one poll reads every declared
// input, each read has a time limit, and a read has one of three
// outcomes. The input's state names it, and its graph inputs change
// only when what the page holds changes.

import { test } from "node:test";
import assert from "node:assert/strict";
import { inputs, labelOf, ReadFailed } from "./inputs.ts";

/// A host with a clock the test moves. `after` keeps the timers;
/// `advance` runs the ones that are due, in order.
function host({ random = () => 0, visible = true } = {}) {
  let now = 1_000_000;
  let timers = [];
  let seq = 0;
  const requests = [];
  const shown = [];
  const h = {
    states: {},
    requested: requests,
    shown,
    visible,
    now: () => now,
    random,
    after: (ms, fn) => {
      const t = { at: now + ms, fn, id: seq++ };
      timers.push(t);
      return () => (timers = timers.filter((x) => x !== t));
    },
    isVisible: () => h.visible,
    onVisible: (fn) => shown.push(fn),
    request: async (...feeds) => void requests.push(feeds.sort()),
  };
  h.advance = async (ms) => {
    const until = now + ms;
    for (;;) {
      timers.sort((a, b) => a.at - b.at || a.id - b.id);
      const next = timers[0];
      if (!next || next.at > until) break;
      timers.shift();
      now = next.at;
      await next.fn();
      await settle();
    }
    now = until;
  };
  h.show = async () => {
    h.visible = true;
    for (const fn of shown) await fn();
  };
  return h;
}

const settle = () => new Promise((r) => setTimeout(r, 0));

/// One declaration, as a UI's ui.json writes it.
const declared = (id, more = {}) => ({ id, name: `the ${id}`, from: `GET ${id}`, every_s: 30, limit_s: 10, keeps: "last", feeds: [id], ...more });

/// A reader whose answers the test sets: a value (keyed by itself), null
/// for "none kept", an Error to fail, or "hang" for a read that never
/// answers.
function reader() {
  const r = {
    next: "v1",
    reads: 0,
    adopted: [],
    signals: [],
    read: async ({ signal }) => {
      r.reads++;
      r.signals.push(signal);
      const next = typeof r.next === "function" ? r.next() : r.next;
      if (next === "hang") return new Promise(() => {});
      if (next instanceof Error) throw next;
      if (next === null) return null;
      if (typeof next === "object") return next;
      return { value: next, key: next };
    },
    adopt: (value) => void r.adopted.push(value),
  };
  return r;
}

test("the first read answers a document: the page adopts it, and its graph inputs change", async () => {
  const h = host();
  const r = reader();
  const run = inputs([declared("edits", { feeds: ["edits", "clock"] })], { edits: r }, h);
  await run.start();
  assert.deepEqual(r.adopted, ["v1"]);
  assert.deepEqual(h.requested, [["clock", "edits"]]);
  assert.equal(h.states.edits.kind, "answered");
});

test("a read that answers the same document changes nothing", async () => {
  const h = host();
  const r = reader();
  const run = inputs([declared("edits")], { edits: r }, h);
  await run.start();
  await h.advance(30_000);
  assert.equal(r.reads, 2);
  assert.deepEqual(r.adopted, ["v1"]);
  assert.equal(h.requested.length, 1);
});

test("a changed document is adopted at the next poll", async () => {
  const h = host();
  const r = reader();
  const run = inputs([declared("edits")], { edits: r }, h);
  await run.start();
  r.next = "v2";
  await h.advance(30_000);
  assert.deepEqual(r.adopted, ["v1", "v2"]);
  assert.equal(h.requested.length, 2);
});

test("none kept and failed are two states, never the same null", async () => {
  const h = host();
  const none = reader();
  none.next = null;
  const failed = reader();
  failed.next = new Error("HTTP 502");
  const run = inputs([declared("stops"), declared("edits")], { stops: none, edits: failed }, h);
  await run.start();
  assert.equal(h.states.stops.kind, "none");
  assert.equal(h.states.edits.kind, "failed");
  assert.equal(h.states.edits.held, false);
  assert.equal(h.states.edits.message, "HTTP 502");
  assert.equal(h.states.edits.sinceMs, h.now());
});

test("a read that never answers fails at its time limit, and the poll goes on", async () => {
  const h = host();
  const hang = reader();
  hang.next = "hang";
  const other = reader();
  const run = inputs([declared("edits", { limit_s: 5 }), declared("live")], { edits: hang, live: other }, h);
  const started = run.start();
  await h.advance(5_000);
  await started;
  assert.equal(h.states.edits.kind, "failed");
  assert.match(h.states.edits.message, /5 s/);
  assert.equal(hang.signals[0].aborted, true);
  assert.equal(h.states.live.kind, "answered");
});

test("keeps: last — a failed read keeps the last document, says since when, and keeps its age", async () => {
  const h = host();
  const r = reader();
  const run = inputs([declared("edits")], { edits: r }, h);
  await run.start();
  const answeredAt = h.now();
  r.next = new Error("offline");
  await h.advance(30_000);
  assert.deepEqual(r.adopted, ["v1"]);
  assert.equal(h.states.edits.kind, "failed");
  assert.equal(h.states.edits.held, true);
  assert.equal(h.states.edits.asOfMs, answeredAt);
  const since = h.states.edits.sinceMs;
  await h.advance(60_000);
  assert.equal(h.states.edits.sinceMs, since, "failed since the first failure");
});

test("keeps: none — a failed read drops the document", async () => {
  const h = host();
  const r = reader();
  const run = inputs([declared("live", { keeps: "none" })], { live: r }, h);
  await run.start();
  r.next = new Error("HTTP 503");
  await h.advance(30_000);
  assert.deepEqual(r.adopted, ["v1", null]);
  assert.equal(h.states.live.held, false);
  assert.equal(h.requested.length, 2);
});

test("a failure with nothing held changes the graph inputs once, so a node that requires it says so", async () => {
  const h = host();
  const r = reader();
  r.next = new Error("offline");
  const run = inputs([declared("edits")], { edits: r }, h);
  await run.start();
  await h.advance(120_000);
  assert.deepEqual(r.adopted, []);
  assert.equal(h.requested.length, 1);
});

test("an answer from the upstream's working copy carries the upstream's age, not the read's", async () => {
  const h = host();
  const r = reader();
  r.next = { value: "v1", key: "v1", asOfMs: 5_000 };
  const run = inputs([declared("edits")], { edits: r }, h);
  await run.start();
  assert.equal(h.states.edits.kind, "answered");
  assert.equal(h.states.edits.asOfMs, 5_000);
  r.next = new Error("offline");
  await h.advance(30_000);
  assert.equal(h.states.edits.asOfMs, 5_000, "a later failure keeps the upstream's age");
});

test("an input past its age is old: answered or held", async () => {
  const h = host();
  const r = reader();
  const start = h.now();
  r.next = () => ({ value: "v1", key: "v1", asOfMs: start });
  const run = inputs([declared("live", { old_after_s: 90 })], { live: r }, h);
  await run.start();
  assert.equal(h.states.live.old, false);
  await h.advance(90_000);
  assert.equal(h.states.live.old, false);
  await h.advance(30_000);
  assert.equal(h.states.live.kind, "answered");
  assert.equal(h.states.live.old, true, "a frozen feed answers, and is old");
});

test("one poll: a hidden page reads nothing, and reads what is due when it shows", async () => {
  const h = host();
  const r = reader();
  const run = inputs([declared("edits")], { edits: r }, h);
  await run.start();
  h.visible = false;
  await h.advance(300_000);
  assert.equal(r.reads, 1);
  await h.show();
  await settle();
  assert.equal(r.reads, 2);
});

test("each input is read at its own interval", async () => {
  const h = host();
  const fast = reader();
  const slow = reader();
  const run = inputs([declared("live", { every_s: 20 }), declared("edits", { every_s: 60 })], { live: fast, edits: slow }, h);
  await run.start();
  await h.advance(60_000);
  assert.equal(fast.reads, 4);
  assert.equal(slow.reads, 2);
});

test("a boot-only input is read once", async () => {
  const h = host();
  const r = reader();
  const run = inputs([declared("feeds", { every_s: null })], { feeds: r }, h);
  await run.start();
  await h.advance(600_000);
  assert.equal(r.reads, 1);
});

test("after a failure the next read waits its interval and a random part of it, so pages spread", async () => {
  const early = host({ random: () => 0 });
  const late = host({ random: () => 0.99 });
  const reads = [];
  for (const h of [early, late]) {
    const r = reader();
    r.next = new Error("offline");
    const run = inputs([declared("edits")], { edits: r }, h);
    await run.start();
    await h.advance(30_000);
    reads.push(r.reads);
  }
  assert.deepEqual(reads, [2, 1]);
});

test("Retry-After holds the next read until then", async () => {
  const h = host();
  const r = reader();
  r.next = new ReadFailed("HTTP 503", { retryAfterMs: 120_000 });
  const run = inputs([declared("edits")], { edits: r }, h);
  await run.start();
  await h.advance(119_000);
  assert.equal(r.reads, 1);
  await h.advance(1_000);
  assert.equal(r.reads, 2);
});

test("an input read after another sees what that one adopted in the same poll", async () => {
  const h = host();
  let network = null;
  const net = reader();
  net.adopt = (v) => void (network = v);
  const edits = reader();
  edits.next = () => `edits over ${network}`;
  const run = inputs([declared("network"), declared("edits", { after: ["network"] })], { network: net, edits }, h);
  await run.start();
  assert.deepEqual(edits.adopted, ["edits over v1"]);
  assert.deepEqual(h.requested, [["edits", "network"]]);
});

test("the reader sees what the page holds, to answer the same key cheaply", async () => {
  const h = host();
  const held = [];
  const r = reader();
  const read = r.read;
  r.read = (ctx) => (held.push(ctx.held), read(ctx));
  const run = inputs([declared("network")], { network: r }, h);
  await run.start();
  await h.advance(30_000);
  assert.deepEqual(held, [null, "v1"]);
});

test("read now: a write's new head is read at once, and the poll keeps its interval", async () => {
  const h = host();
  const r = reader();
  const run = inputs([declared("edits")], { edits: r }, h);
  await run.start();
  await h.advance(10_000);
  r.next = "v2";
  await run.read("edits");
  assert.deepEqual(r.adopted, ["v1", "v2"]);
  await h.advance(30_000);
  assert.equal(r.reads, 3);
});

test("a stopped runner reads nothing more: not at its interval, not when the page shows, not on read now", async () => {
  const h = host();
  const r = reader();
  const run = inputs([declared("edits")], { edits: r }, h);
  await run.start();
  run.stop();
  await h.advance(300_000);
  await h.show();
  await run.read("edits");
  await settle();
  assert.equal(r.reads, 1);
});

test("an adopt that throws fails its input, and the poll goes on", async () => {
  const h = host();
  const r = reader();
  let throws = true;
  r.adopt = (v) => {
    if (throws) throw new Error("the module refused it");
    r.adopted.push(v);
  };
  const run = inputs([declared("edits")], { edits: r }, h);
  await run.start();
  assert.equal(h.states.edits.kind, "failed");
  assert.equal(h.states.edits.message, "the module refused it");
  throws = false;
  await h.advance(30_000);
  assert.deepEqual(r.adopted, ["v1"]);
  assert.equal(h.states.edits.kind, "answered");
});

test("a refusal is said with its reason; only a read with no answer says no answer", async () => {
  const h = host();
  const refused = reader();
  refused.next = new Error("the served network registers as sha256:1, the server pins sha256:0");
  const silent = reader();
  silent.next = new ReadFailed("no answer");
  const run = inputs([declared("network"), declared("edits")], { network: refused, edits: silent }, h);
  await run.start();
  const clock = () => "7:55 AM";
  assert.equal(h.states.network.refused, true);
  assert.equal(h.states.edits.refused, false);
  assert.equal(labelOf(h.states.network, clock), "Network could not be read: the served network registers as sha256:1, the server pins sha256:0.");
  assert.equal(labelOf(h.states.edits, clock), "Edits could not be read: no answer since 7:55 AM.");
});

test("a declaration and its reader match, one for one", () => {
  const h = host();
  assert.throws(() => inputs([declared("edits")], {}, h), /edits/);
  assert.throws(() => inputs([], { edits: reader() }, h), /edits/);
  assert.throws(() => inputs([declared("edits", { after: ["network"] })], { edits: reader() }, h), /network/);
});
