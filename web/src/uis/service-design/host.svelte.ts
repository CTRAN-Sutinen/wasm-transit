/// The service-design UI's host layer — the ONLY file of this UI that
/// calls the capability bus. It owns I/O (feed and layer bytes, the
/// export download, the URL hash), the plan set (shared with every
/// other shell through state.svelte.ts: JS owns the documents, and
/// the undo stack is earlier versions of them), and the selection.
/// Every number, list, and line it writes into `sd` is an op's answer,
/// adopted as-is; the shell's components take these as props and emit
/// events back here.
///
/// The UI is six views over one selection (docs/units/ui-service-
/// design.md, "The views"): plans, network, coverage, hubs, route,
/// proposal. The selection — plan, baseline, service group, route,
/// direction, stop, and the hour window — lives in the URL hash, so
/// a view can be bookmarked and returned to.
///
/// Every frame fails on its own: a node whose capability did not
/// assemble or whose op refused writes the frame's failure into
/// `sd.failed`, and the frame says so while the rest of the view
/// answers (a production failure, shown gracefully).

import { getAllPlans, getActivePlanId, putPlan, readPlan, setActivePlan, restorePlans } from "../../state.svelte";
import {
  analysisCap,
  analysisModule,
  authorCap,
  authorModule,
  directoryCap,
  directoryModule,
  ensureAnalysisLayer,
  gtfsCap,
  gtfsModule,
  networkCap,
  networkModule,
  plansChain,
  readStoredNetwork,
  type AnalysisLayerHandle,
} from "../../capabilities.svelte";
import { answer, answerGraph, hashIn, rememberHash, servedNetwork, today, type CapModule } from "../kit";
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { DirectoryStop } from "../../types/cap_directory/DirectoryStop";
import type { EntityFocus } from "../../types/EntityFocus";
import type { FeedInfo } from "../../types/FeedInfo";
import type { HourlyTrips } from "../../types/HourlyTrips";
import type { Network } from "../../types/Network";
import type { NetworkEdit } from "../../types/NetworkEdit";
import type { NetworkError } from "../../types/NetworkError";
import type { NetworkWarning } from "../../types/NetworkWarning";
import type { RouteConnections } from "../../types/RouteConnections";
import type { RouteDetail } from "../../types/RouteDetail";
import type { RouteFrequency } from "../../types/RouteFrequency";
import type { RouteOverlap } from "../../types/RouteOverlap";
import type { ServiceGroup } from "../../types/ServiceGroup";
import type { ServiceTypeComparison } from "../../types/ServiceTypeComparison";
import type { StopGaps } from "../../types/StopGaps";
import type { Timetable } from "../../types/Timetable";
import type { TypeCoverage } from "../../types/TypeCoverage";
import type { UnservedPoint } from "../../types/UnservedPoint";
import type { PlanIssue } from "../../types/cap_plans/PlanIssue";
import type { PlanPeriod } from "../../types/cap_plans/PlanPeriod";

export type View = "plans" | "network" | "coverage" | "hubs" | "route" | "proposal";
export const VIEWS: { id: View; label: string }[] = [
  { id: "plans", label: "Plans" },
  { id: "network", label: "Network" },
  { id: "coverage", label: "Coverage" },
  { id: "hubs", label: "Hubs" },
  { id: "route", label: "Route" },
  { id: "proposal", label: "Proposal" },
];

export type PlanRow = { key: string; id: string; name: string; effectiveFrom: string | null };
export type CoverageParams = { radius_m: number; frequent_secs: number; min_departures: number };
/// The hour window, service-day hours [from, to).
export type HourWindow = { from: number; to: number };

export const sd = $state({
  status: "Load a GTFS feed to begin.",
  busy: false,
  view: "network" as View,
  feed: null as { label: string; info: FeedInfo | null } | null,
  plans: [] as PlanRow[],
  activeKey: "",
  compareKey: "",
  chain: null as { inForce: string | null; periods: PlanPeriod[]; issues: PlanIssue[] } | null,
  issues: [] as NetworkError[],
  warnings: [] as NetworkWarning[],
  groups: [] as ServiceGroup[],
  group: "",
  hourly: null as HourlyTrips[] | null,
  frequency: null as RouteFrequency | null,
  comparison: null as ServiceTypeComparison | null,
  routes: [] as DirectoryRoute[],
  route: "",
  routeDetail: null as RouteDetail | null,
  direction: null as string | null,
  timetable: null as Timetable | null,
  focus: null as EntityFocus | null,
  selectedStop: "",
  window: null as HourWindow | null,
  layer: null as AnalysisLayerHandle | null,
  coverageParams: { radius_m: 400, frequent_secs: 900, min_departures: 2 } as CoverageParams,
  coverage: null as TypeCoverage[] | null,
  unserved: null as UnservedPoint[] | null,
  overlaps: null as RouteOverlap[] | null,
  stops: [] as DirectoryStop[],
  stop: "",
  connections: null as RouteConnections[] | null,
  gaps: null as StopGaps | null,
  editing: false,
  /// The picked trip on the string-line: "trip@start".
  selectedTrip: "",
  unservedPick: null as number | null,
  canUndo: false,
  canRedo: false,
  exported: "",
  /// Each frame's failure, by frame: why it has no answer (a
  /// capability that did not assemble, or an op that refused).
  failed: {} as Record<string, string>,
});

// ---------------------------------------------------------------------
// documents
// ---------------------------------------------------------------------

function active(): Network | null {
  return getAllPlans()[sd.activeKey] ?? null;
}

function baseline(): Network | null {
  return getAllPlans()[sd.compareKey] ?? null;
}

/// The selected group's representative day (the date its day-scoped
/// ops answer on) — the group's own label, never a calendar nobody
/// picked.
function groupDate(): string {
  return sd.groups.find((g) => g.service_type === sd.group)?.date ?? "";
}

function syncPlanRows(): void {
  sd.plans = Object.entries(getAllPlans()).map(([key, d]) => ({
    key,
    id: d.plan?.id ?? "",
    name: d.plan?.name || "(unframed import)",
    effectiveFrom: d.plan?.effective_from ?? null,
  }));
}

/// The issues cap-author answered with a document it authored — the
/// health node adopts them instead of registering that document in
/// cap-network to ask again.
const authoredIssues = new WeakMap<Network, NetworkError[]>();

/// Earlier versions of each plan (JS owns the documents): undo puts
/// the last one back; nothing is inverted.
const undo: Record<string, Network[]> = {};
const redo: Record<string, Network[]> = {};

function syncHistory(): void {
  sd.canUndo = (undo[sd.activeKey]?.length ?? 0) > 0;
  sd.canRedo = (redo[sd.activeKey]?.length ?? 0) > 0;
}

// ---------------------------------------------------------------------
// failure, per frame
// ---------------------------------------------------------------------

function message(e: unknown): string {
  return (e as Error)?.message ?? String(e);
}

/// A capability the frame needs, or the frame's failure.
function need<T>(module: T | null, name: string): T {
  if (!module) throw new Error(`The ${name} capability did not load.`);
  return module;
}

/// Run one frame's answer: a throw becomes that frame's failure and
/// clears what it shows; the next answer clears the failure.
async function framed(frame: string, clear: () => void, run: () => Promise<void>): Promise<void> {
  try {
    await run();
    delete sd.failed[frame];
  } catch (e) {
    clear();
    sd.failed[frame] = message(e);
    console.error(frame, e);
  }
}

// ---------------------------------------------------------------------
// TRANSIENT — this UI's answer graph (src/uis/kit.ts, "the answer
// graph"). Inputs: "active" / "compare" / "plans" (the documents),
// "group" / "route" / "direction" / "stop" / "window" (the selection),
// "view" (which view is on screen), "layer" / "coverage-params".
// ---------------------------------------------------------------------

const on = (...views: View[]) => () => views.includes(sd.view);

const request = answerGraph(
  [
    {
      id: "document.active",
      frame: "",
      needs: ["active"],
      run: async () => {
        await readPlan(sd.activeKey, readStoredNetwork);
      },
    },
    {
      id: "analysis.service_groups",
      frame: "Groups",
      needs: ["document.active"],
      run: (touch) =>
        framed("Groups", () => (sd.groups = []), async () => {
          const doc = active();
          if (!doc) return;
          const analysis = need(await analysisModule(), "analysis");
          const hash = hashIn(analysis, doc);
          sd.groups = answer(analysis, "service_groups", [hash], () => analysisCap(analysis).serviceGroups(hash));
          // The selection keys on a group's base; it stays while the
          // new answer still heads a row with it.
          if (!sd.groups.some((g) => g.service_type === sd.group)) {
            sd.group = sd.groups[0]?.service_type ?? "";
            touch("group");
          }
        }),
    },
    {
      id: "directory.routes",
      frame: "ContextBar",
      needs: ["document.active"],
      run: () =>
        framed("ContextBar", () => (sd.routes = []), async () => {
          const doc = active();
          if (!doc) return;
          const directory = need(await directoryModule(), "directory");
          const hash = hashIn(directory, doc);
          sd.routes = answer(directory, "routes", [hash], () => directoryCap(directory).routes(hash));
        }),
    },
    {
      id: "analysis.route_frequency",
      frame: "FrequencyGrid",
      needs: ["document.active", "group", "view"],
      visible: on("network", "route"),
      run: () =>
        framed("FrequencyGrid", () => (sd.frequency = null), async () => {
          const doc = active();
          if (!doc || !sd.group) return void (sd.frequency = null);
          const analysis = need(await analysisModule(), "analysis");
          const args = [hashIn(analysis, doc), sd.group] as const;
          sd.frequency = answer(analysis, "route_frequency", [...args], () => analysisCap(analysis).routeFrequency(...args));
        }),
    },
    {
      id: "analysis.hourly_trips",
      frame: "DayProfile",
      needs: ["document.active", "group", "view"],
      visible: on("network"),
      run: () =>
        framed("DayProfile", () => (sd.hourly = null), async () => {
          const doc = active();
          const date = groupDate();
          if (!doc || !date) return void (sd.hourly = null);
          const analysis = need(await analysisModule(), "analysis");
          const args = [hashIn(analysis, doc), date] as const;
          sd.hourly = answer(analysis, "hourly_trips", [...args], () => analysisCap(analysis).hourlyTrips(...args));
        }),
    },
    {
      id: "document.compare",
      frame: "",
      needs: ["compare"],
      run: async () => {
        if (sd.compareKey) await readPlan(sd.compareKey, readStoredNetwork);
      },
    },
    {
      id: "analysis.service_type_compare",
      frame: "Comparison",
      needs: ["document.active", "document.compare", "group"],
      run: () =>
        framed("Comparison", () => (sd.comparison = null), async () => {
          const [doc, base] = [active(), baseline()];
          if (!doc || !base || !sd.group) return void (sd.comparison = null);
          const analysis = need(await analysisModule(), "analysis");
          const args = [hashIn(analysis, base), hashIn(analysis, doc), sd.group] as const;
          sd.comparison = answer(analysis, "service_type_compare", [...args], () =>
            analysisCap(analysis).serviceTypeCompare(...args),
          );
        }),
    },
    {
      id: "directory.route_detail",
      frame: "RouteHeader",
      needs: ["document.active", "route", "view"],
      visible: on("route"),
      run: (touch) =>
        framed("RouteHeader", () => (sd.routeDetail = null), async () => {
          const doc = active();
          if (!doc || !sd.route) return void (sd.routeDetail = null);
          const directory = need(await directoryModule(), "directory");
          const hash = hashIn(directory, doc);
          const route = sd.route;
          sd.routeDetail = answer(directory, "route_detail", [hash, route], () => directoryCap(directory).routeDetail(hash, route));
          const directions = sd.routeDetail?.directions ?? [];
          if (!directions.some((d) => d.direction === sd.direction)) {
            sd.direction = directions[0]?.direction ?? null;
            touch("direction");
          }
        }),
    },
    {
      id: "directory.timetable",
      frame: "StringLine",
      needs: ["document.active", "document.compare", "route", "direction", "group", "view"],
      visible: on("route"),
      run: () =>
        framed("StringLine", () => (sd.timetable = null), async () => {
          const [doc, base] = [active(), baseline()];
          if (!doc || !sd.route || !sd.group) return void (sd.timetable = null);
          const directory = need(await directoryModule(), "directory");
          const args = [hashIn(directory, doc), sd.route, sd.direction, sd.group, base ? hashIn(directory, base) : null] as const;
          sd.timetable = answer(directory, "timetable", [...args], () => directoryCap(directory).timetable(...args));
        }),
    },
    {
      id: "directory.focus",
      frame: "RouteMap",
      needs: ["document.active", "route", "view"],
      visible: on("route"),
      run: () =>
        framed("RouteMap", () => (sd.focus = null), async () => {
          const doc = active();
          if (!doc || !sd.route) return void (sd.focus = null);
          const directory = need(await directoryModule(), "directory");
          const hash = hashIn(directory, doc);
          const route = sd.route;
          sd.focus = answer(directory, "focus", [hash, "route", route], () => directoryCap(directory).focus(hash, "route", route));
        }),
    },
    {
      id: "analysis.type_coverage",
      frame: "Coverage",
      needs: ["document.active", "layer", "coverage-params", "window", "view"],
      visible: on("coverage"),
      run: () =>
        framed("Coverage", () => (sd.coverage = null), async () => {
          const doc = active();
          if (!doc || !sd.layer) return void (sd.coverage = null);
          const analysis = need(await analysisModule(), "analysis");
          const p = sd.coverageParams;
          const window = sd.window ? { start_secs: sd.window.from * 3600, end_secs: sd.window.to * 3600 } : null;
          const args = [hashIn(analysis, doc), sd.layer.layer_hash, p.radius_m, p.frequent_secs, p.min_departures, window] as const;
          sd.coverage = answer(analysis, "type_coverage", [...args], () => analysisCap(analysis).typeCoverage(...args));
        }),
    },
    {
      id: "analysis.unserved_points",
      frame: "Unserved",
      needs: ["document.active", "group", "layer", "coverage-params", "view"],
      visible: on("coverage"),
      run: () =>
        framed("Unserved", () => (sd.unserved = null), async () => {
          const doc = active();
          const date = groupDate();
          if (!doc || !sd.layer || !date) return void (sd.unserved = null);
          const analysis = need(await analysisModule(), "analysis");
          const args = [hashIn(analysis, doc), sd.layer.layer_hash, sd.coverageParams.radius_m, date] as const;
          sd.unserved = answer(analysis, "unserved_points", [...args], () => analysisCap(analysis).unservedPoints(...args));
        }),
    },
    {
      id: "directory.stops",
      frame: "HubPicker",
      needs: ["document.active", "view"],
      visible: on("hubs"),
      run: () =>
        framed("HubPicker", () => (sd.stops = []), async () => {
          const doc = active();
          if (!doc) return;
          const directory = need(await directoryModule(), "directory");
          const hash = hashIn(directory, doc);
          sd.stops = answer(directory, "stops", [hash, null], () => directoryCap(directory).stops(hash, null));
        }),
    },
    {
      id: "analysis.route_overlap",
      frame: "Corridors",
      needs: ["document.active", "group", "view"],
      visible: on("hubs"),
      run: () =>
        framed("Corridors", () => (sd.overlaps = null), async () => {
          const doc = active();
          const date = groupDate();
          if (!doc || !date) return void (sd.overlaps = null);
          const analysis = need(await analysisModule(), "analysis");
          const args = [hashIn(analysis, doc), date] as const;
          sd.overlaps = answer(analysis, "route_overlap", [...args], () => analysisCap(analysis).routeOverlap(...args));
        }),
    },
    {
      id: "analysis.hub_connections",
      frame: "HubMatrix",
      needs: ["document.active", "group", "stop", "view"],
      visible: on("hubs"),
      run: () =>
        framed("HubMatrix", () => (sd.connections = null), async () => {
          const doc = active();
          const date = groupDate();
          if (!doc || !date || !sd.stop) return void (sd.connections = null);
          const analysis = need(await analysisModule(), "analysis");
          const args = [hashIn(analysis, doc), date, sd.stop] as const;
          sd.connections = answer(analysis, "hub_connections", [...args], () => analysisCap(analysis).hubConnections(...args));
        }),
    },
    {
      id: "analysis.stop_gaps",
      frame: "StopWaits",
      needs: ["document.active", "group", "stop", "view"],
      visible: on("hubs"),
      run: () =>
        framed("StopWaits", () => (sd.gaps = null), async () => {
          const doc = active();
          const date = groupDate();
          if (!doc || !date || !sd.stop) return void (sd.gaps = null);
          const analysis = need(await analysisModule(), "analysis");
          const args = [hashIn(analysis, doc), date, sd.stop] as const;
          sd.gaps = answer(analysis, "stop_gaps", [...args], () => analysisCap(analysis).stopGaps(...args));
        }),
    },
    {
      id: "plans.chain",
      frame: "PickTimeline",
      needs: ["plans"],
      run: () =>
        framed("PickTimeline", () => (sd.chain = null), async () => {
          const infos = sd.plans.filter((p) => p.id).map((p) => ({ id: p.id, name: p.name, effective_from: p.effectiveFrom }));
          sd.chain = need(await plansChain(infos, today()), "plans");
        }),
    },
    {
      // The plan's health — last: registering the document in
      // cap-network is the cost, and cap-author already answered it
      // for a document it authored.
      id: "network.validate",
      frame: "Readiness",
      needs: ["document.active"],
      run: () =>
        framed("Readiness", () => (sd.issues = []), async () => {
          const doc = active();
          if (!doc) return;
          const authored = authoredIssues.get(doc);
          if (authored) return void (sd.issues = authored);
          const network = need(await networkModule(), "network");
          const hash = hashIn(network, doc);
          sd.issues = answer(network, "validate", [hash], () => networkCap(network).validate(hash));
        }),
    },
  ],
  (e) => {
    sd.status = `Error: ${message(e)}`;
    console.error(e);
  },
);

/// One host step at a time: ops run on this thread, so the status
/// paints a frame before a real-size feed's work starts.
async function step<T>(label: string, run: () => Promise<T>): Promise<T | undefined> {
  sd.busy = true;
  sd.status = label;
  await new Promise((r) => setTimeout(r, 20));
  try {
    return await run();
  } catch (e) {
    sd.status = `Error: ${message(e)}`;
    console.error(e);
    return undefined;
  } finally {
    sd.busy = false;
  }
}

function planStatus(): void {
  const name = sd.plans.find((p) => p.key === sd.activeKey)?.name ?? "";
  sd.status = `${name}: ${sd.groups.length} service groups.`;
}

// ---------------------------------------------------------------------
// the URL hash — the selection, bookmarkable
// ---------------------------------------------------------------------

function writeHash(): void {
  const q = new URLSearchParams();
  q.set("view", sd.view);
  if (sd.activeKey) q.set("plan", sd.activeKey);
  if (sd.compareKey) q.set("base", sd.compareKey);
  if (sd.group) q.set("group", sd.group);
  if (sd.route) q.set("route", sd.route);
  if (sd.direction != null) q.set("dir", sd.direction);
  if (sd.stop) q.set("stop", sd.stop);
  if (sd.window) q.set("hours", `${sd.window.from}-${sd.window.to}`);
  history.replaceState(null, "", `#${q}`);
}

function readHash(): void {
  const q = new URLSearchParams(location.hash.slice(1));
  const view = q.get("view") as View | null;
  if (view && VIEWS.some((v) => v.id === view)) sd.view = view;
  sd.group = q.get("group") ?? "";
  sd.route = q.get("route") ?? "";
  sd.direction = q.get("dir");
  sd.stop = q.get("stop") ?? "";
  const hours = q.get("hours")?.match(/^(\d+)-(\d+)$/);
  sd.window = hours ? { from: Number(hours[1]), to: Number(hours[2]) } : null;
  const plan = q.get("plan");
  if (plan && getAllPlans()[plan]) sd.activeKey = plan;
  const base = q.get("base");
  if (base && getAllPlans()[base]) sd.compareKey = base;
}

/// Set inputs and answer what reads them, then keep the hash.
async function select(...inputs: string[]): Promise<void> {
  writeHash();
  await request(...inputs);
  writeHash();
}

// ---------------------------------------------------------------------
// boot and loaders
// ---------------------------------------------------------------------

/// The plan set restores as stored; a plan is read into the current
/// shape when it is first opened (`document.*` nodes).
export async function boot(): Promise<void> {
  await restorePlans();
  syncPlanRows();
  sd.activeKey = getAllPlans()[getActivePlanId()] ? getActivePlanId() : (sd.plans[0]?.key ?? "");
  readHash();
  if (sd.activeKey) {
    await openPlan(sd.activeKey);
    return;
  }
  // A server started with -feed publishes its network: the base
  // every fork copies, with no import here.
  const key = await step("Loading the published network…", async () => {
    const network = need(await networkModule(), "network");
    const served = await servedNetwork(network);
    if (!served) return null;
    const key = `plan-${crypto.randomUUID()}`;
    putPlan(key, served.network);
    sd.feed = { label: served.network.name || "the published network", info: null };
    syncPlanRows();
    return key;
  });
  if (key) await openPlan(key);
  else if (!sd.status.startsWith("Error")) sd.status = "Load a GTFS feed to begin.";
}

/// A feed becomes a plan: the network capability imports it into an
/// unframed document (the base every fork copies).
export async function loadFeedBytes(bytes: Uint8Array, label: string): Promise<void> {
  const key = await step(`Importing ${label}…`, async () => {
    const network = need(await networkModule(), "network");
    const gtfs = await gtfsModule();
    const info = gtfs ? gtfsCap(gtfs).feedInfo(gtfsCap(gtfs).loadFeed(bytes).feedHash) : null;
    const cap = networkCap(network);
    const { network: doc, networkHash } = cap.import(cap.loadFeed(bytes).feedHash);
    rememberHash(network, doc, networkHash);
    const key = `plan-${crypto.randomUUID()}`;
    putPlan(key, doc);
    sd.feed = { label, info };
    syncPlanRows();
    return key;
  });
  if (key) await openPlan(key);
}

export async function loadLayerBytes(bytes: Uint8Array): Promise<void> {
  await framed("Coverage", () => (sd.layer = null), async () => {
    sd.layer = need(await ensureAnalysisLayer(bytes), "analysis");
  });
  await select("layer");
}

// ---------------------------------------------------------------------
// the plan set and the chain
// ---------------------------------------------------------------------

export async function openPlan(key: string): Promise<void> {
  if (!getAllPlans()[key]) return;
  sd.activeKey = key;
  setActivePlan(key);
  if (!sd.compareKey || sd.compareKey === key) sd.compareKey = sd.plans.find((p) => p.key !== key)?.key ?? "";
  sd.warnings = [];
  syncHistory();
  await step("Answering the plan…", () => select("active", "compare", "plans", "view"));
  planStatus();
}

export function setCompare(key: string): Promise<void> {
  sd.compareKey = key;
  return select("compare");
}

/// The service-change workflow: the fork is the active document under
/// a new plan frame (a set_plan edit; the host mints the identity).
export async function fork(name: string, effectiveFrom: string): Promise<void> {
  const doc = active();
  if (!doc || !name.trim() || !effectiveFrom) {
    sd.status = "A fork needs a source plan, a name, and an effective date.";
    return;
  }
  const from = sd.activeKey;
  const applied = await apply(doc, [
    { edit: "set_plan", plan: { id: crypto.randomUUID(), name: name.trim(), effective_from: effectiveFrom } },
  ]);
  if (!applied) return;
  const key = `plan-${applied.network.plan.id}`;
  authoredIssues.set(applied.network, applied.issues);
  putPlan(key, applied.network);
  syncPlanRows();
  sd.compareKey = from;
  await openPlan(key);
  sd.warnings = applied.warnings;
  sd.status = `Forked "${name.trim()}" from ${sd.plans.find((p) => p.key === from)?.name}.`;
}

// ---------------------------------------------------------------------
// the selection — each handler sets its input and requests it
// ---------------------------------------------------------------------

export function setView(view: View): Promise<void> {
  sd.view = view;
  return select("view");
}

export function selectGroup(group: string): Promise<void> {
  sd.group = group;
  return select("group");
}

/// A route chosen anywhere opens the route view.
export function openRoute(route: string): Promise<void> {
  if (sd.route !== route) sd.selectedStop = "";
  sd.route = route;
  sd.view = "route";
  return select("route", "view");
}

export function clearRoute(): Promise<void> {
  sd.route = "";
  return select("route");
}

export function selectDirection(direction: string | null): Promise<void> {
  sd.direction = direction;
  sd.selectedStop = "";
  return select("direction");
}

export function selectTimetableStop(stop: string): void {
  sd.selectedStop = sd.selectedStop === stop ? "" : stop;
}

/// A stop chosen anywhere opens the hubs view on it.
export function openStop(stop: string): Promise<void> {
  sd.stop = stop;
  sd.view = "hubs";
  return select("stop", "view");
}

export function clearStop(): Promise<void> {
  sd.stop = "";
  return select("stop");
}

export function setWindow(window: HourWindow | null): Promise<void> {
  sd.window = window;
  return select("window");
}

export function toggleEdit(): void {
  sd.editing = !sd.editing;
}

export function selectTrip(key: string): void {
  sd.selectedTrip = key;
}

export function pickUnserved(i: number): void {
  sd.unservedPick = i;
}

/// A failed frame asks again: errors are never cached, so every node
/// re-answers its inputs.
export function retry(): Promise<void> {
  return select("active", "compare", "plans", "view", "layer", "stop");
}

export function setCoverageParams(params: CoverageParams): Promise<void> {
  sd.coverageParams = params;
  return select("coverage-params");
}

// ---------------------------------------------------------------------
// changes — NetworkEdits through cap-author, the answer adopted
// ---------------------------------------------------------------------

async function apply(doc: Network, edits: NetworkEdit[]) {
  const module = await authorModule();
  if (!module) {
    sd.status = "The authoring capability did not load — changes are off.";
    return null;
  }
  const cap = authorCap(module);
  try {
    const applied = cap.apply(hashIn(module, doc), edits);
    rememberHash(module, applied.network, applied.networkHash);
    return applied;
  } catch (e) {
    sd.status = `Change refused: ${message(e)}`;
    return null;
  }
}

async function adopt(next: Network, issues: NetworkError[] | null, warnings: NetworkWarning[]): Promise<void> {
  if (issues) authoredIssues.set(next, issues);
  putPlan(sd.activeKey, next);
  syncHistory();
  await step("Answering the change…", () => select("active"));
  sd.warnings = warnings;
  planStatus();
}

/// Apply a change to the ACTIVE plan and re-answer everything that
/// reads it. The base import is never edited in place: changing an
/// unframed plan asks for a fork first.
export async function change(edits: NetworkEdit[]): Promise<void> {
  const doc = active();
  if (!doc) return;
  if (!doc.plan?.id) {
    sd.status = "This is the unframed import — fork it into a plan before changing it.";
    return;
  }
  const applied = await apply(doc, edits);
  if (!applied) return;
  (undo[sd.activeKey] ??= []).push(doc);
  redo[sd.activeKey] = [];
  await adopt(applied.network, applied.issues, applied.warnings);
}

export async function undoChange(): Promise<void> {
  const prev = undo[sd.activeKey]?.pop();
  const doc = active();
  if (!prev || !doc) return;
  (redo[sd.activeKey] ??= []).push(doc);
  await adopt(prev, null, []);
}

export async function redoChange(): Promise<void> {
  const next = redo[sd.activeKey]?.pop();
  const doc = active();
  if (!next || !doc) return;
  (undo[sd.activeKey] ??= []).push(doc);
  await adopt(next, null, []);
}

/// The edits omit `direction` for the patterns in none.
function where() {
  return { route: sd.route, direction: sd.direction ?? undefined, service_type: sd.group };
}

/// The route view's gestures, stated against the selected route,
/// direction, and group (cap-author owns every rule). A frequency
/// target is not here: what it keeps of the trips already there is
/// D-54, and the op's rigid grid is QAF-90.
export const gestures = {
  span: (first_secs: number | null, last_secs: number | null) =>
    change([{ edit: "set_span", ...where(), first_secs: first_secs ?? undefined, last_secs: last_secs ?? undefined }]),
  running: (from_secs: number, to_secs: number, delta_secs: number) =>
    change([{ edit: "adjust_running_time", ...where(), from_secs, to_secs, delta_secs }]),
  /// One trip moved on its own: it keeps its id, running times, and
  /// block.
  retime: (trip: string, to_secs: number) => change([{ edit: "retime_trip", trip, to_secs }]),
  insertStop: (stop: string, after: string) =>
    change([{ edit: "insert_direction_stop", route: sd.route, direction: sd.direction ?? undefined, stop, after }]),
  removeStop: (stop: string) =>
    change([{ edit: "remove_direction_stop", route: sd.route, direction: sd.direction ?? undefined, stop }]).then(() => {
      sd.selectedStop = "";
    }),
};

// ---------------------------------------------------------------------
// the publish path — the network's export, downloaded
// ---------------------------------------------------------------------

export async function exportPlan(): Promise<void> {
  const doc = active();
  if (!doc) return;
  await framed("Export", () => (sd.exported = ""), async () => {
    const network: CapModule = need(await networkModule(), "network");
    const out = networkCap(network).export(hashIn(network, doc));
    const bytes = Uint8Array.from(atob(out.zipB64), (c) => c.charCodeAt(0));
    const link = document.createElement("a");
    link.href = URL.createObjectURL(new Blob([bytes], { type: "application/zip" }));
    link.download = `${doc.plan?.name || "network"}.zip`;
    link.click();
    URL.revokeObjectURL(link.href);
    sd.exported = out.feedHash;
  });
}
