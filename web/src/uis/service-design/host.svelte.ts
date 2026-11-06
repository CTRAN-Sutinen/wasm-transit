/// The service-design UI's host layer — the ONLY file of this UI that
/// calls the capability bus. It owns I/O (feed and layer bytes, the
/// export download, the URL hash), the plan set (plans.svelte.ts:
/// JS owns the documents, and the undo stack is earlier versions of
/// them), and the selection.
/// Every number, list, and line it writes into `sd` is an op's answer,
/// adopted as-is; the shell's components take these as props and emit
/// events back here.
///
/// The UI is six views over one selection (docs/units/ui-service-
/// design.md, "The views"): plans, network, coverage, hubs, route,
/// proposal. The selection — plan, baseline, service group (a
/// combination of service types that run together, D-55), route,
/// direction, stop, and the hour window — lives in the URL hash, so
/// a view can be bookmarked and returned to.
///
/// Every frame fails on its own: a node whose capability did not
/// assemble or whose op refused writes the frame's failure into
/// `sd.failed`, and the frame says so while the rest of the view
/// answers (a production failure, shown gracefully).

import { getAllPlans, getActivePlanId, putPlan, readPlan, setActivePlan, restorePlans } from "../../plans.svelte";
import { ask, assemble, available, keep, load, produced, servedNetworkValue, Doc } from "../../engine";
import { answerGraph, today } from "../kit";

// This page's capabilities answer inside its linked assembly — one
// module, one registry: each document registers once (D-77).
assemble("service-design");
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { DirectoryStop } from "../../types/cap_directory/DirectoryStop";
import type { EntityFocus } from "../../types/EntityFocus";
import type { FeedInfo } from "../../types/FeedInfo";
import type { HourlyTrips } from "../../types/HourlyTrips";
import type { NetworkStats } from "../../types/NetworkStats";
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
import type { CombinationCoverage } from "../../types/CombinationCoverage";
import type { UnservedPoint } from "../../types/UnservedPoint";
import type { LayerField } from "../../types/LayerField";
import type { LayerReading } from "../../types/LayerReading";
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
  /// The selected group: a combination's id (its members joined " + ").
  group: "",
  /// The service type the route view's gestures change — one member
  /// of the group (D-8: gestures stay per service type; the panel
  /// warns what else runs with it, D-95).
  editType: "",
  hourly: null as HourlyTrips[] | null,
  dayStats: null as NetworkStats | null,
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
  layer: null as LayerInfo | null,
  coverageParams: { radius_m: 400, frequent_secs: 900, min_departures: 2 } as CoverageParams,
  /// The selected combination's coverage (null: none selected, or its
  /// members never run together).
  coverage: null as CombinationCoverage | null,
  /// How the coverage op read the layer (its label, field, resolution).
  coverageReading: null as LayerReading | null,
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
  /// The last publish's answer, as the server gave it: the published
  /// set's hash, or why the pick was refused (D-74, D-76).
  published: null as { plans_hash: string } | null,
  publishRefused: [] as object[],
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

function selectedGroup(): ServiceGroup | null {
  return sd.groups.find((g) => g.id === sd.group) ?? null;
}

/// The selected combination's members — what its pooled reads ask by.
function groupMembers(): string[] {
  return selectedGroup()?.members ?? [];
}

/// The date the day-scoped ops (hourly_trips, route_overlap, the hub
/// reads) are asked on: the combination's first
/// date. Every date a combination holds on runs exactly its members
/// (the D-55 law: the date-scoped reads answer alike on each), so this
/// is an address, never a label — the views name the combination and
/// its spans.
function groupDay(): string {
  return selectedGroup()?.dates[0] ?? "";
}

/// Keep the gestures' service type a member of the selected group.
function syncEditType(): void {
  const members = groupMembers();
  if (!members.includes(sd.editType)) sd.editType = members[0] ?? "";
}

function syncPlanRows(): void {
  sd.plans = Object.entries(getAllPlans()).map(([key, d]) => ({
    key,
    id: d.plan?.id ?? "",
    name: d.plan?.name || "(unframed import)",
    effectiveFrom: d.plan?.effective_from ?? null,
  }));
}

/// The issues and warnings cap-author answered with a document it
/// authored (its `apply` validates the result) — the health node adopts
/// them instead of registering that document in cap-network to ask
/// again. Every other document's are network `validate`'s.
const authoredHealth = new WeakMap<Network, { issues: NetworkError[]; warnings: NetworkWarning[] }>();

/// A weighted layer as the coverage frame shows it: what the load
/// answered, and the planner's reading of it — which field is the
/// weight and what it means (D-47).
export type LayerInfo = { features: number; points: number; polygons: number; fields: LayerField[]; field: string; label: string };
/// The layer's document, by which the coverage ops read it.
let layer: Doc | null = null;

/// One engine document per plan version (JS owns the documents; the
/// engine registers each version once per module).
const docs = new WeakMap<Network, Doc<Network>>();
function docOf(network: Network): Doc<Network> {
  let doc = docs.get(network);
  if (!doc) docs.set(network, (doc = new Doc("network", { value: network })));
  return doc;
}

/// A stored plan read into the current shape: cap-author's `apply`
/// with no edits answers the canonical document (a plan saved under
/// older field names comes back in the current shape). Null when the
/// authoring capability did not assemble.
async function readStoredNetwork(stored: unknown): Promise<Network | null> {
  if (!(await available("author"))) return null;
  const answer = await ask("author", "apply", { network_hash: new Doc("network", { value: stored as Network }), edits: [] });
  docs.set(answer.network, produced("network", answer.network, "author", answer.network_hash));
  return answer.network;
}

/// Name what the modules keep: each plan's current network (the
/// compare baseline is one of them) and the layer. Every `apply` mints
/// a network, and networks are durable, so without this a session
/// would hold every version it made. A version on the undo stack is
/// dropped too; undo registers it again (the engine's recovery).
function release(): Promise<void> {
  return keep([...Object.values(getAllPlans()).map(docOf), layer]);
}

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
          sd.groups = (await ask("analysis", "service_groups", { network_hash: docOf(doc) })).groups;
          // The selection keys on a combination's id; it stays while
          // the new answer still holds that combination.
          if (!sd.groups.some((g) => g.id === sd.group)) {
            sd.group = sd.groups[0]?.id ?? "";
            touch("group");
          }
          syncEditType();
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
          sd.routes = (await ask("directory", "routes", { network_hash: docOf(doc) })).routes;
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
          sd.frequency = (await ask("analysis", "route_frequency", { network_hash: docOf(doc), members: groupMembers() })).frequency;
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
          const date = groupDay();
          if (!doc || !date) return void (sd.hourly = null);
          sd.hourly = (await ask("analysis", "hourly_trips", { network_hash: docOf(doc), date })).hours;
        }),
    },
    {
      // The day's numbers beside its shape, on the same date: when
      // service runs, how many stops it serves, where routes meet.
      id: "analysis.network_stats",
      frame: "DayProfile",
      needs: ["document.active", "group", "view"],
      visible: on("network"),
      run: () =>
        framed("DayProfile", () => (sd.dayStats = null), async () => {
          const doc = active();
          const date = groupDay();
          if (!doc || !date) return void (sd.dayStats = null);
          sd.dayStats = (
            await ask("analysis", "network_stats", {
              network_hash: docOf(doc),
              date,
              include_routes: false,
              include_stops: false,
              include_service_types: false,
            })
          ).stats;
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
          sd.comparison = (
            await ask("analysis", "service_type_compare", { base_hash: docOf(base), network_hash: docOf(doc), members: groupMembers() })
          ).comparison;
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
          sd.routeDetail = (await ask("directory", "route_detail", { network_hash: docOf(doc), route_id: sd.route })).detail;
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
          sd.timetable = (
            await ask("directory", "timetable", {
              network_hash: docOf(doc),
              route_id: sd.route,
              direction: sd.direction ?? undefined,
              members: groupMembers(),
              compare_hash: base ? docOf(base) : undefined,
            })
          ).timetable;
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
          sd.focus = (await ask("directory", "focus", { network_hash: docOf(doc), kind: "route", id: sd.route })).focus;
        }),
    },
    {
      id: "analysis.type_coverage",
      frame: "Coverage",
      needs: ["document.active", "group", "layer", "coverage-params", "window", "view"],
      visible: on("coverage"),
      run: () =>
        framed("Coverage", () => (sd.coverage = null), async () => {
          const doc = active();
          if (!doc || !layer || !sd.layer || !sd.group) return void ((sd.coverage = null), (sd.coverageReading = null));
          const p = sd.coverageParams;
          const window = sd.window ? { start_secs: sd.window.from * 3600, end_secs: sd.window.to * 3600 } : undefined;
          const answer = await ask("analysis", "type_coverage", {
            network_hash: docOf(doc),
            layer_hash: layer,
            members: groupMembers(),
            field: sd.layer.field,
            label: sd.layer.label,
            radius_m: p.radius_m,
            frequent_secs: p.frequent_secs,
            min_departures: p.min_departures,
            window,
          });
          sd.coverage = answer.coverage;
          sd.coverageReading = answer.layer;
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
          if (!doc || !layer || !sd.layer || !sd.group) return void (sd.unserved = null);
          sd.unserved =
            (
              await ask("analysis", "unserved_points", {
                network_hash: docOf(doc),
                layer_hash: layer,
                field: sd.layer.field,
                label: sd.layer.label,
                radius_m: sd.coverageParams.radius_m,
                members: groupMembers(),
              })
            ).unserved?.points ?? null;
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
          sd.stops = (await ask("directory", "stops", { network_hash: docOf(doc), limit: null })).stops;
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
          const date = groupDay();
          if (!doc || !date) return void (sd.overlaps = null);
          sd.overlaps = (await ask("analysis", "route_overlap", { network_hash: docOf(doc), date })).overlaps;
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
          const date = groupDay();
          if (!doc || !date || !sd.stop) return void (sd.connections = null);
          sd.connections = (await ask("analysis", "hub_connections", { network_hash: docOf(doc), date, stop_id: sd.stop })).connections;
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
          const date = groupDay();
          if (!doc || !date || !sd.stop) return void (sd.gaps = null);
          sd.gaps = (await ask("analysis", "stop_gaps", { network_hash: docOf(doc), date, stop_id: sd.stop })).gaps;
        }),
    },
    {
      id: "plans.chain",
      frame: "PickTimeline",
      needs: ["plans"],
      run: () =>
        framed("PickTimeline", () => (sd.chain = null), async () => {
          const infos = sd.plans.filter((p) => p.id).map((p) => ({ id: p.id, name: p.name, effective_from: p.effectiveFrom }));
          const date = today();
          sd.chain = {
            inForce: (await ask("plans", "resolve", { plans: infos, date })).plan,
            periods: (await ask("plans", "periods", { plans: infos })).periods,
            issues: (await ask("plans", "validate", { plans: infos })).issues,
          };
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
        framed("Readiness", () => ((sd.issues = []), (sd.warnings = [])), async () => {
          const doc = active();
          if (!doc) return;
          const { issues, warnings } =
            authoredHealth.get(doc) ?? (await ask("network", "validate", { network_hash: docOf(doc) }));
          sd.issues = issues;
          sd.warnings = warnings;
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
    const served = await servedNetworkValue();
    if (!served?.network.value) return null;
    const network = served.network.value;
    docs.set(network, served.network);
    const key = `plan-${crypto.randomUUID()}`;
    putPlan(key, network);
    sd.feed = { label: network.name || "the published network", info: null };
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
    const feed = new Doc("feed", { bytes });
    const info = (await available("gtfs")) ? (await ask("gtfs", "feed_info", { feed_hash: feed })).info : null;
    const imported = await ask("network", "import", { feed_hash: feed });
    const doc = imported.network;
    docs.set(doc, produced("network", doc, "network", imported.network_hash));
    const key = `plan-${crypto.randomUUID()}`;
    putPlan(key, doc);
    sd.feed = { label, info };
    syncPlanRows();
    return key;
  });
  if (key) await openPlan(key);
}

export async function loadLayerBytes(bytes: Uint8Array): Promise<void> {
  await framed("Coverage", () => ((sd.layer = null), (layer = null)), async () => {
    const doc = new Doc("layer", { bytes });
    const loaded = await load("analysis", doc);
    layer = doc;
    const fields = (loaded.fields ?? []) as LayerField[];
    // The first weight until the planner picks one; its name until
    // they say what it means.
    const field = fields[0]?.name ?? "";
    sd.layer = {
      features: Number(loaded.features),
      points: Number(loaded.points),
      polygons: Number(loaded.polygons),
      fields,
      field,
      label: field,
    };
  });
  await select("layer");
}

/// The planner's reading of the layer: which field is the weight, and
/// what it means.
export function setLayerReading(field: string, label: string): Promise<void> {
  if (!sd.layer) return Promise.resolve();
  sd.layer = { ...sd.layer, field, label: label.trim() || field };
  return select("layer");
}

// ---------------------------------------------------------------------
// the plan set and the chain
// ---------------------------------------------------------------------

export async function openPlan(key: string): Promise<void> {
  if (!getAllPlans()[key]) return;
  sd.activeKey = key;
  setActivePlan(key);
  if (!sd.compareKey || sd.compareKey === key) sd.compareKey = sd.plans.find((p) => p.key !== key)?.key ?? "";
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
/// `through` (optional) is the period's last day: the edit carries every
/// weekly calendar to it (D-41).
export async function fork(name: string, effectiveFrom: string, through = ""): Promise<void> {
  const doc = active();
  if (!doc || !name.trim() || !effectiveFrom) {
    sd.status = "A fork needs a source plan, a name, and an effective date.";
    return;
  }
  const from = sd.activeKey;
  const applied = await apply(doc, [
    {
      edit: "set_plan",
      plan: { id: crypto.randomUUID(), name: name.trim(), effective_from: effectiveFrom },
      ...(through ? { through } : {}),
    },
  ]);
  if (!applied) return;
  const key = `plan-${applied.network.plan.id}`;
  authoredHealth.set(applied.network, { issues: applied.issues, warnings: applied.warnings });
  putPlan(key, applied.network);
  syncPlanRows();
  sd.compareKey = from;
  await openPlan(key);
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
  syncEditType();
  return select("group");
}

/// Which member of the group the gestures change (D-8).
export function setEditType(type: string): void {
  sd.editType = type;
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
  if (!(await available("author"))) {
    sd.status = "The authoring capability did not load — changes are off.";
    return null;
  }
  try {
    const applied = await ask("author", "apply", { network_hash: docOf(doc), edits });
    docs.set(applied.network, produced("network", applied.network, "author", applied.network_hash));
    return applied;
  } catch (e) {
    sd.status = `Change refused: ${message(e)}`;
    return null;
  }
}

async function adopt(next: Network, health: { issues: NetworkError[]; warnings: NetworkWarning[] } | null): Promise<void> {
  if (health) authoredHealth.set(next, health);
  putPlan(sd.activeKey, next);
  syncHistory();
  await release();
  await step("Answering the change…", () => select("active"));
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
  await adopt(applied.network, { issues: applied.issues, warnings: applied.warnings });
}

export async function undoChange(): Promise<void> {
  const prev = undo[sd.activeKey]?.pop();
  const doc = active();
  if (!prev || !doc) return;
  (redo[sd.activeKey] ??= []).push(doc);
  await adopt(prev, null);
}

export async function redoChange(): Promise<void> {
  const next = redo[sd.activeKey]?.pop();
  const doc = active();
  if (!next || !doc) return;
  (undo[sd.activeKey] ??= []).push(doc);
  await adopt(next, null);
}

/// The edits omit `direction` for the patterns in none. A gesture
/// acts on one service type (D-8), the member the panel names.
function where() {
  return { route: sd.route, direction: sd.direction ?? undefined, service_type: sd.editType };
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
// the publish path — the network's export, downloaded; and the pick
// published to the deployment (POST /plans), its answer shown as given
// ---------------------------------------------------------------------

/// Publish the active plan as a pick: the server's modules validate it
/// and decide whether it is refused (docs/FORMATS.md, POST /plans); the
/// page sends the document and shows what came back.
export async function publishPlan(): Promise<void> {
  const doc = active();
  if (!doc) return;
  sd.published = null;
  sd.publishRefused = [];
  await framed("Publish", () => {}, async () => {
    const resp = await fetch("/plans", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(doc) });
    if (resp.status === 422) {
      const answer = (await resp.json()) as { issues?: object[]; refusals?: object[] };
      sd.publishRefused = [...(answer.issues ?? []), ...(answer.refusals ?? [])];
      return;
    }
    if (!resp.ok) throw new Error(`the server did not publish it: ${(await resp.text()).trim() || resp.status}`);
    sd.published = (await resp.json()) as { plans_hash: string };
  });
}

export async function exportPlan(): Promise<void> {
  const doc = active();
  if (!doc) return;
  await framed("Export", () => (sd.exported = ""), async () => {
    const out = await ask("network", "export", { network_hash: docOf(doc) });
    const bytes = Uint8Array.from(atob(out.zip_b64), (c) => c.charCodeAt(0));
    const link = document.createElement("a");
    link.href = URL.createObjectURL(new Blob([bytes], { type: "application/zip" }));
    link.download = `${doc.plan?.name || "network"}.zip`;
    link.click();
    URL.revokeObjectURL(link.href);
    sd.exported = out.feed_hash;
  });
}
