// Pins the outbox (src/outbox.ts) while the server is away.
//
// 1. Writes are sent in the order they were made. A write that cannot
//    reach the server (a network failure, a 503) keeps waiting, and
//    the writes after it wait behind it.
// 2. Every attempt of a write sends the same idempotency key. An answer
//    the server kept (2xx) takes the write out.
// 3. A refusal (400, 403, 409, 422) keeps the write, held with the
//    server's reasons, and the writes after it go on. Only the person
//    drops a held write, or sends it again.
// 4. No valid sign-in (a 401) holds the outbox, and fails nothing.
// 5. The outbox survives a reload: what waits is read back in order.
// 6. A write that waited carries the device's time it was made, as a
//    claim; a write sent at once does not.
//
import { test } from "node:test";
import assert from "node:assert/strict";
import { Outbox } from "./outbox.ts";

// Answers one per send, then `net.then`, which is a kept write until the
// test changes it.
function fixture(answers) {
  const sent = [];
  const saved = { value: undefined };
  const net = { then: { status: 200, body: {} } };
  const send = async (url, body) => {
    sent.push({ url, body: JSON.parse(JSON.stringify(body)) });
    const next = answers.shift() ?? net.then;
    if (next === "offline") throw new TypeError("Failed to fetch");
    return next;
  };
  const store = {
    load: async () => saved.value,
    save: async (v) => void (saved.value = JSON.parse(JSON.stringify(v))),
  };
  let clock = 1_000;
  const box = new Outbox({
    send,
    store,
    now: () => clock,
    newKey: (() => {
      let n = 0;
      return () => `device/${++n}`;
    })(),
  });
  return { box, sent, saved, net, tick: (ms) => (clock += ms) };
}

const note = (text) => ({
  log: "notes",
  ops: [{ cap: "notes", op: "write", args: { id: "n1", text } }],
  label: text,
});

test("writes go in order; one that cannot reach the server waits, and the rest wait behind it", async () => {
  const { box, sent, net } = fixture([]);
  net.then = "offline";
  await box.queue(note("first"));
  await box.queue(note("second"));
  assert.deepEqual(
    sent.map((s) => s.body.ops[0].args.text),
    ["first", "first"],
    "the second waits behind the first",
  );
  assert.deepEqual(
    box.items.map((i) => [i.label, i.state]),
    [
      ["first", "waiting"],
      ["second", "waiting"],
    ],
  );
  assert.equal(box.offline, true);
  net.then = { status: 200, body: {} };
  await box.flush();
  assert.deepEqual(
    sent.map((s) => s.body.ops[0].args.text),
    ["first", "first", "first", "second"],
  );
  assert.equal(box.offline, false);
  assert.equal(box.items.length, 0);
});

test("a 503 keeps the write waiting", async () => {
  const { box } = fixture([
    { status: 503, body: "the store does not answer", retryAfter: 2 },
  ]);
  const result = await box.queue(note("x"));
  assert.equal(result.state, "waiting");
  assert.equal(box.items[0].state, "waiting");
});

test("every attempt sends the same key, and a kept answer takes the write out", async () => {
  const { box, sent } = fixture([
    "offline",
    "offline",
    { status: 200, body: { seq: 4 } },
  ]);
  await box.queue(note("x"));
  await box.flush();
  await box.flush();
  assert.equal(new Set(sent.map((s) => s.body.key)).size, 1);
  assert.equal(sent[0].url, "logs/notes");
  assert.equal(box.items.length, 0);
});

test("a refusal is held with its reasons, and the writes after it go on", async () => {
  const refusals = { op: 0, refusals: [{ code: "blank" }] };
  const { box, sent } = fixture([
    { status: 422, body: refusals },
    { status: 200, body: {} },
  ]);
  const first = await box.queue(note(" "));
  assert.equal(first.state, "held");
  await box.queue(note("fine"));
  assert.equal(sent.length, 2);
  assert.deepEqual(
    box.items.map((i) => [i.label, i.state, i.status]),
    [[" ", "held", 422]],
  );
  assert.deepEqual(box.items[0].reasons, refusals);
  await box.flush();
  assert.equal(sent.length, 2, "a held write is not sent again on its own");
  await box.discard(box.items[0].key);
  assert.equal(box.items.length, 0, "the person drops it");
});

test("a held write sent again goes with its key, and the person's new read", async () => {
  const { box, sent } = fixture([
    { status: 409, body: { conflict: "note n1" } },
    { status: 200, body: {} },
  ]);
  await box.queue({ ...note("x"), readSeq: 3 });
  const key = box.items[0].key;
  await box.retry(key, { readSeq: 7, readSeqs: { alerts: 4 } });
  assert.equal(sent[1].body.key, key);
  assert.equal(sent[1].body.read_seq, 7);
  assert.deepEqual(
    sent[1].body.read_seqs,
    { alerts: 4 },
    "and the other logs' seqs it read (D-233)",
  );
  assert.equal(box.items.length, 0);
});

test("no valid sign-in holds the outbox and fails nothing", async () => {
  const { box, sent, net } = fixture([]);
  net.then = { status: 401, body: "missing bearer token" };
  await box.queue(note("a"));
  await box.queue(note("b"));
  assert.deepEqual(
    sent.map((s) => s.body.ops[0].args.text),
    ["a", "a"],
  );
  assert.equal(box.needsSignIn, true);
  assert.deepEqual(
    box.items.map((i) => i.state),
    ["waiting", "waiting"],
  );
  net.then = { status: 200, body: {} };
  await box.flush();
  assert.equal(box.needsSignIn, false);
  assert.equal(box.items.length, 0);
});

test("what waits is read back after a reload, in order", async () => {
  const { box, saved, net } = fixture([]);
  net.then = "offline";
  await box.queue(note("one"));
  await box.queue(note("two"));
  const sent = [];
  const again = new Outbox({
    send: async (url, body) => (sent.push(body), { status: 200, body: {} }),
    store: { load: async () => saved.value, save: async () => {} },
    now: () => 0,
    newKey: () => "unused",
  });
  await again.restore();
  assert.deepEqual(
    again.items.map((i) => i.label),
    ["one", "two"],
  );
  await again.flush();
  assert.deepEqual(
    sent.map((b) => b.ops[0].args.text),
    ["one", "two"],
  );
  assert.deepEqual(
    sent.map((b) => b.key),
    ["device/1", "device/2"],
    "the keys made before the reload",
  );
});

test("a write that waited carries the time it was made, as a claim", async () => {
  const { box, sent, tick } = fixture([
    { status: 200, body: {} },
    "offline",
    { status: 200, body: {} },
  ]);
  await box.queue(note("at once"));
  assert.equal(
    sent[0].body.claimed_at_ms,
    undefined,
    "sent at once: the server's stamp is the time",
  );
  await box.queue(note("later"));
  tick(60_000);
  await box.flush();
  assert.equal(sent[2].body.claimed_at_ms, 1_000);
});

test("a write that is not a log's intent is sent as given", async () => {
  const { box, sent } = fixture([{ status: 200, body: {} }]);
  await box.queue({ url: "plans", body: { id: "p1" }, label: "Publish" });
  assert.deepEqual(sent[0], { url: "plans", body: { id: "p1" } });
});

test("a route that takes a key gets it, and a file goes as it is, after nothing else", async () => {
  const { box, sent } = fixture([
    { status: 200, body: {} },
    { status: 200, body: {} },
  ]);
  await box.queue({
    url: "attachments",
    body: "PHOTO-BYTES",
    contentType: "image/jpeg",
    label: "Photo",
  });
  await box.queue({
    url: "stop-requests/work-order",
    body: { request: "R-1", action: "open" },
    keyed: true,
    label: "Open",
  });
  assert.deepEqual(sent[0], { url: "attachments", body: "PHOTO-BYTES" });
  assert.deepEqual(sent[1].body, {
    request: "R-1",
    action: "open",
    key: "device/2",
  });
});
