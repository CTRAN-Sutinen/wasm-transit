// ABI smoke test: instantiates the wasm artifact directly (no
// wasm-bindgen, zero imports) and verifies the flat-ABI contract plus
// the core domain flows — the same artifact the browser and the Go
// server run.
//
// Lives next to the artifact crate: it tests the ABI, in Node.
// Run from anywhere: `node crates/transit-wasm/abi-smoke.mjs` (paths
// resolve from this file's location). Also `just test`.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const ARTIFACT = path.join(here, "../../web/pkg/transit_wasm.wasm");
const FIXTURE = path.join(here, "../../web/e2e/fixtures/sample-gtfs.zip");

const bytes = readFileSync(ARTIFACT);
const mod = new WebAssembly.Module(bytes);
const imports = WebAssembly.Module.imports(mod);
const assert = (cond, msg) => {
  if (!cond) throw new Error("FAIL: " + msg);
  console.log("ok - " + msg);
};

assert(imports.length === 0, `artifact has zero imports, got ${imports.length}`);
assert(WebAssembly.Module.exports(mod).some((e) => e.name === "transit_invoke"), "transit_invoke exported");

const inst = new WebAssembly.Instance(mod, {});
const { transit_alloc, transit_free, transit_invoke, transit_invoke_bytes, memory } = inst.exports;

const OP_LOAD_FEED = 0;
const OP_DECODE_RT = 1;
const DATE = "2026-10-23";

function invoke(request) {
  const payload = Buffer.from(JSON.stringify(request));
  const ptr = transit_alloc(payload.length);
  new Uint8Array(memory.buffer, ptr, payload.length).set(payload);
  const outPtr = transit_invoke(ptr, payload.length);
  transit_free(ptr, payload.length);
  return read(outPtr);
}

function invokeBytes(op, payload) {
  const ptr = transit_alloc(payload.length);
  new Uint8Array(memory.buffer, ptr, payload.length).set(payload);
  const outPtr = transit_invoke_bytes(op, ptr, payload.length);
  transit_free(ptr, payload.length);
  return read(outPtr);
}

function read(outPtr) {
  const len = new DataView(memory.buffer).getUint32(outPtr, true);
  const outcome = JSON.parse(Buffer.from(memory.buffer, outPtr + 4, len).toString("utf8"));
  transit_free(outPtr, len + 4);
  if (outcome.error) throw new Error("op error: " + outcome.error);
  return outcome.ok;
}

/// Invoke and return the RAW wrapper text ({"ok":...}/{"error":...})
/// — the golden corpus compares these bytes exactly.
function rawInvoke(requestText) {
  const payload = Buffer.from(requestText);
  const ptr = transit_alloc(payload.length);
  new Uint8Array(memory.buffer, ptr, payload.length).set(payload);
  const outPtr = transit_invoke(ptr, payload.length);
  transit_free(ptr, payload.length);
  const len = new DataView(memory.buffer).getUint32(outPtr, true);
  const text = Buffer.from(memory.buffer, outPtr + 4, len).toString("utf8");
  transit_free(outPtr, len + 4);
  return text;
}

function rawInvokeBytes(op, payload) {
  const ptr = transit_alloc(payload.length);
  new Uint8Array(memory.buffer, ptr, payload.length).set(payload);
  const outPtr = transit_invoke_bytes(op, ptr, payload.length);
  transit_free(ptr, payload.length);
  const len = new DataView(memory.buffer).getUint32(outPtr, true);
  const text = Buffer.from(memory.buffer, outPtr + 4, len).toString("utf8");
  transit_free(outPtr, len + 4);
  return text;
}

const feed = invokeBytes(OP_LOAD_FEED, readFileSync(FIXTURE));
const feedId = feed.feed_id;
assert(feed.summary.stops === 3, "load_feed registers and summarizes");

const boardAt = (stop) =>
  invoke({ op: "departures", feed_id: feedId, stop_id: stop, date: DATE, from_secs: 0, limit: 10 })
    .departures.map((d) => `${d.trip_id}@${d.departure_time_hhmmss}`);

const baseB = boardAt("B");
assert(JSON.stringify(baseB) === JSON.stringify(["t12@07:30:00", "t10@08:10:00", "t20@08:30:00", "t30@08:50:00"]),
  `baseline board at B, got ${JSON.stringify(baseB)}`);

// The whole edit-document flow through JSON.
const record = (over = {}) => ({
  id: "e1",
  diversion: {
    id: "d1", route_id: "10", direction_id: null,
    first_kept_stop: "A", last_kept_stop: "B",
    skipped_stops: ["C"], detour: null,
    added_secs: 300, start_date: DATE, end_date: DATE,
  },
  status: "draft", rev: 1, updated_at: 1, ...over,
});

const reports = invoke({ op: "set_diversions", feed_id: feedId, records: [record()] }).reports;
assert(reports.length === 1 && reports[0].valid && reports[0].affected_trips === 3,
  "reports validate and count trips");

assert(JSON.stringify(boardAt("B")) === JSON.stringify(["t12@07:30:00", "t10@08:15:00", "t20@08:30:00", "t30@08:55:00"]),
  "effective board reflects the edit");
assert(boardAt("C").length === 0, "C loses service");

const cmp = invoke({ op: "plan_trip_compare", feed_id: feedId, from: "A", to: "B", date: DATE, depart_secs: 27000, max_legs: 6 });
assert(cmp.active.arrival_hhmmss === "08:15:00" && cmp.baseline.arrival_hhmmss === "08:10:00",
  "plan compare: 08:15 vs 08:10");

// GeoJSON comes back as plain data.
const stops = invoke({ op: "stops_geojson", feed_id: feedId }).collection;
assert(stops.type === "FeatureCollection" && stops.features.length === 3, "typed geojson collection");
const trip = invoke({ op: "trip_path_geojson", feed_id: feedId, trip_id: "t10", date: DATE }).feature;
assert(trip && trip.geometry.coordinates.length === 5, "full trip path via ABI (5-point shape)");

// feed_info op (the manifest's version source).
const info = invoke({ op: "feed_info", feed_id: feedId }).info;
assert(info && info.feed_version === "2026.10.1", "feed_info surfaces the artifact version");

// Retire = tombstone.
invoke({ op: "set_diversions", feed_id: feedId, records: [record({ status: "retired", rev: 2 })] });
assert(JSON.stringify(boardAt("B")) === JSON.stringify(baseB), "retired records never apply");

// Error outcomes, not traps.
const errOutcome = (() => {
  const payload = Buffer.from(JSON.stringify({ op: "summary", feed_id: 99 }));
  const ptr = transit_alloc(payload.length);
  new Uint8Array(memory.buffer, ptr, payload.length).set(payload);
  const outPtr = transit_invoke(ptr, payload.length);
  transit_free(ptr, payload.length);
  const len = new DataView(memory.buffer).getUint32(outPtr, true);
  const o = JSON.parse(Buffer.from(memory.buffer, outPtr + 4, len).toString("utf8"));
  transit_free(outPtr, len + 4);
  return o;
})();
assert(errOutcome.error && errOutcome.error.includes("unknown feed_id"), "unknown feed: error outcome");

// A second registry entry: multiple feeds coexist in one instance.
const second = invokeBytes(OP_LOAD_FEED, readFileSync(FIXTURE));
assert(second.feed_id !== feedId, "registry assigns distinct feed ids");

// An authored network document drives the same op surface a parsed
// zip does (testdata/network.json is shared with the corpus and the
// native dispatch tests). GTFS is also the bootstrap: infer a
// document from the loaded feed first.
{
  const imported = invoke({ op: "network_import", feed_id: feedId });
  assert(imported.network && Object.keys(imported.network.stops).length === 3,
    "network_import infers the fixture feed (3 stops)");
  assert(invoke({ op: "network_validate", network: imported.network }).issues.length === 0,
    "the inferred document validates clean");

  const netDoc = JSON.parse(readFileSync(path.join(here, "../../testdata/network.json"), "utf8"));
  const v = invoke({ op: "network_validate", network: netDoc });
  assert(v.issues.length === 0, "fixture network validates clean");
  const loaded = invoke({ op: "network_load", network: netDoc });
  assert(loaded.summary.stops === 3 && loaded.summary.trips === 3, "network loads with summary");
  const netFeed = loaded.feed_id;
  const deps = invoke({ op: "departures", feed_id: netFeed, stop_id: "A", date: "2026-10-23", from_secs: 0, limit: 10 }).departures;
  assert(deps.length === 3 && deps[0].trip_id === "10-weekday@28800", "derived trip ids are <service>@<start>");
  const plan = invoke({ op: "plan_trip", feed_id: netFeed, from: "A", to: "C", date: "2026-10-23", depart_secs: 28800, max_legs: 3 }).itinerary;
  assert(plan && plan.legs.length === 1 && plan.legs[0].trip_id === "10-weekday@28800", "plan on the planner's document");
  const broken = JSON.parse(JSON.stringify(netDoc));
  broken.patterns["10-inbound"].stops[1].stop = "ghost";
  const invalid = JSON.parse(rawInvoke(JSON.stringify({ op: "network_load", network: broken })));
  assert(invalid.error && invalid.error.includes("invalid network"), "invalid documents are error outcomes");
}

// Non-UTF-8 request payloads: an error outcome, never a trap.
{
  const payload = Buffer.from([0xff, 0xfe, 0x00, 0b10101010, 0xf0]);
  const ptr = transit_alloc(payload.length);
  new Uint8Array(memory.buffer, ptr, payload.length).set(payload);
  const outPtr = transit_invoke(ptr, payload.length);
  const len = new DataView(memory.buffer).getUint32(outPtr, true);
  const outcome = JSON.parse(Buffer.from(memory.buffer, outPtr + 4, len).toString("utf8"));
  transit_free(outPtr, len + 4);
  transit_free(ptr, payload.length);
  assert(outcome.error && /UTF-8/.test(outcome.error), "invalid UTF-8 yields an error outcome");
}

// Golden corpus replay: byte-identical with the native Rust and Go
// replays — the cross-host equivalence proof. The corpus's own load
// step registers a fresh feed, so prior sections don't matter.
{
  const corpusPath = path.join(here, "../../testdata/corpus.json");
  const corpus = JSON.parse(readFileSync(corpusPath, "utf8"));
  const fixtureBytes = readFileSync(FIXTURE);
  let feedToken = "";
  let netToken = "";
  for (const step of corpus.steps) {
    let got;
    if (step.bytes_op !== undefined) {
      got = rawInvokeBytes(step.bytes_op, fixtureBytes);
      feedToken = String(JSON.parse(got).ok.feed_id);
    } else {
      if (step.request.includes("$FEED") && !feedToken) throw new Error(`corpus step ${step.name} uses $FEED before load`);
      if (step.request.includes("$NET") && !netToken) throw new Error(`corpus step ${step.name} uses $NET before network_load`);
      got = rawInvoke(step.request.replaceAll("$FEED", feedToken).replaceAll("$NET", netToken));
    }
    // A step with bind registers a second feed (the network
    // document); its id is captured like the bytes load above.
    if (step.bind === "$NET") {
      netToken = String(JSON.parse(got).ok.feed_id);
    }
    const want = step.expect.replaceAll("$FEED", feedToken).replaceAll("$NET", netToken);
    assert(got === want, `corpus step ${step.name} diverges`);
  }
  assert(corpus.steps.length >= 14, "corpus has the full step set");
}

console.log("smoke passed");

// Sync op: merge + submit, through the same artifact the server runs.
const synced = invoke({
  op: "sync_edits", feed_id: null,
  client: [record({ status: "draft", rev: 5, updated_at: 200 })],
  server: [record({ status: "retired", rev: 6, updated_at: 300 })],
  now_ms: 1000,
});
assert(synced.records.length === 1, "sync merges to one record per id");
assert(synced.records[0].status === "retired", "server's newer tombstone wins");
assert(synced.reports.length === 0, "no feed: no reports");

const syncedFresh = invoke({
  op: "sync_edits", feed_id: null,
  client: [record({ status: "draft", rev: 1, updated_at: 100 })],
  server: [],
  now_ms: 1000,
});
assert(syncedFresh.records[0].status === "submitted", "drafts become submitted");
assert(syncedFresh.records[0].rev === 2, "submit bumps rev");

console.log("sync smoke passed");
