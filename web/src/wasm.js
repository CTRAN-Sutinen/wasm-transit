/// The browser host: a thin wrapper over the wasm artifact's flat C
/// ABI (see crates/transit-wasm). The artifact has zero imports, so
/// instantiation is trivial — no wasm-bindgen glue anywhere. This
/// module preserves the JS API the app already speaks (`GtfsHandle`
/// with the same method names and return shapes), so components and
/// tests are unaffected by the ABI underneath.
///
/// Protocol notes (crates/transit-wasm/src/lib.rs has the full
/// contract): requests are JSON, results are
/// `[u32 LE length][payload]` buffers that the host reads then frees;
/// outcomes are `{"ok": ...}` or `{"error": ...}`.

import wasmUrl from "../pkg/transit_wasm.wasm?url";

// Op codes for the bulk-bytes ABI (transit_ops::OP_LOAD_FEED etc.).
const OP_LOAD_FEED = 0;
const OP_DECODE_RT = 1;

let instance = null;

/// Instantiate the artifact once (fetch is JS-owned I/O, per the
/// project's boundary rules).
async function instantiate() {
  if (instance) return instance;
  const bytes = await (await fetch(wasmUrl)).arrayBuffer();
  // Zero imports: every host is this easy.
  const { instance: inst } = await WebAssembly.instantiate(bytes, {});
  instance = inst;
  return instance;
}

function readResult(outPtr) {
  const buffer = instance.exports.memory.buffer;
  const len = new DataView(buffer).getUint32(outPtr, true);
  const payload = new Uint8Array(buffer, outPtr + 4, len);
  const outcome = JSON.parse(new TextDecoder().decode(payload));
  instance.exports.transit_free(outPtr, len + 4);
  if (outcome.error) throw new Error(outcome.error);
  return outcome.ok;
}

function invoke(request) {
  const payload = new TextEncoder().encode(JSON.stringify(request));
  const ptr = instance.exports.transit_alloc(payload.length);
  new Uint8Array(instance.exports.memory.buffer, ptr, payload.length).set(payload);
  const outPtr = instance.exports.transit_invoke(ptr, payload.length);
  instance.exports.transit_free(ptr, payload.length);
  return readResult(outPtr);
}

function invokeBytes(op, bytes) {
  const ptr = instance.exports.transit_alloc(bytes.length);
  new Uint8Array(instance.exports.memory.buffer, ptr, bytes.length).set(bytes);
  const outPtr = instance.exports.transit_invoke_bytes(op, ptr, bytes.length);
  instance.exports.transit_free(ptr, bytes.length);
  return readResult(outPtr);
}

export class GtfsHandle {
  #feedId;

  constructor(feedId) {
    this.#feedId = feedId;
  }

  static load(bytes) {
    const resp = invokeBytes(OP_LOAD_FEED, bytes);
    return new GtfsHandle(resp.feed_id);
  }

  summary() { return invoke({ op: "summary", feed_id: this.#feedId }).summary; }

  stops() { return invoke({ op: "stops", feed_id: this.#feedId }).stops; }

  routes() { return invoke({ op: "routes", feed_id: this.#feedId }).routes; }

  route_stop_sequence(routeId, directionId) {
    return invoke({ op: "route_stop_sequence", feed_id: this.#feedId, route_id: routeId, direction_id: directionId ?? null }).stops;
  }

  departures(stopId, date, fromSecs, limit) {
    return invoke({ op: "departures", feed_id: this.#feedId, stop_id: stopId, date, from_secs: fromSecs, limit }).departures;
  }

  active_services(date) {
    return invoke({ op: "active_services", feed_id: this.#feedId, date }).services;
  }

  blocks() { return invoke({ op: "blocks", feed_id: this.#feedId }).blocks; }

  block_trips(blockId, date) {
    return invoke({ op: "block_trips", feed_id: this.#feedId, block_id: blockId, date }).trips;
  }

  stops_geojson() { return invoke({ op: "stops_geojson", feed_id: this.#feedId }).collection; }

  trip_path_geojson(tripId, date) {
    return invoke({ op: "trip_path_geojson", feed_id: this.#feedId, trip_id: tripId, date }).feature ?? null;
  }

  trip_segment_geojson(tripId, fromStopId, toStopId, date) {
    return invoke({ op: "trip_segment_geojson", feed_id: this.#feedId, trip_id: tripId, from_stop_id: fromStopId, to_stop_id: toStopId, date }).feature ?? null;
  }

  stop_point(stopId) {
    return invoke({ op: "stop_point", feed_id: this.#feedId, stop_id: stopId }).feature ?? null;
  }

  plan_trip(from, to, date, departSecs, maxLegs) {
    return invoke({ op: "plan_trip", feed_id: this.#feedId, from, to, date, depart_secs: departSecs, max_legs: maxLegs }).itinerary ?? null;
  }

  plan_trip_compare(from, to, date, departSecs, maxLegs) {
    const resp = invoke({ op: "plan_trip_compare", feed_id: this.#feedId, from, to, date, depart_secs: departSecs, max_legs: maxLegs });
    return { active: resp.active ?? null, baseline: resp.baseline ?? null };
  }

  set_diversions(records) {
    return invoke({ op: "set_diversions", feed_id: this.#feedId, records }).reports;
  }
}

export function decode_rt(bytes) {
  return invokeBytes(OP_DECODE_RT, bytes).overview;
}

/// The entry point main.js awaits: instantiate the artifact, then
/// hand back the same API surface the app expects.
export async function loadWasm() {
  await instantiate();
  return { GtfsHandle, decode_rt };
}
