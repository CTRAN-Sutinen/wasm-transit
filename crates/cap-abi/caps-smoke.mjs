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
assert(byId.network, "manifest advertises the network capability");
assert(byId.router.consumes.feed === "gtfs", "router declares its registry edge");
assert(byId.analysis.consumes.network === "network", "analysis consumes networks from the network capability");

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
  assert(Object.keys(cap.speaks).length > 0, `capability ${cap.id} declares the documents it speaks`);
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

// The analysis flow — the first REAL capability extracted from the
// monolith: a network document in by content hash, planner numbers
// out by hash.
const analysis = shell(loadModule(byId.analysis));
const NETWORK = {
  name: "Cap Analysis Net",
  plan: { id: "", name: "" },
  anchor: "noon_minus_twelve",
  stops: {
    A: { name: "Main & 1st", lat: 45.5, lon: -73.5, transfer: false },
    B: { name: "Main & 2nd", lat: 45.501, lon: -73.501, transfer: false },
  },
  routes: { "10": { name: "10", mode: "bus" } },
  patterns: {
    "pat-10-0-0": {
      route: "10",
      name: "10 → Main & 2nd",
      stops: [
        { stop: "A", timepoint: true, distance_m: 0.0 },
        { stop: "B", timepoint: true, distance_m: 500.0 },
      ],
      timing: [{ start_secs: 0, end_secs: 108000, at_timepoints: [0, 600] }],
    },
  },
  service_types: {
    WK: {
      name: "WK",
      rule: {
        days: { mon: true, tue: true, wed: true, thu: true, fri: true, sat: false, sun: false },
        ranges: [{ start: "2026-01-01", end: "2026-12-31" }],
        exceptions: [],
      },
    },
  },
  schedules: {
    "WK:pat-10-0-0": {
      pattern: "pat-10-0-0",
      service_type: "WK",
      schedule: { kind: "exact_times", starts: [28800] },
    },
  },
  blocks: {},
  notes: [],
};
const loadedNet = analysis.invoke({ op: "load_network", network: NETWORK });
assert(loadedNet.network_hash.startsWith("sha256:"), "analysis registers the network by content hash");
const stats = analysis.invoke({
  op: "network_stats",
  network_hash: loadedNet.network_hash,
  date: "2026-01-05", // a Monday: WK runs
});
assert(stats.stats.trips === 1, `analysis computes the document's weekday trips: ${stats.stats.trips}`);
const overview = analysis.invoke({ op: "service_types_overview", network_hash: loadedNet.network_hash });
assert(overview.service_types.length === 1, "the design axis answers");
const layer = analysis.invokeBytes(0, Buffer.from(JSON.stringify({
  type: "FeatureCollection",
  features: [
    { type: "Feature", properties: { pop: 500 }, geometry: { type: "Point", coordinates: [-73.5, 45.5] } },
    { type: "Feature", properties: { pop: 500 }, geometry: { type: "Point", coordinates: [-74.0, 46.0] } },
  ],
})));
assert(layer.points === 2, "analysis registers a population layer by content hash");
const coverage = analysis.invoke({
  op: "type_coverage",
  network_hash: loadedNet.network_hash,
  layer_hash: layer.layer_hash,
  radius_m: 500,
  frequent_secs: 3600,
  min_departures: 2,
});
assert(coverage.coverage.length === 1, "type coverage answers by hash pair");
assert(
  coverage.coverage[0].served_pct === 50,
  `half the layer is served (one point at a stop, one far away): ${coverage.coverage[0].served_pct}`,
);
const unknownNet = analysis.raw({ op: "network_stats", network_hash: "sha256:missing", date: "2026-01-05" });
assert(unknownNet.error && unknownNet.error.includes("unknown network"), "unknown hashes error, never panic");

// The network flow — the second REAL capability extracted from the
// monolith: the planner's whole bootstrap → author → publish path.
// The same feed bytes into a THIRD module, identity asserted; import
// derives the document (hash returned, resolutions reported);
// export produces a GTFS zip whose identity any module agrees on.
const network = shell(loadModule(byId.network));
const loadedNetCap = network.invokeBytes(0, feed);
assert(loadedNetCap.op === "load_feed", "network load_feed answers on the bulk-bytes seam");
assert(
  loadedNetCap.feed_hash === loadedGtfs.feed_hash,
  "three capabilities agree on the feed identity",
);
const imported = network.invoke({ op: "import", feed_hash: loadedNetCap.feed_hash });
assert(
  imported.network && Object.keys(imported.network.stops).length === 3,
  "import infers the fixture feed (3 stops)",
);
assert(
  imported.network_hash.startsWith("sha256:"),
  "import registers the derived document and returns its hash",
);
assert(
  Array.isArray(imported.notes) && imported.notes.length > 0,
  "the fixture's dropped single-stop trip is reported — never silent",
);
const validated = network.invoke({ op: "validate", network_hash: imported.network_hash });
assert(validated.issues.length === 0, "the derived document validates clean");
const loadedDoc = network.invoke({ op: "load_document", network: imported.network });
assert(
  loadedDoc.network_hash === imported.network_hash,
  "load_document is idempotent by content hash",
);
const exported = network.invoke({ op: "export", network_hash: imported.network_hash });
const zip = Buffer.from(exported.zip_b64, "base64");
assert(
  "sha256:" + createHash("sha256").update(zip).digest("hex") === exported.feed_hash,
  "the export's hash is the content hash of the decoded bytes",
);
assert(
  zip[0] === 0x50 && zip[1] === 0x4b,
  "the export payload is zip bytes",
);
// The composition moment of the publish path: the exported artifact is
// a first-class feed — cap-gtfs loads the SAME bytes and returns the
// SAME identity the export advertised.
const published = gtfs.invokeBytes(0, zip);
assert(
  published.feed_hash === exported.feed_hash,
  "the exported zip loads into cap-gtfs under the advertised hash",
);
// Broken documents: validate reports every issue, export refuses.
const broken = JSON.parse(JSON.stringify(imported.network));
broken.patterns[Object.keys(broken.patterns)[0]].stops[1].stop = "ghost";
const brokenHash = network.invoke({ op: "load_document", network: broken }).network_hash;
const brokenIssues = network.invoke({ op: "validate", network_hash: brokenHash });
assert(brokenIssues.issues.length > 0, "a referential break is reported");
const refused = network.raw({ op: "export", network_hash: brokenHash });
assert(refused.error && refused.error.includes("invalid network"), "export refuses invalid documents");
const unknownFeed = network.raw({ op: "import", feed_hash: "sha256:missing" });
assert(unknownFeed.error && unknownFeed.error.includes("unknown feed"), "unknown hashes error, never panic");

// The realtime flow — the third capability extracted from the
// monolith: protobuf bytes in on the bulk seam, delays and vehicles
// out. A pure decoder; garbage bytes error, never trap.
const rt = shell(loadModule(byId.rt));
const rtPayload = readFileSync(path.join(here, "../../web/e2e/fixtures/sample-rt.pb"));
const decoded = rt.invokeBytes(0, rtPayload);
assert(decoded.op === "decode", "rt decode answers on the bulk-bytes seam");
assert(decoded.overview.trip_updates === 1, "one trip update in the fixture");
assert(decoded.overview.vehicles === 1, "one vehicle in the fixture");
const trip = decoded.overview.trips[0];
assert(trip.trip_id === "t10", "the delayed trip is t10");
assert(trip.stop_updates[0].departure_delay === 120, "t10 is +120 s at A");
const vehicle = decoded.overview.vehicle_positions[0];
assert(vehicle.label === "4201", "the vehicle label (the bus number) survives decode");
const garbage = rt.raw({ op: "decode", bytes: [0xff, 0xff] });
assert(garbage.error && /decode/.test(garbage.error), "garbage bytes are an error outcome, never a trap");

console.log("capability modules: ABI smoke ok");
