// Only this file of the UI calls the bus. Every number, list, and line that it
// writes into `sd` is an op's answer, adopted as given.
//
// The selection (plan, baseline, service group, route, direction, stop, hour
// window) lives in the URL hash, so a view can be bookmarked. A service group
// is a combination of service types that run together.

import { type ReadPlan, getAllPlans, getActivePlanId, putPlan, readPlan, setActivePlan, restorePlans } from "../../plans.svelte";
import { ask, assemble, available, keep, load, produced, publishedPlanSet, readManifest, served, servedNetworkValue, Doc, type DeploymentManifest } from "../../engine";
import { stopWarnings, withStopVersions } from "../picks";
import { answerGraph, applyBuild, buildWaiting, onBuildReady, clockEvery, clockLabelAt, inputGraph, inputLabel, installApp, reason, setAgencyZone, today, type FrameState, type InputDeclaration, type InputState, type Reader } from "../kit";
import descriptor from "./ui.json";

import { resumeSignIn } from "../../signin.svelte";
import { discardWrite, heldReason, openOutbox, outbox, queueWrite, retryWrite, sendWaiting } from "../../outbox.svelte";
// Each document registers once in the page's one linked module.
assemble("service-design");
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { DirectoryStop } from "../../types/cap_directory/DirectoryStop";
import type { EntityFocus } from "../../types/EntityFocus";
import type { FeedInfo } from "../../types/FeedInfo";
import type { HourlyTrips } from "../../types/HourlyTrips";
import type { NetworkStats } from "../../types/NetworkStats";
import type { Network } from "../../types/Network";
import type { Boundary } from "../../types/Boundary";
import type { PublishedPlans } from "../../types/cap_plans/PublishedPlans";
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
import type { MissedConnection } from "../../types/MissedConnection";
import type { StopWait } from "../../types/StopWait";
import type { Suggestion } from "../../types/transit_runs/Suggestion";
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
/// Service-day hours, [from, to).
export type HourWindow = { from: number; to: number };

export const sd = $state({
  updateReady: false,
  status: "Load a GTFS feed to begin.",
  /// Set when the planner must not miss the status (a refusal, an error, a change that left the plan with issues).
  alert: false,
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
  /// One member of the group. Gestures stay per service type, and the panel warns what else runs with it (D-95).
  editType: "",
  hourly: null as HourlyTrips[] | null,
  dayStats: null as NetworkStats | null,
  frequency: null as RouteFrequency | null,
  comparison: null as ServiceTypeComparison | null,
  vehicles: null as Suggestion | null,
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
  /// Null when none is selected, or when its members never run together.
  coverage: null as CombinationCoverage | null,
  coverageReading: null as LayerReading | null,
  unserved: null as UnservedPoint[] | null,
  overlaps: null as RouteOverlap[] | null,
  stops: [] as DirectoryStop[],
  stop: "",
  connections: null as RouteConnections[] | null,
  gaps: null as StopGaps | null,
  missed: null as MissedConnection[] | null,
  waits: null as StopWait[] | null,
  editing: false,
  /// The picked trip on the string-line: "trip@start".
  selectedTrip: "",
  unservedPick: null as number | null,
  canUndo: false,
  canRedo: false,
  exported: "",
  published: null as { plans_hash: string } | null,
  publishRefused: [] as object[],
  frames: {} as Record<string, FrameState>,
  inputs: {} as Record<string, InputState>,
});

function active(): Network | null {
  return getAllPlans()[sd.activeKey] ?? null;
}

function baseline(): Network | null {
  return getAllPlans()[sd.compareKey] ?? null;
}

function selectedGroup(): ServiceGroup | null {
  return sd.groups.find((g) => g.id === sd.group) ?? null;
}

function groupMembers(): string[] {
  return selectedGroup()?.members ?? [];
}

/// Every date that a combination holds runs exactly its members, so its first date works as an address.
function groupDay(): string {
  return selectedGroup()?.dates[0] ?? "";
}

function syncEditType(): void {
  const members = groupMembers();
  if (!members.includes(sd.editType)) sd.editType = members[0] ?? "";
}

const BASE_NAME = "Base network";

function syncPlanRows(): void {
  sd.plans = Object.entries(getAllPlans()).map(([key, d]) => ({
    key,
    id: d.plan?.id ?? "",
    name: d.plan?.name || BASE_NAME,
    effectiveFrom: d.plan?.effective_from ?? null,
  }));
}

/// The health node adopts these instead of registering the authored document in cap-network to ask again.
const authoredHealth = new WeakMap<Network, { issues: NetworkError[]; warnings: NetworkWarning[] }>();

/// The planner's reading of the layer: the weight field and what it means.
export type LayerInfo = { features: number; points: number; polygons: number; fields: LayerField[]; field: string; label: string };
let layer: Doc | null = null;

const docs = new WeakMap<Network, Doc<Network>>();
function docOf(network: Network): Doc<Network> {
  let doc = docs.get(network);
  if (!doc) docs.set(network, (doc = new Doc("network", { value: network })));
  return doc;
}

async function readStoredSet(stored: unknown): Promise<Record<string, unknown>> {
  return (await ask("network", "stored_plans", { document: stored })).plans;
}

/// A plan saved in an older version comes forward through the migration capability.
/// A network with no schema_version is v1. Null when the authoring capability did not assemble.
async function readStoredNetwork(stored: unknown): Promise<ReadPlan | null> {
  if (!(await available("author"))) return null;
  let migrated = false;
  if (await available("migrate")) {
    const { version, reads } = await ask("migrate", "version", { kind: "network", document: stored });
    if (version < reads) {
      stored = (await ask("migrate", "migrate", { kind: "network", document: stored })).document;
      migrated = true;
    }
  }
  const answer = await ask("author", "apply", { network_hash: new Doc("network", { value: stored as Network }), edits: [] });
  docs.set(answer.network, produced("network", answer.network, "author", answer.network_hash));
  return { network: answer.network, migrated };
}

/// Every `apply` creates a network, and networks are durable, so without this a session would hold every version it made. A version on the undo stack is dropped too, and undo registers it again (the engine's recovery).
function release(): Promise<void> {
  return keep([...Object.values(getAllPlans()).map(docOf), layer]);
}

const undo: Record<string, Network[]> = {};
const redo: Record<string, Network[]> = {};

function syncHistory(): void {
  sd.canUndo = (undo[sd.activeKey]?.length ?? 0) > 0;
  sd.canRedo = (redo[sd.activeKey]?.length ?? 0) > 0;
}

function say(text: string, alert = false): void {
  sd.status = text;
  sd.alert = alert;
}

function message(e: unknown): string {
  return (e as Error)?.message ?? String(e);
}

/// A throw becomes the frame's failure and clears what it shows. The next answer clears the failure.
async function act(frame: string, clear: () => void, run: () => Promise<void>): Promise<void> {
  try {
    await run();
    delete sd.frames[frame];
  } catch (e) {
    clear();
    sd.frames[frame] = { kind: "failed", message: message(e) };
    console.error(frame, e);
  }
}

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
      caps: ["analysis"],
      clear: () => (sd.groups = []),
      run: async (touch) => {
        const doc = active();
        if (!doc) return;
        sd.groups = (await ask("analysis", "service_groups", { network_hash: docOf(doc) })).groups;
        if (!sd.groups.some((g) => g.id === sd.group)) {
          sd.group = sd.groups[0]?.id ?? "";
          touch("group");
        }
        syncEditType();
      },
    },
    {
      id: "directory.routes",
      frame: "ContextBar",
      needs: ["document.active"],
      caps: ["directory"],
      clear: () => (sd.routes = []),
      run: async () => {
        const doc = active();
        if (!doc) return;
        sd.routes = (await ask("directory", "routes", { network_hash: docOf(doc) })).routes;
      },
    },
    {
      id: "analysis.route_frequency",
      frame: "FrequencyGrid",
      needs: ["document.active", "group", "view"],
      caps: ["analysis"],
      visible: on("network", "route"),
      clear: () => (sd.frequency = null),
      run: async () => {
        const doc = active();
        if (!doc || !sd.group) return void (sd.frequency = null);
        sd.frequency = (await ask("analysis", "route_frequency", { network_hash: docOf(doc), members: groupMembers() })).frequency;
      },
    },
    {
      id: "analysis.hourly_trips",
      frame: "DayProfile",
      needs: ["document.active", "group", "view"],
      caps: ["analysis"],
      visible: on("network"),
      clear: () => (sd.hourly = null),
      run: async () => {
        const doc = active();
        const date = groupDay();
        if (!doc || !date) return void (sd.hourly = null);
        sd.hourly = (await ask("analysis", "hourly_trips", { network_hash: docOf(doc), date })).hours;
      },
    },
    {
      id: "analysis.network_stats",
      frame: "DayProfile",
      needs: ["document.active", "group", "view"],
      caps: ["analysis"],
      visible: on("network"),
      clear: () => (sd.dayStats = null),
      run: async () => {
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
      },
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
      caps: ["analysis"],
      clear: () => (sd.comparison = null),
      run: async () => {
        const [doc, base] = [active(), baseline()];
        if (!doc || !base || !sd.group) return void (sd.comparison = null);
        sd.comparison = (
          await ask("analysis", "service_type_compare", { base_hash: docOf(base), network_hash: docOf(doc), members: groupMembers() })
        ).comparison;
      },
    },
    {
      // Runs under the terminals' recovery floors, since this UI loads no work rules.
      id: "runcut.suggest_blocks",
      frame: "Vehicles",
      needs: ["document.active", "group"],
      caps: ["runcut"],
      clear: () => (sd.vehicles = null),
      run: async () => {
        const doc = active();
        const date = groupDay();
        if (!doc || !date) return void (sd.vehicles = null);
        sd.vehicles = (await ask("runcut", "suggest_blocks", { network_hash: docOf(doc), edits_hash: null, date })).suggestion;
      },
    },
    {
      id: "directory.route_detail",
      frame: "RouteHeader",
      needs: ["document.active", "route", "view"],
      caps: ["directory"],
      visible: on("route"),
      clear: () => (sd.routeDetail = null),
      run: async (touch) => {
        const doc = active();
        if (!doc || !sd.route) return void (sd.routeDetail = null);
        sd.routeDetail = (await ask("directory", "route_detail", { network_hash: docOf(doc), route_id: sd.route })).detail;
        const directions = sd.routeDetail?.directions ?? [];
        if (!directions.some((d) => d.direction === sd.direction)) {
          sd.direction = directions[0]?.direction ?? null;
          touch("direction");
        }
      },
    },
    {
      id: "directory.timetable",
      frame: "StringLine",
      needs: ["document.active", "document.compare", "route", "direction", "group", "view"],
      caps: ["directory"],
      visible: on("route"),
      clear: () => (sd.timetable = null),
      run: async () => {
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
      },
    },
    {
      id: "directory.focus",
      frame: "RouteMap",
      needs: ["document.active", "route", "view"],
      caps: ["directory"],
      visible: on("route"),
      clear: () => (sd.focus = null),
      run: async () => {
        const doc = active();
        if (!doc || !sd.route) return void (sd.focus = null);
        sd.focus = (await ask("directory", "focus", { network_hash: docOf(doc), kind: "route", id: sd.route })).focus;
      },
    },
    {
      id: "analysis.type_coverage",
      frame: "Coverage",
      needs: ["document.active", "group", "layer", "coverage-params", "window", "view"],
      caps: ["analysis"],
      visible: on("coverage"),
      clear: () => (sd.coverage = null),
      run: async () => {
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
      },
    },
    {
      id: "analysis.unserved_points",
      frame: "Unserved",
      needs: ["document.active", "group", "layer", "coverage-params", "view"],
      caps: ["analysis"],
      visible: on("coverage"),
      clear: () => (sd.unserved = null),
      run: async () => {
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
      },
    },
    {
      id: "directory.stops",
      frame: "HubPicker",
      needs: ["document.active", "view"],
      caps: ["directory"],
      visible: on("hubs", "route"),
      clear: () => (sd.stops = []),
      run: async () => {
        const doc = active();
        if (!doc) return;
        sd.stops = (await ask("directory", "stops", { network_hash: docOf(doc), limit: null })).stops;
      },
    },
    {
      id: "analysis.route_overlap",
      frame: "Corridors",
      needs: ["document.active", "group", "view"],
      caps: ["analysis"],
      visible: on("hubs"),
      clear: () => (sd.overlaps = null),
      run: async () => {
        const doc = active();
        const date = groupDay();
        if (!doc || !date) return void (sd.overlaps = null);
        sd.overlaps = (await ask("analysis", "route_overlap", { network_hash: docOf(doc), date })).overlaps;
      },
    },
    {
      id: "analysis.hub_connections",
      frame: "HubMatrix",
      needs: ["document.active", "group", "stop", "view"],
      caps: ["analysis"],
      visible: on("hubs"),
      clear: () => (sd.connections = null),
      run: async () => {
        const doc = active();
        const date = groupDay();
        if (!doc || !date || !sd.stop) return void (sd.connections = null);
        sd.connections = (await ask("analysis", "hub_connections", { network_hash: docOf(doc), date, stop_id: sd.stop })).connections;
      },
    },
    {
      id: "analysis.stop_gaps",
      frame: "StopWaits",
      needs: ["document.active", "group", "stop", "view"],
      caps: ["analysis"],
      visible: on("hubs"),
      clear: () => (sd.gaps = null),
      run: async () => {
        const doc = active();
        const date = groupDay();
        if (!doc || !date || !sd.stop) return void (sd.gaps = null);
        sd.gaps = (await ask("analysis", "stop_gaps", { network_hash: docOf(doc), date, stop_id: sd.stop })).gaps;
      },
    },
    {
      id: "analysis.missed_connections",
      frame: "NearMisses",
      needs: ["document.active", "group", "stop", "view"],
      caps: ["analysis"],
      visible: on("hubs"),
      clear: () => (sd.missed = null),
      run: async () => {
        const doc = active();
        const date = groupDay();
        if (!doc || !date || !sd.stop) return void (sd.missed = null);
        sd.missed = (await ask("analysis", "missed_connections", { network_hash: docOf(doc), date, stop_id: sd.stop })).missed;
      },
    },
    {
      id: "analysis.longest_waits",
      frame: "LongestWaits",
      needs: ["document.active", "group", "window", "view"],
      caps: ["analysis"],
      visible: on("hubs"),
      clear: () => (sd.waits = null),
      run: async () => {
        const doc = active();
        const date = groupDay();
        if (!doc || !date) return void (sd.waits = null);
        const window = sd.window ? { start_secs: sd.window.from * 3600, end_secs: sd.window.to * 3600 } : undefined;
        sd.waits = (await ask("analysis", "longest_waits", { network_hash: docOf(doc), date, window, limit: 25 })).waits;
      },
    },
    {
      id: "plans.chain",
      frame: "PickTimeline",
      needs: ["plans"],
      caps: ["plans"],
      clear: () => (sd.chain = null),
      run: async () => {
        const infos = sd.plans.filter((p) => p.id).map((p) => ({ id: p.id, name: p.name, effective_from: p.effectiveFrom }));
        const date = today();
        sd.chain = {
          inForce: (await ask("plans", "resolve", { plans: infos, date })).plan,
          periods: (await ask("plans", "periods", { plans: infos })).periods,
          issues: (await ask("plans", "validate", { plans: infos })).issues,
        };
      },
    },
    {
      // Last, because registering the document in cap-network is the cost, and cap-author already answered it for a document it authored.
      id: "network.validate",
      frame: "Readiness",
      needs: ["document.active", "published", "stops-doc"],
      caps: ["network"],
      clear: () => ((sd.issues = []), (sd.warnings = [])),
      run: async () => {
        const doc = active();
        if (!doc) return;
        const { issues, warnings } =
          authoredHealth.get(doc) ?? (await ask("network", "validate", { network_hash: docOf(doc) }));
        sd.issues = issues;
        // Stops that the plan visits and that are not in service over its period also warn, as the stops document records them.
        sd.warnings = [...warnings, ...(await stopWarnings(docOf(doc), heldPlans, heldStops))];
      },
    },
  ],
  sd.frames,
  (e) => {
    say(`Error: ${message(e)}`, true);
    console.error(e);
  },
  sd.inputs,
);

/// Ops run on this thread, so the status paints a frame before a real-size feed's work starts.
async function step<T>(label: string, run: () => Promise<T>): Promise<T | undefined> {
  sd.busy = true;
  say(label);
  await new Promise((r) => setTimeout(r, 20));
  try {
    return await run();
  } catch (e) {
    say(`Error: ${message(e)}`, true);
    console.error(e);
    return undefined;
  } finally {
    sd.busy = false;
  }
}

function planStatus(): void {
  const name = sd.plans.find((p) => p.key === sd.activeKey)?.name ?? "";
  say(`${name}: ${sd.groups.length} service groups.`);
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/// A change that breaks a block says so where the planner made it, not only on the Proposal view.
function changeStatus(done: string): void {
  const health =
    sd.issues.length || sd.warnings.length
      ? `The plan now has ${plural(sd.issues.length, "issue")} and ${plural(sd.warnings.length, "warning")} (Proposal view).`
      : "The plan has no issues.";
  say(`${done} ${health}`, sd.issues.length > 0);
}

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

async function select(...inputs: string[]): Promise<void> {
  writeHash();
  await request(...inputs);
  writeHash();
}

type Manifest = DeploymentManifest;

let manifest: Manifest | null = null;
/// The published network is read only while no plan is open.
let heldPlans: PublishedPlans | null = null;
let heldStops: Doc | null = null;
let servedBase: { network: Doc<Network>; boundary: Doc<Boundary> } | null = null;

const readers: Record<string, Reader<any>> = {
  manifest: {
    read: async ({ signal }) => {
      const read = await readManifest(signal);
      return { value: read.manifest, key: JSON.stringify(read.manifest), asOfMs: read.asOfMs ?? undefined, why: read.why ?? undefined };
    },
    adopt: (m: Manifest | null) => {
      manifest = m;
      setAgencyZone(m?.timezone);
    },
  },
  plans: {
    read: async ({ signal, held }) => {
      if (!manifest) throw new Error("the deployment's manifest is not read");
      const pin = manifest.plans_hash;
      if (!pin) return null;
      if (held && heldPlansPin === pin) return { value: held, key: pin };
      const set = await publishedPlanSet({ manifest, signal });
      heldPlansPin = set ? pin : null;
      return set ? { value: set, key: pin } : null;
    },
    adopt: (set: PublishedPlans | null) => void (heldPlans = set),
  },
  stops: {
    read: async ({ signal, held }) => {
      if (!manifest) throw new Error("the deployment's manifest is not read");
      const pin = manifest.stops_hash;
      if (typeof pin !== "string") return null;
      if (held?.pin === pin) return { value: held, key: pin };
      const doc = await served("stops", { manifest, signal });
      return doc ? { value: doc, key: pin } : null;
    },
    adopt: (doc: Doc | null) => void (heldStops = doc),
  },
  network: {
    // The base that a first plan forks. Read only while the device holds no plan.
    read: async ({ signal }) => {
      if (Object.keys(getAllPlans()).length) return null;
      const got = await servedNetworkValue(signal);
      return got ? { value: got, key: got.network.pin ?? "" } : null;
    },
    adopt: (got) => void (servedBase = got),
  },
};
let heldPlansPin: string | null = null;

const inputs = inputGraph(descriptor.inputs as InputDeclaration[], readers, sd.inputs, request);

const WRITTEN = ["manifest", "plans", "stops"];

export function offline(): boolean {
  const m = sd.inputs.manifest;
  return m?.kind === "failed" || (m?.kind === "answered" && m.why === "the server is not answering");
}

export function asOf(): string {
  const m = sd.inputs.manifest;
  if (!offline() || !m || (m.kind !== "answered" && m.kind !== "failed") || m.asOfMs == null) return "";
  return clockLabelAt(m.asOfMs);
}

export async function boot(): Promise<void> {
  void installApp();
  onBuildReady(() => (sd.updateReady = true));
  await Promise.all([restorePlans(readStoredSet), resumeSignIn()]);
  await openOutbox("service-design", () => void published());
  await inputs.start();
  clockEvery(POLL_MS, () => {
    void sendWaiting();
    sd.updateReady = buildWaiting();
  });
  syncPlanRows();
  sd.activeKey = getAllPlans()[getActivePlanId()] ? getActivePlanId() : (sd.plans[0]?.key ?? "");
  readHash();
  if (sd.activeKey) {
    await openPlan(sd.activeKey);
    return;
  }
  // A server started with -feed publishes its network, the base that every fork copies. No import is needed here.
  const key = await step("Loading the published network…", async () => {
    const base = sd.inputs.network;
    if (base?.kind === "failed") throw new Error(inputLabel(base));
    const served = servedBase;
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
  else if (!sd.status.startsWith("Error")) say("Load a GTFS feed to begin.");
}

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
  await act("Coverage", () => ((sd.layer = null), (layer = null)), async () => {
    const doc = new Doc("layer", { bytes });
    const loaded = await load("analysis", doc);
    layer = doc;
    const fields = (loaded.fields ?? []) as LayerField[];
    // The first field is the weight until the planner picks one.
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

export function setLayerReading(field: string, label: string): Promise<void> {
  if (!sd.layer) return Promise.resolve();
  sd.layer = { ...sd.layer, field, label: label.trim() || field };
  return select("layer");
}

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

/// The fork is the active document under a new plan frame (a set_plan edit). `through` is the period's last day: the edit carries every weekly calendar to it.
export async function fork(name: string, effectiveFrom: string, through = ""): Promise<void> {
  const doc = active();
  if (!doc || !name.trim() || !effectiveFrom) {
    say("A fork needs a source plan, a name, and an effective date.", true);
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
  say(`Forked "${name.trim()}" from ${sd.plans.find((p) => p.key === from)?.name}.`);
}

export function setView(view: View): Promise<void> {
  sd.view = view;
  return select("view");
}

export function selectGroup(group: string): Promise<void> {
  sd.group = group;
  syncEditType();
  return select("group");
}

export function setEditType(type: string): void {
  sd.editType = type;
}

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

export function why(frame: string): string {
  return reason(sd.frames[frame]);
}

/// Errors are never cached.
export function retry(): Promise<void> {
  return request.retry();
}

export function setCoverageParams(params: CoverageParams): Promise<void> {
  sd.coverageParams = params;
  return select("coverage-params");
}

async function apply(doc: Network, edits: NetworkEdit[]) {
  if (!(await available("author"))) {
    say("The authoring capability did not load — changes are off.", true);
    return null;
  }
  try {
    const applied = await ask("author", "apply", { network_hash: docOf(doc), edits });
    docs.set(applied.network, produced("network", applied.network, "author", applied.network_hash));
    return applied;
  } catch (e) {
    say(`Change refused: ${message(e)}`, true);
    return null;
  }
}

async function adopt(next: Network, health: { issues: NetworkError[]; warnings: NetworkWarning[] } | null, done: string): Promise<void> {
  if (health) authoredHealth.set(next, health);
  putPlan(sd.activeKey, next);
  // The last export and publish answered the version before this one.
  sd.exported = "";
  sd.published = null;
  sd.publishRefused = [];
  delete sd.frames.Export;
  delete sd.frames.Publish;
  syncHistory();
  await release();
  await step("Answering the change…", () => select("active"));
  changeStatus(done);
}

/// The base import is never edited in place. Changing an unframed plan asks for a fork first.
export async function change(edits: NetworkEdit[]): Promise<void> {
  const doc = active();
  if (!doc) return;
  if (!doc.plan?.id) {
    say("This is the base network. Fork it into a plan (Plans view) before changing it.", true);
    return;
  }
  const applied = await apply(doc, edits);
  if (!applied) return;
  (undo[sd.activeKey] ??= []).push(doc);
  redo[sd.activeKey] = [];
  await adopt(applied.network, { issues: applied.issues, warnings: applied.warnings }, "Changed.");
}

export async function undoChange(): Promise<void> {
  const prev = undo[sd.activeKey]?.pop();
  const doc = active();
  if (!prev || !doc) return;
  (redo[sd.activeKey] ??= []).push(doc);
  await adopt(prev, null, "Undone.");
}

export async function redoChange(): Promise<void> {
  const next = redo[sd.activeKey]?.pop();
  const doc = active();
  if (!next || !doc) return;
  (undo[sd.activeKey] ??= []).push(doc);
  await adopt(next, null, "Redone.");
}

/// A gesture acts on one service type, the member the panel names.
function where() {
  return { route: sd.route, direction: sd.direction ?? undefined, service_type: sd.editType };
}

/// A frequency target is not here. What it keeps of the trips already there is D-54, and the op's rigid grid is QAF-90.
export const gestures = {
  span: (first_secs: number | null, last_secs: number | null) =>
    change([{ edit: "set_span", ...where(), first_secs: first_secs ?? undefined, last_secs: last_secs ?? undefined }]),
  running: (from_secs: number, to_secs: number, delta_secs: number) =>
    change([{ edit: "adjust_running_time", ...where(), from_secs, to_secs, delta_secs }]),
  retime: (trip: string, to_secs: number) => change([{ edit: "retime_trip", trip, to_secs }]),
  insertStop: (stop: string, after: string) =>
    change([{ edit: "insert_direction_stop", route: sd.route, direction: sd.direction ?? undefined, stop, after }]),
  removeStop: (stop: string) =>
    change([{ edit: "remove_direction_stop", route: sd.route, direction: sd.direction ?? undefined, stop }]).then(() => {
      sd.selectedStop = "";
    }),
};

/// The server's modules validate the pick and decide whether it is refused (server/openapi.json, POST /plans). The page shows what came back.
export async function publishPlan(): Promise<void> {
  const doc = active();
  if (!doc) return;
  sd.published = null;
  sd.publishRefused = [];
  // The publish runs when it is sent, against the published plans as they are then. While the server does not answer, it waits on this device.
  await act("Publish", () => {}, async () => {
    const outcome = await queueWrite({ url: "plans", body: doc, label: `Publish "${doc.plan?.name ?? "plan"}"` });
    if (outcome.state === "waiting") {
      say(outbox.needsSignIn ? "The publish waits on this device. Sign in to send it." : "The publish waits on this device. It is sent when the server answers.");
      return;
    }
    if (outcome.state === "sent") {
      sd.published = outcome.body as { plans_hash: string };
      return;
    }
    await discardWrite(outcome.key);
    if (outcome.status === 422) {
      const answer = (outcome.reasons ?? {}) as { issues?: object[]; refusals?: object[] };
      sd.publishRefused = [...(answer.issues ?? []), ...(answer.refusals ?? [])];
      return;
    }
    if (outcome.status === 403) throw new Error("your sign-in may not publish plans here");
    throw new Error(`the server did not publish it: ${typeof outcome.reasons === "string" ? outcome.reasons.trim() : outcome.status}`);
  });
}

export async function exportPlan(): Promise<void> {
  const doc = active();
  if (!doc) return;
  await act("Export", () => (sd.exported = ""), async () => {
    // the export carries the stops' history over the plan's period
    const out = await ask("network", "export", { network_hash: await withStopVersions(docOf(doc), heldPlans, heldStops) });
    const bytes = Uint8Array.from(atob(out.zip_b64), (c) => c.charCodeAt(0));
    const link = document.createElement("a");
    link.href = URL.createObjectURL(new Blob([bytes], { type: "application/zip" }));
    link.download = `${doc.plan?.name || "network"}.zip`;
    link.click();
    URL.revokeObjectURL(link.href);
    sd.exported = out.feed_hash;
  });
}

const POLL_MS = 30_000;

async function published(): Promise<void> {
  say("A publish that waited on this device was sent.");
  await inputs.read(...WRITTEN);
}

export function heldWrites(): { key: string; label: string; reason: string }[] {
  return outbox.held.map((w) => ({ key: w.key, label: w.label, reason: heldReason(w) }));
}

export function sendNow(): Promise<void> {
  return sendWaiting();
}

export async function retryHeld(key: string): Promise<void> {
  await retryWrite(key);
}

export function discardHeld(key: string): Promise<void> {
  return discardWrite(key);
}

/// The plans live on the device, so a reload keeps them. It waits while a change is under way.
export function applyUpdate(): void {
  if (sd.busy) return;
  applyBuild();
}
