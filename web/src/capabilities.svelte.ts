/// The capability bus: the host-side resolver.
///
/// Capabilities advertise (the manifest); consumers declare needs;
/// this module executes — demand-driven load, hash verification at
/// the seam, per-capability status for partial assemblies. There is
/// no orchestration PROCEDURE here, only declarations plus a dumb
/// executor: the manifest describes the assembly, screens ask for
/// what they need, and everything with domain judgment stays inside
/// capability modules (where the gates live).
///
/// Composition happens through documents: JS holds the bytes (it
/// already owns the documents by the project's ownership rules),
/// loads them into each capability that needs them, and the modules
/// agree on identity via the returned content hash — asserted, not
/// assumed.

import type { CapManifest } from "./types/cap_abi/CapManifest";
import type { Capability } from "./types/cap_abi/Capability";

import type { Request as GtfsRequest } from "./types/cap_gtfs/Request";
import type { Response as GtfsResponse } from "./types/cap_gtfs/Response";
import type { Request as BoardCapRequest } from "./types/cap_board/Request";
import type { Response as BoardCapResponse } from "./types/cap_board/Response";
import type { Request as RuncutCapRequest } from "./types/cap_runcut/Request";
import type { Response as RuncutCapResponse } from "./types/cap_runcut/Response";
import type { Request as CalendarCapRequest } from "./types/cap_calendar/Request";
import type { Response as CalendarCapResponse } from "./types/cap_calendar/Response";
import type { Request as DirectoryCapRequest } from "./types/cap_directory/Request";
import type { Response as DirectoryCapResponse } from "./types/cap_directory/Response";
import type { Request as PlansCapRequest } from "./types/cap_plans/Request";
import type { Response as PlansCapResponse } from "./types/cap_plans/Response";
import type { PlanInfo } from "./types/cap_plans/PlanInfo";
import type { PlanIssue } from "./types/cap_plans/PlanIssue";
import type { PlanPeriod } from "./types/cap_plans/PlanPeriod";
import type { Request as AuthorCapRequest } from "./types/cap_author/Request";
import type { Response as AuthorCapResponse } from "./types/cap_author/Response";
import type { Request as DivertCapRequest } from "./types/cap_divert/Request";
import type { Response as DivertCapResponse } from "./types/cap_divert/Response";
import type { DiversionDraft } from "./types/DiversionDraft";
import type { DiversionRow } from "./types/DiversionRow";
import type { DraftView } from "./types/DraftView";
import type { Coord } from "./types/Coord";
import type { EntityFocus } from "./types/EntityFocus";
import type { EntityKind } from "./types/EntityKind";
import type { LegPath } from "./types/LegPath";
import type { NetworkEdit } from "./types/NetworkEdit";
import type { NetworkWarning } from "./types/NetworkWarning";
import type { CreatedEntity } from "./types/CreatedEntity";
import type { Request as SyncCapRequest } from "./types/cap_sync/Request";
import type { Response as SyncCapResponse } from "./types/cap_sync/Response";
import type { DirectoryStop } from "./types/cap_directory/DirectoryStop";
import type { DirectoryRoute } from "./types/cap_directory/DirectoryRoute";
import type { PatternStops } from "./types/cap_directory/PatternStops";
import type { Watchlist } from "./types/cap_board/Watchlist";
import type { MigrationNote } from "./types/cap_board/MigrationNote";
import type { WatchBoardStop } from "./types/cap_board/WatchBoardStop";
import type { Request as RouterRequest } from "./types/cap_router/Request";
import type { Response as RouterResponse } from "./types/cap_router/Response";
import type { Request as AnalysisRequest } from "./types/cap_analysis/Request";
import type { Response as AnalysisResponse } from "./types/cap_analysis/Response";
import type { Request as NetworkCapRequest } from "./types/cap_network/Request";
import type { Response as NetworkCapResponse } from "./types/cap_network/Response";
import type { Request as RtCapRequest } from "./types/cap_rt/Request";
import type { Response as RtCapResponse } from "./types/cap_rt/Response";
import type { Request as StreetsCapRequest } from "./types/cap_streets/Request";
import type { Response as StreetsCapResponse } from "./types/cap_streets/Response";
import type { RtOverview } from "./types/RtOverview";
import type { StreetPoint } from "./types/StreetPoint";
import type { StreetPosition } from "./types/StreetPosition";
import type { TripIdMap } from "./types/TripIdMap";
import type { StopBoard } from "./types/StopBoard";
import type { LiveDay } from "./types/LiveDay";
import type { LiveSummary } from "./types/LiveSummary";
import type { BlockChainTrip } from "./types/BlockChainTrip";
import type { DayBlock } from "./types/DayBlock";
import type { TripStopView } from "./types/TripStopView";
import type { EditRecord } from "./types/EditRecord";
import type { EditReport } from "./types/EditReport";
import type { Network } from "./types/Network";
import type { NetworkError } from "./types/NetworkError";
import type { NetworkStats } from "./types/NetworkStats";
import type { ImportNote } from "./types/ImportNote";
import type { ServiceTypeDetail } from "./types/ServiceTypeDetail";
import type { ServiceTypeSummary } from "./types/ServiceTypeSummary";
import type { TypeCoverage } from "./types/TypeCoverage";
import type { FeedInfo } from "./types/FeedInfo";
import type { FeedSummary } from "./types/FeedSummary";
import type { Itinerary } from "./types/Itinerary";
import type { StopSummary } from "./types/StopSummary";
import { get as idbGet, set as idbSet } from "idb-keyval";

/// The flat C ABI every capability module exports (identical to the
/// monolith's — a capability differs in which ops it answers, never
/// in how it is called).
type CapExports = {
  memory: WebAssembly.Memory;
  transit_alloc(len: number): number;
  transit_free(ptr: number, len: number): void;
  transit_invoke(ptr: number, len: number): number;
  transit_invoke_bytes(op: number, ptr: number, len: number): number;
};

export type CapStatus = "unavailable" | "loading" | "ready";

const KERNEL_VERSION = 1;

/// One loaded capability module: the bytes were fetched, verified
/// against the manifest's hash, and instantiated. Nothing else.
class CapModule {
  readonly cap: Capability;
  readonly #exports: CapExports;

  constructor(cap: Capability, exports: WebAssembly.Exports) {
    this.cap = cap;
    this.#exports = exports as unknown as CapExports;
  }

  invoke(
    request:
      | GtfsRequest
      | RouterRequest
      | AnalysisRequest
      | NetworkCapRequest
      | BoardCapRequest
      | RuncutCapRequest
      | CalendarCapRequest
      | DirectoryCapRequest
      | RtCapRequest
      | StreetsCapRequest
      | PlansCapRequest
      | SyncCapRequest
      | AuthorCapRequest
      | DivertCapRequest,
  ): GtfsResponse | RouterResponse | AnalysisResponse | NetworkCapResponse | BoardCapResponse | RuncutCapResponse | CalendarCapResponse | DirectoryCapResponse | RtCapResponse | StreetsCapResponse | PlansCapResponse | SyncCapResponse | AuthorCapResponse | DivertCapResponse {
    const wasm = this.#exports;
    const payload = new TextEncoder().encode(JSON.stringify(request));
    const ptr = wasm.transit_alloc(payload.length);
    new Uint8Array(wasm.memory.buffer, ptr, payload.length).set(payload);
    const outPtr = wasm.transit_invoke(ptr, payload.length);
    wasm.transit_free(ptr, payload.length);
    return this.#read(outPtr) as
      | GtfsResponse
      | RouterResponse
      | AnalysisResponse
      | NetworkCapResponse
      | BoardCapResponse
      | RuncutCapResponse
      | CalendarCapResponse
      | DirectoryCapResponse
      | RtCapResponse
      | StreetsCapResponse
      | PlansCapResponse
      | SyncCapResponse
      | AuthorCapResponse;
  }

  invokeBytes(
    op: number,
    bytes: Uint8Array,
  ): GtfsResponse | RouterResponse | AnalysisResponse | NetworkCapResponse | RtCapResponse | StreetsCapResponse {
    const wasm = this.#exports;
    const ptr = wasm.transit_alloc(bytes.length);
    new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
    const outPtr = wasm.transit_invoke_bytes(op, ptr, bytes.length);
    wasm.transit_free(ptr, bytes.length);
    return this.#read(outPtr) as
      | GtfsResponse
      | RouterResponse
      | AnalysisResponse
      | NetworkCapResponse
      | RtCapResponse
      | StreetsCapResponse;
  }

  #read(outPtr: number): unknown {
    const wasm = this.#exports;
    const len = new DataView(wasm.memory.buffer).getUint32(outPtr, true);
    const payload = new Uint8Array(wasm.memory.buffer, outPtr + 4, len);
    const outcome = JSON.parse(new TextDecoder().decode(payload)) as
      | { ok: unknown }
      | { error: string };
    wasm.transit_free(outPtr, len + 4);
    if ("error" in outcome) throw new Error(outcome.error);
    return outcome.ok;
  }
}

/// sha256 of bytes as "sha256:<hex>" — the manifest's format, same
/// formula the generator and the Rust side use.
export async function contentHash(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const hex = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
  return "sha256:" + hex;
}

/// The assembly state. Screens render from this; the resolver is the
/// only writer.
export const caps = $state({
  manifest: null as CapManifest | null,
  statuses: {} as Record<string, CapStatus>,
  errors: {} as Record<string, string>,
  modules: {} as Record<string, CapModule>,
});

/// Load the manifest and validate it AT THE SEAM: wrong kernel, no
/// capabilities, or duplicate ids are assembly errors, not runtime
/// surprises.
export async function loadManifest(): Promise<CapManifest> {
  const manifest = (await (await fetch("caps/manifest.json")).json()) as CapManifest;
  if (manifest.kernel !== KERNEL_VERSION) {
    throw new Error(`manifest kernel v${manifest.kernel} != host kernel v${KERNEL_VERSION}`);
  }
  const ids = new Set<string>();
  for (const cap of manifest.capabilities) {
    if (ids.has(cap.id)) throw new Error(`duplicate capability id ${cap.id}`);
    if (!cap.hash.startsWith("sha256:")) throw new Error(`capability ${cap.id} has no content hash`);
    ids.add(cap.id);
    caps.statuses[cap.id] = "unavailable";
  }
  caps.manifest = manifest;
  return manifest;
}

const loading = new Map<string, Promise<CapModule | null>>();

/// Ensure a set of capabilities, demand-driven and memoized. Fetches
/// by manifest url, VERIFIES the bytes against the pinned hash, and
/// instantiates. A failed capability resolves null and is marked
/// unavailable — the assembly degrades per capability, never fails
/// wholesale.
export async function ensure(ids: string[]): Promise<Record<string, CapModule | null>> {
  if (!caps.manifest) await loadManifest();
  const byId = new Map((caps.manifest?.capabilities ?? []).map((c) => [c.id, c]));
  const out: Record<string, CapModule | null> = {};
  for (const id of ids) {
    const cap = byId.get(id);
    if (!cap) throw new Error(`manifest has no capability ${id}`);
    out[id] = await ensureOne(cap);
  }
  return out;
}

function ensureOne(cap: Capability): Promise<CapModule | null> {
  const existing = caps.modules[cap.id];
  if (existing) return Promise.resolve(existing);
  const pending = loading.get(cap.id);
  if (pending) return pending;

  const load = (async () => {
    caps.statuses[cap.id] = "loading";
    try {
      const bytes = new Uint8Array(await (await fetch(cap.url)).arrayBuffer());
      // Integrity at the seam: the manifest pins the exact bytes.
      const hash = await contentHash(bytes);
      if (hash !== cap.hash) {
        throw new Error(`capability ${cap.id} hash mismatch: fetched ${hash}, manifest pins ${cap.hash}`);
      }
      const { instance } = await WebAssembly.instantiate(bytes, {});
      const module = new CapModule(cap, instance.exports);
      caps.modules[cap.id] = module;
      caps.statuses[cap.id] = "ready";
      return module;
    } catch (error) {
      caps.statuses[cap.id] = "unavailable";
      caps.errors[cap.id] = error instanceof Error ? error.message : String(error);
      return null;
    } finally {
      loading.delete(cap.id);
    }
  })();
  loading.set(cap.id, load);
  return load;
}

/// Forget a loaded capability (the demo's degraded-state simulation):
/// the next `ensure` re-fetches it — against whatever url the
/// manifest now carries.
export function forgetCapability(id: string): void {
  delete caps.modules[id];
  caps.statuses[id] = "unavailable";
}

// ---------------------------------------------------------------------
// The document store: versioned local data, immutable history.
//
// A migration never destroys: the migrated document is a NEW record
// (its own content hash) whose `produced_by` names the record it came
// from. `current` is the only mutable thing — a pointer, moved
// forward by migrations and back by rollback. The whole store
// persists to IndexedDB as plain data (the project's persistence
// rule: state survives reloads; proxies never cross).
// ---------------------------------------------------------------------

export type DocRecord = {
  hash: string;
  version: number;
  /// Canonical JSON of the document.
  bytes: string;
  /// How this version came to exist. Absent = authored/seeded.
  produced_by?: { op: "migrate"; from: string; by: string };
};

const DOC_KEY = "caps-documents";

type DocKinds = Record<string, { records: Record<string, DocRecord>; current: string | null }>;

export const documents = $state({
  kinds: {} as DocKinds,
  restored: false,
});

async function persistDocuments(): Promise<void> {
  await idbSet(DOC_KEY, JSON.parse(JSON.stringify($state.snapshot(documentData()))));
}

function documentData(): { kinds: DocKinds } {
  return { kinds: documents.kinds };
}

export async function restoreDocuments(): Promise<void> {
  if (documents.restored) return;
  const stored = await idbGet<DocKinds>(DOC_KEY);
  if (stored) documents.kinds = stored;
  documents.restored = true;
}

/// Hash a document's canonical bytes (the string IS the bytes here —
/// documents cross the ABI as JSON).
async function hashDocument(bytes: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(bytes));
  const hex = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
  return "sha256:" + hex;
}

/// Store a document version; idempotent by content hash. Seeding with
/// the same bytes twice lands on the same record.
export async function putDocument(kind: string, bytes: string, version: number): Promise<DocRecord> {
  await restoreDocuments();
  const hash = await hashDocument(bytes);
  const record: DocRecord = { hash, version, bytes };
  const entry = (documents.kinds[kind] ??= { records: {}, current: null });
  entry.records[hash] = record;
  entry.current = hash;
  await persistDocuments();
  return record;
}

export function currentDocument(kind: string): DocRecord | null {
  const entry = documents.kinds[kind];
  if (!entry?.current) return null;
  return entry.records[entry.current] ?? null;
}

/// The chain of versions from current back to the seed (the
/// `produced_by` links). Rollback walks this list.
export function documentHistory(kind: string): DocRecord[] {
  const chain: DocRecord[] = [];
  let record = currentDocument(kind);
  while (record) {
    chain.push(record);
    const from = record.produced_by?.from;
    record = from ? (documents.kinds[kind]?.records[from] ?? null) : null;
  }
  return chain;
}

/// Roll back: move the current pointer one step back along the
/// produced_by chain. The forward record stays addressable — nothing
/// is destroyed, history is just re-pointed.
export async function rollbackDocument(kind: string): Promise<DocRecord | null> {
  const entry = documents.kinds[kind];
  const current = currentDocument(kind);
  const from = current?.produced_by?.from;
  if (!entry || !from) return null;
  entry.current = from;
  await persistDocuments();
  return entry.records[from] ?? null;
}

/// Version arithmetic, host-side and domain-free: can this capability
/// read the current document, or migrate it to a version it speaks?
export function documentPlan(kind: string, cap: Capability): { readable: boolean; migration: boolean } {
  const current = currentDocument(kind);
  if (!current) return { readable: false, migration: false };
  const spoken = cap.speaks[kind] ?? [];
  if (spoken.includes(current.version)) return { readable: true, migration: false };
  const steps = cap.migrates[kind] ?? [];
  const bridged = steps.some((step) => step.from === current.version && spoken.includes(step.to));
  return { readable: bridged, migration: bridged };
}

// ---------------------------------------------------------------------
// Typed per-capability wrappers — components never see the ABI.
// ---------------------------------------------------------------------

const OP_LOAD_FEED = 0;

export type GtfsLoadFeed = { feedHash: string; summary: FeedSummary };

const gtfsFeeds: Record<string, Map<string, string>> = {};

/// The current gtfs module, if the bus assembled one.
export async function gtfsModule(): Promise<CapModule | null> {
  const modules = await ensure(["gtfs"]);
  return modules.gtfs ?? null;
}

/// Register the active feed in cap-gtfs, memoized by the bytes'
/// content hash — the station board's fallback queries it by hash.
/// THE MODULE IS THE HASH AUTHORITY (its returned feed_hash is the
/// identity; the memo key is only the host's "did I already load
/// these bytes" check).
export async function ensureGtfsFeed(bytes: Uint8Array): Promise<string | null> {
  const module = await gtfsModule();
  if (!module) return null;
  const memoKey = await contentHash(bytes as Uint8Array<ArrayBuffer>);
  const memo = (gtfsFeeds[module.cap.id] ??= new Map());
  if (!memo.has(memoKey)) {
    memo.set(memoKey, gtfsCap(module).loadFeed(bytes).feedHash);
  }
  return memo.get(memoKey) ?? null;
}

export function gtfsCap(module: CapModule) {
  return {
    loadFeed(bytes: Uint8Array): GtfsLoadFeed {
      const resp = module.invokeBytes(OP_LOAD_FEED, bytes) as Extract<
        GtfsResponse,
        { op: "load_feed" }
      >;
      return { feedHash: resp.feed_hash, summary: resp.summary };
    },
    /// The feed's own identity (version, publisher).
    feedInfo(feedHash: string): FeedInfo | null {
      const resp = module.invoke({
        op: "feed_info",
        feed_hash: feedHash,
      } as GtfsRequest) as Extract<GtfsResponse, { op: "feed_info" }>;
      return resp.info;
    },
  };
}

/// Migrate the current watchlist forward through a capability's
/// migration op: the module produces the new document, the STORE keeps
/// both — the new record's produced_by names the old one, and
/// rollback is a pointer move. Returns the new record and the loss
/// notes (never silent).
export async function migrateWatchlistForward(
  capabilityId: string,
): Promise<{ record: DocRecord; notes: MigrationNote[] } | { error: string }> {
  const module = caps.modules[capabilityId];
  const current = currentDocument("watchlist");
  if (!module || !current) return { error: "no watchlist or capability loaded" };
  const document = JSON.parse(current.bytes) as Watchlist;
  const board = boardCap(module);
  const { document: migrated, notes } = board.migrateWatchlist(document);
  const canonical = JSON.stringify(migrated);
  const hash = await hashDocument(canonical);
  const entry = (documents.kinds["watchlist"] ??= { records: {}, current: null });
  const record: DocRecord = {
    hash,
    version: migrated.version === "v2" ? 2 : 1,
    bytes: canonical,
    produced_by: { op: "migrate", from: current.hash, by: capabilityId },
  };
  entry.records[hash] = record;
  entry.current = hash;
  await persistDocuments();
  return { record, notes };
}

// ---------------------------------------------------------------------
// The analysis capability — the first REAL capability extracted from
// the monolith (Stage B): the planner's screens read their numbers
// through the bus when it assembles, and fall back to the monolith
// when it doesn't. Documents are registered ONCE per edit (hash-
// memoized); every query addresses the hash — the fix for per-op
// whole-document serialization, on the hot authoring path.
// ---------------------------------------------------------------------

const OP_LOAD_LAYER = 0;

export function analysisCap(module: CapModule) {
  return {
    networkStats(networkHash: string, date: string): NetworkStats {
      const resp = module.invoke({
        op: "network_stats",
        network_hash: networkHash,
        date,
        // The day-summary surface wants the numbers; the screens
        // below carry the routes/stops detail.
        include_routes: false,
        include_stops: false,
        include_service_types: false,
      } as AnalysisRequest) as Extract<AnalysisResponse, { op: "network_stats" }>;
      return resp.stats;
    },
    serviceTypesOverview(networkHash: string): ServiceTypeSummary[] {
      const resp = module.invoke({
        op: "service_types_overview",
        network_hash: networkHash,
      } as AnalysisRequest) as Extract<AnalysisResponse, { op: "service_types_overview" }>;
      return resp.service_types;
    },
    serviceTypeDetail(networkHash: string, serviceType: string): ServiceTypeDetail {
      const resp = module.invoke({
        op: "service_type_detail",
        network_hash: networkHash,
        service_type: serviceType,
      } as AnalysisRequest) as Extract<AnalysisResponse, { op: "service_type_detail" }>;
      return resp.detail;
    },
    typeCoverage(
      networkHash: string,
      layerHash: string,
      radiusM: number,
      frequentSecs: number,
      minDepartures: number,
    ): TypeCoverage[] {
      const resp = module.invoke({
        op: "type_coverage",
        network_hash: networkHash,
        layer_hash: layerHash,
        radius_m: radiusM,
        frequent_secs: frequentSecs,
        min_departures: minDepartures,
      } as AnalysisRequest) as Extract<AnalysisResponse, { op: "type_coverage" }>;
      return resp.coverage;
    },
  };
}

/// Register the current network document in cap-analysis, memoized —
/// the document crosses the seam once per edit, not once per query.
/// THE MODULE IS THE HASH AUTHORITY: its canonical serialization
/// decides the document's identity (the host's JSON.stringify can
/// differ in float formatting, so the host never mints its own).
export async function ensureAnalysisNetwork(document: Network): Promise<string | null> {
  const module = await analysisModule();
  if (!module) return null;
  const memoKey = await hashDocument(JSON.stringify(document));
  const memo = (analysisNetworks[module.cap.id] ??= new Map());
  if (!memo.has(memoKey)) {
    const resp = module.invoke({
      op: "load_network",
      network: document,
    } as AnalysisRequest) as Extract<AnalysisResponse, { op: "load_network" }>;
    memo.set(memoKey, resp.network_hash);
  }
  return memo.get(memoKey) ?? null;
}

/// The layer registration answer — the whole import handle: the
/// content hash coverage addresses the layer by, plus the field the
/// parse picked and the people it found (what the import UI shows).
export type AnalysisLayerHandle = {
  layer_hash: string;
  points: number;
  pop_field: string;
  total_pop: number;
};

/// Register a population layer in cap-analysis, memoized like
/// networks. Same rule: the module's returned hash is the identity.
export async function ensureAnalysisLayer(
  bytes: Uint8Array,
): Promise<AnalysisLayerHandle | null> {
  const module = await analysisModule();
  if (!module) return null;
  const memoKey = await contentHash(bytes as Uint8Array<ArrayBuffer>);
  const memo = (analysisLayers[module.cap.id] ??= new Map());
  if (!memo.has(memoKey)) {
    const resp = module.invokeBytes(OP_LOAD_LAYER, bytes) as Extract<
      AnalysisResponse,
      { op: "load_population_layer" }
    >;
    memo.set(memoKey, {
      layer_hash: resp.layer_hash,
      points: Number(resp.points),
      pop_field: resp.pop_field,
      total_pop: resp.total_pop,
    });
  }
  return memo.get(memoKey) ?? null;
}

const analysisNetworks: Record<string, Map<string, string>> = {};
const analysisLayers: Record<string, Map<string, AnalysisLayerHandle>> = {};

/// The current analysis module, if the bus assembled one.
export async function analysisModule(): Promise<CapModule | null> {
  const modules = await ensure(["analysis"]);
  return modules.analysis ?? null;
}

export function routerCap(module: CapModule) {
  return {
    loadNetwork(document: Network): string {
      const resp = module.invoke({
        op: "load_network",
        network: document,
      } as RouterRequest) as Extract<RouterResponse, { op: "load_network" }>;
      return resp.network_hash;
    },
    loadEdits(
      networkHash: string,
      records: EditRecord[],
    ): { editsHash: string; reports: EditReport[] } {
      const resp = module.invoke({
        op: "load_edits",
        network_hash: networkHash,
        records,
      } as RouterRequest) as Extract<RouterResponse, { op: "load_edits" }>;
      return { editsHash: resp.edits_hash, reports: resp.reports };
    },
    plan(
      networkHash: string,
      editsHash: string | null,
      from: string,
      to: string,
      date: string,
      departSecs: number,
      maxLegs: number,
    ): Itinerary | null {
      const resp = module.invoke({
        op: "plan",
        network_hash: networkHash,
        edits_hash: editsHash,
        from,
        to,
        date,
        depart_secs: departSecs,
        max_legs: maxLegs,
      } as RouterRequest) as Extract<RouterResponse, { op: "plan" }>;
      return resp.itinerary;
    },
    planCompare(
      networkHash: string,
      editsHash: string | null,
      from: string,
      to: string,
      date: string,
      departSecs: number,
      maxLegs: number,
    ): { active: Itinerary | null; baseline: Itinerary | null } {
      const resp = module.invoke({
        op: "plan_compare",
        network_hash: networkHash,
        edits_hash: editsHash,
        from,
        to,
        date,
        depart_secs: departSecs,
        max_legs: maxLegs,
      } as RouterRequest) as Extract<RouterResponse, { op: "plan_compare" }>;
      return { active: resp.active, baseline: resp.baseline };
    },
    /// Where an itinerary runs, one path per leg, over the effective
    /// day it was planned on (the same edits hash).
    itineraryPaths(networkHash: string, editsHash: string | null, date: string, itinerary: Itinerary): LegPath[] {
      const resp = module.invoke({
        op: "itinerary_paths",
        network_hash: networkHash,
        edits_hash: editsHash,
        date,
        itinerary,
      } as RouterRequest) as Extract<RouterResponse, { op: "itinerary_paths" }>;
      return resp.paths;
    },
  };
}

/// The current router module, if the bus assembled one.
export async function routerModule(): Promise<CapModule | null> {
  const modules = await ensure(["router"]);
  return modules.router ?? null;
}

/// The composition law, host-side: load the same document bytes into
/// two capabilities and assert they agree on its identity. This is
/// the moment "same document, same truth" stops being a convention —
/// it is a check the host performs at every composition.
export function assertSameDocument(a: GtfsLoadFeed, b: GtfsLoadFeed, kind = "feed"): void {
  if (a.feedHash !== b.feedHash) {
    throw new Error(`${kind} identity disagrees across capabilities: ${a.feedHash} != ${b.feedHash}`);
  }
}

// ---------------------------------------------------------------------
// The network capability — the second REAL capability extracted from
// the monolith (the codec: import / validate / export). The planner's
// bootstrap, authoring feedback loop, and publish path all cross the
// bus now; when the capability cannot assemble the planner degrades
// loudly (bootstrap and publish refuse with an honest status,
// validation turns off) — never silently.
// ---------------------------------------------------------------------

export type NetworkImport = {
  network: Network;
  notes: ImportNote[];
  networkHash: string;
};

export function networkCap(module: CapModule) {
  return {
    loadFeed(bytes: Uint8Array): GtfsLoadFeed {
      const resp = module.invokeBytes(OP_LOAD_FEED, bytes) as Extract<
        NetworkCapResponse,
        { op: "load_feed" }
      >;
      return { feedHash: resp.feed_hash, summary: resp.summary };
    },
    import(feedHash: string): NetworkImport {
      const resp = module.invoke({
        op: "import",
        feed_hash: feedHash,
      } as NetworkCapRequest) as Extract<NetworkCapResponse, { op: "import" }>;
      return {
        network: resp.network,
        notes: resp.notes,
        networkHash: resp.network_hash,
      };
    },
    loadDocument(network: Network): string {
      const resp = module.invoke({
        op: "load_network",
        network,
      } as NetworkCapRequest) as Extract<NetworkCapResponse, { op: "load_network" }>;
      return resp.network_hash;
    },
    validate(networkHash: string): NetworkError[] {
      const resp = module.invoke({
        op: "validate",
        network_hash: networkHash,
      } as NetworkCapRequest) as Extract<NetworkCapResponse, { op: "validate" }>;
      return resp.issues;
    },
    export(networkHash: string): { feedHash: string; summary: FeedSummary; zipB64: string } {
      const resp = module.invoke({
        op: "export",
        network_hash: networkHash,
      } as NetworkCapRequest) as Extract<NetworkCapResponse, { op: "export" }>;
      return { feedHash: resp.feed_hash, summary: resp.summary, zipB64: resp.zip_b64 };
    },
    /// The import's trip-id map: derived (schedule, start) ↔ agency
    /// trip ids. None for an authored (non-imported) network.
    /// The agency trip ids behind derived trips, in the order asked.
    agencyTripIds(networkHash: string, trips: { schedule: string; start_secs: number }[]): string[][] {
      const resp = module.invoke({
        op: "agency_trip_ids",
        network_hash: networkHash,
        trips,
      } as NetworkCapRequest) as Extract<NetworkCapResponse, { op: "agency_trip_ids" }>;
      return resp.ids;
    },
    tripMap(networkHash: string): TripIdMap | null {
      const resp = module.invoke({
        op: "trip_map",
        network_hash: networkHash,
      } as NetworkCapRequest) as Extract<NetworkCapResponse, { op: "trip_map" }>;
      return resp.map ?? null;
    },
  };
}

/// The current network module, if the bus assembled one.
export async function networkModule(): Promise<CapModule | null> {
  const modules = await ensure(["network"]);
  return modules.network ?? null;
}

const networkDocuments: Record<string, Map<string, string>> = {};

/// Register the current network document in cap-network, memoized —
/// the document crosses the seam once per edit, not once per query.
/// THE MODULE IS THE HASH AUTHORITY: its canonical serialization
/// decides the document's identity (the host never mints its own).
export async function ensureNetworkDocument(document: Network): Promise<string | null> {
  const module = await networkModule();
  if (!module) return null;
  const memoKey = await hashDocument(JSON.stringify(document));
  const memo = (networkDocuments[module.cap.id] ??= new Map());
  if (!memo.has(memoKey)) {
    memo.set(memoKey, networkCap(module).loadDocument(document));
  }
  return memo.get(memoKey) ?? null;
}

/// Standard base64 → bytes (the export op's payload crosses the JSON
/// seam encoded; the host decodes and holds the bytes).
export function base64Decode(text: string): Uint8Array {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

// ---------------------------------------------------------------------
// The realtime capability — the third REAL capability extracted from
// the monolith (the decoder): live delays and vehicles from GTFS-rt
// protobuf bytes. A pure decoder over ephemeral payloads; when it
// cannot assemble, live data is off and the app says so (the board
// keeps its schedule times, the map keeps its last good positions).
// ---------------------------------------------------------------------

const OP_RT_DECODE = 0;
const OP_RT_LOAD_FEED = 1;

export type Live = { liveHash: string; live: LiveDay; summary: LiveSummary };

export function rtCap(module: CapModule) {
  return {
    /// Decode a payload; the module registers the snapshot under
    /// `rtHash` for `live` to address.
    decode(bytes: Uint8Array): { overview: RtOverview; rtHash: string } {
      const resp = module.invokeBytes(OP_RT_DECODE, bytes) as Extract<
        RtCapResponse,
        { op: "decode" }
      >;
      return { overview: resp.overview, rtHash: resp.rt_hash };
    },
    loadFeed(bytes: Uint8Array): string {
      const resp = module.invokeBytes(OP_RT_LOAD_FEED, bytes) as Extract<
        RtCapResponse,
        { op: "load_feed" }
      >;
      return resp.feed_hash;
    },
    loadTripMap(tripMap: TripIdMap): string {
      const resp = module.invoke({ op: "load_trip_map", trip_map: tripMap } as RtCapRequest) as Extract<
        RtCapResponse,
        { op: "load_trip_map" }
      >;
      return resp.trip_map_hash;
    },
    /// THE live document: the snapshot(s) resolved into model
    /// vocabulary, classified under the module's default window.
    live(feedHash: string, rtHash: string | null, vpHash: string | null, tripMapHash: string): Live {
      const resp = module.invoke({
        op: "live",
        feed_hash: feedHash,
        rt_hash: rtHash ?? undefined,
        vp_hash: vpHash ?? undefined,
        trip_map_hash: tripMapHash,
      } as RtCapRequest) as Extract<RtCapResponse, { op: "live" }>;
      return { liveHash: resp.live_hash, live: resp.live, summary: resp.summary };
    },
  };
}

/// The current realtime module, if the bus assembled one.
export async function rtModule(): Promise<CapModule | null> {
  const modules = await ensure(["rt"]);
  return modules.rt ?? null;
}

// ---------------------------------------------------------------------
// The day-view capabilities — the need cut of the day views, each one
// question over the NETWORK document (the core data model) plus the
// OPERATOR EDIT document:
//   * board   — when does service leave this stop? (a stop, a
//     station's pooled bays, a personal saved set: the watchlist)
//   * runcut  — what does this vehicle do today? (the day's blocks
//     list, one block's chain with the layover ripple)
//   * calendar— which service keys run today? (from the effective day)
// When an assembly cannot form, the views that need it degrade
// loudly. The boarding chain (feed → import → day-view module) is
// the bus's composition law in miniature: the feed loads into
// cap-network, import produces the network document, and the SAME
// document loads into each day-view module — whose returned hash is
// asserted equal to the import's. One document, one truth, checked
// at the host.
// ---------------------------------------------------------------------

export function boardCap(module: CapModule) {
  return {
    loadNetwork(network: Network): string {
      const resp = module.invoke({
        op: "load_network",
        network,
      } as BoardCapRequest) as Extract<BoardCapResponse, { op: "load_network" }>;
      return resp.network_hash;
    },
    loadEdits(
      networkHash: string,
      records: EditRecord[],
    ): { editsHash: string; reports: EditReport[] } {
      const resp = module.invoke({
        op: "load_edits",
        network_hash: networkHash,
        records,
      } as BoardCapRequest) as Extract<BoardCapResponse, { op: "load_edits" }>;
      return { editsHash: resp.edits_hash, reports: resp.reports };
    },
    /// Register a live document (cap-rt's `live`) — the same
    /// identity rt answered.
    loadLive(live: LiveDay): string {
      const resp = module.invoke({ op: "load_live", live } as BoardCapRequest) as Extract<
        BoardCapResponse,
        { op: "load_live" }
      >;
      return resp.live_hash;
    },
    /// THE board at a stop — pooling, realtime-first ordering, and
    /// each row's status are the module's answer, never the host's.
    stopBoard(
      networkHash: string,
      editsHash: string | null,
      stopId: string,
      date: string,
      fromSecs: number,
      limit: number,
      liveHash: string | null,
    ): StopBoard {
      const resp = module.invoke({
        op: "stop_board",
        network_hash: networkHash,
        edits_hash: editsHash ?? undefined,
        stop_id: stopId,
        date,
        from_secs: fromSecs,
        limit,
        live_hash: liveHash ?? undefined,
      } as BoardCapRequest) as Extract<BoardCapResponse, { op: "stop_board" }>;
      return resp.board;
    },
    loadWatchlist(document: Watchlist): { watchlistHash: string; version: number } {
      const resp = module.invoke({
        op: "load_watchlist",
        document,
      } as BoardCapRequest) as Extract<BoardCapResponse, { op: "load_watchlist" }>;
      return { watchlistHash: resp.watchlist_hash, version: resp.version };
    },
    migrateWatchlist(document: Watchlist): { document: Watchlist; notes: MigrationNote[] } {
      const resp = module.invoke({
        op: "migrate_watchlist",
        document,
      } as BoardCapRequest) as Extract<BoardCapResponse, { op: "migrated_watchlist" }>;
      return { document: resp.document, notes: resp.notes };
    },
    watchlistBoard(
      watchlistHash: string,
      networkHash: string,
      editsHash: string | null,
      date: string,
      fromSecs: number,
      limit: number,
    ): WatchBoardStop[] {
      const resp = module.invoke({
        op: "watchlist_board",
        watchlist_hash: watchlistHash,
        network_hash: networkHash,
        edits_hash: editsHash,
        date,
        from_secs: fromSecs,
        limit,
      } as BoardCapRequest) as Extract<BoardCapResponse, { op: "watchlist_board" }>;
      return resp.stops;
    },
  };
}

export function runcutCap(module: CapModule) {
  return {
    loadNetwork(network: Network): string {
      const resp = module.invoke({
        op: "load_network",
        network,
      } as RuncutCapRequest) as Extract<RuncutCapResponse, { op: "load_network" }>;
      return resp.network_hash;
    },
    loadEdits(
      networkHash: string,
      records: EditRecord[],
    ): { editsHash: string; reports: EditReport[] } {
      const resp = module.invoke({
        op: "load_edits",
        network_hash: networkHash,
        records,
      } as RuncutCapRequest) as Extract<RuncutCapResponse, { op: "load_edits" }>;
      return { editsHash: resp.edits_hash, reports: resp.reports };
    },
    /// The day's blocks list — the index facet (vehicles out, with
    /// that day's running trip counts).
    blocks(networkHash: string, editsHash: string | null, date: string): DayBlock[] {
      const resp = module.invoke({
        op: "blocks",
        network_hash: networkHash,
        edits_hash: editsHash,
        date,
      } as RuncutCapRequest) as Extract<RuncutCapResponse, { op: "blocks" }>;
      return resp.blocks;
    },
    /// One block's authored chain with the layover ripple — the
    /// detail facet.
    blockTrips(
      networkHash: string,
      editsHash: string | null,
      blockId: string,
      date: string,
    ): BlockChainTrip[] {
      const resp = module.invoke({
        op: "block_trips",
        network_hash: networkHash,
        edits_hash: editsHash,
        block_id: blockId,
        date,
      } as RuncutCapRequest) as Extract<RuncutCapResponse, { op: "block_trips" }>;
      return resp.trips;
    },
    /// One derived trip's EFFECTIVE stops on the date — the trip
    /// detail (its line is tripPath).
    tripStops(
      networkHash: string,
      editsHash: string | null,
      schedule: string,
      startSecs: number,
      date: string,
    ): TripStopView[] {
      const resp = module.invoke({
        op: "trip_stops",
        network_hash: networkHash,
        edits_hash: editsHash,
        schedule,
        start_secs: startSecs,
        date,
      } as RuncutCapRequest) as Extract<RuncutCapResponse, { op: "trip_stops" }>;
      return resp.stops;
    },
    /// One derived trip's line on the date, over the effective day (a
    /// diverted trip draws its diversion).
    tripPath(networkHash: string, editsHash: string | null, schedule: string, startSecs: number, date: string): Coord[] {
      const resp = module.invoke({
        op: "trip_path",
        network_hash: networkHash,
        edits_hash: editsHash,
        schedule,
        start_secs: startSecs,
        date,
      } as RuncutCapRequest) as Extract<RuncutCapResponse, { op: "trip_path" }>;
      return resp.path;
    },
  };
}

export function calendarCap(module: CapModule) {
  return {
    loadNetwork(network: Network): string {
      const resp = module.invoke({
        op: "load_network",
        network,
      } as CalendarCapRequest) as Extract<CalendarCapResponse, { op: "load_network" }>;
      return resp.network_hash;
    },
    /// The service keys running on a date — from the effective day
    /// (schedule-only when no edit document is registered).
    activeServices(networkHash: string, date: string): string[] {
      const resp = module.invoke({
        op: "active_services",
        network_hash: networkHash,
        date,
      } as CalendarCapRequest) as Extract<CalendarCapResponse, { op: "active_services" }>;
      return resp.services;
    },
  };
}

/// The current board module, if the bus assembled one.
export async function boardModule(): Promise<CapModule | null> {
  const modules = await ensure(["board"]);
  return modules.board ?? null;
}

/// The current runcut module, if the bus assembled one.
export async function runcutModule(): Promise<CapModule | null> {
  const modules = await ensure(["runcut"]);
  return modules.runcut ?? null;
}

/// The current calendar module, if the bus assembled one.
export async function calendarModule(): Promise<CapModule | null> {
  const modules = await ensure(["calendar"]);
  return modules.calendar ?? null;
}

/// Everything a day view needs from a loaded feed: the derived
/// network document (by hash, loaded in the day-view module) and the
/// import's trip-id map (the join rt delays and click-throughs are
/// keyed by — rt and the feed's own ops speak the AGENCY's ids, the
/// model mints its own).
export type Boarding = {
  feedHash: string;
  networkHash: string;
  tripMap: TripIdMap;
};

/// Board the loaded feed into a day-view or planning module (board,
/// runcut, router — the modules that answer with trip-level
/// vocabulary): load the bytes into cap-network, import the network
/// document, load that SAME document into the module (identities
/// asserted equal), and carry the trip-id map. Memoized by (module,
/// feed) — the import runs once per feed per module per session,
/// never per query.
function boardings(capId: "board" | "runcut" | "router") {
  const memo = new Map<string, Promise<Boarding | null>>();
  return async function ensureBoarding(bytes: Uint8Array): Promise<Boarding | null> {
    const memoKey = await contentHash(bytes as Uint8Array<ArrayBuffer>);
    const hit = memo.get(memoKey);
    if (hit) return hit;
    const boarding = (async () => {
      const modules = await ensure(["network", capId]);
      const network = modules.network;
      const cap = modules[capId];
      if (!network || !cap) return null;
      const feedHash = networkCap(network).loadFeed(bytes).feedHash;
      const imported = networkCap(network).import(feedHash);
      const map = networkCap(network).tripMap(imported.networkHash);
      if (!map) return null; // an imported network always has one
      const loaders = { board: boardCap, runcut: runcutCap, router: routerCap } as const;
      const loadedHash = loaders[capId](cap).loadNetwork(imported.network);
      if (loadedHash !== imported.networkHash) {
        throw new Error(
          `network identity disagrees across capabilities: ${loadedHash} != ${imported.networkHash}`,
        );
      }
      return { feedHash, networkHash: imported.networkHash, tripMap: map };
    })();
    boarding.catch(() => memo.delete(memoKey));
    memo.set(memoKey, boarding);
    return boarding;
  };
}

export const ensureBoardBoarding = boardings("board");
export const ensureRuncutBoarding = boardings("runcut");
export const ensureRouterBoarding = boardings("router");

const importedDocuments = new Map<string, Promise<Network | null>>();

/// The rider-side NETWORK DOCUMENT, imported once per feed and held
/// by the HOST — JS owns documents; the capabilities hold only
/// hash-addressed views. The map's view code reads stops, patterns,
/// and authored alignments from this document (the map is a view,
/// not a capability).
export function importedNetworkDocument(bytes: Uint8Array): Promise<Network | null> {
  const load = async (): Promise<Network | null> => {
    const memoKey = await contentHash(bytes as Uint8Array<ArrayBuffer>);
    const memo = importedDocuments.get(memoKey);
    if (memo) return memo;
    const run = (async () => {
      const modules = await ensure(["network"]);
      const network = modules.network;
      if (!network) return null;
      const feedHash = networkCap(network).loadFeed(bytes).feedHash;
      return networkCap(network).import(feedHash).network;
    })();
    run.catch(() => importedDocuments.delete(memoKey));
    importedDocuments.set(memoKey, run);
    return run;
  };
  return load();
}

const editReportCaches = new Map<string, Promise<EditReport[] | null>>();

/// The edit document's validation reports — the day views validate
/// per record against the network at load_edits; the UI reads the
/// reports off the same registration (memoized by document content:
/// reports are content-derived). An empty document reports nothing.
export function ensureEditReports(
  bytes: Uint8Array,
  records: EditRecord[],
): Promise<EditReport[] | null> {
  if (records.length === 0) return Promise.resolve([]);
  const load = async (): Promise<EditReport[] | null> => {
    const memoKey =
      (await contentHash(bytes as Uint8Array<ArrayBuffer>)) + "\n" + JSON.stringify(records);
    const memo = editReportCaches.get(memoKey);
    if (memo) return memo;
    const run = (async () => {
      const boarding = await ensureBoardBoarding(bytes);
      const module = boarding ? await boardModule() : null;
      if (!module || !boarding) return null;
      return boardCap(module).loadEdits(boarding.networkHash, records).reports;
    })();
    run.catch(() => editReportCaches.delete(memoKey));
    editReportCaches.set(memoKey, run);
    return run;
  };
  return load();
}

const calendarNetworks = new Map<string, Promise<string | null>>();

/// Load the imported network document into the calendar module
/// (the chips-grade read: service keys by date), memoized by feed
/// bytes — the document crosses the seam once per feed.
export async function ensureCalendarNetwork(bytes: Uint8Array): Promise<string | null> {
  const memoKey = await contentHash(bytes as Uint8Array<ArrayBuffer>);
  const memo = calendarNetworks.get(memoKey);
  if (memo) return memo;
  const load = (async () => {
    const modules = await ensure(["network", "calendar"]);
    const network = modules.network;
    const calendar = modules.calendar;
    if (!network || !calendar) return null;
    const feedHash = networkCap(network).loadFeed(bytes).feedHash;
    const imported = networkCap(network).import(feedHash);
    const loadedHash = calendarCap(calendar).loadNetwork(imported.network);
    if (loadedHash !== imported.networkHash) {
      throw new Error(
        `network identity disagrees across capabilities: ${loadedHash} != ${imported.networkHash}`,
      );
    }
    return imported.networkHash;
  })();
  load.catch(() => calendarNetworks.delete(memoKey));
  calendarNetworks.set(memoKey, load);
  return load;
}

/// Register the operator edit document against a boarded network, in
/// a given day-view or planning module, memoized by (module,
/// network, document) — the edits document crosses the seam once per
/// change, not once per query. An EMPTY document is schedule-only:
/// no load, null hash (the day views' contract).
function editsRegistry(
  capId: "board" | "runcut" | "router",
  moduleOf: () => Promise<CapModule | null>,
) {
  const memo = new Map<string, Promise<string | null>>();
  return function ensureEdits(
    networkHash: string,
    records: EditRecord[],
  ): Promise<string | null> {
    if (records.length === 0) return Promise.resolve(null);
    const memoKey = capId + "\n" + networkHash + "\n" + JSON.stringify(records);
    const hit = memo.get(memoKey);
    if (hit) return hit;
    const load = (async () => {
      const module = await moduleOf();
      if (!module) return null;
      const loaders = { board: boardCap, runcut: runcutCap, router: routerCap } as const;
      return loaders[capId](module).loadEdits(networkHash, records).editsHash;
    })();
    load.catch(() => memo.delete(memoKey));
    memo.set(memoKey, load);
    return load;
  };
}

export const ensureBoardEdits = editsRegistry("board", boardModule);
export const ensureRuncutEdits = editsRegistry("runcut", runcutModule);
export const ensureRouterEdits = editsRegistry("router", routerModule);

// ---------------------------------------------------------------------
// ---------------------------------------------------------------------
// The directory capability — what exists in the network: the stops
// (with their walking-group structure), the routes, one route's
// patterns with their ordered stops (list → detail). The pickers and
// the names join read here — MODEL questions, never feed reads
// (the feed line reads the gtfs capability's load_feed summary).
// ---------------------------------------------------------------------

export function directoryCap(module: CapModule) {
  return {
    loadNetwork(network: Network): string {
      const resp = module.invoke({
        op: "load_network",
        network,
      } as DirectoryCapRequest) as Extract<DirectoryCapResponse, { op: "load_network" }>;
      return resp.network_hash;
    },
    stops(networkHash: string, limit: number | null): DirectoryStop[] {
      const resp = module.invoke({
        op: "stops",
        network_hash: networkHash,
        limit,
      } as DirectoryCapRequest) as Extract<DirectoryCapResponse, { op: "stops" }>;
      return resp.stops;
    },
    stopsByIds(networkHash: string, ids: string[]): DirectoryStop[] {
      const resp = module.invoke({
        op: "stops_by_ids",
        network_hash: networkHash,
        ids,
      } as DirectoryCapRequest) as Extract<DirectoryCapResponse, { op: "stops_by_ids" }>;
      return resp.stops;
    },
    routes(networkHash: string): DirectoryRoute[] {
      const resp = module.invoke({
        op: "routes",
        network_hash: networkHash,
      } as DirectoryCapRequest) as Extract<DirectoryCapResponse, { op: "routes" }>;
      return resp.routes;
    },
    /// One route's patterns with their ordered stops — the model's
    /// directions, what the diversion editor anchors against.
    routeStopSequence(networkHash: string, routeId: string): PatternStops[] {
      const resp = module.invoke({
        op: "route_stop_sequence",
        network_hash: networkHash,
        route_id: routeId,
      } as DirectoryCapRequest) as Extract<DirectoryCapResponse, { op: "route_stop_sequence" }>;
      return resp.patterns;
    },
    /// Where one entity is on the map: its lines and stops.
    focus(networkHash: string, kind: EntityKind, id: string): EntityFocus | null {
      const resp = module.invoke({
        op: "focus",
        network_hash: networkHash,
        kind,
        id,
      } as DirectoryCapRequest) as Extract<DirectoryCapResponse, { op: "focus" }>;
      return resp.focus;
    },
  };
}

/// Where an entity of a PLAN document is (the planner's drill-down):
/// the document registered in the directory module once per version.
const focusHashes = new WeakMap<Network, string>();
export async function planEntityFocus(doc: Network, kind: EntityKind, id: string): Promise<EntityFocus | null> {
  const module = await directoryModule();
  if (!module) return null;
  const dir = directoryCap(module);
  let hash = focusHashes.get(doc);
  if (!hash) {
    hash = dir.loadNetwork(doc);
    focusHashes.set(doc, hash);
  }
  return dir.focus(hash, kind, id);
}

/// The current directory module, if the bus assembled one.
export async function directoryModule(): Promise<CapModule | null> {
  const modules = await ensure(["directory"]);
  return modules.directory ?? null;
}

/// The loaded directory's answers, held in host state so pickers and
/// name lookups read synchronously (the module loads once per feed;
/// views key on the reactive fields).
export const directory = $state({
  networkHash: null as string | null,
  stops: [] as DirectoryStop[],
  routes: [] as DirectoryRoute[],
});

const directoryLoads = new Map<string, Promise<boolean>>();

/// Load the imported network document into the directory module and
/// pull its lists into host state — memoized by the feed bytes'
/// content hash, so concurrent views share one load.
export function ensureDirectory(bytes: Uint8Array): Promise<boolean> {
  const load = async (): Promise<boolean> => {
    const memoKey = await contentHash(bytes as Uint8Array<ArrayBuffer>);
    const memo = directoryLoads.get(memoKey);
    if (memo) return memo;
    const run = (async () => {
      const modules = await ensure(["network", "directory"]);
      const network = modules.network;
      const dir = modules.directory;
      if (!network || !dir) return false;
      const feedHash = networkCap(network).loadFeed(bytes).feedHash;
      const imported = networkCap(network).import(feedHash);
      const loadedHash = directoryCap(dir).loadNetwork(imported.network);
      if (loadedHash !== imported.networkHash) {
        throw new Error(
          `network identity disagrees across capabilities: ${loadedHash} != ${imported.networkHash}`,
        );
      }
      directory.networkHash = imported.networkHash;
      directory.stops = directoryCap(dir).stops(imported.networkHash, null);
      directory.routes = directoryCap(dir).routes(imported.networkHash);
      return true;
    })();
    run.catch(() => directoryLoads.delete(memoKey));
    directoryLoads.set(memoKey, run);
    return run;
  };
  return load();
}

/// The feed line's summary — the feed document's own read (what it
/// contains at a glance), through the gtfs capability's load_feed.
const feedSummaries = new Map<string, Promise<FeedSummary | null>>();

export function ensureFeedSummary(bytes: Uint8Array): Promise<FeedSummary | null> {
  const load = async (): Promise<FeedSummary | null> => {
    const memoKey = await contentHash(bytes as Uint8Array<ArrayBuffer>);
    const memo = feedSummaries.get(memoKey);
    if (memo) return memo;
    const run = (async () => {
      const module = await gtfsModule();
      if (!module) return null;
      return gtfsCap(module).loadFeed(bytes).summary;
    })();
    run.catch(() => feedSummaries.delete(memoKey));
    feedSummaries.set(memoKey, run);
    return run;
  };
  return load();
}

// The streets capability — the detour editor's street graph (the
// first unit of the monolith decomposition, U1): a GeoJSON street
// network registers under its content hash, then pure snap/route
// queries against the parsed graph. Reference data the HOST owns
// (an OSM extract or open-data layer, re-imported per session); the
// module writes nothing. When the assembly cannot form, street
// import refuses with an honest status and detour clicks fall back
// to stops — never silently to freehand chords.
// ---------------------------------------------------------------------

const OP_LOAD_STREETS = 0;

export type StreetsGraph = { streetsHash: string; segments: number; nodes: number };

export function streetsCap(module: CapModule) {
  return {
    /// Bytes in once: the GeoJSON parses into the routing graph and
    /// registers under its content hash (idempotent — the same
    /// bytes are the same graph).
    loadStreets(bytes: Uint8Array): StreetsGraph {
      const resp = module.invokeBytes(OP_LOAD_STREETS, bytes) as Extract<
        StreetsCapResponse,
        { op: "load_streets" }
      >;
      return { streetsHash: resp.streets_hash, segments: resp.segments, nodes: resp.nodes };
    },
    /// The nearest street position to a click: the projected point
    /// plus its graph address — what a detour waypoint commits to.
    snap(streetsHash: string, lat: number, lon: number): StreetPosition | null {
      const resp = module.invoke({
        op: "snap",
        streets_hash: streetsHash,
        lat,
        lon,
      } as StreetsCapRequest) as Extract<StreetsCapResponse, { op: "snap" }>;
      return resp.position ?? null;
    },
    /// The street geometry between two positions (shortest path).
    /// Empty when the positions sit on disconnected parts — the
    /// caller falls back to a straight line.
    route(streetsHash: string, from: StreetPosition, to: StreetPosition): StreetPoint[] {
      const resp = module.invoke({
        op: "route",
        streets_hash: streetsHash,
        from,
        to,
      } as StreetsCapRequest) as Extract<StreetsCapResponse, { op: "route" }>;
      return resp.points;
    },
  };
}

/// The current streets module, if the bus assembled one.
export async function streetsModule(): Promise<CapModule | null> {
  const modules = await ensure(["streets"]);
  return modules.streets ?? null;
}

const streetGraphs = new Map<string, Promise<StreetsGraph | null>>();

/// Register a street network in cap-streets, memoized by the bytes'
/// content hash — the GeoJSON crosses the seam once per import, and
/// THE MODULE IS THE HASH AUTHORITY (its returned streets_hash is
/// the identity; the memo key is only the host's "did I already
/// load these bytes" check).
export function ensureStreets(bytes: Uint8Array): Promise<StreetsGraph | null> {
  const load = async (): Promise<StreetsGraph | null> => {
    const memoKey = await contentHash(bytes as Uint8Array<ArrayBuffer>);
    const memo = streetGraphs.get(memoKey);
    if (memo) return memo;
    const graph = (async () => {
      const module = await streetsModule();
      if (!module) return null;
      return streetsCap(module).loadStreets(bytes);
    })();
    graph.catch(() => streetGraphs.delete(memoKey));
    streetGraphs.set(memoKey, graph);
    return graph;
  };
  return load();
}

/// Sync snap for the map's click handlers (an event handler cannot
/// await). A streets handle only exists after ensureStreets loaded
/// the module, so a missing module here means the assembly was
/// degraded mid-session — the click then behaves as "no street
/// nearby", same as an off-graph click.
export function streetsSnap(streetsHash: string, lat: number, lon: number): StreetPosition | null {
  const module = caps.modules["streets"];
  if (!module) return null;
  return streetsCap(module).snap(streetsHash, lat, lon);
}

/// Sync route, same standing as streetsSnap: empty (straight-line
/// fallback) when the module is gone.
export function streetsRoute(
  streetsHash: string,
  from: StreetPosition,
  to: StreetPosition,
): StreetPoint[] {
  const module = caps.modules["streets"];
  if (!module) return [];
  return streetsCap(module).route(streetsHash, from, to);
}

// ---------------------------------------------------------------------
// The plans capability — the service-plan chain: which pick governs a
// date, every member's governing window, and chain validity. Pure
// questions over the plan set's metadata (the host owns the plan
// set; the module holds no registries), answered async because the
// module assembles like every other capability.
// ---------------------------------------------------------------------

export function plansCap(module: CapModule) {
  return {
    resolve(plans: PlanInfo[], date: string): string | null {
      const resp = module.invoke({
        op: "resolve",
        plans,
        date,
      } as PlansCapRequest) as Extract<PlansCapResponse, { op: "resolve" }>;
      return resp.plan;
    },
    validate(plans: PlanInfo[]): PlanIssue[] {
      const resp = module.invoke({
        op: "validate",
        plans,
      } as PlansCapRequest) as Extract<PlansCapResponse, { op: "validate" }>;
      return resp.issues;
    },
    periods(plans: PlanInfo[]): PlanPeriod[] {
      const resp = module.invoke({
        op: "periods",
        plans,
      } as PlansCapRequest) as Extract<PlansCapResponse, { op: "periods" }>;
      return resp.periods;
    },
  };
}

/// The current plans module, if the bus assembled one.
// ---------------------------------------------------------------------
// The authoring capability — every change to a network document is a
// list of NetworkEdits applied by cap-author: the gestures carry the
// model's coherence rules (aligned timing, sorted starts, layovers
// between trips, minted ids), so the browser, the runner, and the
// server author the same document from the same edits.
// ---------------------------------------------------------------------

export type Applied = {
  networkHash: string;
  network: Network;
  issues: NetworkError[];
  warnings: NetworkWarning[];
  created: CreatedEntity[];
};

export function authorCap(module: CapModule) {
  return {
    loadNetwork(network: Network): string {
      const resp = module.invoke({ op: "load_network", network } as AuthorCapRequest) as Extract<
        AuthorCapResponse,
        { op: "load_network" }
      >;
      return resp.network_hash;
    },
    /// Apply edits atomically to a registered document: the next
    /// document (registered under its own hash) and what it is.
    apply(networkHash: string, edits: NetworkEdit[]): Applied {
      const resp = module.invoke({ op: "apply", network_hash: networkHash, edits } as AuthorCapRequest) as Extract<
        AuthorCapResponse,
        { op: "apply" }
      >;
      return {
        networkHash: resp.network_hash,
        network: resp.network,
        issues: resp.issues,
        warnings: resp.warnings,
        created: resp.created,
      };
    },
  };
}

/// The current authoring module, if the bus assembled one.
export async function authorModule(): Promise<CapModule | null> {
  const modules = await ensure(["author"]);
  return modules.author ?? null;
}

// ---------------------------------------------------------------------
// The diversion-authoring capability — the editor's draft answered
// over the network (default pattern, interior skips, alignment,
// traced path), the dispatcher's list, and the edit document's next
// states (propose / retire / restore; sanitize for stored copies).
// The host passes the clock and the record ids.
// ---------------------------------------------------------------------

export function divertCap(module: CapModule) {
  const call = <K extends DivertCapResponse["op"]>(req: DivertCapRequest) =>
    module.invoke(req) as Extract<DivertCapResponse, { op: K }>;
  return {
    loadNetwork: (network: Network): string =>
      call<"load_network">({ op: "load_network", network }).network_hash,
    draft: (networkHash: string, draft: DiversionDraft): DraftView =>
      call<"draft">({ op: "draft", network_hash: networkHash, draft }).view,
    list: (networkHash: string, records: EditRecord[]): DiversionRow[] =>
      call<"list">({ op: "list", network_hash: networkHash, records }).rows,
    propose: (networkHash: string, draft: DiversionDraft, records: EditRecord[], recordId: string, nowMs: number): EditRecord[] =>
      call<"propose">({ op: "propose", network_hash: networkHash, draft, records, record_id: recordId, now_ms: nowMs }).records,
    retire: (records: EditRecord[], recordId: string, nowMs: number): EditRecord[] =>
      call<"retire">({ op: "retire", records, record_id: recordId, now_ms: nowMs }).records,
    restore: (records: EditRecord[], recordId: string, nowMs: number): EditRecord[] =>
      call<"restore">({ op: "restore", records, record_id: recordId, now_ms: nowMs }).records,
    sanitize: (records: unknown): { records: EditRecord[]; dropped: number } => {
      const resp = call<"sanitize">({ op: "sanitize", records });
      return { records: resp.records, dropped: resp.dropped };
    },
  };
}

/// The diversion-authoring module, if the bus assembled one.
export async function divertModule(): Promise<CapModule | null> {
  const modules = await ensure(["divert"]);
  return modules.divert ?? null;
}

/// The imported network registered in the divert module — the base
/// network every diversion is authored against. Memoized per document
/// (registration serializes the whole network).
const divertHashes = new WeakMap<Network, string>();
export async function divertOverFeed(
  bytes: Uint8Array,
): Promise<{ divert: ReturnType<typeof divertCap>; networkHash: string } | null> {
  const [module, network] = await Promise.all([divertModule(), importedNetworkDocument(bytes)]);
  if (!module || !network) return null;
  const divert = divertCap(module);
  let networkHash = divertHashes.get(network);
  if (!networkHash) {
    networkHash = divert.loadNetwork(network);
    divertHashes.set(network, networkHash);
  }
  return { divert, networkHash };
}

export async function plansModule(): Promise<CapModule | null> {
  const modules = await ensure(["plans"]);
  return modules.plans ?? null;
}

/// The whole chain view in one load (the workspace root reads all
/// three facets together): the plan in force on the date, every
/// pick's governing window, and the chain issues. Null when the
/// capability did not assemble — the caller degrades loudly.
export async function plansChain(
  infos: PlanInfo[],
  date: string,
): Promise<{ inForce: string | null; periods: PlanPeriod[]; issues: PlanIssue[] } | null> {
  const module = await plansModule();
  if (!module) return null;
  const cap = plansCap(module);
  return {
    inForce: cap.resolve(infos, date),
    periods: cap.periods(infos),
    issues: cap.validate(infos),
  };
}
