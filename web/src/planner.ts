/// Planner-persona actions: authoring an edited network document
/// (transit-network `Network`) in the browser, against the same wasm
/// artifact the server runs. The document is JS-owned state
/// (state.svelte.js `networkDoc`, plain data, wholesale replacement —
/// the same discipline as the edit document); these functions are
/// the only writers:
///
///   source → deriveFromFeed (GTFS is the bootstrap), loadNetworkText/
///            loadNetworkFile (round-tripping documents)
///   check  → validate (issues from the artifact, never throws)
///   edit   → NetworkEdits through the authoring capability
///            (cap-author `apply`): the gestures carry the rules,
///            this file adopts the answer — it never edits a document
///   apply  → materializeNetwork (registers a derived feed; the
///            Rider/Dispatcher views then run on the authored network)
///
/// The planner works off an existing network: there is no blank
/// canvas. GTFS-the-format never appears here — inference,
/// validation and materialization live in the NETWORK CAPABILITY
/// (cap-network over the shared transit-codec library; the monolith
/// keeps no copy), and this file only moves documents across the bus.

import { app, getNetworkDoc, setNetworkDoc, getAllPlans, getActivePlanId, putPlan, setActivePlan, removePlan as removePlanFromStore } from "./state.svelte";
import type { CoverageStats } from "./types/CoverageStats";
import type { DayState } from "./types/DayState";
import type { HeadwayBand } from "./types/HeadwayBand";
import type { ImportNote } from "./types/ImportNote";
import type { Mode } from "./types/Mode";
import type { Network } from "./types/Network";
import type { NetworkStats } from "./types/NetworkStats";
import type { PlanInfo } from "./types/cap_plans/PlanInfo";
import { pushEdits, serviceDate, FEED_KEY, setFeedBytes, refreshLive } from "./actions";
import { get as idbGet } from "idb-keyval";
import {
  ensureAnalysisNetwork,
  ensureAnalysisLayer,
  analysisModule,
  analysisCap,
  ensureNetworkDocument,
  networkModule,
  networkCap,
  base64Decode,
  plansChain,
  authorCap,
  authorModule,
  type Applied,
} from "./capabilities.svelte";
import type { NetworkEdit } from "./types/NetworkEdit";
import type { CreatedEntity } from "./types/CreatedEntity";

export type EntityKind = "stop" | "route" | "pattern" | "schedule" | "service_type" | "block";

/// One planner screen. The workspace is a stack: the PLAN CHAIN at
/// the root (service changes are the frame everything else lives
/// in), overview per active plan, drill-downs push, Back pops.
export type PlannerScreen =
  | { view: "plans" }
  | { view: "overview" }
  | { view: "document" }
  | { view: "routes" }
  | { view: "stops" }
  | { view: "patterns" }
  | { view: "schedules" }
  | { view: "service_types" }
  | { view: "blocks" }
  | { view: "route"; id: string }
  | { view: "stop"; id: string }
  | { view: "pattern"; id: string }
  | { view: "schedule"; id: string }
  | { view: "service_type"; id: string }
  | { view: "block"; id: string };

export type Focus = { lines: number[][][]; points: number[][] };
export type DayName = "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun";

// ---------------------------------------------------------------
// time helpers — service-day times, hours may pass 24
// ---------------------------------------------------------------

export function fmtHHMMSS(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  return [h, m, s].map((n) => String(n).padStart(2, "0")).join(":");
}

export function parseHHMMSS(text: string | null | undefined): number | null {
  const parts = (text ?? "").trim().split(":").map(Number);
  if (parts.length < 2 || parts.length > 3 || parts.some((n) => Number.isNaN(n))) return null;
  const [h, m, s = 0] = parts;
  if (m < 0 || m > 59 || s < 0 || s > 59 || h < 0 || h > 47) return null;
  return h * 3600 + m * 60 + s;
}

// ---------------------------------------------------------------
// source / validate / apply
// ---------------------------------------------------------------

/** Re-run validation on the current document — through the network
 * capability, the codec's home since the extraction. The op always
 * returns the whole issue list (snake_case variant + fields), never
 * throws, and the UI never blocks on it.
 *
 * Stage B of the extraction: when the capability cannot assemble
 * (offline, stale manifest), VALIDATION IS OFF and the planner says
 * so — authoring continues, and `materializeNetwork` still refuses
 * broken documents (it exports through the same capability). */
export let validatePath: "bus" | null = null;
let validateSeq = 0;

export async function validateNetwork(): Promise<void> {
  const doc = getNetworkDoc();
  const seq = ++validateSeq;
  if (!doc) {
    app.networkIssues = [];
    app.networkValidationOff = false;
    validatePath = null;
    return;
  }
  const networkHash = await ensureNetworkDocument(doc);
  if (seq !== validateSeq) return;
  if (!networkHash) {
    app.networkIssues = [];
    app.networkValidationOff = true;
    validatePath = null;
    app.status = "Network capability unavailable — document validation is off until it loads.";
    return;
  }
  const module = await networkModule();
  if (!module || seq !== validateSeq) return;
  app.networkIssues = networkCap(module).validate(networkHash);
  app.networkValidationOff = false;
  validatePath = "bus";
}

// Validation is whole-document work in the artifact; on a real
// agency feed that is real work per edit. Debounce it: the chip
// catches up a beat after the last change instead of on every one.
let validateTimer: ReturnType<typeof setTimeout> | null = null;
function validateNetworkSoon(): void {
  if (validateTimer !== null) clearTimeout(validateTimer);
  validateTimer = setTimeout(validateNetwork, 250);
}

function setDoc(next: Network, label: string, { validateNow = false } = {}): void {
  setNetworkDoc(next);
  app.networkLabel = label;
  if (validateNow) {
    if (validateTimer !== null) clearTimeout(validateTimer);
    validateNetwork();
  } else {
    validateNetworkSoon();
  }
  refreshAnalysisSoon();
}

// ---------------------------------------------------------------
// analysis — the planner's design axis, answered by the artifact
// ---------------------------------------------------------------

/** Coverage analysis parameters (presentation choices; the artifact
 * takes them as op arguments): within 400 m of a stop is "served";
 * headways at or under 15 minutes are "frequent". */
const COVERAGE_RADIUS_M = 400;
const FREQUENT_HEADWAY_SECS = 15 * 60;
/** The monolith's default for a frequent pocket (dispatch's null → 6);
 * the capability path passes it explicitly so both paths agree. */
const MIN_DEPARTURES = 6;

let analysisTimer: ReturnType<typeof setTimeout> | null = null;
/** Async analysis runs out of order with edits; a stale refresh must
 * never overwrite a newer one's results. */
let analysisSeq = 0;

/** Which path served the last analysis refresh — the bus (the
 * capability extracted from the monolith) or the monolith fallback.
 * Exposed for e2e/inspection like the map hook. */
export let analysisPath: "bus" | "monolith" | null = null;

/** Whole-document analysis is real work in the artifact on a real
 * feed — debounce it like validation, one beat after the last edit. */
function refreshAnalysisSoon(): void {
  if (analysisTimer !== null) clearTimeout(analysisTimer);
  analysisTimer = setTimeout(refreshAnalysis, 250);
}

/** Recompute the planner's views: the service-type overview, per-type
 * coverage (when a population layer is loaded), and the drilled-in
 * type's detail. Best-effort: analysis never blocks authoring.
 *
 * Stage C: the numbers come from the ANALYSIS CAPABILITY — the ops
 * are deleted from the monolith. The capability registers the
 * document once per edit and queries by hash; when it cannot
 * assemble (offline, stale manifest), the planner degrades to
 * authoring-without-numbers and says so — never silently. */
export async function refreshAnalysis(): Promise<void> {
  const doc = getNetworkDoc();
  const seq = ++analysisSeq;
  const clearViews = () => {
    app.typeOverview = [];
    app.dayStats = null;
    app.serviceTypeDetail = null;
    app.typeCoverage = null;
  };
  if (!doc) {
    clearViews();
    return;
  }

  try {
    const networkHash = await ensureAnalysisNetwork(doc);
    if (seq !== analysisSeq) return;
    if (!networkHash) {
      // The capability did not assemble: numbers are off, authoring
      // continues. Degrade loudly, per capability.
      clearViews();
      analysisPath = null;
      app.status = "Analysis capability unavailable — planner numbers are off until it loads.";
      return;
    }
    const module = await analysisModule();
    if (!module || seq !== analysisSeq) return;
    const analysis = analysisCap(module);
    const top = app.plannerStack[app.plannerStack.length - 1];
    app.typeOverview = analysis.serviceTypesOverview(networkHash);
    // The day-shape header: the picked day's numbers (the
    // "what does a Wednesday look like" question — dead UI surface
    // until this screen existed).
    app.dayStats = analysis.networkStats(networkHash, app.plannerDay || serviceDate());
    if (app.popLayer && app.popLayerBytes) {
      const layer = await ensureAnalysisLayer(app.popLayerBytes);
      if (seq !== analysisSeq) return;
      app.typeCoverage = layer
        ? analysis.typeCoverage(
            networkHash,
            layer.layer_hash,
            COVERAGE_RADIUS_M,
            FREQUENT_HEADWAY_SECS,
            MIN_DEPARTURES,
          )
        : null;
    } else {
      app.typeCoverage = null;
    }
    app.serviceTypeDetail =
      top && top.view === "service_type" && "id" in top
        ? analysis.serviceTypeDetail(networkHash, top.id)
        : null;
    analysisPath = "bus";
  } catch (e) {
    console.warn("analysis failed:", e);
    clearViews();
  }
}

/** Recompute coverage after a population layer import/clear. */
export function refreshCoverage(): void {
  refreshAnalysis();
}

// ---------------------------------------------------------------
// service plans — the chain: forks, picks, and the frame of
// everything else in the workspace
// ---------------------------------------------------------------

function newStoreKey(): string {
  return `plan-${crypto.randomUUID ? crypto.randomUUID() : Date.now()}`;
}

function newPlanId(): string {
  return crypto.randomUUID ? crypto.randomUUID() : `plan-${Date.now()}`;
}

/** Chain metadata of every stored plan — only framed plans (with a
 * plan identity) participate in the chain ops. */
function planInfos(): PlanInfo[] {
  return Object.values(getAllPlans())
    .filter((d) => d.plan?.id)
    .map((d) => ({ id: d.plan.id, name: d.plan.name, effective_from: d.plan.effective_from ?? null }));
}

/** Refresh the chain view through the PLANS CAPABILITY: the plan in
 * force today, each pick's governing period, and chain issues.
 * Cheap — run on workspace entry and every plan CRUD. Async (the
 * module assembles like every capability) and sequenced: a stale
 * refresh never lands over a newer one. When the capability cannot
 * assemble, the chain is off and the planner says so — never
 * silently. */
let planChainSeq = 0;
export async function refreshPlanChain(): Promise<void> {
  const infos = planInfos();
  const seq = ++planChainSeq;
  try {
    const chain = await plansChain(infos, serviceDate());
    if (seq !== planChainSeq) return;
    if (!chain) {
      app.planChain = { inForce: null, periods: [], issues: [] };
      app.status = "Plans capability unavailable — the chain is off until it loads.";
      return;
    }
    app.planChain = chain;
  } catch (e) {
    console.warn("plan chain refresh failed:", e);
  }
}

/** The service-change workflow: fork a plan into the NEXT pick. The
 * fork carries the whole world it changes — permanent stop/route
 * changes are only made at picks — with fresh plan metadata. */
export async function forkPlan(fromKey: string, name: string, effectiveFrom: string): Promise<void> {
  const src = getAllPlans()[fromKey];
  if (!src) {
    app.status = "Fork needs a source plan.";
    return;
  }
  if (!name.trim() || !effectiveFrom) {
    app.status = "A new pick needs a name and an effective date.";
    return;
  }
  // Edits still in flight belong to the source: fork what they made.
  await editChain;
  const source = getAllPlans()[fromKey] ?? src;
  // The fork is the source document under a new plan frame — a
  // set_plan edit; the host mints the sync identity (ids from
  // randomness are the host's, never a module's).
  const applied = await applyTo(source, [
    { edit: "set_plan", plan: { id: newPlanId(), name: name.trim(), effective_from: effectiveFrom } },
  ]);
  if (!applied) return;
  const doc = applied.network;
  const key = `plan-${doc.plan.id}`;
  putPlan(key, doc);
  setActivePlan(key);
  app.plannerStack = [{ view: "plans" }, { view: "overview" }];
  app.plannerMapUp = true;
  app.plannerSelection = null;
  app.plannerFocus = null;
  setDoc(doc, `pick "${doc.plan.name}"`, { validateNow: true });
  refreshPlanChain();
  app.status =
    `Forked into "${doc.plan.name}" (effective ${effectiveFrom}) — the pick date is a ` +
    "target; it can move until it's real. Structure changes live here.";
}

/** Open a plan: it becomes the active document the whole workspace
 * operates on. */
export function switchPlan(key: string): void {
  if (!getAllPlans()[key]) return;
  setActivePlan(key);
  app.plannerStack = [{ view: "plans" }, { view: "overview" }];
  app.plannerMapUp = true;
  app.plannerSelection = null;
  app.plannerFocus = null;
  validateNetwork();
  refreshAnalysis();
  refreshPlanChain();
}

/** Remove a plan from the set (the workspace root's delete). */
export function deletePlan(key: string): void {
  removePlanFromStore(key);
  app.plannerSelection = null;
  app.plannerFocus = null;
  refreshPlanChain();
  app.status = "Plan removed from the set.";
}

/** Frame the ACTIVE document as a pick: name and effective date
 * (the target service change date). Framing assigns the sync
 * identity. Dates are targets — they move during planning. */
export async function updatePlanMeta({ name, effectiveFrom }: { name?: string; effectiveFrom?: string | null }): Promise<void> {
  // Field-wise, in order: the model merges (update_plan), so quick
  // successive changes never overwrite each other. The host mints the
  // identity an unversioned plan is stamped with.
  await edit([
    {
      edit: "update_plan",
      name,
      effective_from: effectiveFrom === undefined ? undefined : effectiveFrom ?? "",
      id: newPlanId(),
    },
  ]);
  refreshPlanChain();
}

/** "12 min headway" / "—" for nullable headway stats. */
export function fmtHeadwaySafe(secs: number | null | undefined): string {
  if (secs == null) return "—";
  return `${Math.round(secs / 60)} min headway`;
}

/** Trip count of a schedule (exact times: starts; bands: expanded). */
export function scheduleTripCount(schedule: { kind: string; starts?: number[]; bands?: { start_secs: number; end_secs: number; headway_secs: number }[] }): number {
  if (schedule.kind === "exact_times") return schedule.starts?.length ?? 0;
  return (schedule.bands ?? []).reduce(
    (n, b) => n + (b.headway_secs > 0 ? Math.ceil((b.end_secs - b.start_secs) / b.headway_secs) : 0),
    0,
  );
}

/** The planner's bootstrap: infer a network document from the feed
 * the app already has loaded (Rider/Dispatcher views load feeds).
 * Stage B of the extraction: the inference runs in the NETWORK
 * CAPABILITY — the same bytes, the same laws the CLI dogfoods — and
 * the raw feed bytes come from the load-time cache (JS owns the
 * documents; the artifact's feed registry is a view, not a store). */
export async function deriveFromFeed(): Promise<void> {
  if (!app.feedLoaded) {
    app.status = "Load a GTFS feed first (Rider or Dispatcher view).";
    return;
  }
  // The capability runs on this thread: yield a frame so the status
  // paints before the (potentially seconds-long) load + inference +
  // JSON transfer on a real feed. A worker host is the eventual fix
  // for the freeze itself.
  app.status = "Deriving a network from the loaded feed…";
  await new Promise((r) => setTimeout(r, 30));
  try {
    const cached = await idbGet<{ label: string; bytes: Uint8Array }>(FEED_KEY);
    if (!cached) {
      app.status = "The raw feed bytes are not cached — reload the GTFS feed, then derive.";
      return;
    }
    const module = await networkModule();
    if (!module) {
      app.status =
        "Network capability unavailable — bootstrap needs it (the capability bus did not assemble).";
      return;
    }
    const cap = networkCap(module);
    const loaded = cap.loadFeed(new Uint8Array(cached.bytes));
    const { network: net, notes } = cap.import(loaded.feedHash);
    app.networkNotes = notes;
    // The derived document is a plan in the set — unversioned until
    // the planner frames it (gives it a pick date). The chain is the
    // workspace root; the plan set outlives every session.
    net.plan = { id: "", name: "", effective_from: null };
    const key = newStoreKey();
    putPlan(key, net);
    setActivePlan(key);
    app.plannerStack = [{ view: "plans" }, { view: "overview" }];
    app.plannerMapUp = true;
    app.plannerSelection = null;
    app.plannerFocus = null;
    setDoc(net, "derived from loaded feed", { validateNow: true });
    refreshPlanChain();
    app.status = `Derived a network from the loaded feed (${Object.keys(net.patterns).length} patterns, ${Object.keys(net.schedules).length} schedules) — frame it as a pick, edit it, then load it into the app.`;
  } catch (e) {
    app.status = `Error: ${e?.message ?? e}`;
  }
}

export function loadNetworkText(text: string, label: string): boolean {
  let doc;
  try {
    doc = JSON.parse(text);
  } catch (e) {
    app.status = `Not valid JSON: ${e.message}`;
    return false;
  }
  // An opened document becomes a plan in the set (its own plan
  // metadata rides along inside the document).
  if (!doc.plan) doc.plan = { id: "", name: "", effective_from: null };
  const key = doc.plan.id ? `plan-${doc.plan.id}` : newStoreKey();
  putPlan(key, doc);
  setActivePlan(key);
  app.plannerStack = [{ view: "plans" }, { view: "overview" }];
  app.plannerMapUp = true;
  app.plannerSelection = null;
  app.plannerFocus = null;
  setDoc(doc, label, { validateNow: true });
  refreshPlanChain();
  const n = app.networkIssues.length;
  app.status =
    n === 0
      ? `Network "${doc.name ?? label}" parsed and valid — load it into the app to use it.`
      : `Network "${doc.name ?? label}" parsed with ${n} issue(s).`;
  return true;
}

export async function loadNetworkFile(file: File): Promise<void> {
  loadNetworkText(await file.text(), file.name);
}

export function exportNetworkText(): string {
  const doc = getNetworkDoc();
  return doc ? JSON.stringify(doc, null, 2) : "";
}

export function downloadNetwork(): void {
  const doc = getNetworkDoc();
  if (!doc) return;
  const name = (doc.name ?? "network").replace(/[^\w.-]+/g, "-").toLowerCase();
  const blob = new Blob([JSON.stringify(doc, null, 2)], { type: "application/json" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `${name}.network.json`;
  a.click();
  URL.revokeObjectURL(a.href);
}

/** Materialize the document into the app's active feed (same registry
 * a GTFS zip lands in). Mirrors loadGtfsBytes's side effects minus
 * caching: the document, not a zip, is the source of truth. Stage B
 * of the extraction: the document EXPORTS through the network
 * capability (the publish path — deterministic GTFS zip bytes), and
 * the produced bytes land where every feed lands (the feed document
 * cache the whole bus reads). Invalid documents are refused — the
 * capability's own gate, not a JS guess. */
export async function materializeNetwork(): Promise<void> {
  const doc = getNetworkDoc();
  if (!doc) {
    app.status = "No network document loaded.";
    return;
  }
  await validateNetwork(); // flush any debounced check first — this is the gate
  if (app.networkIssues.length > 0) {
    app.status = `Cannot load: ${app.networkIssues.length} validation issue(s) to fix first.`;
    return;
  }
  try {
    const networkHash = await ensureNetworkDocument(doc);
    const module = await networkModule();
    if (!networkHash || !module) {
      app.status =
        "Network capability unavailable — cannot materialize until it loads (the capability bus did not assemble).";
      return;
    }
    const { feedHash, zipB64 } = networkCap(module).export(networkHash);
    const bytes = base64Decode(zipB64);
    setFeedBytes(bytes); // the bus reads this document everywhere
    app.feedLoaded = true;
    app.selectedStop = null;
    app.selectedTrip = null;
    app.tripFeature = null;
    app.plan = null;
    app.planBaseline = null;
    app.planNotice = null;
    pushEdits();
    void refreshLive(); // the new feed's trip map: live re-resolves
    app.status = `Materialized "${doc.name}" — the Rider and Dispatcher views now run on the authored network (published GTFS ${feedHash.slice(0, 19)}…).`;
  } catch (e) {
    app.status = `Error: ${e?.message ?? e}`;
  }
}

// ---------------------------------------------------------------
// editing — every change is a list of NetworkEdits applied by the
// AUTHORING CAPABILITY (cap-author). The gestures carry the model's
// coherence rules (timing aligned with timepoints, starts sorted and
// unique, layovers only between trips, minted ids) — the browser,
// the runner, and the server author the same document from the same
// edits. This file sends edits and adopts the answer; it never
// changes a document itself.
// ---------------------------------------------------------------

/** Documents registered in the authoring module, by identity —
 * per module instance (a module re-fetched starts empty). */
const authored = new WeakMap<object, WeakMap<Network, string>>();

async function applyTo(doc: Network, edits: NetworkEdit[]): Promise<Applied | null> {
  const module = await authorModule();
  if (!module) {
    app.status = "Authoring capability unavailable — edits are off until it loads (the capability bus did not assemble).";
    return null;
  }
  const cap = authorCap(module);
  let hashes = authored.get(module);
  if (!hashes) authored.set(module, (hashes = new WeakMap()));
  const hash = hashes.get(doc) ?? cap.loadNetwork(doc);
  try {
    const applied = cap.apply(hash, edits);
    hashes.set(applied.network, applied.networkHash);
    return applied;
  } catch (e) {
    app.status = `Edit refused: ${(e as Error).message}`;
    return null;
  }
}

/** Edits apply in the order they are made: each change chains on the
 * document the previous one answered. */
let editChain: Promise<unknown> = Promise.resolve();

/** Apply edits to the plan that was ACTIVE when they were made (a
 * change lands on its own plan even if the planner has moved on)
 * and adopt the answer — the issues come with it (no second
 * validation pass). Resolves to what the change created. */
export function edit(edits: NetworkEdit[]): Promise<CreatedEntity[]> {
  const key = getActivePlanId();
  const run = editChain.then(async () => {
    const doc = getAllPlans()[key];
    if (!doc) return [];
    const applied = await applyTo(doc, edits);
    if (!applied) return [];
    putPlan(key, applied.network);
    if (getActivePlanId() === key) {
      app.networkIssues = applied.issues;
      app.networkValidationOff = false;
      validatePath = "bus";
      refreshAnalysisSoon();
    }
    return applied.created;
  });
  editChain = run.catch(() => undefined);
  return run;
}

/** Create, then open the new entity's editor (set metadata in context). */
async function create(kind: EntityKind, gesture: NetworkEdit): Promise<void> {
  const [made] = await edit([gesture]);
  if (made) selectEntity(kind, made.id);
}

// --- stops ------------------------------------------------------

export function addStop(): void {
  void create("stop", { edit: "add_stop" });
}

export function updateStop(
  id: string,
  { name, lat, lon, transfer }: { name?: string; lat?: number; lon?: number; transfer?: boolean },
): void {
  void edit([{ edit: "update_stop", id, name, lat, lon, transfer }]);
}

export function removeStop(id: string): void {
  void edit([{ edit: "remove_stop", id }]);
}

// --- routes -----------------------------------------------------

const MODES: Mode[] = ["bus", "tram", "subway", "rail", "ferry", "cable_car", "gondola", "funicular", "trolleybus", "monorail", "other"];

export function modes(): Mode[] {
  return MODES;
}

export function addRoute(): void {
  void create("route", { edit: "add_route" });
}

export function updateRoute(id: string, { name, mode }: { name?: string; mode?: Mode }): void {
  void edit([{ edit: "update_route", id, name, mode }]);
}

export function removeRoute(id: string): void {
  void edit([{ edit: "remove_route", id }]);
}

// --- patterns ---------------------------------------------------

export function addPattern(routeId?: string): void {
  void create("pattern", { edit: "add_pattern", route: routeId });
}

export function updatePattern(id: string, { name, route }: { name?: string; route?: string }): void {
  void edit([{ edit: "update_pattern", id, name, route }]);
}

export function removePattern(id: string): void {
  void edit([{ edit: "remove_pattern", id }]);
}

export function addPatternStop(patternId: string, stopId: string): void {
  if (!stopId) return;
  void edit([{ edit: "insert_pattern_stop", pattern: patternId, stop: stopId }]);
}

export function removePatternStop(patternId: string, index: number): void {
  void edit([{ edit: "remove_pattern_stop", pattern: patternId, index }]);
}

export function setPatternStop(patternId: string, index: number, { stop, distance_m }: { stop?: string; distance_m?: number }): void {
  void edit([{ edit: "set_pattern_stop", pattern: patternId, index, stop, distance_m }]);
}

export function togglePatternTimepoint(patternId: string, index: number): void {
  void edit([{ edit: "toggle_timepoint", pattern: patternId, index }]);
}

// --- timing profiles --------------------------------------------

export function addTimingProfile(patternId: string): void {
  void edit([{ edit: "add_timing_profile", pattern: patternId }]);
}

export function removeTimingProfile(patternId: string, index: number): void {
  void edit([{ edit: "remove_timing_profile", pattern: patternId, index }]);
}

export function setTimingProfileField(patternId: string, index: number, { start_secs, end_secs }: { start_secs?: number | null; end_secs?: number | null }): void {
  void edit([{ edit: "set_timing_profile", pattern: patternId, index, start_secs: start_secs ?? undefined, end_secs: end_secs ?? undefined }]);
}

export function setTimepointTime(patternId: string, profileIndex: number, timepointIndex: number, secs: number | null): void {
  if (secs === null) return; // an unparsed field is not a change
  void edit([{ edit: "set_timepoint_time", pattern: patternId, profile: profileIndex, timepoint: timepointIndex, secs }]);
}

// --- calendars --------------------------------------------------

const DAYS: DayName[] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

export function dayNames(): DayName[] {
  return DAYS;
}

export function addServiceType(): void {
  // The host owns the clock: a new type runs a year from the day the
  // planner is looking at.
  void create("service_type", { edit: "add_service_type", from: app.plannerDay || serviceDate() });
}

export function toggleServiceTypeDay(id: string, day: DayName): void {
  void edit([{ edit: "toggle_service_day", id, day }]);
}

export function setServiceTypeRange(id: string, index: number, { start, end }: { start?: string; end?: string }): void {
  void edit([{ edit: "set_service_range", id, index, start: start || undefined, end: end || undefined }]);
}

export function addServiceTypeRange(id: string): void {
  void edit([{ edit: "add_service_range", id }]);
}

export function removeServiceTypeRange(id: string, index: number): void {
  void edit([{ edit: "remove_service_range", id, index }]);
}

export function addServiceTypeException(id: string, date: string, state: DayState): void {
  if (!date) return;
  void edit([{ edit: "set_service_exception", id, date, state }]);
}

export function removeServiceTypeException(id: string, date: string): void {
  void edit([{ edit: "remove_service_exception", id, date }]);
}

export function removeServiceType(id: string): void {
  void edit([{ edit: "remove_service_type", id }]);
}

export function updateServiceType(id: string, { name }: { name?: string }): void {
  if (name === undefined) return;
  void edit([{ edit: "update_service_type", id, name }]);
}

// --- schedules (one pattern's trips under one service type) ---

export function addSchedule(): void {
  void create("schedule", { edit: "add_schedule" });
}

export function setScheduleRefs(id: string, { pattern, service_type }: { pattern?: string; service_type?: string }): void {
  void edit([{ edit: "set_schedule_refs", id, pattern: pattern || undefined, service_type: service_type || undefined }]);
}

export function removeSchedule(id: string): void {
  void edit([{ edit: "remove_schedule", id }]);
}

export function setScheduleStarts(scheduleId: string, starts: number[]): void {
  void edit([{ edit: "set_starts", schedule: scheduleId, starts }]);
}

/** Move one trip of an exact-times schedule — cap-author's
 * `retime_trip` carries the blocks along (a retime never strands the
 * runcut). */
export function retimeScheduleStart(scheduleId: string, fromSecs: number, toSecs: number): void {
  void edit([{ edit: "retime_trip", schedule: scheduleId, from_secs: fromSecs, to_secs: toSecs }]);
}

export function addScheduleStart(scheduleId: string): void {
  void edit([{ edit: "add_start", schedule: scheduleId }]);
}

export function removeScheduleStart(scheduleId: string, index: number): void {
  void edit([{ edit: "remove_start", schedule: scheduleId, index }]);
}

/** Set one field of one headway band. */
export function setBandField(scheduleId: string, index: number, field: keyof HeadwayBand, value: number): void {
  void edit([{ edit: "set_band", schedule: scheduleId, index, [field]: value }]);
}

export function addBand(scheduleId: string): void {
  void edit([{ edit: "add_band", schedule: scheduleId }]);
}

export function removeBand(scheduleId: string, index: number): void {
  void edit([{ edit: "remove_band", schedule: scheduleId, index }]);
}

// --- blocks (the runcut) ----------------------------------------

export function addBlock(): void {
  void create("block", { edit: "add_block" });
}

export function removeBlock(id: string): void {
  void edit([{ edit: "remove_block", id }]);
}

export function addBlockTrip(blockId: string, scheduleId: string, startSecs: number | null): void {
  if (!scheduleId || startSecs === null) {
    app.status = "Pick a schedule and a valid start time first.";
    return;
  }
  void edit([{ edit: "add_block_trip", block: blockId, trip: { schedule: scheduleId, start_secs: startSecs } }]);
}

export function removeBlockTrip(blockId: string, index: number): void {
  void edit([{ edit: "remove_block_trip", block: blockId, index }]);
}

export function setBlockLayover(blockId: string, afterTrip: number, secs: number | null): void {
  void edit([{ edit: "set_layover", block: blockId, after_trip: afterTrip, secs }]);
}

// ---------------------------------------------------------------
// drill-down: select an entity, focus the map on it
// ---------------------------------------------------------------

/// Ordered [lon, lat] of a pattern's stops (skips stops without
/// coordinates).
function patternCoords(doc: Network, patternId: string | undefined | null): number[][] {
  const p = patternId ? doc?.patterns?.[patternId] : undefined;
  if (!p) return [];
  const out = [];
  for (const s of p.stops) {
    const stop = doc.stops?.[s.stop];
    if (stop) out.push([stop.lon, stop.lat]);
  }
  return out;
}

/// The pattern's drawn alignment as [lon, lat] lines: the authored
/// path when there is one, otherwise straight chords between its
/// stops (the pre-shape rendering — still right for paths that never
/// got one).
function patternLines(doc: Network, patternId: string | undefined | null): number[][][] {
  const p = patternId ? doc?.patterns?.[patternId] : undefined;
  if (!p) return [];
  if (p.path && p.path.length >= 2) return [p.path.map((c) => [c.lon, c.lat])];
  const coords = patternCoords(doc, patternId);
  return coords.length >= 2 ? [coords] : [];
}

function focusFor(doc: Network | null, kind: EntityKind, id: string): Focus | null {
  if (!doc) return null;
  if (kind === "pattern") {
    return { lines: patternLines(doc, id), points: patternCoords(doc, id) };
  }
  if (kind === "schedule") {
    const pattern = doc.schedules?.[id]?.pattern ?? null;
    return { lines: patternLines(doc, pattern), points: patternCoords(doc, pattern) };
  }
  if (kind === "service_type") {
    // The type's whole network: every pattern scheduled under it.
    const lines: number[][][] = [];
    const points: number[][] = [];
    const patternIds = new Set(
      Object.values(doc.schedules ?? {})
        .filter((s) => s.service_type === id)
        .map((s) => s.pattern),
    );
    for (const pid of patternIds) {
      lines.push(...patternLines(doc, pid));
      points.push(...patternCoords(doc, pid));
    }
    return { lines, points };
  }
  if (kind === "route") {
    const lines: number[][][] = [];
    const points: number[][] = [];
    for (const [pid, p] of Object.entries(doc.patterns ?? {})) {
      if (p.route !== id) continue;
      lines.push(...patternLines(doc, pid));
      points.push(...patternCoords(doc, pid));
    }
    return { lines, points };
  }
  if (kind === "stop") {
    const s = doc.stops?.[id];
    return s ? { lines: [], points: [[s.lon, s.lat]] } : null;
  }
  return null; // calendars and blocks have no geometry
}

/** Drill into one entity: push its screen and frame it on the map. */
export function selectEntity(kind: EntityKind, id: string): void {
  if (!id) return;
  const screen: PlannerScreen = { view: kind, id } as PlannerScreen;
  plannerNavigate(screen);
  app.plannerFocus = focusFor(getNetworkDoc(), kind, id);
}

/** Push a screen onto the stack. Data screens hide the map (one
 * laptop screen: tables get the width); entity screens bring it
 * back for framing. */
export function plannerNavigate(screen: PlannerScreen): void {
  app.plannerStack = [...app.plannerStack, screen];
  app.plannerMapUp = screenHasMap(screen);
  syncSelection();
  if (screen.view === "service_type") refreshAnalysisSoon(); // drill-down detail
}

/** Pop one screen (the Back button). */
export function plannerBack(): void {
  if (app.plannerStack.length <= 1) return;
  app.plannerStack = app.plannerStack.slice(0, -1);
  const top = app.plannerStack[app.plannerStack.length - 1];
  app.plannerMapUp = screenHasMap(top);
  syncSelection();
  if (top.view === "service_type") refreshAnalysisSoon();
}

/** Jump to a screen from the menu — a fresh stack rooted there. */
export function plannerGo(view: PlannerScreen["view"]): void {
  app.plannerStack = [{ view: "overview" }, { view } as PlannerScreen];
  app.plannerMapUp = screenHasMap(app.plannerStack[app.plannerStack.length - 1]);
  syncSelection();
}

/** Back to the overview (the root). */
export function plannerHome(): void {
  app.plannerStack = [{ view: "overview" }];
  app.plannerMapUp = true;
  syncSelection();
}

/** Manual map toggle — the per-screen default is a default. */
export function togglePlannerMap(): void {
  app.plannerMapUp = !app.plannerMapUp;
}

function screenHasMap(screen: PlannerScreen): boolean {
  // The map is a view: entity screens and the service-type day view
  // frame things on it; pure data screens give the rail the width.
  return (
    screen.view === "overview" ||
    screen.view === "route" ||
    screen.view === "stop" ||
    screen.view === "pattern" ||
    screen.view === "schedule" ||
    screen.view === "service_type"
  );
}

/** Keep plannerSelection (what forms read) equal to the entity on
 * top of the stack, and clear the map focus when it is gone. */
function syncSelection(): void {
  const top = app.plannerStack[app.plannerStack.length - 1];
  const kind = ["route", "stop", "pattern", "schedule", "service_type", "block"].includes(top.view);
  app.plannerSelection =
    kind && "id" in top ? { kind: top.view as EntityKind, id: top.id } : null;
}

/** Clear focus (persona switch away, or a removed entity). */
export function clearSelection(): void {
  app.plannerSelection = null;
  app.plannerFocus = null;
}
