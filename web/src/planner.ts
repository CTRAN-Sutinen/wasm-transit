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
///   edit   → one mutation path: mutateNetwork clones, edits,
///            replaces, revalidates — every entity CRUD below it
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
import type { PlanInfo } from "./types/PlanInfo";
import { pushEdits, serviceDate, FEED_KEY, setFeedBytes } from "./actions";
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
} from "./capabilities.svelte";

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
  if (!doc || !app.wasm) {
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
      const layerHash = await ensureAnalysisLayer(app.popLayerBytes);
      if (seq !== analysisSeq) return;
      app.typeCoverage = layerHash
        ? analysis.typeCoverage(
            networkHash,
            layerHash,
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

/** Refresh the chain view from the artifact: the plan in force
 * today, each pick's governing period, and chain issues. Cheap —
 * run on workspace entry and every plan CRUD. */
export function refreshPlanChain(): void {
  if (!app.wasm) return;
  const infos = planInfos();
  try {
    app.planChain = {
      inForce: app.wasm.plans_resolve(infos, serviceDate()),
      periods: app.wasm.plans_periods(infos),
      issues: app.wasm.plans_validate(infos),
    };
  } catch (e) {
    console.warn("plan chain refresh failed:", e);
  }
}

/** The service-change workflow: fork a plan into the NEXT pick. The
 * fork carries the whole world it changes — permanent stop/route
 * changes are only made at picks — with fresh plan metadata. */
export function forkPlan(fromKey: string, name: string, effectiveFrom: string): void {
  const src = getAllPlans()[fromKey];
  if (!src) {
    app.status = "Fork needs a source plan.";
    return;
  }
  if (!name.trim() || !effectiveFrom) {
    app.status = "A new pick needs a name and an effective date.";
    return;
  }
  const doc = structuredClone(src);
  doc.plan = { id: newPlanId(), name: name.trim(), effective_from: effectiveFrom };
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
export function updatePlanMeta({ name, effectiveFrom }: { name?: string; effectiveFrom?: string | null }): void {
  mutateNetwork((d) => {
    if (!d.plan.id) d.plan.id = newPlanId();
    if (name !== undefined) d.plan.name = name;
    if (effectiveFrom !== undefined) d.plan.effective_from = effectiveFrom === "" ? null : effectiveFrom;
  });
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
  if (!app.gtfs) {
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
    if (!app.wasm) return;
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
 * the produced artifact loads through the same LoadFeed op a parsed
 * zip does. Invalid documents are refused — the capability's own
 * gate, not a JS guess. */
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
  if (!app.wasm) return;
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
    setFeedBytes(bytes); // the bus reads the same document the monolith parses
    app.gtfs = app.wasm.GtfsHandle.load(bytes);
    app.selectedStop = null;
    app.selectedTripId = null;
    app.tripFeature = null;
    app.plan = null;
    app.planBaseline = null;
    app.planNotice = null;
    pushEdits();
    app.status = `Materialized "${doc.name}" — the Rider and Dispatcher views now run on the authored network (published GTFS ${feedHash.slice(0, 19)}…).`;
  } catch (e) {
    app.status = `Error: ${e?.message ?? e}`;
  }
}

// ---------------------------------------------------------------
// editing — one mutation path: clone → edit → replace → revalidate
// ---------------------------------------------------------------

export function mutateNetwork(fn: (doc: Network) => void): void {
  const doc = structuredClone(getNetworkDoc());
  if (!doc) return;
  fn(doc);
  setDoc(doc, app.networkLabel);
}

function nextId(map: Record<string, unknown> | undefined | null, prefix: string): string {
  if (!map) return `${prefix}1`;
  let n = 1;
  while (Object.prototype.hasOwnProperty.call(map, `${prefix}${n}`)) n++;
  return `${prefix}${n}`;
}

// --- stops ------------------------------------------------------

export function addStop(): void {
  const doc = getNetworkDoc();
  const i = Object.keys(doc?.stops ?? {}).length + 1;
  const id = nextId(doc?.stops, "s");
  mutateNetwork((d) => {
    d.stops[id] = { name: `Stop ${i}`, lat: 45.52, lon: -122.68, transfer: false };
  });
  selectEntity("stop", id); // creation opens the editor: set metadata now
}

export function updateStop(
  id: string,
  { name, lat, lon, transfer }: { name?: string; lat?: number; lon?: number; transfer?: boolean },
): void {
  mutateNetwork((d) => {
    const s = d.stops[id];
    if (!s) return;
    if (name !== undefined) s.name = name;
    if (lat !== undefined && !Number.isNaN(lat)) s.lat = lat;
    if (lon !== undefined && !Number.isNaN(lon)) s.lon = lon;
    if (transfer !== undefined) s.transfer = transfer;
  });
}

export function removeStop(id: string): void {
  mutateNetwork((d) => {
    delete d.stops[id];
  });
}

// --- routes -----------------------------------------------------

const MODES: Mode[] = ["bus", "tram", "subway", "rail", "ferry", "cable_car", "gondola", "funicular", "trolleybus", "monorail", "other"];

export function modes(): Mode[] {
  return MODES;
}

export function addRoute(): void {
  const doc = getNetworkDoc();
  const i = Object.keys(doc?.routes ?? {}).length + 1;
  const id = nextId(doc?.routes, "r");
  mutateNetwork((d) => {
    d.routes[id] = { name: `Route ${i}`, mode: "bus" };
  });
  // Creation opens the detail screen: set the metadata now, in
  // context, not from a faceless list.
  selectEntity("route", id);
}

export function updateRoute(id: string, { name, mode }: { name?: string; mode?: Mode }): void {
  mutateNetwork((d) => {
    const r = d.routes[id];
    if (!r) return;
    if (name !== undefined) r.name = name;
    if (mode !== undefined) r.mode = mode;
  });
}

export function removeRoute(id: string): void {
  mutateNetwork((d) => {
    delete d.routes[id];
  });
}

// --- patterns ---------------------------------------------------

export function addPattern(routeId?: string): void {
  const doc = getNetworkDoc();
  if (!doc) return;
  const targetRoute = routeId ?? Object.keys(doc.routes ?? {}).sort()[0];
  if (!targetRoute) {
    app.status = "Add a route first.";
    return;
  }
  const stopIds = Object.keys(doc.stops ?? {}).sort();
  if (stopIds.length < 1) {
    app.status = "Add stops first.";
    return;
  }
  const id = nextId(doc.patterns, "pat");
  mutateNetwork((d) => {
    d.patterns[id] = {
      route: targetRoute,
      name: `Pattern ${Object.keys(d.patterns).length + 1}`,
      stops: [
        { stop: stopIds[0], timepoint: true, distance_m: 0 },
        { stop: stopIds[1 % stopIds.length], timepoint: true, distance_m: 500 },
      ],
      timing: [{ start_secs: 0, end_secs: 30 * 3600, at_timepoints: [0, 600] }],
    };
  });
  selectEntity("pattern", id);
}

export function updatePattern(id: string, { name, route }: { name?: string; route?: string }): void {
  mutateNetwork((d) => {
    const p = d.patterns[id];
    if (!p) return;
    if (name !== undefined) p.name = name;
    if (route !== undefined) p.route = route;
  });
}

export function removePattern(id: string): void {
  mutateNetwork((d) => {
    delete d.patterns[id];
  });
}

/** Insert a stop visit before the pattern's last stop (endpoints stay
 * endpoints; non-timepoint, so timing vectors are untouched). */
export function addPatternStop(patternId: string, stopId: string): void {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p || !stopId || p.stops.length < 2) return;
    const last = p.stops[p.stops.length - 1];
    const before = p.stops[p.stops.length - 2];
    const distance = (before.distance_m + last.distance_m) / 2;
    p.stops.splice(p.stops.length - 1, 0, { stop: stopId, timepoint: false, distance_m: distance });
  });
}

/** Middle stops only — the first and last are timepoints by model
 * invariant and the UI keeps them fixed. (No reordering: the stop
 * sequence is geometry — added in order along the road, baked in by
 * the alignment once it exists.) */
export function removePatternStop(patternId: string, index: number): void {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p) return;
    if (index <= 0 || index >= p.stops.length - 1) return;
    const wasTimepoint = p.stops[index].timepoint;
    const before = p.stops.slice(0, index).filter((s) => s.timepoint).length;
    p.stops.splice(index, 1);
    if (wasTimepoint) {
      for (const prof of p.timing) prof.at_timepoints.splice(before, 1);
    }
  });
}

export function setPatternStop(patternId: string, index: number, { stop, distance_m }: { stop?: string; distance_m?: number }): void {
  mutateNetwork((d) => {
    const row = d.patterns[patternId]?.stops[index];
    if (!row) return;
    if (stop !== undefined) row.stop = stop;
    if (distance_m !== undefined && !Number.isNaN(distance_m)) row.distance_m = distance_m;
  });
}

/** Toggle a middle stop's timepoint flag, keeping every timing
 * profile's vector aligned (insert a midpoint / drop the entry). */
export function togglePatternTimepoint(patternId: string, index: number): void {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p) return;
    if (index <= 0 || index >= p.stops.length - 1) return; // endpoints stay
    const row = p.stops[index];
    const pos = p.stops.slice(0, index).filter((s) => s.timepoint).length;
    const turningOff = row.timepoint;
    row.timepoint = !row.timepoint;
    for (const prof of p.timing) {
      if (turningOff) {
        prof.at_timepoints.splice(pos, 1);
      } else {
        const prev = pos > 0 ? prof.at_timepoints[pos - 1] : 0;
        const next = pos < prof.at_timepoints.length ? prof.at_timepoints[pos] : prev;
        prof.at_timepoints.splice(pos, 0, Math.round((prev + next) / 2));
      }
    }
  });
}

// --- timing profiles --------------------------------------------

export function addTimingProfile(patternId: string): void {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p) return;
    const last = p.timing[p.timing.length - 1];
    const start = last ? last.end_secs : 0;
    p.timing.push({
      start_secs: start,
      end_secs: start + 6 * 3600,
      at_timepoints: last ? [...last.at_timepoints] : [0],
    });
  });
}

export function removeTimingProfile(patternId: string, index: number): void {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p || p.timing.length <= 1) return;
    p.timing.splice(index, 1);
  });
}

export function setTimingProfileField(patternId: string, index: number, { start_secs, end_secs }: { start_secs?: number | null; end_secs?: number | null }): void {
  mutateNetwork((d) => {
    const prof = d.patterns[patternId]?.timing[index];
    if (!prof) return;
    if (start_secs !== undefined && start_secs !== null) prof.start_secs = start_secs;
    if (end_secs !== undefined && end_secs !== null) prof.end_secs = end_secs;
  });
}

export function setTimepointTime(patternId: string, profileIndex: number, timepointIndex: number, secs: number | null): void {
  mutateNetwork((d) => {
    const prof = d.patterns[patternId]?.timing[profileIndex];
    if (!prof || timepointIndex >= prof.at_timepoints.length || secs === null) return;
    prof.at_timepoints[timepointIndex] = secs;
  });
}

// --- calendars --------------------------------------------------

const DAYS: DayName[] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

export function dayNames(): DayName[] {
  return DAYS;
}

export function addServiceType(): void {
  const doc = getNetworkDoc();
  const id = nextId(doc?.service_types, "cal");
  mutateNetwork((d) => {
    d.service_types[id] = {
      name: `Service type ${id}`,
      rule: {
        days: { mon: true, tue: true, wed: true, thu: true, fri: true, sat: false, sun: false },
        ranges: [{ start: "2026-01-01", end: "2026-12-31" }],
        exceptions: [],
      },
    };
  });
  selectEntity("service_type", id);
}

export function toggleServiceTypeDay(id: string, day: DayName): void {
  mutateNetwork((d) => {
    const st = d.service_types[id];
    if (st) st.rule.days[day] = !st.rule.days[day];
  });
}

export function setServiceTypeRange(id: string, index: number, { start, end }: { start?: string; end?: string }): void {
  mutateNetwork((d) => {
    const r = d.service_types[id]?.rule.ranges[index];
    if (!r) return;
    if (start) r.start = start;
    if (end) r.end = end;
  });
}

export function addServiceTypeRange(id: string): void {
  mutateNetwork((d) => {
    const st = d.service_types[id];
    if (!st) return;
    const last = st.rule.ranges[st.rule.ranges.length - 1];
    if (last) {
        st.rule.ranges.push({ start: last.end, end: last.end });
    } else {
        st.rule.ranges.push({ start: "2026-01-01", end: "2026-12-31" });
    }
  });
}

export function removeServiceTypeRange(id: string, index: number): void {
  mutateNetwork((d) => {
    d.service_types[id]?.rule.ranges.splice(index, 1);
  });
}

export function addServiceTypeException(id: string, date: string, state: DayState): void {
  if (!date) return;
  mutateNetwork((d) => {
    const st = d.service_types[id];
    if (!st) return;
    const existing = st.rule.exceptions.find((e) => e.date === date);
    if (existing) existing.state = state;
    else st.rule.exceptions.push({ date, state });
    st.rule.exceptions.sort((a, b) => (a.date < b.date ? -1 : 1));
  });
}

export function removeServiceTypeException(id: string, date: string): void {
  mutateNetwork((d) => {
    const st = d.service_types[id];
    if (!st) return;
    st.rule.exceptions = st.rule.exceptions.filter((e) => e.date !== date);
  });
}

export function removeServiceType(id: string): void {
  mutateNetwork((d) => {
    delete d.service_types[id];
  });
}

export function updateServiceType(id: string, { name }: { name?: string }): void {
  mutateNetwork((d) => {
    const st = d.service_types[id];
    if (!st) return;
    if (name !== undefined) st.name = name;
  });
}

// --- schedules (one pattern's trips under one service type) ---

export function addSchedule(): void {
  const doc = getNetworkDoc();
  if (!doc) return;
  const patternId = Object.keys(doc.patterns ?? {}).sort()[0];
  const typeId = Object.keys(doc.service_types ?? {}).sort()[0];
  if (!patternId || !typeId) {
    app.status = "Add a pattern and a service type first.";
    return;
  }
  const id = nextId(doc.schedules, "svc");
  mutateNetwork((d) => {
    d.schedules[id] = {
      pattern: patternId,
      service_type: typeId,
      schedule: { kind: "exact_times", starts: [8 * 3600] },
    };
  });
  selectEntity("schedule", id);
}

export function setScheduleRefs(id: string, { pattern, service_type }: { pattern?: string; service_type?: string }): void {
  mutateNetwork((d) => {
    const s = d.schedules[id];
    if (!s) return;
    if (pattern) s.pattern = pattern;
    if (service_type) s.service_type = service_type;
  });
}

export function removeSchedule(id: string): void {
  mutateNetwork((d) => {
    delete d.schedules[id];
  });
}

/** Replace an exact-times service's trip starts (sorted, unique). */
export function setScheduleStarts(scheduleId: string, starts: number[]): void {
  const clean = [...new Set(starts)].sort((a, b) => a - b);
  mutateNetwork((d) => {
    const svc = d.schedules[scheduleId];
    if (svc) svc.schedule = { kind: "exact_times", starts: clean };
  });
}

export function addScheduleStart(scheduleId: string): void {
  const doc = getNetworkDoc();
  const starts = (doc?.schedules?.[scheduleId]?.schedule as { starts?: number[] } | undefined)?.starts;
  if (!starts) return;
  const last = starts.length ? starts[starts.length - 1] : 6 * 3600;
  setScheduleStarts(scheduleId, [...starts, Math.min(last + 600, 47 * 3600)]);
}

export function removeScheduleStart(scheduleId: string, index: number): void {
  const doc = getNetworkDoc();
  const starts = (doc?.schedules?.[scheduleId]?.schedule as { starts?: number[] } | undefined)?.starts;
  if (!starts || starts.length <= 1) return;
  setScheduleStarts(scheduleId, starts.filter((_, i) => i !== index));
}

/** Set one field of one headway band. */
export function setBandField(scheduleId: string, index: number, field: string, value: number): void {
  mutateNetwork((d) => {
    const svc = d.schedules[scheduleId];
    if (svc?.schedule?.kind !== "headway_bands" || !svc.schedule.bands[index]) return;
    (svc.schedule.bands[index] as HeadwayBand)[field as keyof HeadwayBand] = value as never;
  });
}

export function addBand(scheduleId: string): void {
  mutateNetwork((d) => {
    const svc = d.schedules[scheduleId];
    if (svc?.schedule?.kind !== "headway_bands") return;
    const bands = svc.schedule.bands;
    const last = bands[bands.length - 1];
    const start = last ? last.end_secs : 6 * 3600;
    bands.push({ start_secs: start, end_secs: start + 3 * 3600, headway_secs: 900 });
  });
}

export function removeBand(scheduleId: string, index: number): void {
  mutateNetwork((d) => {
    const svc = d.schedules[scheduleId];
    if (svc?.schedule?.kind !== "headway_bands" || svc.schedule.bands.length <= 1) return;
    svc.schedule.bands.splice(index, 1);
  });
}

// --- blocks (the runcut) ----------------------------------------

export function addBlock(): void {
  const doc = getNetworkDoc();
  const id = nextId(doc?.blocks, "b");
  mutateNetwork((d) => {
    d.blocks[id] = { trips: [], layovers: [] };
  });
  selectEntity("block", id);
}

export function removeBlock(id: string): void {
  mutateNetwork((d) => {
    delete d.blocks[id];
  });
}

export function addBlockTrip(blockId: string, scheduleId: string, startSecs: number | null): void {
  if (!scheduleId || startSecs === null) {
    app.status = "Pick a schedule and a valid start time first.";
    return;
  }
  mutateNetwork((d) => {
    const b = d.blocks[blockId];
    if (!b) return;
    b.trips.push({ schedule: scheduleId, start_secs: startSecs });
    b.trips.sort((a, c) => a.start_secs - c.start_secs || (a.schedule < c.schedule ? -1 : 1));
    // Layovers after the (possibly moved) last trip are meaningless.
    b.layovers = b.layovers.filter((l) => l.after_trip + 1 < b.trips.length);
  });
}

export function removeBlockTrip(blockId: string, index: number): void {
  mutateNetwork((d) => {
    const b = d.blocks[blockId];
    if (!b) return;
    b.trips.splice(index, 1);
    b.layovers = b.layovers.filter((l) => l.after_trip + 1 < b.trips.length);
  });
}

export function setBlockLayover(blockId: string, afterTrip: number, secs: number | null): void {
  mutateNetwork((d) => {
    const b = d.blocks[blockId];
    if (!b || afterTrip + 1 >= b.trips.length) return;
    const existing = b.layovers.find((l) => l.after_trip === afterTrip);
    if (secs === null) {
      b.layovers = b.layovers.filter((l) => l.after_trip !== afterTrip);
    } else if (existing) {
      existing.secs = secs;
    } else {
      b.layovers.push({ after_trip: afterTrip, secs, at: null });
    }
  });
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
