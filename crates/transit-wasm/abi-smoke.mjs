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

// The sync flow's record fixture (the edit document's LWW +
// tombstone rules are transit-sync's, pinned over the artifact's
// sync_edits seam below — the host-side store the server keeps).
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

// The map's geojson ops, set_diversions, and the edit-document flow
// are GONE from the monolith (U6: the map renders from model
// documents in host view code; the day views hold their own edit
// registries and validate at load_edits). The pins for the
// effective-day facts live in cap-runcut's dispatch test (the
// patched trip answers A→B, B at 08:55) and caps-smoke's runcut
// flow — never here.

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
// the day-view capabilities own the day views — and so are the
// planning ops (cap-router plans over the network document and the
// effective day, diversion delta included) and the map's geojson
// ops with set_diversions (the map renders from model documents in
// host view code; the day views hold their own edit registries);
// the smoke pins every retirement through the standard outcome,
// never a trap.
{
  for (const retired of ["active_services", "blocks", "feed_info", "departures", "block_trips", "plan_trip", "plan_trip_compare", "stops_geojson", "trip_path_geojson", "route_path_geojson", "trip_segment_geojson", "stop_point", "set_diversions"]) {
    const outcome = JSON.parse(rawInvoke(JSON.stringify({ op: retired, feed_id: feedId })));
    assert(outcome.error && /unknown variant/.test(outcome.error),
      `the retired ${retired} op errors through the standard outcome`);
  }
}

// Error outcomes, not traps.
const errOutcome = (() => {
  const payload = Buffer.from(JSON.stringify({ op: "sync_edits", feed_id: 99, client: [], server: [], now_ms: 0 }));
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

// The picker and summary ops are RETIRED on the monolith (the
// directory owns what exists; the feed line reads the gtfs
// capability's load_feed): every retired op errors with the
// unknown-variant outcome, never traps, never silently serves.
for (const retired of ["summary", "stops", "routes", "route_stop_sequence"]) {
  const o = JSON.parse(rawInvoke(JSON.stringify({ op: retired, feed_id: 0, route_id: "10", direction_id: null })));
  assert(o.error && o.error.includes("unknown variant"), retired + " is retired: " + JSON.stringify(o));
}

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

// The population-layer parse is GONE from the monolith — cap-analysis
// owns the layers and the coverage that reads them (U4: the UI's
// import handle comes from load_population_layer's registration
// answer). The artifact must REJECT the retired op and bulk code with
// the standard outcome, never a trap.
{
  const outcome = JSON.parse(rawInvoke(JSON.stringify({ op: "coverage_load", bytes: [], pop_field: null })));
  assert(outcome.error && /unknown variant|invalid request/.test(outcome.error),
    "the retired coverage_load op errors through the standard outcome");
  const loaded = JSON.parse(rawInvokeBytes(3, new TextEncoder().encode("{}")));
  assert(loaded.error && loaded.error.includes("unknown bytes op code"),
    "the retired coverage-load bulk op code errors through the standard outcome");
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
  // are gone (the day-view capabilities own the day views), so are
  // the picker and summary steps (the directory owns what exists),
  // the plan-compare step (cap-router owns planning), and the
  // geojson / set_diversions steps (the map renders from model
  // documents in host view code; the day views hold their own edit
  // registries). What survives is the sync seam's own surface.
  const wantSteps = [
    "load_fixture", "plans_resolve",
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
