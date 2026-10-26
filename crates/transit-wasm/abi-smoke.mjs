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
const ghost = invoke({ op: "route_path_geojson", feed_id: feedId, route_id: "10", direction_id: null, date: DATE }).feature;
assert(ghost && ghost.properties.trip_id === "t30" && ghost.geometry.coordinates.length === 2,
  "route ghost: representative trip (t30) follows the effective path — C is skipped by the edit above");
const ghostNone = invoke({ op: "route_path_geojson", feed_id: feedId, route_id: "missing", direction_id: null, date: DATE }).feature;
assert(ghostNone === null, "route ghost: unknown route is null");

// Street ops: bytes in once through the bulk ABI, then snap/route
// queries as JSON. The Elbow route follows street geometry, not the
// diagonal.
const OP_STREETS_LOAD = 2;
const elbow = new TextEncoder().encode(JSON.stringify({
  type: "FeatureCollection",
  features: [{
    type: "Feature",
    geometry: { type: "LineString", coordinates: [[-73.5030, 45.5040], [-73.5080, 45.5040], [-73.5080, 45.5090]] },
  }],
}));
const loaded = invokeBytes(OP_STREETS_LOAD, elbow);
assert(loaded.segments === 2 && loaded.nodes === 3, "streets_load: Elbow graph via ABI");
const posA = invoke({ op: "street_snap", streets_id: loaded.streets_id, lat: 45.5040, lon: -73.5030 }).position;
const posB = invoke({ op: "street_snap", streets_id: loaded.streets_id, lat: 45.5080, lon: -73.5080 }).position;
assert(posA && posB, "street_snap via ABI");
const streetRoute = invoke({ op: "street_route", streets_id: loaded.streets_id, from: posA, to: posB }).points;
assert(streetRoute.length === 2 &&
  Math.abs(streetRoute[0].lat - 45.5040) < 1e-9 && Math.abs(streetRoute[0].lon + 73.5080) < 1e-9,
  "street_route: the corner, not the diagonal, via ABI");

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

// The network codec ops are GONE from the monolith — they live in the
// cap-network capability now (the second extraction). The artifact
// must REJECT the retired ops with the standard error outcome, never
// a trap: the deletion is itself an ABI contract change this smoke
// pins.
{
  const outcome = JSON.parse(rawInvoke(JSON.stringify({ op: "network_import", feed_id: feedId })));
  assert(outcome.error && /unknown variant|invalid request/.test(outcome.error),
    "the retired network op errors through the standard outcome");
  const rt = JSON.parse(rawInvoke(JSON.stringify({ op: "decode_rt", bytes: [] })));
  assert(rt.error && /unknown variant|invalid request/.test(rt.error),
    "the retired rt op errors through the standard outcome (cap-rt owns the decoder)");
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
  for (const step of corpus.steps) {
    let got;
    if (step.bytes_op !== undefined) {
      got = rawInvokeBytes(step.bytes_op, fixtureBytes);
      feedToken = String(JSON.parse(got).ok.feed_id);
    } else {
      if (step.request.includes("$FEED") && !feedToken) throw new Error(`corpus step ${step.name} uses $FEED before load`);
      got = rawInvoke(step.request.replaceAll("$FEED", feedToken));
    }
    const want = step.expect.replaceAll("$FEED", feedToken);
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
