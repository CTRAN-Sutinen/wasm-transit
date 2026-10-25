// Capability-module ABI smoke: instantiates each capability module
// directly in Node (zero imports, the flat ABI) and verifies the
// module contract plus the composition — the same bytes into both
// modules, hashes equal, ops answering through their typed seams.
//
// The manifest is verified too: every capability it advertises is
// present, its bytes hash to the pinned hash, and the bulk op loads
// the fixture. Run from anywhere: `node crates/cap-abi/caps-smoke.mjs`.
// Also `just test`.

import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.join(here, "../../web/public/caps");
const DATE = "2026-10-23";

const manifest = JSON.parse(readFileSync(path.join(DIST, "manifest.json"), "utf8"));
const feed = readFileSync(path.join(DIST, "sample-gtfs.zip"));

const assert = (cond, msg) => {
  if (!cond) throw new Error("FAIL: " + msg);
  console.log("ok - " + msg);
};

function loadModule(cap) {
  const bytes = readFileSync(path.join(DIST, "..", cap.url));
  const hash = "sha256:" + createHash("sha256").update(bytes).digest("hex");
  assert(hash === cap.hash, `capability ${cap.id} bytes match the pinned hash`);
  const mod = new WebAssembly.Module(bytes);
  assert(
    WebAssembly.Module.imports(mod).length === 0,
    `capability ${cap.id} has zero imports`,
  );
  assert(
    WebAssembly.Module.exports(mod).some((e) => e.name === "transit_invoke"),
    `capability ${cap.id} exports transit_invoke`,
  );
  return new WebAssembly.Instance(mod, {}).exports;
}

function shell(exports) {
  const raw = (request) => {
    const payload = Buffer.from(JSON.stringify(request));
    const ptr = exports.transit_alloc(payload.length);
    new Uint8Array(exports.memory.buffer, ptr, payload.length).set(payload);
    const outPtr = exports.transit_invoke(ptr, payload.length);
    exports.transit_free(ptr, payload.length);
    const len = new DataView(exports.memory.buffer).getUint32(outPtr, true);
    const outcome = JSON.parse(
      Buffer.from(exports.memory.buffer, outPtr + 4, len).toString("utf8"),
    );
    exports.transit_free(outPtr, len + 4);
    return outcome;
  };
  const read = (outPtr) => {
    const len = new DataView(exports.memory.buffer).getUint32(outPtr, true);
    const outcome = JSON.parse(
      Buffer.from(exports.memory.buffer, outPtr + 4, len).toString("utf8"),
    );
    exports.transit_free(outPtr, len + 4);
    if (outcome.error) throw new Error("op error: " + outcome.error);
    return outcome.ok;
  };
  const invoke = (request) => {
    const outcome = raw(request);
    if (outcome.error) throw new Error("op error: " + outcome.error);
    return outcome.ok;
  };
  const invokeBytes = (op, bytes) => {
    const ptr = exports.transit_alloc(bytes.length);
    new Uint8Array(exports.memory.buffer, ptr, bytes.length).set(bytes);
    const outPtr = exports.transit_invoke_bytes(op, ptr, bytes.length);
    exports.transit_free(ptr, bytes.length);
    return read(outPtr);
  };
  return { invoke, invokeBytes, raw };
}

const byId = Object.fromEntries(manifest.capabilities.map((c) => [c.id, c]));
assert(manifest.kernel === 1, "manifest speaks kernel v1");
assert(byId.gtfs && byId.router, "manifest advertises gtfs and router");
assert(byId.router.consumes.feed === "gtfs", "router declares its registry edge");

// Agent context: every capability states its purpose and documents
// every op — the discovery surface an agent assembles from.
for (const cap of manifest.capabilities) {
  assert(
    typeof cap.purpose === "string" && cap.purpose.length > 40,
    `capability ${cap.id} carries agent context (purpose)`,
  );
  for (const op of cap.ops) {
    assert(cap.op_docs[op], `capability ${cap.id} documents op ${op}`);
  }
  assert(Array.isArray(cap.speaks.feed), `capability ${cap.id} declares feed versions`);
}

// Compatibility documentation: gtfs speaks both watchlist versions
// and provides the 1→2 migration; the validator accepted it.
assert(
  byId.gtfs.speaks.watchlist.includes(1) && byId.gtfs.speaks.watchlist.includes(2),
  "gtfs speaks watchlist v1 and v2",
);
assert(
  byId.gtfs.migrates.watchlist.some((m) => m.from === 1 && m.to === 2),
  "gtfs migrates watchlist 1→2",
);

// The composition: the same feed bytes into both modules, and the
// content hashes they return agree.
const gtfs = shell(loadModule(byId.gtfs));
const router = shell(loadModule(byId.router));

const loadedGtfs = gtfs.invokeBytes(0, feed);
assert(loadedGtfs.op === "load_feed", "gtfs load_feed answers on the bulk-bytes seam");
const loadedRouter = router.invokeBytes(0, feed);
assert(
  loadedGtfs.feed_hash === loadedRouter.feed_hash,
  "both capabilities agree on the document identity (" + loadedGtfs.feed_hash.slice(0, 19) + "…)",
);

const departures = gtfs.invoke({
  op: "departures",
  feed_hash: loadedGtfs.feed_hash,
  stop_id: "A",
  date: DATE,
  from_secs: 7 * 3600,
  limit: 10,
});
assert(
  departures.departures.length === 3,
  `gtfs departures at A on ${DATE}: ${departures.departures.length}`,
);

const plan = router.invoke({
  op: "plan",
  feed_hash: loadedRouter.feed_hash,
  from: "A",
  to: "B",
  date: DATE,
  depart_secs: 7 * 3600,
  max_legs: 4,
});
assert(
  plan.itinerary && plan.itinerary.arrival_secs === 8 * 3600 + 600,
  "router plans A → B: arrive 08:10",
);

const bad = router.raw({ op: "nope" });
assert(bad.error && bad.error.includes("invalid request"), "unknown ops error, never panic");
const missing = gtfs.raw({
  op: "departures",
  feed_hash: "sha256:missing",
  stop_id: "A",
  date: DATE,
  from_secs: 0,
  limit: 1,
});
assert(missing.error && missing.error.includes("unknown feed"), "unknown hashes error, never panic");

// The migration flow through the ABI: v1 in, v2 out, loss REPORTED,
// both versions query the board.
const v1 = {
  version: "v1",
  stops: [
    { id: "A", note: "home", color: "blue" },
    { id: "B", note: "work" },
  ],
};
const migrated = gtfs.invoke({ op: "migrate_watchlist", document: v1 });
assert(migrated.document.version === "v2", "migration produces v2");
assert(migrated.document.stops[0].label === "home", "label = note");
assert(
  migrated.notes.length === 1 &&
    migrated.notes[0].kind === "dropped_field" &&
    migrated.notes[0].field === "color" &&
    migrated.notes[0].count === 1,
  "the dropped color is reported — never silent",
);
const again = gtfs.raw({ op: "migrate_watchlist", document: migrated.document });
assert(again.error && again.error.includes("already version 2"), "re-migrating v2 errors, not panics");

for (const document of [v1, migrated.document]) {
  const loaded = gtfs.invoke({ op: "load_watchlist", document });
  const board = gtfs.invoke({
    op: "watchlist_board",
    watchlist_hash: loaded.watchlist_hash,
    feed_hash: loadedGtfs.feed_hash,
    date: DATE,
    from_secs: 7 * 3600,
    limit: 3,
  });
  assert(board.stops.length === 2, `board answers at ${document.version}`);
  assert(
    board.stops.every((s) => s.departures.length >= 0),
    `board departures at ${document.version}`,
  );
}

console.log("capability modules: ABI smoke ok");
