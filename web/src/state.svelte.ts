/// Shared reactive state — the single owner of "what is on screen".
/// Every component reads and writes through this one object, so there
/// is exactly one source of truth for UI state; the map replays from
/// it when it finishes loading; feeds update only the slice they own
/// (TripUpdates can never wipe vehicle positions).
///
/// Fully typed against the generated artifact types (src/types) — the
/// app object is where the artifact's contract meets the UI.

import type { EditRecord } from "./types/EditRecord";
import type { EditReport } from "./types/EditReport";
import type { Itinerary } from "./types/Itinerary";
import type { Network } from "./types/Network";
import type { NetworkError } from "./types/NetworkError";
import type { RtOverview } from "./types/RtOverview";
import type { TripLineFeature } from "./types/TripLineFeature";
import type { VehicleView } from "./types/VehicleView";
import type { EntityKind, Focus } from "./planner";
import type { GtfsHandle, WasmApi } from "./wasm";
import type { PlanFeature } from "./actions";
import type { Streets } from "./streets";

/// The diversion editor's form state (UI only — the edit document
/// below is the persisted truth).
export type DiversionDraft = {
  routeId: string;
  directionId: string; // "" = any, "0"/"1" filter
  firstKept: string;
  lastKept: string;
  skipped: Set<string>;
  addedMins: number;
  start: string;
  end: string;
  detour: [number, number][]; // as drawn on the map
};

export type AppState = {
  // wasm exports, injected at startup (main.ts)
  wasm: WasmApi | null;

  status: string,

  // Persona views: which workflow the panel serves. The map is shared
  // by all of them; the panel content is focused per persona.
  persona: "rider" | "dispatcher" | "planner",

  // GTFS static
  gtfs: GtfsHandle | null,
  selectedStop: string | null,
  clock: number, // bumped to re-derive time-dependent views
  // Bumped whenever the *effective feed* changes (edits pushed to
  // wasm). wasm internals are not observable from JS, so derived
  // views (departure boards, block chains) read this to know when to
  // re-query. `clock` means "time moved"; `feedEpoch` means "data
  // moved".
  feedEpoch: number,
  selectedTripId: string | null,
  tripFeature: TripLineFeature | null, // GeoJSON feature for the selected trip

  // Validation reports for the edit document, from wasm, aligned by
  // id. Replaced wholesale on every push.
  editReports: EditReport[],

  // GTFS-rt
  rtOverview: RtOverview | null, // decode_rt() result (delays panel)
  vehicles: VehicleView[], // last good vehicle positions (map dots)

  // Trip planning (RAPTOR)
  plan: Itinerary | null,
  planNotice: string | null, // inline feedback when planning fails
  planBaseline: Itinerary | null, // same plan against the base feed when diversions are active
  itineraryFeatures: PlanFeature[], // GeoJSON features for drawing the plan on the map

  // Diversion editor form (see DiversionDraft)
  diversionDraft: DiversionDraft | null,
  pickingAnchor: "first" | "last" | null, // stop clicks set anchors
  drawingDetour: boolean, // map clicks append detour vertices

  // URL fields and poll toggles
  gtfsUrl: string,
  rtUrl: string,
  vpUrl: string,
  rtPolling: boolean,
  vpPolling: boolean,

  // Persistence: the raw feed zip is cached in IndexedDB so refreshes
  // restore instantly without re-downloading.
  cachedFeed: string | null,

  // Network authoring (planner persona). Issues come back from the
  // artifact (`network_validate`): typed, whole-list, never thrown.
  networkIssues: NetworkError[],
  networkLabel: string,
  // Drill-down: the entity the planner is focused on, and what the
  // map should frame for it (plain data, MapView just draws it).
  plannerSelection: { kind: EntityKind; id: string } | null,
  plannerFocus: Focus | null,
};

/// The sync document (transit-sync `EditRecord[]`) — **owned here**.
/// Plain data under `$state.raw`: replaced wholesale, never mutated
/// in place. That is what makes it safe to persist (IndexedDB can
/// structured-clone it directly — no proxies) and to sync to an ops
/// server. wasm never owns it; `set_diversions` gives wasm a
/// materialized view and returns validation reports.
let editDocument = $state.raw<EditRecord[]>([]);
export function getEditDocument(): EditRecord[] {
  return editDocument;
}
export function setEditDocument(next: EditRecord[]): void {
  editDocument = next;
}

/// The authored network document (transit-network `Network`) — same
/// discipline as the edit document: JS-owned, plain data, wholesale
/// replacement. wasm only ever sees it as an argument to
/// `network_validate` / `network_load`; the materialized feed it
/// registers is a derived view. Deliberately not persisted yet —
/// planner sync is a separate (future) concern.
let networkDoc = $state.raw<Network | null>(null);
export function getNetworkDoc(): Network | null {
  return networkDoc;
}
export function setNetworkDoc(next: Network | null): void {
  networkDoc = next;
}

/// The imported street network (dispatcher detour snapping) — same
/// discipline: plain data, wholesale replacement. Never proxied
/// (extracts can be tens of MB) and not persisted: it is reference
/// data the dispatcher re-imports, not authored state worth cloning
/// into IndexedDB.
let streets = $state.raw<Streets | null>(null);
export function getStreets(): Streets | null {
  return streets;
}
export function setStreets(next: Streets | null): void {
  streets = next;
}

export const app = $state<AppState>({
  wasm: null,

  status: "loading wasm...",

  persona: "rider",

  gtfs: null,
  selectedStop: null,
  clock: 0,
  feedEpoch: 0,
  selectedTripId: null,
  tripFeature: null,

  editReports: [],

  rtOverview: null,
  vehicles: [],

  plan: null,
  planNotice: null,
  planBaseline: null,
  itineraryFeatures: [],

  diversionDraft: null,
  pickingAnchor: null,
  drawingDetour: false,

  gtfsUrl: "",
  rtUrl: "",
  vpUrl: "",
  rtPolling: false,
  vpPolling: false,

  cachedFeed: null,

  networkIssues: [],
  networkLabel: "",
  plannerSelection: null,
  plannerFocus: null,
});
