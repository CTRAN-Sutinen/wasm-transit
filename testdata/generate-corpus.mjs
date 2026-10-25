// Generates testdata/corpus.json: the golden request->response corpus
// every host must replay byte-identically. Expected values are
// captured FROM the artifact — never hand-written — so the corpus is
// by construction what the artifact produces.
//
// The `$FEED` placeholder is substituted with the actual registry id
// at replay time (hosts may register feeds before replaying).
//
// Run: node testdata/generate-corpus.mjs   (or `just corpus`)

import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const ARTIFACT = path.join(here, "../web/pkg/transit_wasm.wasm");
const FIXTURE = path.join(here, "../web/e2e/fixtures/sample-gtfs.zip");
const OUT = path.join(here, "corpus.json");

const bytes = readFileSync(ARTIFACT);
const { instance } = await WebAssembly.instantiate(bytes, {});
const { transit_alloc, transit_free, transit_invoke, transit_invoke_bytes, memory } = instance.exports;
const OP_LOAD_FEED = 0;

// Invoke a JSON request; return the RAW wrapper text ({"ok":...} or
// {"error":...}) — the replay compares these bytes exactly.
function rawInvoke(requestText) {
  const payload = Buffer.from(requestText);
  const ptr = transit_alloc(payload.length);
  new Uint8Array(memory.buffer, ptr, payload.length).set(payload);
  const outPtr = transit_invoke(ptr, payload.length);
  transit_free(ptr, payload.length);
  return readRaw(outPtr);
}

function readRaw(outPtr) {
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
  return readRaw(outPtr);
}

const feedIdPlaceholder = "$FEED";

// The corpus: one op per response shape, plus the edit-document flow
// and an error outcome. Requests are written with $FEED placeholders.
const steps = [
  { name: "summary", request: `{"op":"summary","feed_id":${feedIdPlaceholder}}` },
  { name: "feed_info", request: `{"op":"feed_info","feed_id":${feedIdPlaceholder}}` },
  { name: "active_services", request: `{"op":"active_services","feed_id":${feedIdPlaceholder},"date":"2026-10-23"}` },
  {
    name: "route_stop_sequence",
    request: `{"op":"route_stop_sequence","feed_id":${feedIdPlaceholder},"route_id":"10","direction_id":null}`,
  },
  {
    name: "departures_base",
    request: `{"op":"departures","feed_id":${feedIdPlaceholder},"stop_id":"B","date":"2026-10-23","from_secs":0,"limit":10}`,
  },
  {
    name: "set_diversions_draft",
    request: `{"op":"set_diversions","feed_id":${feedIdPlaceholder},"records":[{"id":"e1","diversion":{"id":"d1","route_id":"10","direction_id":null,"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],"detour":null,"added_secs":300,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"draft","rev":1,"updated_at":1}]}`,
  },
  {
    name: "departures_effective",
    request: `{"op":"departures","feed_id":${feedIdPlaceholder},"stop_id":"B","date":"2026-10-23","from_secs":0,"limit":10}`,
  },
  {
    name: "departures_skipped_stop_loses_service",
    request: `{"op":"departures","feed_id":${feedIdPlaceholder},"stop_id":"C","date":"2026-10-23","from_secs":0,"limit":10}`,
  },
  {
    name: "plan_trip_compare",
    request: `{"op":"plan_trip_compare","feed_id":${feedIdPlaceholder},"from":"A","to":"B","date":"2026-10-23","depart_secs":27000,"max_legs":6}`,
  },
  {
    name: "stops_geojson",
    request: `{"op":"stops_geojson","feed_id":${feedIdPlaceholder}}`,
  },
  {
    name: "trip_path_geojson",
    request: `{"op":"trip_path_geojson","feed_id":${feedIdPlaceholder},"trip_id":"t10","date":"2026-10-23"}`,
  },
  {
    name: "block_trips",
    request: `{"op":"block_trips","feed_id":${feedIdPlaceholder},"block_id":"B1","date":"2026-10-23"}`,
  },
  {
    name: "sync_edits_deterministic",
    request: `{"op":"sync_edits","feed_id":${feedIdPlaceholder},"client":[],"server":[],"now_ms":1000}`,
  },
  // The authored-network flow (network_import / network_load) moved
  // out of the monolith with the codec: cap-network owns it now,
  // and its module tests hold the "a planner's document drives the
  // same ops" proof (export → zip → parse → the same op surface).
  { name: "unknown_feed_errors", request: `{"op":"summary","feed_id":999}` },
];

// Load the fixture (first feed in a fresh instance: id 0), then capture
// each step's raw wrapper text, restoring $FEED in the responses.
const loadText = rawInvokeBytes(OP_LOAD_FEED, readFileSync(FIXTURE));
const loadOutcome = JSON.parse(loadText);
const feedId = String(loadOutcome.ok.feed_id);

const corpusSteps = [
  { name: "load_fixture", bytes_op: OP_LOAD_FEED, expect: loadText.replaceAll(`"feed_id":${feedId}`, `"feed_id":${feedIdPlaceholder}`) },
];
for (const step of steps) {
  const request = step.request.replaceAll(feedIdPlaceholder, feedId);
  const expect = rawInvoke(request);
  corpusSteps.push({ name: step.name, request: step.request, expect });
}

const corpus = {
  fixture: "web/e2e/fixtures/sample-gtfs.zip",
  note: "Golden corpus: every host replays these against the artifact and must get byte-identical wrapper text. $FEED substitutes the GTFS feed id. Regenerate with: just corpus",
  steps: corpusSteps,
};
writeFileSync(OUT, JSON.stringify(corpus, null, 2) + "\n");
console.log(`wrote ${OUT} (${corpusSteps.length} steps, feed_id ${feedId})`);
