/// The service-change UI's host layer — the ONLY file of this UI that
/// calls the capability bus. It owns I/O (feed bytes, the export
/// download, the URL hash), the plan set (plans.svelte.ts: JS owns the
/// documents; the undo stack is earlier versions of them), and the
/// selection. Every number, list, and line it writes into `sc` is an
/// op's answer, adopted as-is.
///
/// The UI follows the stages a service change takes, as dogfood
/// sessions on C-TRAN worked them (docs/architecture/generated-uis.md):
/// the plan and its calendar (the service keys, edited), a scan of the
/// network hour by hour, the stops (each one shared by every route that
/// visits it, read on its own card), one route's day changed in place
/// with a hub it serves beside it, the vehicles and hours the change needs (adopting
/// the suggested blocks), and the proposal across every day type. The
/// selection — plan, baseline, the day type's combination, stage,
/// the open stop, route, direction, hub, hour window — lives in the URL hash, which
/// the page follows (back and forward move through it).
///
/// The authoring path (fork, apply, undo, export, publish) is the
/// service-design host's, copied: one UI never imports another's files.

import { getAllPlans, getActivePlanId, putPlan, readPlan, removePlan, setActivePlan, restorePlans } from "../../plans.svelte";
import { ask, assemble, available, keep, produced, publishedPickValue, publishedPlanSet, refreshedFromStops, servedNetworkValue, Doc } from "../../engine";
import { answerGraph, readAgencyZone, today } from "../kit";

// This page's capabilities answer inside its linked assembly (D-77).
assemble("service-change");
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { DirectoryStop } from "../../types/cap_directory/DirectoryStop";
import type { FeedInfo } from "../../types/FeedInfo";
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
import type { FleetSummary } from "../../types/FleetSummary";
import type { Suggestion } from "../../types/transit_runs/Suggestion";
import type { BlockChainTrip } from "../../types/BlockChainTrip";
import type { PlanIssue } from "../../types/cap_plans/PlanIssue";
import type { PlanPeriod } from "../../types/cap_plans/PlanPeriod";
import type { PublishedPlans } from "../../types/cap_plans/PublishedPlans";

export type Stage = "calendar" | "scan" | "stops" | "route" | "resources" | "proposal";
export const STAGES: { id: Stage; label: string }[] = [
  { id: "calendar", label: "Plan & calendar" },
  { id: "scan", label: "Scan" },
  { id: "stops", label: "Stops" },
  { id: "route", label: "Route" },
  { id: "resources", label: "Vehicles & hours" },
  { id: "proposal", label: "Proposal" },
];
export type ScanView = "grid" | "map";
/// A plan on the timeline (D-139): a pick the deployment publishes
/// (read-only: a change goes into a fork) or a draft on this device.
/// `loaded`: its document is on this device (a published pick is
/// fetched the first time it opens).
export type PlanRow = { key: string; id: string; name: string; effectiveFrom: string | null; published: boolean; loaded: boolean };
/// The hour window, service-day hours [from, to).
export type HourWindow = { from: number; to: number };

export const sc = $state({
  status: "Load a GTFS feed to begin.",
  alert: false,
  busy: false,
  feed: null as { label: string; info: FeedInfo | null } | null,
  plans: [] as PlanRow[],
  /// The deployment's published plan set (null: it publishes none).
  pickSet: null as PublishedPlans | null,
  activeKey: "",
  compareKey: "",
  chain: null as { inForce: string | null; periods: PlanPeriod[]; issues: PlanIssue[] } | null,
  issues: [] as NetworkError[],
  warnings: [] as NetworkWarning[],
  dayTypes: [] as DayType[],
  /// The plan's period was read and runs no service (no day type).
  noService: false,
  /// The selected combination's id (its members joined " + ").
  group: "",
  /// The member of the group the changes act on (D-8).
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
  /// The block open on Vehicles & hours, and its chain.
  block: "",
  blockTrips: null as BlockChainTrip[] | null,
  planCompare: null as DayTypeComparison[] | null,
  frequency: null as RouteFrequency | null,
  /// The stop open on the Stops stage, and its card.
  stopOpen: "",
  stopCard: null as StopDetail | null,
  // The drawer: a route, or a hub.
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
  failed: {} as Record<string, string>,
});

// ---------------------------------------------------------------------
// documents
// ---------------------------------------------------------------------

function active(): Network | null {
  return getAllPlans()[sc.activeKey] ?? null;
}

function baseline(): Network | null {
  return getAllPlans()[sc.compareKey] ?? null;
}

/// The selected combination's members, as day_types answered them.
export function groupMembers(): string[] {
  for (const t of sc.dayTypes) for (const g of t.groups) if (g.id === sc.group) return g.members;
  return [];
}

/// The date the date-scoped hub reads are asked on: the combination's
/// first date in the period. Every date a combination holds runs
/// exactly its members (D-55), so this is an address, never a label.
function groupDay(): string {
  for (const t of sc.dayTypes) for (const g of t.groups) if (g.id === sc.group) return g.dates[0] ?? "";
  return "";
}

/// The period the plan answers for: from its pick on (the plan frame,
/// the one field a host reads of a document), else from today.
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

/// A published pick's key until (and after) its document is fetched.
const pickKey = (id: string) => `published:${id}`;

/// The timeline's rows: every plan on this device, then every
/// published pick not on it yet. A plan whose identity the deployment
/// publishes is published, wherever it was made.
function syncPlanRows(): void {
  const picks = (sc.pickSet?.plans ?? []).filter((p) => p.status === "published");
  const publishedIds = new Set(picks.map((p) => p.id));
  const rows: PlanRow[] = Object.entries(getAllPlans()).map(([key, d]) => ({
    key,
    id: d.plan?.id ?? "",
    name: d.plan?.name || d.name || "Unnamed plan",
    effectiveFrom: d.plan?.effective_from ?? null,
    // The deployment's feed (its import, `served:`) is in production
    // too until a pick takes effect: read-only like a pick.
    published: (!!d.plan?.id && publishedIds.has(d.plan.id)) || key.startsWith("served:"),
    loaded: true,
  }));
  for (const p of picks) {
    if (!rows.some((r) => r.id === p.id)) {
      rows.push({ key: pickKey(p.id), id: p.id, name: p.name, effectiveFrom: p.effective_from, published: true, loaded: false });
    }
  }
  sc.plans = rows;
}

/// What runs in production today: the published pick in force, else
/// — before the first takes effect — the deployment's feed (the rule
/// the deployment serves by, engine `servedNetwork`).
export function runsToday(): string | null {
  return sc.chain?.inForce ?? sc.plans.find((p) => p.key.startsWith("served:"))?.id ?? null;
}

/// Can the open plan change? A draft with a pick date can; a published
/// pick is read-only (D-139), and a plan with no pick date sets one
/// first.
export function editable(key = sc.activeKey): boolean {
  const row = sc.plans.find((p) => p.key === key);
  return !!row && !row.published && !!row.id;
}

/// A published pick's document, fetched once and kept on this device
/// (the status says so only while it is fetched).
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
  syncPlanRows();
  return true;
}

/// A feed's document stamped as a pick on its first service date
/// (D-140): the date it is listed under, changeable on its row.
async function stampAsPick(doc: Network, name: string, firstServiceDate: string | null): Promise<Network> {
  if (doc.plan?.id || !firstServiceDate) return doc;
  const applied = await apply(doc, [{ edit: "set_plan", plan: { id: crypto.randomUUID(), name, effective_from: firstServiceDate } }]);
  if (!applied) return doc;
  authoredHealth.set(applied.network, { issues: applied.issues, warnings: applied.warnings });
  return applied.network;
}

const authoredHealth = new WeakMap<Network, { issues: NetworkError[]; warnings: NetworkWarning[] }>();

const docs = new WeakMap<Network, Doc<Network>>();
function docOf(network: Network): Doc<Network> {
  let doc = docs.get(network);
  if (!doc) docs.set(network, (doc = new Doc("network", { value: network })));
  return doc;
}

async function readStoredNetwork(stored: unknown): Promise<Network | null> {
  if (!(await available("author"))) return null;
  if (await available("migrate")) {
    const { version, reads } = await ask("migrate", "version", { kind: "network", document: stored });
    if (version < reads) stored = (await ask("migrate", "migrate", { kind: "network", document: stored })).document;
  }
  const answer = await ask("author", "apply", { network_hash: new Doc("network", { value: stored as Network }), edits: [] });
  docs.set(answer.network, produced("network", answer.network, "author", answer.network_hash));
  return answer.network;
}

/// A draft's stops refreshed from the deployment's stops document for
/// its window (D-156: a pick's stops are a copy, and the copy never
/// wins). Not an edit: it is not on the undo stack. A published pick is
/// the server's, refreshed when it publishes.
async function refreshStops(key: string, published: PublishedPlans | null): Promise<void> {
  const doc = getAllPlans()[key];
  if (!doc) return;
  try {
    const refreshed = await refreshedFromStops(docOf(doc), published);
    if (!refreshed?.value) return;
    docs.set(refreshed.value, refreshed);
    putPlan(key, refreshed.value);
  } catch (e) {
    say(`The stops could not be refreshed: ${e instanceof Error ? e.message : e}`, true);
  }
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

// ---------------------------------------------------------------------
// status and failure, per frame
// ---------------------------------------------------------------------

function say(text: string, alert = false): void {
  sc.status = text;
  sc.alert = alert;
}

function message(e: unknown): string {
  return (e as Error)?.message ?? String(e);
}

async function framed(frame: string, clear: () => void, run: () => Promise<void>): Promise<void> {
  try {
    await run();
    delete sc.failed[frame];
  } catch (e) {
    clear();
    sc.failed[frame] = message(e);
    console.error(frame, e);
  }
}

// ---------------------------------------------------------------------
// TRANSIENT — this UI's answer graph (src/uis/kit.ts). Inputs:
// "active" / "compare" / "plans" (the documents), "group" / "route" /
// "direction" / "stop" / "stop-open" / "window" (the selection), "stage" /
// "scan-view" (what is on screen).
// ---------------------------------------------------------------------

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
      run: (touch) =>
        framed("DayTypes", () => (sc.dayTypes = []), async () => {
          const doc = active();
          if (!doc) return;
          sc.dayTypes = (await ask("analysis", "day_types", { network_hash: docOf(doc), from: periodFrom() })).day_types;
          sc.noService = sc.dayTypes.length === 0;
          // The selection keys on a combination's id; it stays while
          // the plan's period still holds it, else the first type's
          // regular service.
          if (!sc.dayTypes.some((t) => t.groups.some((g) => g.id === sc.group))) {
            sc.group = sc.dayTypes[0]?.groups[0]?.id ?? "";
            touch("group");
          }
          syncEditType();
        }),
    },
    {
      id: "directory.routes",
      frame: "Jump",
      needs: ["document.active"],
      run: () =>
        framed("Jump", () => (sc.routes = []), async () => {
          const doc = active();
          if (!doc) return;
          sc.routes = (await ask("directory", "routes", { network_hash: docOf(doc) })).routes;
        }),
    },
    {
      id: "directory.route_lines",
      frame: "Map",
      needs: ["document.active", "group", "stage", "scan-view"],
      // The network's lines: the scan's map, and the Stops stage's
      // ground under the stops.
      visible: () => (sc.stage === "scan" && sc.scanView === "map") || sc.stage === "stops",
      run: () =>
        framed("Map", () => (sc.lines = []), async () => {
          const doc = active();
          if (!doc || !sc.group) return void (sc.lines = []);
          sc.lines = (await ask("directory", "route_lines", { network_hash: docOf(doc), members: groupMembers() })).routes;
        }),
    },
    {
      id: "analysis.service_type_compare",
      frame: "Comparison",
      // Read only where it marks the change: the scan's changed routes
      // and the route's ledger and before → after. The comparison is
      // the proposal's; elsewhere it stays quiet.
      needs: ["document.active", "document.compare", "group", "stage"],
      visible: () => !!sc.compareKey && (sc.stage === "scan" || sc.stage === "route"),
      hide: () => (sc.comparison = null),
      run: () =>
        framed("Comparison", () => (sc.comparison = null), async () => {
          const [doc, base] = [active(), baseline()];
          if (!doc || !base || !sc.group) return void (sc.comparison = null);
          sc.comparison = (
            await ask("analysis", "service_type_compare", { base_hash: docOf(base), network_hash: docOf(doc), members: groupMembers() })
          ).comparison;
        }),
    },
    {
      id: "directory.service_keys",
      frame: "Keys",
      needs: ["document.active", "stage"],
      visible: () => sc.stage === "calendar",
      run: () =>
        framed("Keys", () => (sc.keys = []), async () => {
          const doc = active();
          if (!doc) return;
          sc.keys = (await ask("directory", "service_keys", { network_hash: docOf(doc) })).keys;
        }),
    },
    {
      // What the change needs, on a date the day type runs: the
      // fleet as blocked, and the blocks suggested for the trips no
      // block carries (D-46).
      id: "runcut.fleet",
      frame: "Fleet",
      needs: ["document.active", "group", "stage"],
      visible: () => sc.stage === "resources",
      run: () =>
        framed("Fleet", () => ((sc.fleet = null), (sc.suggestion = null)), async () => {
          const doc = active();
          const date = groupDay();
          if (!doc || !date) return void ((sc.fleet = null), (sc.suggestion = null));
          const at = { network_hash: docOf(doc), edits_hash: null, date };
          sc.fleet = (await ask("runcut", "fleet_summary", at)).summary;
          sc.suggestion = (await ask("runcut", "suggest_blocks", at)).suggestion;
        }),
    },
    {
      // One block's chain on the same date: its trips in order and
      // the layover after each.
      id: "runcut.block_trips",
      frame: "Block",
      needs: ["document.active", "group", "stage", "block"],
      visible: () => sc.stage === "resources" && !!sc.block,
      run: () =>
        framed("Block", () => (sc.blockTrips = null), async () => {
          const doc = active();
          const date = groupDay();
          if (!doc || !date || !sc.block) return void (sc.blockTrips = null);
          const answer = await ask("runcut", "block_trips", { network_hash: docOf(doc), edits_hash: null, block_id: sc.block, date });
          sc.blockTrips = answer.trips;
        }),
    },
    {
      id: "analysis.plan_compare",
      frame: "Proposal",
      needs: ["document.active", "document.compare", "stage"],
      visible: () => sc.stage === "proposal",
      run: () =>
        framed("Proposal", () => (sc.planCompare = null), async () => {
          const [doc, base] = [active(), baseline()];
          if (!doc || !base) return void (sc.planCompare = null);
          sc.planCompare = (
            await ask("analysis", "plan_compare", { base_hash: docOf(base), network_hash: docOf(doc), from: periodFrom() })
          ).day_types;
        }),
    },
    {
      id: "analysis.route_frequency",
      frame: "Grid",
      needs: ["document.active", "group", "stage", "scan-view"],
      visible: () => (sc.stage === "scan" && sc.scanView === "grid") || sc.stage === "route",
      run: () =>
        framed("Grid", () => (sc.frequency = null), async () => {
          const doc = active();
          if (!doc || !sc.group) return void (sc.frequency = null);
          sc.frequency = (await ask("analysis", "route_frequency", { network_hash: docOf(doc), members: groupMembers() })).frequency;
        }),
    },
    {
      id: "directory.route_detail",
      frame: "Route",
      needs: ["document.active", "route", "stage"],
      visible: () => sc.stage === "route" && !!sc.route,
      run: (touch) =>
        framed("Route", () => (sc.routeDetail = null), async () => {
          const doc = active();
          if (!doc || !sc.route) return void (sc.routeDetail = null);
          sc.routeDetail = (await ask("directory", "route_detail", { network_hash: docOf(doc), route_id: sc.route })).detail;
          const directions = sc.routeDetail?.directions ?? [];
          if (!directions.some((d) => d.direction === sc.direction)) {
            sc.direction = directions[0]?.direction ?? null;
            touch("direction");
          }
        }),
    },
    {
      id: "directory.timetable",
      frame: "StringLine",
      needs: ["document.active", "document.compare", "route", "direction", "group", "stage"],
      visible: () => sc.stage === "route" && !!sc.route,
      run: () =>
        framed("StringLine", () => (sc.timetable = null), async () => {
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
        }),
    },
    {
      id: "directory.stops",
      frame: "Stops",
      needs: ["document.active"],
      run: () =>
        framed("Stops", () => (sc.stops = []), async () => {
          const doc = active();
          if (!doc) return;
          sc.stops = (await ask("directory", "stops", { network_hash: docOf(doc), limit: null })).stops;
        }),
    },
    {
      // The open stop's card: its record and every pattern that
      // visits it, the trips counted under the day type.
      id: "directory.stop_detail",
      frame: "StopCard",
      needs: ["document.active", "group", "stop-open", "stage"],
      visible: () => sc.stage === "stops" && !!sc.stopOpen,
      run: () =>
        framed("StopCard", () => (sc.stopCard = null), async () => {
          const doc = active();
          if (!doc || !sc.stopOpen) return void (sc.stopCard = null);
          sc.stopCard = (
            await ask("directory", "stop_detail", { network_hash: docOf(doc), stop_id: sc.stopOpen, members: groupMembers() })
          ).detail;
        }),
    },
    {
      id: "analysis.hub",
      frame: "Hub",
      needs: ["document.active", "group", "stop", "stage"],
      visible: () => sc.stage === "route" && !!sc.stop,
      run: () =>
        framed("Hub", () => ((sc.gaps = null), (sc.connections = null), (sc.missed = null)), async () => {
          const doc = active();
          const date = groupDay();
          if (!doc || !date || !sc.stop) return void ((sc.gaps = null), (sc.connections = null), (sc.missed = null));
          const at = { network_hash: docOf(doc), date, stop_id: sc.stop };
          sc.gaps = (await ask("analysis", "stop_gaps", at)).gaps;
          sc.connections = (await ask("analysis", "hub_connections", at)).connections;
          sc.missed = (await ask("analysis", "missed_connections", at)).missed;
        }),
    },
    {
      id: "plans.chain",
      frame: "Plans",
      needs: ["plans", "stage"],
      visible: () => sc.stage === "calendar",
      run: () =>
        framed("Plans", () => (sc.chain = null), async () => {
          const infos = sc.plans.filter((p) => p.id).map((p) => ({ id: p.id, name: p.name, effective_from: p.effectiveFrom }));
          // Only a published pick is ever in force (a draft runs
          // nowhere): the plans module answers it from the published set.
          const inForce = sc.pickSet ? (await ask("plans", "in_force", { published: sc.pickSet, date: today() })).plan?.id ?? null : null;
          sc.chain = {
            inForce,
            periods: (await ask("plans", "periods", { plans: infos })).periods,
            issues: (await ask("plans", "validate", { plans: infos })).issues,
          };
        }),
    },
    {
      id: "network.validate",
      frame: "Readiness",
      needs: ["document.active"],
      run: () =>
        framed("Readiness", () => ((sc.issues = []), (sc.warnings = [])), async () => {
          const doc = active();
          if (!doc) return;
          const { issues, warnings } = authoredHealth.get(doc) ?? (await ask("network", "validate", { network_hash: docOf(doc) }));
          sc.issues = issues;
          sc.warnings = warnings;
        }),
    },
  ],
  (e) => {
    say(`Error: ${message(e)}`, true);
    console.error(e);
  },
);

async function step<T>(label: string, run: () => Promise<T>): Promise<T | undefined> {
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

// ---------------------------------------------------------------------
// the URL hash — the selection, followed both ways
// ---------------------------------------------------------------------

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
  const next = `#${q}`;
  if (location.hash === next) return;
  writing = true;
  history.pushState(null, "", next);
  writing = false;
}

/// The hash's selection; returns the inputs that changed.
function readHash(): string[] {
  const q = new URLSearchParams(location.hash.slice(1));
  const changed: string[] = [];
  const set = <K extends keyof typeof sc>(key: K, value: (typeof sc)[K], input: string) => {
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
  set("stage", stage && STAGES.some((x) => x.id === stage) ? stage : "scan", "stage");
  set("scanView", q.get("view") === "map" ? "map" : "grid", "scan-view");
  set("route", q.get("route") ?? "", "route");
  set("direction", q.get("dir"), "direction");
  set("stop", q.get("stop") ?? "", "stop");
  set("stopOpen", q.get("open") ?? "", "stop-open");
  set("block", q.get("block") ?? "", "block");
  const hours = q.get("hours")?.match(/^(\d+)-(\d+)$/);
  set("window", hours ? { from: Number(hours[1]), to: Number(hours[2]) } : null, "window");
  return changed;
}

async function select(...inputs: string[]): Promise<void> {
  writeHash();
  await request(...inputs);
}

// Back, forward, and an edited address all move the page.
addEventListener("hashchange", () => {
  if (writing) return;
  const changed = readHash();
  if (changed.length) void request(...changed);
});
addEventListener("popstate", () => {
  const changed = readHash();
  if (changed.length) void request(...changed);
});

// ---------------------------------------------------------------------
// boot and loaders
// ---------------------------------------------------------------------

/// The page opens on the plan it had open, else what runs today: the
/// pick in force (the deployment's timeline, D-139), else — before the
/// first pick takes effect — the deployment's feed, as a read-only
/// pick on its first service date (D-140), beside the published picks.
export async function boot(): Promise<void> {
  await Promise.all([restorePlans(), readAgencyZone()]);
  sc.pickSet = await publishedPlanSet().catch(() => null);
  syncPlanRows();
  sc.activeKey = getAllPlans()[getActivePlanId()] ? getActivePlanId() : "";
  readHash();
  if (sc.activeKey) {
    await openPlan(sc.activeKey);
    return;
  }
  // Production today: the pick in force, else (before the first takes
  // effect) the deployment's feed.
  if (sc.pickSet?.plans.length && (await available("plans"))) {
    const inForce = (await ask("plans", "in_force", { published: sc.pickSet, date: today() })).plan;
    if (inForce) {
      await openPlan(pickKey(inForce.id));
      return;
    }
  }
  const key = await step("Loading the deployment's feed…", async () => {
    const served = await servedNetworkValue();
    if (!served?.network.value) return null;
    docs.set(served.network.value, served.network);
    const network = await stampAsPick(served.network.value, served.network.value.name || "The deployment's feed", served.firstServiceDate);
    const key = `served:${served.network.pin ?? crypto.randomUUID()}`;
    putPlan(key, network);
    sc.feed = { label: network.name || "the deployment's feed", info: null };
    syncPlanRows();
    return key;
  });
  if (key) await openPlan(key);
  else if (!sc.status.startsWith("Error")) say("Load a GTFS feed to begin.");
}

export async function loadFeedBytes(bytes: Uint8Array, label: string): Promise<void> {
  const key = await step(`Importing ${label}…`, async () => {
    const feed = new Doc("feed", { bytes });
    const info = (await available("gtfs")) ? (await ask("gtfs", "feed_info", { feed_hash: feed })).info : null;
    const imported = await ask("network", "import", { feed_hash: feed });
    docs.set(imported.network, produced("network", imported.network, "network", imported.network_hash));
    const doc = await stampAsPick(imported.network, label.replace(/\.zip$/i, ""), imported.first_service_date ?? null);
    const key = `plan-${doc.plan?.id || crypto.randomUUID()}`;
    putPlan(key, doc);
    sc.feed = { label, info };
    syncPlanRows();
    return key;
  });
  if (key) await openPlan(key);
}

// ---------------------------------------------------------------------
// the plan set
// ---------------------------------------------------------------------

/// A plan opened; nothing is compared until the planner chooses (D-141).
export async function openPlan(key: string): Promise<void> {
  if (!(await ensureLoaded(key))) return;
  if (!sc.plans.find((p) => p.key === key)?.published) await refreshStops(key, sc.pickSet);
  sc.activeKey = key;
  setActivePlan(key);
  if (sc.compareKey === key) sc.compareKey = "";
  syncHistory();
  await step("Reading the plan…", () => select("active", "compare", "plans", "stage"));
  say(`${planName(sc.activeKey)}${sc.compareKey ? ` against ${planName(sc.compareKey)}` : ""}.`);
}

/// A plan renamed (the plan frame's name, through cap-author). The
/// rename is not a change to the plan's service, so it is not on the
/// undo stack.
export async function renamePlan(key: string, name: string): Promise<void> {
  const doc = getAllPlans()[key];
  if (!doc || !name.trim() || !editable(key)) return;
  const applied = await apply(doc, [{ edit: "update_plan", name: name.trim() }]);
  if (!applied) return;
  authoredHealth.set(applied.network, { issues: applied.issues, warnings: applied.warnings });
  putPlan(key, applied.network);
  syncPlanRows();
  await release();
  await select(key === sc.activeKey ? "active" : "compare", "plans");
  say(`Renamed to "${name.trim()}".`);
}

/// A draft's period set: the day it picks on and, optionally, the day
/// it runs through — the fork's own edit (author `set_plan`): the
/// weekly service carried to the end (D-41) and the calendar cut to
/// the period (D-138). A plan with no pick date is stamped as a pick.
/// The dates it drops are a change to its service, so it is undone like
/// one.
export async function setPickDates(key: string, from: string, through = ""): Promise<void> {
  const doc = getAllPlans()[key];
  const row = sc.plans.find((p) => p.key === key);
  if (!doc || !from || row?.published) return;
  const plan = { id: doc.plan?.id || crypto.randomUUID(), name: doc.plan?.name || row?.name || "Unnamed plan", effective_from: from };
  const applied = await apply(doc, [{ edit: "set_plan", plan, ...(through ? { through } : {}) }]);
  if (!applied) return;
  (undo[key] ??= []).push(doc);
  redo[key] = [];
  authoredHealth.set(applied.network, { issues: applied.issues, warnings: applied.warnings });
  putPlan(key, applied.network);
  syncPlanRows();
  syncHistory();
  await release();
  await select(key === sc.activeKey ? "active" : "compare", "plans");
  say(`${planName(key)} now picks on ${from}${through ? ` and runs through ${through}` : ""}.`);
}

/// A plan removed from this device. A published pick stays published
/// (and on the timeline); this drops the copy here and its undo history.
export async function deletePlan(key: string): Promise<void> {
  if (!getAllPlans()[key]) return;
  const name = planName(key);
  removePlan(key);
  delete undo[key];
  delete redo[key];
  syncPlanRows();
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
  say(key ? `${planName(sc.activeKey)} against ${planName(key)}.` : `${planName(sc.activeKey)}, compared with nothing.`);
}

export async function fork(name: string, effectiveFrom: string, through = ""): Promise<void> {
  const doc = active();
  if (!doc || !name.trim() || !effectiveFrom) {
    say("A new plan needs a name and the day it starts.", true);
    return;
  }
  const from = sc.activeKey;
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
  sc.stage = "calendar";
  await openPlan(key);
  say(`Started "${name.trim()}" from ${planName(from)}. Changes go into it; ${planName(from)} stays as it is.`);
}

// ---------------------------------------------------------------------
// the selection
// ---------------------------------------------------------------------

/// The day the plan's period starts, as the views say it.
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

/// A route opens the Route stage, wherever it was chosen.
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

/// A stop opened on the Stops stage, wherever it was chosen (the map,
/// the search box). It stays open until another stop is chosen.
export function openStop(stop: string): Promise<void> {
  sc.stopOpen = stop;
  sc.stage = "stops";
  return select("stop-open", "stage");
}

/// A visit on a stop's card opens its route in the visit's direction.
export function openVisit(route: string, direction: string | null): Promise<void> {
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

/// The route closed: the Route stage is empty again, and the grid and
/// map mark no route.
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

/// A block opened on Vehicles & hours, from wherever it was named (a
/// trip's row, a readiness issue).
export function showBlock(block: string): Promise<void> {
  sc.block = block;
  sc.blockTrips = null;
  sc.stage = "resources";
  return select("block", "stage");
}

/// A day type opened on the Scan, from the proposal's scorecard.
export function scanGroup(group: string): Promise<void> {
  sc.group = group;
  syncEditType();
  sc.stage = "scan";
  return select("group", "stage");
}

/// A block's chain opened (or closed, when it is the open one).
export function openBlock(block: string): Promise<void> {
  sc.block = sc.block === block ? "" : block;
  if (!sc.block) sc.blockTrips = null;
  return select("block");
}

/// A hub the route serves, read beside the route's day.
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

export function retry(): Promise<void> {
  return select("active", "compare", "plans", "stage", "scan-view", "stop-open", "route", "stop", "block");
}

// ---------------------------------------------------------------------
// changes — NetworkEdits through cap-author, the answer adopted
// ---------------------------------------------------------------------

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
  putPlan(sc.activeKey, next);
  sc.exported = "";
  sc.published = null;
  sc.publishRefused = [];
  delete sc.failed.Export;
  delete sc.failed.Publish;
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
  await adopt(applied.network, { issues: applied.issues, warnings: applied.warnings }, "Changed.");
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
  return { route: sc.route, direction: sc.direction ?? undefined, service_type: sc.editType };
}

export const gestures = {
  span: (first_secs: number | null, last_secs: number | null) =>
    change([{ edit: "set_span", ...where(), first_secs: first_secs ?? undefined, last_secs: last_secs ?? undefined }]),
  running: (from_secs: number, to_secs: number, delta_secs: number) =>
    change([{ edit: "adjust_running_time", ...where(), from_secs, to_secs, delta_secs }]),
  retime: (trip: string, to_secs: number) => change([{ edit: "retime_trip", trip, to_secs }]),
  insertStop: (stop: string, after: string) =>
    change([{ edit: "insert_direction_stop", route: sc.route, direction: sc.direction ?? undefined, stop, after }]),
  removeStop: (stop: string) =>
    change([{ edit: "remove_direction_stop", route: sc.route, direction: sc.direction ?? undefined, stop }]).then(() => {
      sc.selectedStop = "";
    }),
};

/// The calendar's edits on one service key (cap-author owns the rules).
export const calendar = {
  toggleDay: (id: string, day: "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun") => change([{ edit: "toggle_service_day", id, day }]),
  /// A date left empty continues the last range (cap-author).
  addRange: (id: string, start: string, end: string) =>
    change([{ edit: "add_service_range", id, ...(start ? { start } : {}), ...(end ? { end } : {}) }]),
  setRange: (id: string, index: number, start: string, end: string) => change([{ edit: "set_service_range", id, index, start, end }]),
  removeRange: (id: string, index: number) => change([{ edit: "remove_service_range", id, index }]),
  setException: (id: string, date: string, state: "added" | "removed") => change([{ edit: "set_service_exception", id, date, state }]),
  removeException: (id: string, date: string) => change([{ edit: "remove_service_exception", id, date }]),
  rename: (id: string, name: string) => change([{ edit: "update_service_type", id, name }]),
  /// A new key: weekdays, a year from the plan's period start.
  add: () => change([{ edit: "add_service_type", from: periodFrom() }]),
  /// Refused while trips run under the key (cap-author).
  remove: (id: string) => change([{ edit: "remove_service_type", id }]),
  /// A route's trips onto another key: each pattern trips it runs
  /// under this key now runs under `to`.
  move: (patternTrips: string[], to: string) => change(patternTrips.map((id) => ({ edit: "set_pattern_trips_refs" as const, id, service_type: to }))),
};

/// The suggested blocks, adopted: their edits applied as one change.
export function adoptBlocks(): Promise<void> {
  const edits = sc.suggestion?.edits ?? [];
  return edits.length ? change(edits) : Promise.resolve();
}

// ---------------------------------------------------------------------
// the publish path
// ---------------------------------------------------------------------

export async function publishPlan(): Promise<void> {
  const doc = active();
  if (!doc) return;
  sc.published = null;
  sc.publishRefused = [];
  await framed("Publish", () => {}, async () => {
    const resp = await fetch("/plans", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(doc) });
    if (resp.status === 422) {
      const answer = (await resp.json()) as { issues?: object[]; refusals?: object[] };
      sc.publishRefused = [...(answer.issues ?? []), ...(answer.refusals ?? [])];
      return;
    }
    if (!resp.ok) throw new Error(`the server did not publish it: ${(await resp.text()).trim() || resp.status}`);
    sc.published = (await resp.json()) as { plans_hash: string };
    // Published now: on the timeline as a pick, read-only (D-139).
    sc.pickSet = await publishedPlanSet().catch(() => sc.pickSet);
    syncPlanRows();
    await select("plans");
    say(`Published "${doc.plan?.name}": it takes effect ${doc.plan?.effective_from}. It no longer changes; fork it to change it.`);
  });
}

export async function exportPlan(): Promise<void> {
  const doc = active();
  if (!doc) return;
  await framed("Export", () => (sc.exported = ""), async () => {
    const out = await ask("network", "export", { network_hash: docOf(doc) });
    const bytes = Uint8Array.from(atob(out.zip_b64), (c) => c.charCodeAt(0));
    const link = document.createElement("a");
    link.href = URL.createObjectURL(new Blob([bytes], { type: "application/zip" }));
    link.download = `${doc.plan?.name || "network"}.zip`;
    link.click();
    URL.revokeObjectURL(link.href);
    sc.exported = out.feed_hash;
  });
}
