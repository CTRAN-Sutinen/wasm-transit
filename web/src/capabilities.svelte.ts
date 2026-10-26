/// The capability-bus PoC: the host-side resolver.
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
import type { Watchlist } from "./types/cap_gtfs/Watchlist";
import type { MigrationNote } from "./types/cap_gtfs/MigrationNote";
import type { WatchBoardStop } from "./types/cap_gtfs/WatchBoardStop";
import type { Request as RouterRequest } from "./types/cap_router/Request";
import type { Response as RouterResponse } from "./types/cap_router/Response";
import type { Request as AnalysisRequest } from "./types/cap_analysis/Request";
import type { Response as AnalysisResponse } from "./types/cap_analysis/Response";
import type { Request as NetworkCapRequest } from "./types/cap_network/Request";
import type { Response as NetworkCapResponse } from "./types/cap_network/Response";
import type { Request as RtCapRequest } from "./types/cap_rt/Request";
import type { Response as RtCapResponse } from "./types/cap_rt/Response";
import type { RtOverview } from "./types/RtOverview";
import type { Network } from "./types/Network";
import type { NetworkError } from "./types/NetworkError";
import type { NetworkStats } from "./types/NetworkStats";
import type { ImportNote } from "./types/ImportNote";
import type { ServiceTypeDetail } from "./types/ServiceTypeDetail";
import type { ServiceTypeSummary } from "./types/ServiceTypeSummary";
import type { TypeCoverage } from "./types/TypeCoverage";
import type { Departure } from "./types/Departure";
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
      | RtCapRequest,
  ): GtfsResponse | RouterResponse | AnalysisResponse | NetworkCapResponse | RtCapResponse {
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
      | RtCapResponse;
  }

  invokeBytes(
    op: number,
    bytes: Uint8Array,
  ): GtfsResponse | RouterResponse | AnalysisResponse | NetworkCapResponse | RtCapResponse {
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
      | RtCapResponse;
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
async function contentHash(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
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

export function gtfsCap(module: CapModule) {
  return {
    loadFeed(bytes: Uint8Array): GtfsLoadFeed {
      const resp = module.invokeBytes(OP_LOAD_FEED, bytes) as Extract<
        GtfsResponse,
        { op: "load_feed" }
      >;
      return { feedHash: resp.feed_hash, summary: resp.summary };
    },
    departures(
      feedHash: string,
      stopId: string,
      date: string,
      fromSecs: number,
      limit: number,
    ): Departure[] {
      const resp = module.invoke({
        op: "departures",
        feed_hash: feedHash,
        stop_id: stopId,
        date,
        from_secs: fromSecs,
        limit,
      } as GtfsRequest) as Extract<GtfsResponse, { op: "departures" }>;
      return resp.departures;
    },
    stops(feedHash: string, limit: number): StopSummary[] {
      const resp = module.invoke({
        op: "stops",
        feed_hash: feedHash,
        limit,
      } as GtfsRequest) as Extract<GtfsResponse, { op: "stops" }>;
      return resp.stops;
    },
    loadWatchlist(document: Watchlist): { watchlistHash: string; version: number } {
      const resp = module.invoke({
        op: "load_watchlist",
        document,
      } as GtfsRequest) as Extract<GtfsResponse, { op: "load_watchlist" }>;
      return { watchlistHash: resp.watchlist_hash, version: resp.version };
    },
    migrateWatchlist(document: Watchlist): { document: Watchlist; notes: MigrationNote[] } {
      const resp = module.invoke({
        op: "migrate_watchlist",
        document,
      } as GtfsRequest) as Extract<GtfsResponse, { op: "migrated_watchlist" }>;
      return { document: resp.document, notes: resp.notes };
    },
    watchlistBoard(
      watchlistHash: string,
      feedHash: string,
      date: string,
      fromSecs: number,
      limit: number,
    ): WatchBoardStop[] {
      const resp = module.invoke({
        op: "watchlist_board",
        watchlist_hash: watchlistHash,
        feed_hash: feedHash,
        date,
        from_secs: fromSecs,
        limit,
      } as GtfsRequest) as Extract<GtfsResponse, { op: "watchlist_board" }>;
      return resp.stops;
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
  const gtfs = gtfsCap(module);
  const { document: migrated, notes } = gtfs.migrateWatchlist(document);
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

/// Register a population layer in cap-analysis, memoized like
/// networks. Same rule: the module's returned hash is the identity.
export async function ensureAnalysisLayer(bytes: Uint8Array): Promise<string | null> {
  const module = await analysisModule();
  if (!module) return null;
  const memoKey = await contentHash(bytes as Uint8Array<ArrayBuffer>);
  const memo = (analysisLayers[module.cap.id] ??= new Map());
  if (!memo.has(memoKey)) {
    const resp = module.invokeBytes(OP_LOAD_LAYER, bytes) as Extract<
      AnalysisResponse,
      { op: "load_population_layer" }
    >;
    memo.set(memoKey, resp.layer_hash);
  }
  return memo.get(memoKey) ?? null;
}

const analysisNetworks: Record<string, Map<string, string>> = {};
const analysisLayers: Record<string, Map<string, string>> = {};

/// The current analysis module, if the bus assembled one.
export async function analysisModule(): Promise<CapModule | null> {
  const modules = await ensure(["analysis"]);
  return modules.analysis ?? null;
}

export function routerCap(module: CapModule) {
  return {
    loadFeed(bytes: Uint8Array): GtfsLoadFeed {
      const resp = module.invokeBytes(OP_LOAD_FEED, bytes) as Extract<
        RouterResponse,
        { op: "load_feed" }
      >;
      return { feedHash: resp.feed_hash, summary: resp.summary };
    },
    plan(
      feedHash: string,
      from: string,
      to: string,
      date: string,
      departSecs: number,
      maxLegs: number,
    ): Itinerary | null {
      const resp = module.invoke({
        op: "plan",
        feed_hash: feedHash,
        from,
        to,
        date,
        depart_secs: departSecs,
        max_legs: maxLegs,
      } as RouterRequest) as Extract<RouterResponse, { op: "plan" }>;
      return resp.itinerary;
    },
  };
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
        op: "load_document",
        network,
      } as NetworkCapRequest) as Extract<NetworkCapResponse, { op: "load_document" }>;
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

export function rtCap(module: CapModule) {
  return {
    decode(bytes: Uint8Array): RtOverview {
      const resp = module.invokeBytes(OP_RT_DECODE, bytes) as Extract<
        RtCapResponse,
        { op: "decode" }
      >;
      return resp.overview;
    },
  };
}

/// The current realtime module, if the bus assembled one.
export async function rtModule(): Promise<CapModule | null> {
  const modules = await ensure(["rt"]);
  return modules.rt ?? null;
}
