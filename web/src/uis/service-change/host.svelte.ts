// Only this file of the UI calls the bus. Every number, list, and line that it
// writes into `sc` is an op's answer, adopted as given.
//
// The selection (plan, baseline, combination, stage, stop, route, direction,
// hub, hour window) lives in the URL hash, and the page follows it both ways.
//
// The authoring path (fork, apply, undo, export, publish) is copied from the
// service-design host, because one UI never imports another's files.

import {
  type ReadPlan,
  getAllPlans,
  getActivePlanId,
  putPlan,
  readPlan,
  removePlan,
  setActivePlan,
  restorePlans,
} from "../../plans.svelte";
import {
  ask,
  assemble,
  available,
  keep,
  produced,
  publishedPickValue,
  publishedPlanSet,
  readManifest,
  served,
  servedNetworkValue,
  Doc,
  type DeploymentManifest,
} from "../../engine";
import { stopWarnings, withStopVersions } from "../picks";
import {
  answerGraph,
  applyBuild,
  buildWaiting,
  onBuildReady,
  clockEvery,
  clockLabelAt,
  inputGraph,
  inputLabel,
  installApp,
  reason,
  setAgencyZone,
  today,
  type FrameState,
  type InputDeclaration,
  type InputState,
  type Reader,
} from "../kit";
import descriptor from "./ui.json";

import { resumeSignIn } from "../../signin.svelte";
import {
  discardWrite,
  heldReason,
  openOutbox,
  outbox,
  queueWrite,
  retryWrite,
  sendWaiting,
} from "../../outbox.svelte";
// Each document registers once in the page's one linked module.
assemble("service-change");
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { DirectoryStop } from "../../types/cap_directory/DirectoryStop";
import type { FeedInfo } from "../../types/FeedInfo";
import type { Boundary } from "../../types/Boundary";
import type { Network } from "../../types/Network";
import type { NetworkEdit } from "../../types/NetworkEdit";
import type { NetworkError } from "../../types/NetworkError";
import type { NetworkWarning } from "../../types/NetworkWarning";
import type { RouteConnections } from "../../types/RouteConnections";
import type { RouteDetail } from "../../types/RouteDetail";
import type { RouteFrequency } from "../../types/RouteFrequency";
import type { RouteLines } from "../../types/RouteLines";
import type { StopDetail } from "../../types/StopDetail";
import type { ServiceTypeComparison } from "../../types/ServiceTypeComparison";
import type { StopGaps } from "../../types/StopGaps";
import type { MissedConnection } from "../../types/MissedConnection";
import type { Timetable } from "../../types/Timetable";
import type { DayType } from "../../types/DayType";
import type { DayTypeComparison } from "../../types/DayTypeComparison";
import type { ServiceKey } from "../../types/ServiceKey";
import type { Stop } from "../../types/Stop";
import type { FleetSummary } from "../../types/FleetSummary";
import type { Suggestion } from "../../types/transit_runs/Suggestion";
import type { BlockChainTrip } from "../../types/BlockChainTrip";
import type { PlanIssue } from "../../types/cap_plans/PlanIssue";
import type { PlanPeriod } from "../../types/cap_plans/PlanPeriod";
import type { PublishedPlans } from "../../types/cap_plans/PublishedPlans";

export type Stage =
  "calendar" | "scan" | "stops" | "route" | "resources" | "proposal";
export const STAGES: { id: Stage; label: string }[] = [
  { id: "calendar", label: "Plan & calendar" },
  { id: "scan", label: "Scan" },
  { id: "stops", label: "Stops" },
  { id: "route", label: "Route" },
  { id: "resources", label: "Vehicles & hours" },
  { id: "proposal", label: "Proposal" },
];
export type ScanView = "grid" | "map";
/// `loaded` is false for a published pick until it first opens. A published pick is read-only.
export type PlanRow = {
  key: string;
  id: string;
  name: string;
  effectiveFrom: string | null;
  published: boolean;
  loaded: boolean;
};
/// Service-day hours, [from, to).
export type HourWindow = { from: number; to: number };

export const sc = $state({
  updateReady: false,
  status: "Load a GTFS feed to begin.",
  alert: false,
  busy: false,
  feed: null as { label: string; info: FeedInfo | null } | null,
  plans: [] as PlanRow[],
  pickSet: null as PublishedPlans | null,
  activeKey: "",
  compareKey: "",
  chain: null as {
    inForce: string | null;
    periods: PlanPeriod[];
    issues: PlanIssue[];
  } | null,
  issues: [] as NetworkError[],
  warnings: [] as NetworkWarning[],
  dayTypes: [] as DayType[],
  noService: false,
  group: "",
  /// The member of the group that the changes act on.
  editType: "",
  stage: "scan" as Stage,
  scanView: "grid" as ScanView,
  window: null as HourWindow | null,
  routes: [] as DirectoryRoute[],
  lines: [] as RouteLines[],
  comparison: null as ServiceTypeComparison | null,
  keys: [] as ServiceKey[],
  fleet: null as FleetSummary | null,
  suggestion: null as Suggestion | null,
  block: "",
  blockTrips: null as BlockChainTrip[] | null,
  planCompare: null as DayTypeComparison[] | null,
  frequency: null as RouteFrequency | null,
  stopOpen: "",
  stopCard: null as StopDetail | null,
  route: "",
  direction: null as string | null,
  routeDetail: null as RouteDetail | null,
  timetable: null as Timetable | null,
  editing: false,
  selectedTrip: "",
  selectedStop: "",
  stop: "",
  stops: [] as DirectoryStop[],
  gaps: null as StopGaps | null,
  connections: null as RouteConnections[] | null,
  missed: null as MissedConnection[] | null,
  canUndo: false,
  canRedo: false,
  exported: "",
  published: null as { plans_hash: string } | null,
  publishRefused: [] as object[],
  frames: {} as Record<string, FrameState>,
  inputs: {} as Record<string, InputState>,
  /// The new-route task offers the stops that the stops document lists in service or planned by the pick date.
  creating: false,
  newRoute: null as NewRouteOptions | null,
  newRouteProblems: [] as string[],
});

export type NewRouteStop = {
  value: string;
  label: string;
  sub?: string;
  fromStops: boolean;
};
export type NewRouteOptions = {
  stops: NewRouteStop[];
  keys: { id: string; name: string }[];
  routeIds: string[];
};
export type NewRouteSpec = {
  id: string;
  name: string;
  longName: string;
  directions: { id: string; name: string; stops: string[]; runSecs: number }[];
  trips: {
    direction: string;
    serviceType: string;
    firstSecs: number;
    lastSecs: number;
    everySecs: number;
  }[];
};

function active(): Network | null {
  return getAllPlans()[sc.activeKey] ?? null;
}

function baseline(): Network | null {
  return getAllPlans()[sc.compareKey] ?? null;
}

export function groupMembers(): string[] {
  for (const t of sc.dayTypes)
    for (const g of t.groups) if (g.id === sc.group) return g.members;
  return [];
}

/// Every date that a combination holds runs exactly its members, so its first date works as an address.
function groupDay(): string {
  for (const t of sc.dayTypes)
    for (const g of t.groups) if (g.id === sc.group) return g.dates[0] ?? "";
  return "";
}

/// The plan frame's pick date is the one field that a host reads of a document.
function periodFrom(): string {
  return active()?.plan?.effective_from || today();
}

function syncEditType(): void {
  const members = groupMembers();
  if (!members.includes(sc.editType)) sc.editType = members[0] ?? "";
}

export function planName(key: string): string {
  return sc.plans.find((p) => p.key === key)?.name ?? "";
}

const pickKey = (id: string) => `published:${id}`;

async function readTimeline() {
  const plans = Object.entries(getAllPlans()).map(([key, d]) => ({
    key,
    id: d.plan?.id ?? "",
    name: d.plan?.name || d.name || "Unnamed plan",
    effective_from: d.plan?.effective_from ?? null,
    // The deployment's feed (its import, `served:`) is in production
    // too until a pick takes effect.
    served: key.startsWith("served:"),
  }));
  return ask("plans", "timeline", {
    plans,
    published: sc.pickSet ?? undefined,
    today: today(),
  });
}

async function syncPlanRows(): Promise<void> {
  const { rows } = await readTimeline();
  sc.plans = rows.map((r) => ({
    key: r.key ?? pickKey(r.id),
    id: r.id,
    name: r.name,
    effectiveFrom: r.effective_from,
    published: r.published,
    loaded: r.loaded,
  }));
}

/// Before the first pick takes effect, the deployment's feed runs (the rule of engine `servedNetwork`).
export function runsToday(): string | null {
  return (
    sc.chain?.inForce ??
    sc.plans.find((p) => p.key.startsWith("served:"))?.id ??
    null
  );
}

/// A published pick is read-only. A plan with no pick date sets one first.
export function editable(key = sc.activeKey): boolean {
  const row = sc.plans.find((p) => p.key === key);
  return !!row && !row.published && !!row.id;
}

async function ensureLoaded(key: string): Promise<boolean> {
  if (getAllPlans()[key]) return true;
  return (await step("Fetching the pick…", () => fetchPick(key))) ?? false;
}

async function fetchPick(key: string): Promise<boolean> {
  const pick = sc.pickSet?.plans.find((p) => pickKey(p.id) === key);
  if (!pick) return false;
  const doc = await publishedPickValue(pick);
  if (!doc?.value) return false;
  docs.set(doc.value, doc);
  putPlan(key, doc.value);
  await syncPlanRows();
  return true;
}

/// The authoring capability decides whether the feed is stamped (`stamp_pick`). The host gives the new id.
async function stampAsPick(doc: Network, name: string): Promise<Network> {
  if (!(await available("author"))) return doc;
  const stamped = await ask("author", "stamp_pick", {
    network_hash: docOf(doc),
    name,
    id: crypto.randomUUID(),
  });
  if (!stamped.stamped) return doc;
  docs.set(
    stamped.network,
    produced("network", stamped.network, "author", stamped.network_hash),
  );
  authoredHealth.set(stamped.network, {
    issues: stamped.issues,
    warnings: stamped.warnings,
  });
  return stamped.network;
}

const authoredHealth = new WeakMap<
  Network,
  { issues: NetworkError[]; warnings: NetworkWarning[] }
>();

const docs = new WeakMap<Network, Doc<Network>>();
function docOf(network: Network): Doc<Network> {
  let doc = docs.get(network);
  if (!doc) docs.set(network, (doc = new Doc("network", { value: network })));
  return doc;
}

async function readStoredSet(
  stored: unknown,
): Promise<Record<string, unknown>> {
  return (await ask("network", "stored_plans", { document: stored })).plans;
}

async function readStoredNetwork(stored: unknown): Promise<ReadPlan | null> {
  if (!(await available("author"))) return null;
  let migrated = false;
  if (await available("migrate")) {
    const { version, reads } = await ask("migrate", "version", {
      kind: "network",
      document: stored,
    });
    if (version < reads) {
      stored = (
        await ask("migrate", "migrate", { kind: "network", document: stored })
      ).document;
      migrated = true;
    }
  }
  const answer = await ask("author", "apply", {
    network_hash: new Doc("network", { value: stored as Network }),
    edits: [],
  });
  docs.set(
    answer.network,
    produced("network", answer.network, "author", answer.network_hash),
  );
  return { network: answer.network, migrated };
}

function release(): Promise<void> {
  return keep(Object.values(getAllPlans()).map(docOf));
}

const undo: Record<string, Network[]> = {};
const redo: Record<string, Network[]> = {};

function syncHistory(): void {
  sc.canUndo = (undo[sc.activeKey]?.length ?? 0) > 0;
  sc.canRedo = (redo[sc.activeKey]?.length ?? 0) > 0;
}

function say(text: string, alert = false): void {
  sc.status = text;
  sc.alert = alert;
}

function message(e: unknown): string {
  return (e as Error)?.message ?? String(e);
}

/// An action's failure belongs to its frame. The next answer clears it.
async function act(
  frame: string,
  clear: () => void,
  run: () => Promise<void>,
): Promise<void> {
  try {
    await run();
    delete sc.frames[frame];
  } catch (e) {
    clear();
    sc.frames[frame] = { kind: "failed", message: message(e) };
    console.error(frame, e);
  }
}

const request = answerGraph(
  [
    {
      id: "document.active",
      frame: "",
      needs: ["active"],
      run: async () => {
        await readPlan(sc.activeKey, readStoredNetwork);
      },
    },
    {
      id: "document.compare",
      frame: "",
      needs: ["compare"],
      run: async () => {
        if (sc.compareKey) await readPlan(sc.compareKey, readStoredNetwork);
      },
    },
    {
      id: "analysis.day_types",
      frame: "DayTypes",
      needs: ["document.active"],
      caps: ["analysis"],
      clear: () => (sc.dayTypes = []),
      run: async (touch) => {
        const doc = active();
        if (!doc) return;
        sc.dayTypes = (
          await ask("analysis", "day_types", {
            network_hash: docOf(doc),
            from: periodFrom(),
          })
        ).day_types;
        sc.noService = sc.dayTypes.length === 0;
        if (!sc.dayTypes.some((t) => t.groups.some((g) => g.id === sc.group))) {
          sc.group = sc.dayTypes[0]?.groups[0]?.id ?? "";
          touch("group");
        }
        syncEditType();
      },
    },
    {
      id: "directory.routes",
      frame: "Jump",
      needs: ["document.active"],
      caps: ["directory"],
      clear: () => (sc.routes = []),
      run: async () => {
        const doc = active();
        if (!doc) return;
        sc.routes = (
          await ask("directory", "routes", { network_hash: docOf(doc) })
        ).routes;
      },
    },
    {
      id: "directory.route_lines",
      frame: "Map",
      needs: ["document.active", "group", "stage", "scan-view"],
      caps: ["directory"],
      visible: () =>
        (sc.stage === "scan" && sc.scanView === "map") || sc.stage === "stops",
      clear: () => (sc.lines = []),
      run: async () => {
        const doc = active();
        if (!doc || !sc.group) return void (sc.lines = []);
        sc.lines = (
          await ask("directory", "route_lines", {
            network_hash: docOf(doc),
            members: groupMembers(),
          })
        ).routes;
      },
    },
    {
      id: "analysis.service_type_compare",
      frame: "Comparison",
      // Read only where it marks the change. The comparison is the proposal's, so elsewhere it stays quiet.
      needs: ["document.active", "document.compare", "group", "stage"],
      caps: ["analysis"],
      visible: () =>
        !!sc.compareKey && (sc.stage === "scan" || sc.stage === "route"),
      clear: () => (sc.comparison = null),
      run: async () => {
        const [doc, base] = [active(), baseline()];
        if (!doc || !base || !sc.group) return void (sc.comparison = null);
        sc.comparison = (
          await ask("analysis", "service_type_compare", {
            base_hash: docOf(base),
            network_hash: docOf(doc),
            members: groupMembers(),
          })
        ).comparison;
      },
    },
    {
      id: "directory.service_keys",
      frame: "Keys",
      needs: ["document.active", "stage"],
      caps: ["directory"],
      visible: () => sc.stage === "calendar",
      clear: () => (sc.keys = []),
      run: async () => {
        const doc = active();
        if (!doc) return;
        sc.keys = (
          await ask("directory", "service_keys", { network_hash: docOf(doc) })
        ).keys;
      },
    },
    {
      id: "runcut.fleet",
      frame: "Fleet",
      needs: ["document.active", "group", "stage"],
      caps: ["runcut"],
      visible: () => sc.stage === "resources",
      clear: () => ((sc.fleet = null), (sc.suggestion = null)),
      run: async () => {
        const doc = active();
        const date = groupDay();
        if (!doc || !date)
          return void ((sc.fleet = null), (sc.suggestion = null));
        const at = { network_hash: docOf(doc), edits_hash: null, date };
        sc.fleet = (await ask("runcut", "fleet_summary", at)).summary;
        sc.suggestion = (await ask("runcut", "suggest_blocks", at)).suggestion;
      },
    },
    {
      id: "runcut.block_trips",
      frame: "Block",
      needs: ["document.active", "group", "stage", "block"],
      caps: ["runcut"],
      visible: () => sc.stage === "resources" && !!sc.block,
      clear: () => (sc.blockTrips = null),
      run: async () => {
        const doc = active();
        const date = groupDay();
        if (!doc || !date || !sc.block) return void (sc.blockTrips = null);
        const answer = await ask("runcut", "block_trips", {
          network_hash: docOf(doc),
          edits_hash: null,
          block_id: sc.block,
          date,
        });
        sc.blockTrips = answer.trips;
      },
    },
    {
      id: "analysis.plan_compare",
      frame: "Proposal",
      needs: ["document.active", "document.compare", "stage"],
      caps: ["analysis"],
      visible: () => sc.stage === "proposal",
      clear: () => (sc.planCompare = null),
      run: async () => {
        const [doc, base] = [active(), baseline()];
        if (!doc || !base) return void (sc.planCompare = null);
        sc.planCompare = (
          await ask("analysis", "plan_compare", {
            base_hash: docOf(base),
            network_hash: docOf(doc),
            from: periodFrom(),
          })
        ).day_types;
      },
    },
    {
      id: "analysis.route_frequency",
      frame: "Grid",
      needs: ["document.active", "group", "stage", "scan-view"],
      caps: ["analysis"],
      visible: () =>
        (sc.stage === "scan" && sc.scanView === "grid") || sc.stage === "route",
      clear: () => (sc.frequency = null),
      run: async () => {
        const doc = active();
        if (!doc || !sc.group) return void (sc.frequency = null);
        sc.frequency = (
          await ask("analysis", "route_frequency", {
            network_hash: docOf(doc),
            members: groupMembers(),
          })
        ).frequency;
      },
    },
    {
      id: "directory.route_detail",
      frame: "Route",
      needs: ["document.active", "route", "stage"],
      caps: ["directory"],
      visible: () => sc.stage === "route" && !!sc.route,
      clear: () => (sc.routeDetail = null),
      run: async (touch) => {
        const doc = active();
        if (!doc || !sc.route) return void (sc.routeDetail = null);
        sc.routeDetail = (
          await ask("directory", "route_detail", {
            network_hash: docOf(doc),
            route_id: sc.route,
          })
        ).detail;
        const directions = sc.routeDetail?.directions ?? [];
        if (!directions.some((d) => d.direction === sc.direction)) {
          sc.direction = directions[0]?.direction ?? null;
          touch("direction");
        }
      },
    },
    {
      id: "directory.timetable",
      frame: "StringLine",
      needs: [
        "document.active",
        "document.compare",
        "route",
        "direction",
        "group",
        "stage",
      ],
      caps: ["directory"],
      visible: () => sc.stage === "route" && !!sc.route,
      clear: () => (sc.timetable = null),
      run: async () => {
        const [doc, base] = [active(), baseline()];
        if (!doc || !sc.route || !sc.group) return void (sc.timetable = null);
        sc.timetable = (
          await ask("directory", "timetable", {
            network_hash: docOf(doc),
            route_id: sc.route,
            direction: sc.direction ?? undefined,
            members: groupMembers(),
            compare_hash: base ? docOf(base) : undefined,
          })
        ).timetable;
      },
    },
    {
      id: "directory.stops",
      frame: "Stops",
      needs: ["document.active"],
      caps: ["directory"],
      clear: () => (sc.stops = []),
      run: async () => {
        const doc = active();
        if (!doc) return;
        sc.stops = (
          await ask("directory", "stops", {
            network_hash: docOf(doc),
            limit: null,
          })
        ).stops;
      },
    },
    {
      id: "directory.stop_detail",
      frame: "StopCard",
      needs: ["document.active", "group", "stop-open", "stage"],
      caps: ["directory"],
      visible: () => sc.stage === "stops" && !!sc.stopOpen,
      clear: () => (sc.stopCard = null),
      run: async () => {
        const doc = active();
        if (!doc || !sc.stopOpen) return void (sc.stopCard = null);
        sc.stopCard = (
          await ask("directory", "stop_detail", {
            network_hash: docOf(doc),
            stop_id: sc.stopOpen,
            members: groupMembers(),
          })
        ).detail;
      },
    },
    {
      id: "analysis.hub",
      frame: "Hub",
      needs: ["document.active", "group", "stop", "stage"],
      caps: ["analysis"],
      visible: () => sc.stage === "route" && !!sc.stop,
      clear: () => (
        (sc.gaps = null),
        (sc.connections = null),
        (sc.missed = null)
      ),
      run: async () => {
        const doc = active();
        const date = groupDay();
        if (!doc || !date || !sc.stop)
          return void (
            (sc.gaps = null),
            (sc.connections = null),
            (sc.missed = null)
          );
        const at = { network_hash: docOf(doc), date, stop_id: sc.stop };
        sc.gaps = (await ask("analysis", "stop_gaps", at)).gaps;
        sc.connections = (
          await ask("analysis", "hub_connections", at)
        ).connections;
        sc.missed = (await ask("analysis", "missed_connections", at)).missed;
      },
    },
    {
      id: "plans.chain",
      frame: "Plans",
      needs: ["plans", "stage"],
      caps: ["plans"],
      visible: () => sc.stage === "calendar",
      clear: () => (sc.chain = null),
      run: async () => {
        const { in_force, periods, issues } = await readTimeline();
        sc.chain = { inForce: in_force, periods, issues };
      },
    },
    {
      id: "network.validate",
      frame: "Readiness",
      needs: ["document.active", "published", "stops-doc"],
      caps: ["network"],
      clear: () => ((sc.issues = []), (sc.warnings = [])),
      run: async () => {
        const doc = active();
        if (!doc) return;
        const { issues, warnings } =
          authoredHealth.get(doc) ??
          (await ask("network", "validate", { network_hash: docOf(doc) }));
        sc.issues = issues;
        // Stops that the plan visits and that are not in service over its period also warn, as the stops document records them.
        sc.warnings = [
          ...warnings,
          ...(await stopWarnings(docOf(doc), sc.pickSet, heldStops)),
        ];
      },
    },
  ],
  sc.frames,
  (e) => {
    say(`Error: ${message(e)}`, true);
    console.error(e);
  },
  sc.inputs,
);

async function step<T>(
  label: string,
  run: () => Promise<T>,
): Promise<T | undefined> {
  sc.busy = true;
  say(label);
  await new Promise((r) => setTimeout(r, 20));
  try {
    return await run();
  } catch (e) {
    say(`Error: ${message(e)}`, true);
    console.error(e);
    return undefined;
  } finally {
    sc.busy = false;
  }
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

function changeStatus(done: string): void {
  const health =
    sc.issues.length || sc.warnings.length
      ? `The plan now has ${plural(sc.issues.length, "issue")} and ${plural(sc.warnings.length, "warning")} (Proposal).`
      : "The plan has no issues.";
  say(`${done} ${health}`, sc.issues.length > 0);
}

let writing = false;

function writeHash(): void {
  const q = new URLSearchParams();
  if (sc.activeKey) q.set("plan", sc.activeKey);
  if (sc.compareKey) q.set("base", sc.compareKey);
  if (sc.group) q.set("group", sc.group);
  q.set("stage", sc.stage);
  if (sc.scanView !== "grid") q.set("view", sc.scanView);
  if (sc.route) q.set("route", sc.route);
  if (sc.direction != null) q.set("dir", sc.direction);
  if (sc.stop) q.set("stop", sc.stop);
  if (sc.stopOpen) q.set("open", sc.stopOpen);
  if (sc.block) q.set("block", sc.block);
  if (sc.window) q.set("hours", `${sc.window.from}-${sc.window.to}`);
  if (sc.creating) q.set("new", "route");
  const next = `#${q}`;
  if (location.hash === next) return;
  writing = true;
  history.pushState(null, "", next);
  writing = false;
}

function readHash(): string[] {
  const q = new URLSearchParams(location.hash.slice(1));
  const changed: string[] = [];
  const set = <K extends keyof typeof sc>(
    key: K,
    value: (typeof sc)[K],
    input: string,
  ) => {
    if (JSON.stringify(sc[key]) !== JSON.stringify(value)) {
      sc[key] = value;
      changed.push(input);
    }
  };
  const plan = q.get("plan");
  if (plan && getAllPlans()[plan]) set("activeKey", plan, "active");
  const base = q.get("base");
  if (base && getAllPlans()[base]) set("compareKey", base, "compare");
  set("group", q.get("group") ?? sc.group, "group");
  const stage = q.get("stage") as Stage | null;
  set(
    "stage",
    stage && STAGES.some((x) => x.id === stage) ? stage : "scan",
    "stage",
  );
  set("scanView", q.get("view") === "map" ? "map" : "grid", "scan-view");
  set("route", q.get("route") ?? "", "route");
  set("direction", q.get("dir"), "direction");
  set("stop", q.get("stop") ?? "", "stop");
  set("stopOpen", q.get("open") ?? "", "stop-open");
  set("block", q.get("block") ?? "", "block");
  const creating = q.get("new") === "route";
  if (creating !== sc.creating) {
    sc.creating = creating;
    if (creating) void readNewRouteOptions();
  }
  const hours = q.get("hours")?.match(/^(\d+)-(\d+)$/);
  set(
    "window",
    hours ? { from: Number(hours[1]), to: Number(hours[2]) } : null,
    "window",
  );
  return changed;
}

async function select(...inputs: string[]): Promise<void> {
  writeHash();
  await request(...inputs);
}

addEventListener("hashchange", () => {
  if (writing) return;
  const changed = readHash();
  if (changed.length) void request(...changed);
});
addEventListener("popstate", () => {
  const changed = readHash();
  if (changed.length) void request(...changed);
});

type Manifest = DeploymentManifest;

let manifest: Manifest | null = null;
/// The published network is read only while no plan is open.
let heldPlans: PublishedPlans | null = null;
let heldStops: Doc | null = null;
let servedBase: { network: Doc<Network>; boundary: Doc<Boundary> } | null =
  null;

const readers: Record<string, Reader<any>> = {
  manifest: {
    read: async ({ signal }) => {
      const read = await readManifest(signal);
      return {
        value: read.manifest,
        key: JSON.stringify(read.manifest),
        asOfMs: read.asOfMs ?? undefined,
        why: read.why ?? undefined,
      };
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
    adopt: async (set: PublishedPlans | null) => {
      heldPlans = set;
      sc.pickSet = set;
      await syncPlanRows();
    },
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

const inputs = inputGraph(
  descriptor.inputs as InputDeclaration[],
  readers,
  sc.inputs,
  request,
);

const WRITTEN = ["manifest", "plans", "stops"];

export function offline(): boolean {
  const m = sc.inputs.manifest;
  return (
    m?.kind === "failed" ||
    (m?.kind === "answered" && m.why === "the server is not answering")
  );
}

export function asOf(): string {
  const m = sc.inputs.manifest;
  if (
    !offline() ||
    !m ||
    (m.kind !== "answered" && m.kind !== "failed") ||
    m.asOfMs == null
  )
    return "";
  return clockLabelAt(m.asOfMs);
}

export async function boot(): Promise<void> {
  void installApp();
  onBuildReady(() => (sc.updateReady = true));
  await Promise.all([restorePlans(readStoredSet), resumeSignIn()]);
  await openOutbox("service-change", () => void published());
  await inputs.start();
  clockEvery(POLL_MS, () => {
    void sendWaiting();
    sc.updateReady = buildWaiting();
  });
  sc.activeKey = getAllPlans()[getActivePlanId()] ? getActivePlanId() : "";
  readHash();
  if (sc.activeKey) {
    await openPlan(sc.activeKey);
    return;
  }
  if (sc.pickSet?.plans.length && (await available("plans"))) {
    const inForce = (
      await ask("plans", "in_force", { published: sc.pickSet, date: today() })
    ).plan;
    if (inForce) {
      await openPlan(pickKey(inForce.id));
      return;
    }
  }
  const key = await step("Loading the deployment's feed…", async () => {
    const base = sc.inputs.network;
    if (base?.kind === "failed") throw new Error(inputLabel(base));
    const served = servedBase;
    if (!served?.network.value) return null;
    docs.set(served.network.value, served.network);
    const network = await stampAsPick(
      served.network.value,
      served.network.value.name || "The deployment's feed",
    );
    const key = `served:${served.network.pin ?? crypto.randomUUID()}`;
    putPlan(key, network);
    sc.feed = { label: network.name || "the deployment's feed", info: null };
    await syncPlanRows();
    return key;
  });
  if (key) await openPlan(key);
  else if (!sc.status.startsWith("Error")) say("Load a GTFS feed to begin.");
}

export async function loadFeedBytes(
  bytes: Uint8Array,
  label: string,
): Promise<void> {
  const key = await step(`Importing ${label}…`, async () => {
    const feed = new Doc("feed", { bytes });
    const info = (await available("gtfs"))
      ? (await ask("gtfs", "feed_info", { feed_hash: feed })).info
      : null;
    const imported = await ask("network", "import", { feed_hash: feed });
    docs.set(
      imported.network,
      produced("network", imported.network, "network", imported.network_hash),
    );
    const doc = await stampAsPick(
      imported.network,
      label.replace(/\.zip$/i, ""),
    );
    const key = `plan-${doc.plan?.id || crypto.randomUUID()}`;
    putPlan(key, doc);
    sc.feed = { label, info };
    await syncPlanRows();
    return key;
  });
  if (key) await openPlan(key);
}

/// Nothing is compared until the planner chooses.
export async function openPlan(key: string): Promise<void> {
  if (!(await ensureLoaded(key))) return;
  sc.activeKey = key;
  setActivePlan(key);
  if (sc.compareKey === key) sc.compareKey = "";
  syncHistory();
  await step("Reading the plan…", () =>
    select("active", "compare", "plans", "stage"),
  );
  say(
    `${planName(sc.activeKey)}${sc.compareKey ? ` against ${planName(sc.compareKey)}` : ""}.`,
  );
}

/// A rename is not a change to the plan's service, so it is not on the undo stack.
export async function renamePlan(key: string, name: string): Promise<void> {
  const doc = getAllPlans()[key];
  if (!doc || !name.trim() || !editable(key)) return;
  const applied = await apply(doc, [
    { edit: "update_plan", name: name.trim() },
  ]);
  if (!applied) return;
  authoredHealth.set(applied.network, {
    issues: applied.issues,
    warnings: applied.warnings,
  });
  putPlan(key, applied.network);
  await syncPlanRows();
  await release();
  await select(key === sc.activeKey ? "active" : "compare", "plans");
  say(`Renamed to "${name.trim()}".`);
}

/// A plan with no pick date is stamped as a pick. The dates it drops change its service, so it is undone like one.
export async function setPickDates(
  key: string,
  from: string,
  through = "",
): Promise<void> {
  const doc = getAllPlans()[key];
  const row = sc.plans.find((p) => p.key === key);
  if (!doc || !from || row?.published) return;
  const plan = {
    id: doc.plan?.id || crypto.randomUUID(),
    name: doc.plan?.name || row?.name || "Unnamed plan",
    effective_from: from,
  };
  const applied = await apply(doc, [
    { edit: "set_plan", plan, ...(through ? { through } : {}) },
  ]);
  if (!applied) return;
  (undo[key] ??= []).push(doc);
  redo[key] = [];
  authoredHealth.set(applied.network, {
    issues: applied.issues,
    warnings: applied.warnings,
  });
  putPlan(key, applied.network);
  await syncPlanRows();
  syncHistory();
  await release();
  await select(key === sc.activeKey ? "active" : "compare", "plans");
  say(
    `${planName(key)} now picks on ${from}${through ? ` and runs through ${through}` : ""}.`,
  );
}

/// A published pick stays published. This drops the copy here and its undo history.
export async function deletePlan(key: string): Promise<void> {
  if (!getAllPlans()[key]) return;
  const name = planName(key);
  removePlan(key);
  delete undo[key];
  delete redo[key];
  await syncPlanRows();
  await release();
  if (sc.compareKey === key) sc.compareKey = "";
  if (sc.activeKey === key) {
    sc.activeKey = "";
    const next = sc.plans.find((p) => p.loaded)?.key ?? sc.plans[0]?.key;
    if (next) await openPlan(next);
    else say("Load a GTFS feed to begin.");
  } else {
    await select("compare", "plans");
  }
  say(`Removed "${name}" from this device.`);
}

export async function setCompare(key: string): Promise<void> {
  if (key && !(await ensureLoaded(key))) return;
  sc.compareKey = key;
  await step("Comparing…", () => select("compare"));
  say(
    key
      ? `${planName(sc.activeKey)} against ${planName(key)}.`
      : `${planName(sc.activeKey)}, compared with nothing.`,
  );
}

export async function fork(
  name: string,
  effectiveFrom: string,
  through = "",
): Promise<void> {
  const doc = active();
  if (!doc || !name.trim() || !effectiveFrom) {
    say("A new plan needs a name and the day it starts.", true);
    return;
  }
  const from = sc.activeKey;
  const applied = await apply(doc, [
    {
      edit: "set_plan",
      plan: {
        id: crypto.randomUUID(),
        name: name.trim(),
        effective_from: effectiveFrom,
      },
      ...(through ? { through } : {}),
    },
  ]);
  if (!applied) return;
  const key = `plan-${applied.network.plan.id}`;
  authoredHealth.set(applied.network, {
    issues: applied.issues,
    warnings: applied.warnings,
  });
  putPlan(key, applied.network);
  await syncPlanRows();
  sc.stage = "calendar";
  await openPlan(key);
  say(
    `Started "${name.trim()}" from ${planName(from)}. Changes go into it; ${planName(from)} stays as it is.`,
  );
}

export function periodStart(): string {
  return periodFrom();
}

export function setStage(stage: Stage): Promise<void> {
  sc.stage = stage;
  return select("stage");
}

export function setScanView(view: ScanView): Promise<void> {
  sc.scanView = view;
  return select("scan-view");
}

export function selectGroup(group: string): Promise<void> {
  sc.group = group;
  syncEditType();
  return select("group");
}

export function setEditType(type: string): void {
  sc.editType = type;
}

export function setWindow(window: HourWindow | null): Promise<void> {
  sc.window = window;
  return select("window");
}

export function openRoute(route: string): Promise<void> {
  if (sc.route !== route) {
    sc.selectedStop = "";
    sc.selectedTrip = "";
    sc.stop = "";
  }
  sc.route = route;
  sc.stage = "route";
  return select("route", "stop", "stage");
}

export function openStop(stop: string): Promise<void> {
  sc.stopOpen = stop;
  sc.stage = "stops";
  return select("stop-open", "stage");
}

export function openVisit(
  route: string,
  direction: string | null,
): Promise<void> {
  if (sc.route !== route) {
    sc.selectedStop = "";
    sc.selectedTrip = "";
    sc.stop = "";
  }
  sc.route = route;
  sc.direction = direction;
  sc.stage = "route";
  return select("route", "direction", "stop", "stage");
}

export function closeRoute(): Promise<void> {
  sc.route = "";
  sc.direction = null;
  sc.routeDetail = null;
  sc.timetable = null;
  sc.selectedStop = "";
  sc.selectedTrip = "";
  sc.stop = "";
  sc.editing = false;
  return select("route", "direction", "stop");
}

export async function startNewRoute(): Promise<void> {
  await closeRoute();
  sc.creating = true;
  sc.newRouteProblems = [];
  sc.stage = "route";
  await select("stage");
  await readNewRouteOptions();
}

export function endNewRoute(): Promise<void> {
  sc.creating = false;
  sc.newRouteProblems = [];
  return select("stage");
}

/// The stops document's stops count when they are in service or on their way (planned, permitted, built) by the pick date.
async function readNewRouteOptions(): Promise<void> {
  const doc = active();
  if (!doc) return;
  const [own, keys, routes] = await Promise.all([
    ask("directory", "stops", { network_hash: docOf(doc), limit: null }),
    ask("directory", "service_keys", { network_hash: docOf(doc) }),
    ask("directory", "routes", { network_hash: docOf(doc) }),
  ]);
  const held = new Set(own.stops.map((x) => x.stop_id));
  const stops: NewRouteStop[] = own.stops.map((x) => ({
    value: x.stop_id,
    label: x.name || x.stop_id,
    sub: x.stop_id,
    fromStops: false,
  }));
  const record = heldStops;
  if (record) {
    const theirs = await ask("stops", "stops_on", {
      stops_hash: record,
      date: periodFrom(),
      statuses: ["planned", "permitted", "built", "in_service"],
    });
    for (const x of theirs.stops) {
      if (held.has(x.stop_id)) continue;
      const name = x.name || x.stop_id;
      stops.push({
        value: x.stop_id,
        label: name,
        sub: `${x.stop_id} · ${x.status.replace("_", " ")} in the stops record`,
        fromStops: true,
      });
    }
  }
  sc.newRoute = {
    stops,
    keys: keys.keys.map((k) => ({ id: k.id, name: k.name })),
    routeIds: routes.routes.map((r) => r.route_id),
  };
}

/// Each stop that the stops document holds and the plan does not is taken in with its record on the pick date. Refused whole, said beside the task.
export async function createRoute(spec: NewRouteSpec): Promise<void> {
  const doc = active();
  if (!doc || !sc.newRoute) return;
  if (!editable()) {
    sc.newRouteProblems = [
      `${planName(sc.activeKey)} does not change: fork it (Plan & calendar) and add the route to the fork.`,
    ];
    return;
  }
  sc.newRouteProblems = [];
  const edits: NetworkEdit[] = [
    { edit: "add_route", id: spec.id },
    {
      edit: "upsert_route",
      id: spec.id,
      route: {
        name: spec.name,
        mode: "bus",
        ...(spec.longName ? { long_name: spec.longName } : {}),
      },
    },
    ...spec.directions.map((d) => ({
      edit: "set_direction" as const,
      route: spec.id,
      id: d.id,
      name: d.name,
    })),
  ];
  const record = heldStops;
  if (record) {
    try {
      const take = await ask("stops", "take_stops", {
        network_hash: docOf(doc),
        stops_hash: record,
        stop_ids: spec.directions.flatMap((d) => d.stops),
        on: periodFrom(),
      });
      edits.push(...take.edits);
    } catch (e) {
      sc.newRouteProblems = [`Not created: ${message(e)}`];
      return;
    }
  }
  for (const d of spec.directions) {
    edits.push({
      edit: "add_pattern_through",
      id: `${spec.id}-${d.id}`,
      route: spec.id,
      direction: d.id,
      stops: d.stops,
      run_secs: d.runSecs,
    });
  }
  for (const t of spec.trips) {
    edits.push({
      edit: "set_headway",
      route: spec.id,
      direction: t.direction,
      service_type: t.serviceType,
      from_secs: t.firstSecs,
      to_secs: t.lastSecs + 1,
      headway_secs: t.everySecs,
      pattern: `${spec.id}-${t.direction}`,
    });
  }
  if (!(await available("author"))) {
    sc.newRouteProblems = [
      "The authoring capability did not load — changes are off.",
    ];
    return;
  }
  let applied;
  try {
    applied = await ask("author", "apply", { network_hash: docOf(doc), edits });
  } catch (e) {
    sc.newRouteProblems = [`Not created: ${message(e)}`];
    return;
  }
  docs.set(
    applied.network,
    produced("network", applied.network, "author", applied.network_hash),
  );
  (undo[sc.activeKey] ??= []).push(doc);
  redo[sc.activeKey] = [];
  sc.creating = false;
  sc.newRoute = null;
  await adopt(
    applied.network,
    { issues: applied.issues, warnings: applied.warnings },
    `Route ${spec.name || spec.id} created.`,
  );
  await openRoute(spec.id);
}

export function showBlock(block: string): Promise<void> {
  sc.block = block;
  sc.blockTrips = null;
  sc.stage = "resources";
  return select("block", "stage");
}

export function scanGroup(group: string): Promise<void> {
  sc.group = group;
  syncEditType();
  sc.stage = "scan";
  return select("group", "stage");
}

export function openBlock(block: string): Promise<void> {
  sc.block = sc.block === block ? "" : block;
  if (!sc.block) sc.blockTrips = null;
  return select("block");
}

export function openHub(stop: string): Promise<void> {
  sc.stop = stop;
  return select("stop");
}

export function selectDirection(direction: string | null): Promise<void> {
  sc.direction = direction;
  sc.selectedStop = "";
  return select("direction");
}

export function selectTimetableStop(stop: string): void {
  sc.selectedStop = sc.selectedStop === stop ? "" : stop;
}

export function selectTrip(key: string): void {
  sc.selectedTrip = key;
}

export function toggleEdit(): void {
  sc.editing = !sc.editing;
}

export function why(frame: string): string {
  return reason(sc.frames[frame]);
}

/// Errors are never cached.
export function retry(): Promise<void> {
  return request.retry();
}

async function apply(doc: Network, edits: NetworkEdit[]) {
  if (!(await available("author"))) {
    say("The authoring capability did not load — changes are off.", true);
    return null;
  }
  try {
    const applied = await ask("author", "apply", {
      network_hash: docOf(doc),
      edits,
    });
    docs.set(
      applied.network,
      produced("network", applied.network, "author", applied.network_hash),
    );
    return applied;
  } catch (e) {
    say(`Change refused: ${message(e)}`, true);
    return null;
  }
}

async function adopt(
  next: Network,
  health: { issues: NetworkError[]; warnings: NetworkWarning[] } | null,
  done: string,
): Promise<void> {
  if (health) authoredHealth.set(next, health);
  putPlan(sc.activeKey, next);
  sc.exported = "";
  sc.published = null;
  sc.publishRefused = [];
  delete sc.frames.Export;
  delete sc.frames.Publish;
  syncHistory();
  await release();
  await step("Re-reading the plan…", () => select("active"));
  changeStatus(done);
}

export async function change(edits: NetworkEdit[]): Promise<void> {
  const doc = active();
  if (!doc) return;
  if (!editable()) {
    say(
      sc.plans.find((p) => p.key === sc.activeKey)?.published
        ? `${planName(sc.activeKey)} is published, so it does not change: fork it (Plan & calendar) and change the fork.`
        : `${planName(sc.activeKey)} has no pick date: set one (Plan & calendar) before changing it.`,
      true,
    );
    return;
  }
  const applied = await apply(doc, edits);
  if (!applied) return;
  (undo[sc.activeKey] ??= []).push(doc);
  redo[sc.activeKey] = [];
  await adopt(
    applied.network,
    { issues: applied.issues, warnings: applied.warnings },
    "Changed.",
  );
}

export async function undoChange(): Promise<void> {
  const prev = undo[sc.activeKey]?.pop();
  const doc = active();
  if (!prev || !doc) return;
  (redo[sc.activeKey] ??= []).push(doc);
  await adopt(prev, null, "Undone.");
}

export async function redoChange(): Promise<void> {
  const next = redo[sc.activeKey]?.pop();
  const doc = active();
  if (!next || !doc) return;
  (undo[sc.activeKey] ??= []).push(doc);
  await adopt(next, null, "Redone.");
}

function where() {
  return {
    route: sc.route,
    direction: sc.direction ?? undefined,
    service_type: sc.editType,
  };
}

export const gestures = {
  span: (first_secs: number | null, last_secs: number | null) =>
    change([
      {
        edit: "set_span",
        ...where(),
        first_secs: first_secs ?? undefined,
        last_secs: last_secs ?? undefined,
      },
    ]),
  running: (from_secs: number, to_secs: number, delta_secs: number) =>
    change([
      {
        edit: "adjust_running_time",
        ...where(),
        from_secs,
        to_secs,
        delta_secs,
      },
    ]),
  retime: (trip: string, to_secs: number) =>
    change([{ edit: "retime_trip", trip, to_secs }]),
  insertStop: (stop: string, after: string) =>
    change([
      {
        edit: "insert_direction_stop",
        route: sc.route,
        direction: sc.direction ?? undefined,
        stop,
        after,
      },
    ]),
  removeStop: (stop: string) =>
    change([
      {
        edit: "remove_direction_stop",
        route: sc.route,
        direction: sc.direction ?? undefined,
        stop,
      },
    ]).then(() => {
      sc.selectedStop = "";
    }),
};

export const calendar = {
  toggleDay: (
    id: string,
    day: "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun",
  ) => change([{ edit: "toggle_service_day", id, day }]),
  addRange: (id: string, start: string, end: string) =>
    change([
      {
        edit: "add_service_range",
        id,
        ...(start ? { start } : {}),
        ...(end ? { end } : {}),
      },
    ]),
  setRange: (id: string, index: number, start: string, end: string) =>
    change([{ edit: "set_service_range", id, index, start, end }]),
  removeRange: (id: string, index: number) =>
    change([{ edit: "remove_service_range", id, index }]),
  setException: (id: string, date: string, state: "added" | "removed") =>
    change([{ edit: "set_service_exception", id, date, state }]),
  removeException: (id: string, date: string) =>
    change([{ edit: "remove_service_exception", id, date }]),
  rename: (id: string, name: string) =>
    change([{ edit: "update_service_type", id, name }]),
  add: () => change([{ edit: "add_service_type", from: periodFrom() }]),
  remove: (id: string) => change([{ edit: "remove_service_type", id }]),
  move: (patternTrips: string[], to: string) =>
    change(
      patternTrips.map((id) => ({
        edit: "set_pattern_trips_refs" as const,
        id,
        service_type: to,
      })),
    ),
};

export function adoptBlocks(): Promise<void> {
  const edits = sc.suggestion?.edits ?? [];
  return edits.length ? change(edits) : Promise.resolve();
}

export async function publishPlan(): Promise<void> {
  const doc = active();
  if (!doc) return;
  sc.published = null;
  sc.publishRefused = [];
  // The publish runs when it is sent, against the published plans as they are then. While the server does not answer, it waits on this device.
  await act(
    "Publish",
    () => {},
    async () => {
      const outcome = await queueWrite({
        url: "plans",
        body: doc,
        label: `Publish "${doc.plan?.name ?? "plan"}"`,
      });
      if (outcome.state === "waiting") {
        say(
          outbox.needsSignIn
            ? "The publish waits on this device. Sign in to send it."
            : "The publish waits on this device. It is sent when the server answers.",
        );
        return;
      }
      if (outcome.state === "sent") {
        sc.published = outcome.body as { plans_hash: string };
        await inputs.read(...WRITTEN);
        await select("plans");
        say(
          `Published "${doc.plan?.name}": it takes effect ${doc.plan?.effective_from}. It no longer changes; fork it to change it.`,
        );
        return;
      }
      await discardWrite(outcome.key);
      if (outcome.status === 422) {
        const answer = (outcome.reasons ?? {}) as {
          issues?: object[];
          refusals?: object[];
        };
        sc.publishRefused = [
          ...(answer.issues ?? []),
          ...(answer.refusals ?? []),
        ];
        return;
      }
      if (outcome.status === 403)
        throw new Error("your sign-in may not publish plans here");
      throw new Error(
        `the server did not publish it: ${typeof outcome.reasons === "string" ? outcome.reasons.trim() : outcome.status}`,
      );
    },
  );
}

export async function exportPlan(): Promise<void> {
  const doc = active();
  if (!doc) return;
  await act(
    "Export",
    () => (sc.exported = ""),
    async () => {
      // the export carries the stops' history over the plan's period
      const out = await ask("network", "export", {
        network_hash: await withStopVersions(docOf(doc), sc.pickSet, heldStops),
      });
      const bytes = Uint8Array.from(atob(out.zip_b64), (c) => c.charCodeAt(0));
      const link = document.createElement("a");
      link.href = URL.createObjectURL(
        new Blob([bytes], { type: "application/zip" }),
      );
      link.download = `${doc.plan?.name || "network"}.zip`;
      link.click();
      URL.revokeObjectURL(link.href);
      sc.exported = out.feed_hash;
    },
  );
}

const POLL_MS = 30_000;

async function published(): Promise<void> {
  await inputs.read(...WRITTEN);
  say("A publish that waited on this device was sent.");
}

/// A published pick that could not be read is said, never shown as none.
export function label(input: string): string {
  return inputLabel(sc.inputs[input]);
}

export function heldWrites(): { key: string; label: string; reason: string }[] {
  return outbox.held.map((w) => ({
    key: w.key,
    label: w.label,
    reason: heldReason(w),
  }));
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
  if (sc.busy) return;
  applyBuild();
}
