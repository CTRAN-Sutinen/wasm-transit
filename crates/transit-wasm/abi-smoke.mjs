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
assert(feed.summary.stops === 6, "load_feed registers and summarizes");

// The whole edit-document flow through JSON. The board op is
// RETIRED (cap-dispatch owns the day views); the effective feed
// pins through the surviving reads: plan compare and the geojson
// paths, both date-scoped by the edit document.
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

const cmp = invoke({ op: "plan_trip_compare", feed_id: feedId, from: "A", to: "B", date: DATE, depart_secs: 27000, max_legs: 6 });
assert(cmp.active.arrival_hhmmss === "08:15:00" && cmp.baseline.arrival_hhmmss === "08:10:00",
  "plan compare: 08:15 vs 08:10");

// GeoJSON comes back as plain data.
const stops = invoke({ op: "stops_geojson", feed_id: feedId }).collection;
assert(stops.type === "FeatureCollection" && stops.features.length === 6, "typed geojson collection");
const trip = invoke({ op: "trip_path_geojson", feed_id: feedId, trip_id: "t10", date: DATE }).feature;
assert(trip && trip.geometry.coordinates.length === 5, "full trip path via ABI (5-point shape)");
const ghost = invoke({ op: "route_path_geojson", feed_id: feedId, route_id: "10", direction_id: null, date: DATE }).feature;
assert(ghost && ghost.properties.trip_id === "t30" && ghost.geometry.coordinates.length === 2,
  "route ghost: representative trip (t30) follows the effective path — C is skipped by the edit above");
const ghostNone = invoke({ op: "route_path_geojson", feed_id: feedId, route_id: "missing", direction_id: null, date: DATE }).feature;
assert(ghostNone === null, "route ghost: unknown route is null");

// The street ops are GONE from the monolith — they live in the
// cap-streets capability now (U1: the detour editor's snap/route
// reads the bus). The artifact must REJECT the retired ops with
// the standard error outcome, never a trap: the deletion is itself
// an ABI contract change this smoke pins.
{
  const snap = JSON.parse(rawInvoke(JSON.stringify({ op: "street_snap", streets_id: 0, lat: 0, lon: 0 })));
  assert(snap.error && /unknown variant|invalid request/.test(snap.error),
    "the retired street_snap op errors through the standard outcome");
  const route = JSON.parse(rawInvoke(JSON.stringify({
    op: "street_route", streets_id: 0,
    from: { lat: 0, lon: 0, dist_m: 0, seg: 0, t: 0 },
    to: { lat: 0, lon: 0, dist_m: 0, seg: 0, t: 0 },
  })));
  assert(route.error && /unknown variant|invalid request/.test(route.error),
    "the retired street_route op errors through the standard outcome");
  // The retired bulk op code errors too (cap-streets owns the graph).
  const loaded = JSON.parse(rawInvokeBytes(2, new TextEncoder().encode("{}")));
  assert(loaded.error && loaded.error.includes("unknown bytes op code"),
    "the retired streets_load bulk op errors through the standard outcome");
}

// The dispatch-read ops (active_services, blocks, block_trips,
// feed_info, departures) are RETIRED — cap-gtfs owns the reads and
// cap-dispatch owns the day views; the smoke pins every retirement
// through the standard outcome, never a trap.
{
  for (const retired of ["active_services", "blocks", "feed_info", "departures", "block_trips"]) {
    const outcome = JSON.parse(rawInvoke(JSON.stringify({ op: retired, feed_id: feedId })));
    assert(outcome.error && /unknown variant/.test(outcome.error),
      `the retired ${retired} op errors through the standard outcome`);
  }
}

// Retire = tombstone.
invoke({ op: "set_diversions", feed_id: feedId, records: [record({ status: "retired", rev: 2 })] });
const cmpBase = invoke({ op: "plan_trip_compare", feed_id: feedId, from: "A", to: "B", date: DATE, depart_secs: 27000, max_legs: 6 });
assert(cmpBase.active.arrival_hhmmss === "08:10:00" && cmpBase.baseline === null,
  "retired records never apply");

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
  // The step set, pinned exactly: the board and block-chain steps
  // are gone (cap-dispatch owns the day views), and the diversion's
  // effect pins through plan_trip_compare and trip_path_geojson.
  const wantSteps = [
    "load_fixture", "summary", "route_stop_sequence", "set_diversions_draft",
    "plan_trip_compare", "stops_geojson", "trip_path_geojson",
    "sync_edits_deterministic", "unknown_feed_errors",
  ];
  assert(
    JSON.stringify(corpus.steps.map((s) => s.name)) === JSON.stringify(wantSteps),
    "corpus has the full step set",
  );
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
