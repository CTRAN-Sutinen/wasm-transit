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
import type { PlanIssue } from "./types/cap_plans/PlanIssue";
import type { PlanPeriod } from "./types/cap_plans/PlanPeriod";
import type { TypeCoverage } from "./types/TypeCoverage";
import type { RtOverview } from "./types/RtOverview";
import type { StopBoard } from "./types/StopBoard";
import type { DayBlock } from "./types/DayBlock";
import type { BlockChainTrip } from "./types/BlockChainTrip";
import type { DraftView } from "./types/DraftView";
import type { DiversionRow } from "./types/DiversionRow";
import type { EntityRow } from "./types/EntityRow";
import type { RouteDetail } from "./types/RouteDetail";
import type { StopDetail } from "./types/StopDetail";
import type { DirectoryStop } from "./types/cap_directory/DirectoryStop";
import type { DirectoryRoute } from "./types/cap_directory/DirectoryRoute";
import type { FeedInfo } from "./types/FeedInfo";
import type { FeedSummary } from "./types/FeedSummary";
import type { Live } from "./capabilities.svelte";
import type { EntityKind, Focus, PlannerScreen } from "./planner";
import type { LineFeature } from "./mapview";
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
/// What the dispatcher has entered — cap-divert's `draft` answers what
/// it amounts to (app.diversionView): the default pattern, the stops
/// skipped, the flattened trace, whether it is ready.
export type DiversionDraft = {
  routeId: string;
  patternId: string; // the pattern anchored against; "" = cap-divert's default
  firstKept: string;
  lastKept: string;
  /// Interior stops the detour still serves (every other one is skipped).
  served: string[];
  addedMins: number;
  start: string;
  end: string;
  /// The committed trace, one entry per waypoint step (Undo reverts
  /// whole steps).
  slices: DetourSlice[];
};

export type AppState = {
  status: string,

  // Persona views: which workflow the panel serves. The map is shared
  // by all of them; the panel content is focused per persona.
  persona: "rider" | "dispatcher" | "planner",

  // The loaded feed's day-to-day state (the bytes live in the
  // host; every read answers through the capability bus).
  selectedStop: string | null,
  clock: number, // bumped to re-derive time-dependent views
  // Bumped whenever the *effective day* changes (the edit document
  // re-registered). Derived views (departure boards, block chains)
  // read this to know when to re-query. `clock` means "time moved";
  // `feedEpoch` means "data moved".
  feedEpoch: number,
  // Whether a feed is loaded (the bytes live with the host; every
  // read answers through the capability bus).
  feedLoaded: boolean,
  // The selected trip instance, "trip@start" (the model's identity).
  selectedTrip: string | null,
  tripFeature: LineFeature | null, // the selected trip's line (the host view builds it)

  // Validation reports for the edit document (the day views
  // validate per record at registration), aligned by
  // id. Replaced wholesale on every push.
  editReports: EditReport[],

  // GTFS-rt. The snapshots live in the rt capability by hash; the
  // LIVE DOCUMENT is its answer in model vocabulary (delays per
  // trip instance, vehicles with their status, the status summary) —
  // every live view renders from it, none re-joins rt.
  rtOverview: RtOverview | null, // the trip-updates payload's decode (alerts count)
  rtHash: string | null, // trip-updates snapshot, as cap-rt registered it
  vpHash: string | null, // vehicle-positions snapshot (a separate feed)
  live: Live | null,

  // The board at the selected stop — cap-board's whole answer
  // (pooling, realtime-first order, row status); off when the bus
  // cannot assemble it.
  board: StopBoard | null,
  boardOff: boolean,
  // Service keys running today (cap-calendar; decoration-grade).
  activeServices: string[],

  // The day's blocks (cap-runcut) and the selected block's chain,
  // each row naming its trip by id.
  blocks: DayBlock[],
  blocksOff: boolean,
  selectedBlock: string,
  blockChain: { trips: BlockChainTrip[] } | null,
  chainOff: boolean,

  // Trip planning (RAPTOR)
  plan: Itinerary | null,
  planNotice: string | null, // inline feedback when planning fails
  planBaseline: Itinerary | null, // same plan against the base feed when diversions are active
  itineraryFeatures: PlanFeature[], // GeoJSON features for drawing the plan on the map

  // Diversion editor form (see DiversionDraft)
  diversionDraft: DiversionDraft | null,
  // cap-divert's answers: the draft's view, the edit document's rows.
  diversionView: DraftView | null,
  diversionRows: DiversionRow[],
  pickingAnchor: "first" | "last" | null, // stop clicks set anchors
  drawingDetour: boolean, // map clicks append detour vertices
  // The selected route's alignment (route_path_geojson) — the ghost
  // the detour is drawn against. Plain data; MapView just draws it.
  routeContext: LineFeature | null,

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
  // The service-plan chain view, through the plans capability: the
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
  // The feed line: the feed's own identity and at-a-glance summary
  // (cap-gtfs), decoration-grade — null when the bus cannot answer.
  feedInfo: FeedInfo | null,
  feedSummary: FeedSummary | null,
  // Drill-down: the entity the planner is focused on, and what the
  // map should frame for it (plain data, MapView just draws it).
  plannerSelection: { kind: EntityKind; id: string } | null,
  plannerFocus: Focus | null,
  // The planner's lists, as cap-directory answers them: the rows of
  // the list screen on top (tagged with the kind they list, so a
  // screen never renders another's answer) and the selected route's
  // composition (tagged with its route), the selected stop's
  // serving routes (tagged with its stop), and every row of every
  // kind, unfiltered (the forms' pickers, the document's counts).
  plannerRows: { kind: EntityKind; rows: EntityRow[] } | null,
  routeDetail: { routeId: string; detail: RouteDetail | null } | null,
  stopDetail: { stopId: string; detail: StopDetail | null } | null,
  pickers: Record<EntityKind, EntityRow[]> | null,
};

/// The sync document (transit-sync `EditRecord[]`) — **owned here**.
/// Plain data under `$state.raw`: replaced wholesale, never mutated
/// in place. That is what makes it safe to persist (IndexedDB can
/// structured-clone it directly — no proxies) and to sync to an ops
/// server. wasm never owns it; the day views read it by content
/// hash and validate it per record at registration.
let editDocument = $state.raw<EditRecord[]>([]);
export function getEditDocument(): EditRecord[] {
  return editDocument;
}

/// Adopt a document cap-divert or cap-sync answered (a stored copy
/// is read through cap-divert's `sanitize` first — actions.readEdits).
export function setEditDocument(next: EditRecord[]): void {
  editDocument = next;
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
  readPlans.add(activePlanId);
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
  readPlans.add(id);
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
/** The plans whose in-memory copy is in the current shape: put by a
 * host (a module answered it) or read back through `readPlan`. */
const readPlans = new Set<string>();

/** Persist the plan set — serialized writes, one pending at a time
 * (the same discipline as the edit document's flushPersist). */
function persistPlans(): void {
  plansWrite = plansWrite
    .then(() => idbSet(PLANS_KEY, plans))
    .catch((e) => console.warn("plan persistence failed:", e));
}

/** Restore the plan set on startup (documents cross IN from
 * IndexedDB; JS owns them, so a shape guard keeps stale builds from
 * wedging the workspace). `read` canonicalizes each stored document
 * through a module (the host layer passes the resolver's
 * `readStoredNetwork`): a plan saved under older field names comes
 * back in the current shape and is persisted that way. A plan the
 * module refuses is kept as stored and reported, never dropped. */
export async function restorePlans(read?: (stored: unknown) => Promise<Network | null>): Promise<void> {
  try {
    const stored = await idbGet(PLANS_KEY);
    if (stored && typeof stored === "object") {
      const next: Record<string, Network> = {};
      for (const [key, doc] of Object.entries(stored as Record<string, unknown>)) {
        try {
          next[key] = (read ? await read(doc) : null) ?? (doc as Network);
          if (read) readPlans.add(key);
        } catch (e) {
          console.warn(`stored plan ${key} could not be read:`, e);
          next[key] = doc as Network;
        }
      }
      plans = next;
      if (read) persistPlans();
    }
    const active = localStorage.getItem(ACTIVE_PLAN_KEY);
    if (active && plans[active]) {
      activePlanId = active;
    }
  } catch (e) {
    console.warn("plan restore failed:", e);
  }
}

/** One stored plan read into the current shape on first use — the
 * on-demand half of a bare `restorePlans()`: a host that opens few of
 * many plans reads only those (each read is a whole-document round
 * trip through a module). The canonical copy replaces the stored one
 * in memory and is written with the plan set's next persist. A plan
 * the module refuses stays as stored and is reported. Plan metadata
 * (`plan`) reads the same in every stored shape, so a host may list
 * plans before reading them. */
export async function readPlan(id: string, read: (stored: unknown) => Promise<Network | null>): Promise<Network | null> {
  const doc = plans[id];
  if (!doc || readPlans.has(id)) return doc ?? null;
  try {
    const next = await read(doc);
    if (next && plans[id] === doc) plans = { ...plans, [id]: next };
  } catch (e) {
    console.warn(`stored plan ${id} could not be read:`, e);
  }
  readPlans.add(id);
  return plans[id];
}

/// The imported streets (detour authoring) — same discipline: plain
/// data, wholesale replacement. The routing graph lives in the
/// cap-streets capability (`streetsHash`); this holds the hash handle
/// and the streets document the import answered. Never proxied (a
/// county's document is tens of MB) and not yet persisted:
/// re-imported per session.
let streets = $state.raw<Streets | null>(null);
export function getStreets(): Streets | null {
  return streets;
}
export function setStreets(next: Streets | null): void {
  streets = next;
}

export const app = $state<AppState>({
  status: "loading…",

  persona: "rider",


  selectedStop: null,
  clock: 0,
  feedEpoch: 0,
  feedLoaded: false,
  selectedTrip: null,
  tripFeature: null,

  editReports: [],

  rtOverview: null,
  rtHash: null,
  vpHash: null,
  live: null,

  board: null,
  boardOff: false,
  activeServices: [],

  blocks: [],
  blocksOff: false,
  selectedBlock: "",
  blockChain: null,
  chainOff: false,

  plan: null,
  planNotice: null,
  planBaseline: null,
  itineraryFeatures: [],

  diversionDraft: null,
  diversionView: null,
  diversionRows: [],
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
  feedInfo: null,
  feedSummary: null,
  plannerSelection: null,
  plannerFocus: null,
  plannerRows: null,
  routeDetail: null,
  stopDetail: null,
  pickers: null,
});

/// The loaded directory's answers (cap-directory over the loaded
/// feed's network), held in host state so pickers and name lookups
/// read synchronously — written by `ensureDirectory` only (the module
/// loads once per feed; views key on the reactive fields).
export const directory = $state({
  networkHash: null as string | null,
  stops: [] as DirectoryStop[],
  routes: [] as DirectoryRoute[],
});
