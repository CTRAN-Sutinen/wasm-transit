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
import type { CoverageStats } from "./types/CoverageStats";
import type { Departure } from "./types/Departure";
import type { EditRecord } from "./types/EditRecord";
import type { EditReport } from "./types/EditReport";
import type { FeatureCollection } from "./types/FeatureCollection";
import type { FeedInfo } from "./types/FeedInfo";
import type { FeedSummary } from "./types/FeedSummary";
import type { ImportNote } from "./types/ImportNote";
import type { Itinerary } from "./types/Itinerary";
import type { Network } from "./types/Network";
import type { NetworkError } from "./types/NetworkError";
import type { NetworkStats } from "./types/NetworkStats";
import type { BlockSummary } from "./types/BlockSummary";
import type { BlockTrip } from "./types/BlockTrip";
import type { RtOverview } from "./types/RtOverview";
import type { RouteSummary } from "./types/RouteSummary";
import type { ServiceTypeDetail } from "./types/ServiceTypeDetail";
import type { ServiceTypeSummary } from "./types/ServiceTypeSummary";
import type { PlanInfo } from "./types/PlanInfo";
import type { PlanIssue } from "./types/PlanIssue";
import type { PlanPeriod } from "./types/PlanPeriod";
import type { TypeCoverage } from "./types/TypeCoverage";
import type { StopFeature } from "./types/StopFeature";
import type { StopSummary } from "./types/StopSummary";
import type { StreetPoint } from "./types/StreetPoint";
import type { StreetPosition } from "./types/StreetPosition";
import type { TripLineFeature } from "./types/TripLineFeature";

// Op codes for the bulk-bytes ABI (transit_ops::OP_LOAD_FEED etc.).
const OP_LOAD_FEED = 0;
const OP_DECODE_RT = 1;
const OP_STREETS_LOAD = 2;
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

  /// The registry id — hosts need it to address ops directly (the
  /// planner's `network_import`, for instance).
  feedId(): number {
    return this.#feedId;
  }

  static load(bytes: Uint8Array): GtfsHandle {
    const resp = invokeBytes(OP_LOAD_FEED, bytes) as { feed_id: number };
    return new GtfsHandle(resp.feed_id);
  }

  /// An authored network document (transit-network) materialized into
  /// the same derived-feed view — same registry, same ops. Invalid
  /// documents throw (validate first via `network_validate`).
  static load_network(network: Network): GtfsHandle {
    const resp = invoke({ op: "network_load", network }) as { feed_id: number };
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

  departures(stopId: string, date: string, fromSecs: number, limit: number): Departure[] {
    return (invoke({
      op: "departures",
      feed_id: this.#feedId,
      stop_id: stopId,
      date,
      from_secs: fromSecs,
      limit,
    }) as { departures: Departure[] }).departures;
  }

  active_services(date: string): string[] {
    return (invoke({
      op: "active_services",
      feed_id: this.#feedId,
      date,
    }) as { services: string[] }).services;
  }

  /// Blocks, optionally scoped to a service date: with `date`, only
  /// blocks running that day, with that day's trip counts.
  blocks(date?: string): BlockSummary[] {
    return (invoke({ op: "blocks", feed_id: this.#feedId, date: date ?? null }) as {
      blocks: BlockSummary[];
    }).blocks;
  }

  block_trips(blockId: string, date: string): BlockTrip[] {
    return (invoke({
      op: "block_trips",
      feed_id: this.#feedId,
      block_id: blockId,
      date,
    }) as { trips: BlockTrip[] }).trips;
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

export function decode_rt(bytes: Uint8Array): RtOverview {
  return (invokeBytes(OP_DECODE_RT, bytes) as { overview: RtOverview }).overview;
}

// ---------------------------------------------------------------
// Street networks (detour authoring). Pure queries against the
// artifact's streets registry: bytes in once, snap/route after.
// ---------------------------------------------------------------

export type StreetsHandle = {
  streets_id: number;
  segments: number;
  nodes: number;
};

export function streets_load(bytes: Uint8Array): StreetsHandle {
  return invokeBytes(OP_STREETS_LOAD, bytes) as StreetsHandle;
}

export function street_snap(streetsId: number, lat: number, lon: number): StreetPosition | null {
  return (
    invoke({
      op: "street_snap",
      streets_id: streetsId,
      lat,
      lon,
    }) as { position: StreetPosition | null }
  ).position ?? null;
}

export function street_route(
  streetsId: number,
  from: StreetPosition,
  to: StreetPosition,
): StreetPoint[] {
  return (
    invoke({
      op: "street_route",
      streets_id: streetsId,
      from,
      to,
    }) as { points: StreetPoint[] }
  ).points;
}

/// Validate an authored network document without loading it — every
/// issue comes back for the authoring UI. Never throws.
export function network_validate(network: Network): NetworkError[] {
  return (invoke({ op: "network_validate", network }) as { issues: NetworkError[] }).issues;
}

/// Infer an authored network document from a loaded feed — the
/// planner's bootstrap path. Deterministic: the same feed bytes give
/// the same document everywhere. The notes list every resolution the
/// inference applied (reported, never silent).
export function network_import(feedId: number): { network: Network; notes: ImportNote[] } {
  return invoke({ op: "network_import", feed_id: feedId }) as {
    network: Network;
    notes: ImportNote[];
  };
}

/// The planner's analysis on one service date: trips, spans,
/// headways at the network, route, and stop levels — plus the
/// composed transfer-point view. Best-effort on a mid-edit document.
export function network_stats(network: Network, date: string): NetworkStats {
  return (invoke({ op: "network_stats", network, date }) as { stats: NetworkStats }).stats;
}

/// The planner's DESIGN axis: every service type's numbers, each on
/// its own representative day.
export function service_types_overview(network: Network): ServiceTypeSummary[] {
  return (invoke({ op: "service_types_overview", network }) as { service_types: ServiceTypeSummary[] })
    .service_types;
}

/// One service type expanded on its representative day: routes,
/// stops, schedules, totals — the drill-down.
export function service_type_detail(network: Network, serviceType: string): ServiceTypeDetail {
  return (invoke({ op: "service_type_detail", network, service_type: serviceType }) as {
    detail: ServiceTypeDetail;
  }).detail;
}

/// Population coverage per service type, each on its representative
/// day.
export function type_coverage(
  network: Network,
  layerId: number,
  radiusM: number,
  frequentSecs: number,
  minDepartures: number | null,
): TypeCoverage[] {
  return (
    invoke({
      op: "type_coverage",
      network,
      layer_id: layerId,
      radius_m: radiusM,
      frequent_secs: frequentSecs,
      min_departures: minDepartures,
    }) as { coverage: TypeCoverage[] }
  ).coverage;
}

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
  layer_id: number;
  points: number;
  pop_field: string;
  total_pop: number;
};

/// Parse and register a population layer (GeoJSON points; the
/// population property auto-detected from the common spellings).
export function coverage_load(bytes: Uint8Array): CoverageLayer {
  return invokeBytes(OP_COVERAGE_LOAD, bytes) as CoverageLayer;
}

/// Population coverage of a network document on a service date.
export function coverage_stats(
  network: Network,
  date: string,
  layerId: number,
  radiusM: number,
  frequentSecs: number,
  minDepartures: number | null,
): CoverageStats {
  return (
    invoke({
      op: "coverage_stats",
      network,
      date,
      layer_id: layerId,
      radius_m: radiusM,
      frequent_secs: frequentSecs,
      min_departures: minDepartures,
    }) as { stats: CoverageStats }
  ).stats;
}

/// The API surface `loadWasm()` hands back — the whole artifact
/// boundary in one type.
export type WasmApi = {
  GtfsHandle: typeof GtfsHandle;
  decode_rt: typeof decode_rt;
  network_validate: typeof network_validate;
  network_import: typeof network_import;
  network_stats: typeof network_stats;
  service_types_overview: typeof service_types_overview;
  service_type_detail: typeof service_type_detail;
  type_coverage: typeof type_coverage;
  plans_resolve: typeof plans_resolve;
  plans_validate: typeof plans_validate;
  plans_periods: typeof plans_periods;
  coverage_load: typeof coverage_load;
  coverage_stats: typeof coverage_stats;
  streets_load: typeof streets_load;
  street_snap: typeof street_snap;
  street_route: typeof street_route;
};

/// The entry point main.js awaits: instantiate the artifact, then
/// hand back the API surface the app expects.
export async function loadWasm(): Promise<WasmApi> {
  await instantiate();
  return {
    GtfsHandle,
    decode_rt,
    network_validate,
    network_import,
    network_stats,
    service_types_overview,
    service_type_detail,
    type_coverage,
    plans_resolve,
    plans_validate,
    plans_periods,
    coverage_load,
    coverage_stats,
    streets_load,
    street_snap,
    street_route,
  };
}
