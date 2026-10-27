/// The browser host: a thin wrapper over the wasm artifact's flat C
/// ABI (see crates/transit-wasm). The artifact has zero imports, so
/// instantiation is trivial — no wasm-bindgen glue anywhere. The
/// payload types are GENERATED from the Rust model (src/types, via
/// ts-rs) — the TS contract is the artifact's contract, never
/// hand-written.
///
/// Protocol notes (crates/transit-wasm/src/lib.rs has the full
/// contract): requests are JSON, results are
/// `[u32 LE length][payload]` buffers that the host reads then frees;
/// outcomes are `{"ok": ...}` or `{"error": ...}`.

import wasmUrl from "../pkg/transit_wasm.wasm?url";
import type { EditRecord } from "./types/EditRecord";
import type { EditReport } from "./types/EditReport";
import type { FeatureCollection } from "./types/FeatureCollection";
import type { FeedSummary } from "./types/FeedSummary";
import type { Itinerary } from "./types/Itinerary";
import type { Network } from "./types/Network";
import type { RouteSummary } from "./types/RouteSummary";
import type { PlanInfo } from "./types/PlanInfo";
import type { PlanIssue } from "./types/PlanIssue";
import type { PlanPeriod } from "./types/PlanPeriod";
import type { StopFeature } from "./types/StopFeature";
import type { StopSummary } from "./types/StopSummary";
import type { TripLineFeature } from "./types/TripLineFeature";

// Op codes for the bulk-bytes ABI (transit_ops::OP_LOAD_FEED etc.).
const OP_LOAD_FEED = 0;
const OP_COVERAGE_LOAD = 3;

/// The flat C ABI the artifact exports.
type WasmExports = {
  memory: WebAssembly.Memory;
  transit_alloc(len: number): number;
  transit_free(ptr: number, len: number): void;
  transit_invoke(ptr: number, len: number): number;
  transit_invoke_bytes(op: number, ptr: number, len: number): number;
};

let instance: WebAssembly.Instance | null = null;

function exports(): WasmExports {
  return instance?.exports as unknown as WasmExports;
}

/// Instantiate the artifact once (fetch is JS-owned I/O, per the
/// project's boundary rules).
async function instantiate(): Promise<WebAssembly.Instance> {
  if (instance) return instance;
  const bytes = await (await fetch(wasmUrl)).arrayBuffer();
  // Zero imports: every host is this easy.
  const { instance: inst } = await WebAssembly.instantiate(bytes, {});
  instance = inst;
  return inst;
}

type Ok = { ok: unknown };

function readResult(outPtr: number): unknown {
  const wasm = exports();
  const buffer = wasm.memory.buffer;
  const len = new DataView(buffer).getUint32(outPtr, true);
  const payload = new Uint8Array(buffer, outPtr + 4, len);
  const outcome = JSON.parse(new TextDecoder().decode(payload)) as
    | Ok
    | { error: string };
  wasm.transit_free(outPtr, len + 4);
  if ("error" in outcome) throw new Error(outcome.error);
  return outcome.ok;
}

function invoke(request: unknown): unknown {
  const wasm = exports();
  const payload = new TextEncoder().encode(JSON.stringify(request));
  const ptr = wasm.transit_alloc(payload.length);
  new Uint8Array(wasm.memory.buffer, ptr, payload.length).set(payload);
  const outPtr = wasm.transit_invoke(ptr, payload.length);
  wasm.transit_free(ptr, payload.length);
  return readResult(outPtr);
}

function invokeBytes(op: number, bytes: Uint8Array): unknown {
  const wasm = exports();
  const ptr = wasm.transit_alloc(bytes.length);
  new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
  const outPtr = wasm.transit_invoke_bytes(op, ptr, bytes.length);
  wasm.transit_free(ptr, bytes.length);
  return readResult(outPtr);
}

export class GtfsHandle {
  #feedId: number;

  constructor(feedId: number) {
    this.#feedId = feedId;
  }

  /// The registry id — hosts need it to address ops directly.
  feedId(): number {
    return this.#feedId;
  }

  static load(bytes: Uint8Array): GtfsHandle {
    const resp = invokeBytes(OP_LOAD_FEED, bytes) as { feed_id: number };
    return new GtfsHandle(resp.feed_id);
  }

  summary(): FeedSummary {
    return (invoke({ op: "summary", feed_id: this.#feedId }) as { summary: FeedSummary }).summary;
  }

  stops(): StopSummary[] {
    return (invoke({ op: "stops", feed_id: this.#feedId }) as { stops: StopSummary[] }).stops;
  }

  routes(): RouteSummary[] {
    return (invoke({ op: "routes", feed_id: this.#feedId }) as { routes: RouteSummary[] }).routes;
  }

  route_stop_sequence(routeId: string, directionId: number | null): StopSummary[] {
    return (invoke({
      op: "route_stop_sequence",
      feed_id: this.#feedId,
      route_id: routeId,
      direction_id: directionId,
    }) as { stops: StopSummary[] }).stops;
  }

  stops_geojson(): FeatureCollection {
    return (invoke({ op: "stops_geojson", feed_id: this.#feedId }) as { collection: FeatureCollection }).collection;
  }

  trip_path_geojson(tripId: string, date: string): TripLineFeature | null {
    return (invoke({
      op: "trip_path_geojson",
      feed_id: this.#feedId,
      trip_id: tripId,
      date,
    }) as { feature: TripLineFeature | null }).feature ?? null;
  }

  /// The route's alignment (representative trip, shapes when present)
  /// — the ghost the diversion editor draws before a detour.
  route_path_geojson(routeId: string, directionId: number | null, date: string): TripLineFeature | null {
    return (invoke({
      op: "route_path_geojson",
      feed_id: this.#feedId,
      route_id: routeId,
      direction_id: directionId,
      date,
    }) as { feature: TripLineFeature | null }).feature ?? null;
  }

  trip_segment_geojson(tripId: string, fromStopId: string, toStopId: string, date: string): TripLineFeature | null {
    return (invoke({
      op: "trip_segment_geojson",
      feed_id: this.#feedId,
      trip_id: tripId,
      from_stop_id: fromStopId,
      to_stop_id: toStopId,
      date,
    }) as { feature: TripLineFeature | null }).feature ?? null;
  }

  stop_point(stopId: string): StopFeature | null {
    return (invoke({
      op: "stop_point",
      feed_id: this.#feedId,
      stop_id: stopId,
    }) as { feature: StopFeature | null }).feature ?? null;
  }

  plan_trip(from: string, to: string, date: string, departSecs: number, maxLegs: number): Itinerary | null {
    return (invoke({
      op: "plan_trip",
      feed_id: this.#feedId,
      from,
      to,
      date,
      depart_secs: departSecs,
      max_legs: maxLegs,
    }) as { itinerary: Itinerary | null }).itinerary ?? null;
  }

  plan_trip_compare(
    from: string,
    to: string,
    date: string,
    departSecs: number,
    maxLegs: number,
  ): { active: Itinerary | null; baseline: Itinerary | null } {
    const resp = invoke({
      op: "plan_trip_compare",
      feed_id: this.#feedId,
      from,
      to,
      date,
      depart_secs: departSecs,
      max_legs: maxLegs,
    }) as { active: Itinerary | null; baseline: Itinerary | null };
    return { active: resp.active ?? null, baseline: resp.baseline ?? null };
  }

  set_diversions(records: EditRecord[]): EditReport[] {
    return (invoke({
      op: "set_diversions",
      feed_id: this.#feedId,
      records,
    }) as { reports: EditReport[] }).reports;
  }
}

// ---------------------------------------------------------------
// Street networks (detour authoring) live in the cap-streets
// capability now — see capabilities.svelte.ts (streetsCap).
// ---------------------------------------------------------------

/// Which service plan is in force on a date (the latest pick at or
/// before it; unversioned plans never resolve).
export function plans_resolve(plans: PlanInfo[], date: string): string | null {
  return (invoke({ op: "plans_resolve", plans, date }) as { plan: string | null }).plan;
}

/// Chain validity: unique plan ids, versioned plans effective-dated,
/// effective dates unique.
export function plans_validate(plans: PlanInfo[]): PlanIssue[] {
  return (invoke({ op: "plans_validate", plans }) as { issues: PlanIssue[] }).issues;
}

/// Every pick's governing period — [effective_from, until the next
/// pick) — derived, never stored.
export function plans_periods(plans: PlanInfo[]): PlanPeriod[] {
  return (invoke({ op: "plans_periods", plans }) as { periods: PlanPeriod[] }).periods;
}

export type CoverageLayer = {
  points: number;
  pop_field: string;
  total_pop: number;
};

/// Parse a population layer (GeoJSON points; the population property
/// auto-detected from the common spellings) for the import handle.
/// Parse-only: coverage computation lives in the analysis capability
/// (cap-analysis), which registers layers by content hash.
export function coverage_load(bytes: Uint8Array): CoverageLayer {
  return invokeBytes(OP_COVERAGE_LOAD, bytes) as CoverageLayer;
}

/// The API surface `loadWasm()` hands back — the whole artifact
/// boundary in one type.
export type WasmApi = {
  GtfsHandle: typeof GtfsHandle;
  plans_resolve: typeof plans_resolve;
  plans_validate: typeof plans_validate;
  plans_periods: typeof plans_periods;
  coverage_load: typeof coverage_load;
};

/// The entry point main.js awaits: instantiate the artifact, then
/// hand back the API surface the app expects.
export async function loadWasm(): Promise<WasmApi> {
  await instantiate();
  return {
    GtfsHandle,
    plans_resolve,
    plans_validate,
    plans_periods,
    coverage_load,
  };
}
