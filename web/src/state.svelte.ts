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
import type { ImportNote } from "./types/ImportNote";
import type { Itinerary } from "./types/Itinerary";
import type { Network } from "./types/Network";
import type { NetworkError } from "./types/NetworkError";
import type { NetworkStats } from "./types/NetworkStats";
import type { ServiceTypeDetail } from "./types/ServiceTypeDetail";
import type { ServiceTypeSummary } from "./types/ServiceTypeSummary";
import type { PlanIssue } from "./types/PlanIssue";
import type { PlanPeriod } from "./types/PlanPeriod";
import type { TypeCoverage } from "./types/TypeCoverage";
import type { RtOverview } from "./types/RtOverview";
import type { TripLineFeature } from "./types/TripLineFeature";
import type { VehicleView } from "./types/VehicleView";
import type { EntityKind, Focus, PlannerScreen } from "./planner";
import type { GtfsHandle, WasmApi } from "./wasm";
import type { PlanFeature } from "./actions";
import type { StreetPosition } from "./types/StreetPosition";
import type { Streets } from "./streets";
import { get as idbGet, set as idbSet } from "idb-keyval";

/// The diversion editor's form state (UI only — the edit document
/// below is the persisted truth).
export type DetourSlice = {
  /// The routed geometry of one waypoint step, as [lat, lon] points.
  points: [number, number][];
  /// The street position the trace ends at — where the next step
  /// routes from (the artifact's graph address). Null when the slice
  /// ends off-street (a stop, a raw corner): the next step falls
  /// back to a straight line.
  endPos: StreetPosition | null;
};
export type DiversionDraft = {
  routeId: string;
  patternId: string; // the model's direction: the pattern anchored against
  firstKept: string;
  lastKept: string;
  skipped: Set<string>;
  addedMins: number;
  start: string;
  end: string;
  /// The committed trace, one entry per waypoint step. `detour` below
  /// is the flattened view (the record field); slices stay so Undo
  /// reverts whole steps.
  slices: DetourSlice[];
  detour: [number, number][]; // flattened slices, as synced
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
  // The selected route's alignment (route_path_geojson) — the ghost
  // the detour is drawn against. Plain data; MapView just draws it.
  routeContext: TripLineFeature | null,

  // URL fields and poll toggles
  gtfsUrl: string,
  rtUrl: string,
  vpUrl: string,
  streetsUrl: string,
  rtPolling: boolean,
  vpPolling: boolean,

  // Persistence: the raw feed zip is cached in IndexedDB so refreshes
  // restore instantly without re-downloading.
  cachedFeed: string | null,

  // Network authoring (planner persona). Issues come back from the
  // NETWORK CAPABILITY (`validate`): typed, whole-list, never
  // thrown. When the capability cannot assemble, issues are empty
  // AND `networkValidationOff` is set — the chip must not claim
  // validity it cannot check (materialize still refuses broken
  // documents: export is the module's own gate).
  networkIssues: NetworkError[],
  networkValidationOff: boolean,
  // Resolutions the artifact's import applied to the last derived
  // document (dropped block trips, unusable transfer points, …) —
  // reported, never silent. Cleared when a document is loaded from
  // JSON (documents carry their own state; notes describe a feed).
  networkNotes: ImportNote[],
  // The planner's workspace is a screen stack: overview at the
  // root, drill-downs push, Back pops. Plain data — nothing here is
  // ever proxied deeply, and the stack is UI-only (never synced).
  plannerStack: PlannerScreen[],
  // The planner's DESIGN axis, from the artifact: every service
  // type's numbers on its representative day (and the type being
  // drilled into, expanded). No calendar dates — planners design
  // types; the type's own day mapping is a detail inside its screen.
  typeOverview: ServiceTypeSummary[],
  // The day-shape numbers on the planner's picked day (the
  // day-summary header of the overview screen): trips, routes,
  // stops, revenue hours, span, transfer points — network_stats
  // through cap-analysis, off when the capability cannot assemble.
  dayStats: NetworkStats | null,
  // The day the planner is inspecting (the day-summary date picker).
  plannerDay: string,
  serviceTypeDetail: ServiceTypeDetail | null,
  // The service-plan chain view, from the artifact's plans ops: the
  // plan in force today, each plan's governing period, chain issues.
  planChain: { inForce: string | null; periods: PlanPeriod[]; issues: PlanIssue[] },
  // Population coverage per service type (null: no layer imported).
  typeCoverage: TypeCoverage[] | null,
  // The imported population layer handle (GeoJSON points; the
  // artifact keeps the data under layer_id). The BYTES stay too —
  // content-addressed registration for the analysis capability
  // (the bus path registers layers by hash; the monolith by id).
  popLayer: { points: number; pop_field: string; total_pop: number; label: string } | null,
  popLayerBytes: Uint8Array | null,
  popUrl: string,
  // The map is a view, not a fixture: data screens (lists, document)
  // hide it and take the full width; entity screens bring it back
  // for framing. Manual toggle overrides the per-screen default.
  plannerMapUp: boolean,
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

/// Shape guard for records crossing IN from outside: the persisted
/// document and sync responses were written by older builds (and the
/// server treats documents as opaque JSON). One malformed entry
/// must not wedge every later mutation — `cloneEdits` throws on a
/// missing `diversion`, silently killing Apply and Retire (this
/// shipped: a stale IndexedDB document made the dispatcher's buttons
/// dead). Mirrors the artifact's serde shape: fields with
/// `#[serde(default)]` are filled when absent, records missing
/// REQUIRED fields are dropped (with a status line saying so).
const ISO_DATE = /^\d{4}-\d{2}-\d{2}$/;
const STATUSES = new Set(["draft", "submitted", "accepted", "retired"]);

function looksLikeRecord(r: unknown): r is EditRecord {
  if (typeof r !== "object" || r === null) return false;
  const rec = r as Record<string, unknown>;
  if (typeof rec.id !== "string" || !STATUSES.has(String(rec.status))) return false;
  if (typeof rec.rev !== "number" || typeof rec.updated_at !== "number") return false;
  const d = rec.diversion as Record<string, unknown> | undefined;
  if (typeof d !== "object" || d === null) return false;
  for (const f of ["id", "route_id", "first_kept_stop", "last_kept_stop"]) {
    if (typeof d[f] !== "string") return false;
  }
  if (!ISO_DATE.test(String(d.start_date)) || !ISO_DATE.test(String(d.end_date))) return false;
  // A detour must be a polyline of numeric [lat, lon] objects — junk
  // here is exactly what serde would reject, wedging set_diversions.
  if (d.detour != null) {
    if (!Array.isArray(d.detour)) return false;
    for (const p of d.detour) {
      const pt = p as Record<string, unknown>;
      if (typeof pt?.lat !== "number" || typeof pt?.lon !== "number"
        || !Number.isFinite(pt.lat) || !Number.isFinite(pt.lon)) return false;
    }
  }
  return true;
}

export function sanitizeEdits(input: unknown): EditRecord[] {
  if (!Array.isArray(input)) return [];
  const kept: EditRecord[] = [];
  let dropped = 0;
  for (const r of input) {
    if (!looksLikeRecord(r)) {
      dropped++;
      continue;
    }
    kept.push({
      ...r,
      diversion: {
        ...r.diversion,
        // serde defaults, filled so a record from before the field
        // existed still validates in the artifact.
        direction_id: r.diversion.direction_id ?? null,
        skipped_stops: r.diversion.skipped_stops ?? [],
        detour: r.diversion.detour ?? null,
        added_secs: r.diversion.added_secs ?? 0,
      },
    });
  }
  if (dropped > 0) {
    app.status = `Dropped ${dropped} malformed edit record(s) from the saved document — they were not valid under the current schema.`;
    console.warn(`dropped ${dropped} malformed edit record(s)`);
  }
  return kept;
}

export function setEditDocument(next: unknown): void {
  editDocument = sanitizeEdits(next);
}

/// The authored network documents (transit-network `Network`) — same
/// discipline as the edit document: JS-owned, plain data, wholesale
/// replacement. A **service plan** is one of these documents framed
/// by its `plan` metadata (identity, name, effective date); the plan
/// set is keyed by a store id (the sync identity inside the document
/// is `plan.id` — empty on unversioned imports). wasm only ever sees
/// a document as an op argument; materialized feeds are derived
/// views. Persisted to IndexedDB (plans survive reloads — service
/// changes span weeks).
let plans = $state.raw<Record<string, Network>>({});
let activePlanId = $state.raw<string>("");

export function getNetworkDoc(): Network | null {
  return activePlanId ? plans[activePlanId] ?? null : null;
}

export function setNetworkDoc(next: Network | null): void {
  if (!next) return;
  if (!activePlanId) {
    // Safety: a document without an active plan still needs a home.
    activePlanId = `plan-${crypto.randomUUID()}`;
    localStorage.setItem(ACTIVE_PLAN_KEY, activePlanId);
  }
  plans = { ...plans, [activePlanId]: next };
  void persistPlans();
}

export function getAllPlans(): Record<string, Network> {
  return plans;
}

export function getActivePlanId(): string {
  return activePlanId;
}

export function putPlan(id: string, doc: Network): void {
  plans = { ...plans, [id]: doc };
  void persistPlans();
}

export function setActivePlan(id: string): void {
  activePlanId = id;
  localStorage.setItem(ACTIVE_PLAN_KEY, id);
}

export function removePlan(id: string): void {
  const { [id]: _drop, ...rest } = plans;
  plans = rest;
  if (activePlanId === id) {
    activePlanId = "";
    localStorage.removeItem(ACTIVE_PLAN_KEY);
  }
  void persistPlans();
}

const PLANS_KEY = "transit:plans";
const ACTIVE_PLAN_KEY = "transit:active-plan";
let plansWrite: Promise<unknown> = Promise.resolve();

/** Persist the plan set — serialized writes, one pending at a time
 * (the same discipline as the edit document's flushPersist). */
function persistPlans(): void {
  plansWrite = plansWrite
    .then(() => idbSet(PLANS_KEY, plans))
    .catch((e) => console.warn("plan persistence failed:", e));
}

/** Restore the plan set on startup (documents cross IN from
 * IndexedDB; JS owns them, so a shape guard keeps stale builds from
 * wedging the workspace). */
export async function restorePlans(): Promise<void> {
  try {
    const stored = await idbGet(PLANS_KEY);
    if (stored && typeof stored === "object") {
      plans = stored as Record<string, Network>;
    }
    const active = localStorage.getItem(ACTIVE_PLAN_KEY);
    if (active && plans[active]) {
      activePlanId = active;
    }
  } catch (e) {
    console.warn("plan restore failed:", e);
  }
}

/// The imported street network (dispatcher detour authoring) — same
/// discipline: plain data, wholesale replacement. The routing graph
/// lives in the cap-streets capability (`streetsHash`); this holds
/// only the hash handle and the display copy. Never proxied (extracts
/// can be tens of MB) and not persisted: reference data,
/// re-imported per session.
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
  routeContext: null,

  gtfsUrl: "",
  rtUrl: "",
  vpUrl: "",
  streetsUrl: "",
  rtPolling: false,
  vpPolling: false,

  cachedFeed: null,

  networkIssues: [],
  networkValidationOff: false,
  networkNotes: [],
  plannerStack: [{ view: "plans" }] as PlannerScreen[],
  typeOverview: [],
  dayStats: null,
  // Today by default; JS owns the clock, the planner moves the day.
  plannerDay: "",
  serviceTypeDetail: null,
  planChain: { inForce: null, periods: [], issues: [] },
  typeCoverage: null,
  popLayer: null,
  popLayerBytes: null,
  popUrl: "",
  plannerMapUp: true,
  networkLabel: "",
  plannerSelection: null,
  plannerFocus: null,
});
