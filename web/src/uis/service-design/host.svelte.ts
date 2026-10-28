/// The service-design UI's host layer — the ONLY file of this UI that calls the
/// capability bus. It owns I/O (feed and layer bytes), the plan set
/// (shared with every other shell through state.svelte.ts: JS owns the
/// documents), and the selection; every number, list, and line it
/// writes into `wb` is an op's answer, adopted as-is. The shell's
/// components take these as props and emit events back here.
///
/// The questions it answers, in the order they are asked (its
/// 2026-10-27 dogfood sessions on C-TRAN): which
/// plan, what each service type costs and covers, which routes run
/// under a type, what one route is and where it runs, what one
/// direction's timetable is, and what a timetable change does to the
/// type against the baseline (the 2026-10-27 "proposing timetable
/// changes" session).

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
  runcutCap,
  runcutModule,
  type AnalysisLayerHandle,
} from "../../capabilities.svelte";
import { answer, answerGraph, hashIn, rememberHash, today } from "../kit";
import type { BlockChainTrip } from "../../types/BlockChainTrip";
import type { DayBlock } from "../../types/DayBlock";
import type { EntityFocus } from "../../types/EntityFocus";
import type { EntityKind } from "../../types/EntityKind";
import type { FeedInfo } from "../../types/FeedInfo";
import type { Network } from "../../types/Network";
import type { NetworkEdit } from "../../types/NetworkEdit";
import type { NetworkError } from "../../types/NetworkError";
import type { NetworkWarning } from "../../types/NetworkWarning";
import type { RouteDetail } from "../../types/RouteDetail";
import type { ServiceTypeComparison } from "../../types/ServiceTypeComparison";
import type { Timetable } from "../../types/Timetable";
import type { ServiceTypeDetail } from "../../types/ServiceTypeDetail";
import type { ServiceTypeSummary } from "../../types/ServiceTypeSummary";
import type { ServiceGroup } from "../../types/ServiceGroup";
import type { TypeCoverage } from "../../types/TypeCoverage";
import type { PlanIssue } from "../../types/cap_plans/PlanIssue";
import type { PlanPeriod } from "../../types/cap_plans/PlanPeriod";

export type PlanRow = { key: string; id: string; name: string; effectiveFrom: string | null };
export type CoverageParams = { radius_m: number; frequent_secs: number; min_departures: number };

export const wb = $state({
  status: "Load a GTFS feed to begin.",
  busy: false,
  feed: null as { label: string; info: FeedInfo | null } | null,
  plans: [] as PlanRow[],
  activeKey: "",
  compareKey: "",
  chain: null as { inForce: string | null; periods: PlanPeriod[]; issues: PlanIssue[] } | null,
  issues: [] as NetworkError[],
  warnings: [] as NetworkWarning[],
  types: [] as ServiceTypeSummary[],
  /// The service GROUPS — the families behind the ids (QAF-65):
  /// the table renders these, one row per base with its riders
  /// folded in; the drill-downs key on the base.
  groups: [] as ServiceGroup[],
  compareGroups: [] as ServiceGroup[],
  compareTypes: [] as ServiceTypeSummary[],
  layer: null as AnalysisLayerHandle | null,
  coverageParams: { radius_m: 400, frequent_secs: 900, min_departures: 2 } as CoverageParams,
  coverage: null as TypeCoverage[] | null,
  selectedType: "",
  typeDetail: null as ServiceTypeDetail | null,
  selectedRoute: "",
  routeDetail: null as RouteDetail | null,
  selectedDirection: null as string | null,
  timetable: null as Timetable | null,
  selectedStop: "",
  comparison: null as ServiceTypeComparison | null,
  /// Null until the fleet panel opens (answered only while it is).
  blocks: null as DayBlock[] | null,
  fleetOpen: false,
  selectedBlock: "",
  blockChain: null as { trips: BlockChainTrip[] } | null,
  focus: null as EntityFocus | null,
});

// ---------------------------------------------------------------------
// documents — registered once per module per document version
// ---------------------------------------------------------------------

function active(): Network | null {
  return getAllPlans()[wb.activeKey] ?? null;
}

function baseline(): Network | null {
  return getAllPlans()[wb.compareKey] ?? null;
}

function fail(e: unknown): void {
  wb.status = `Error: ${(e as Error)?.message ?? e}`;
  console.error(e);
}

/// One host step at a time: ops run on this thread, so the status
/// paints a frame before a real-size feed's work starts.
async function step<T>(label: string, run: () => Promise<T>): Promise<T | undefined> {
  wb.busy = true;
  wb.status = label;
  await new Promise((r) => setTimeout(r, 20));
  try {
    return await run();
  } catch (e) {
    fail(e);
    return undefined;
  } finally {
    wb.busy = false;
  }
}

/// The plan list reads each document's `plan` frame, which every
/// stored shape spells alike — listing a plan never reads it.
function syncPlanRows(): void {
  wb.plans = Object.entries(getAllPlans()).map(([key, d]) => ({
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

// ---------------------------------------------------------------------
// TRANSIENT — this UI's answer graph (src/uis/kit.ts, "the answer
// graph"): the first draft of its flow map for the engine
// docs/units/host-discipline.md designs. Each node is one frame's
// answer and the inputs it reads; the event handlers below only set
// an input and request it. Declared in paint order: what the screen
// shows first answers first, the baseline's answers next, and what
// is folded away or off the first screen last.
//
// Inputs: "active" / "compare" (which documents), "plans" (the plan
// set), "type" / "route" / "direction" / "block" (the selection),
// "fleet" (the fleet panel open), "layer" / "coverage-params".
// ---------------------------------------------------------------------

const request = answerGraph(
  [
    {
      id: "document.active",
      frame: "",
      needs: ["active"],
      run: async () => {
        await readPlan(wb.activeKey, readStoredNetwork);
      },
    },
    {
      id: "analysis.service_groups",
      frame: "TypeTable",
      needs: ["document.active"],
      run: async (touch) => {
        const [doc, analysis] = [active(), await analysisModule()];
        if (!doc) return;
        if (!analysis) throw new Error("analysis capability unavailable");
        const cap = analysisCap(analysis);
        const hash = hashIn(analysis, doc);
        wb.types = answer(analysis, "service_types_overview", [hash], () => cap.serviceTypesOverview(hash));
        wb.groups = answer(analysis, "service_groups", [hash], () => cap.serviceGroups(hash));
        // Selection keys on the group's BASE (a rider-along type never
        // heads a row); it persists across answers when it still does.
        if (!wb.groups.some((g) => g.service_type === wb.selectedType)) {
          wb.selectedType = wb.groups[0]?.service_type ?? "";
          touch("type");
        }
      },
    },
    {
      // A type's routes — and its representative day, the date the
      // fleet answers on (the type's own date, never a calendar
      // nobody picked).
      id: "analysis.service_type_detail",
      frame: "RouteTable",
      needs: ["document.active", "type"],
      run: async () => {
        const [doc, analysis] = [active(), await analysisModule()];
        if (!doc || !analysis || !wb.selectedType) {
          wb.typeDetail = null;
          return;
        }
        const hash = hashIn(analysis, doc);
        const type = wb.selectedType;
        wb.typeDetail = answer(analysis, "service_type_detail", [hash, type], () =>
          analysisCap(analysis).serviceTypeDetail(hash, type),
        );
      },
    },
    {
      id: "directory.route_detail",
      frame: "DirectionTabs",
      needs: ["document.active", "route"],
      run: async (touch) => {
        const [doc, directory] = [active(), await directoryModule()];
        if (!doc || !directory || !wb.selectedRoute) {
          wb.routeDetail = null;
          return;
        }
        const hash = hashIn(directory, doc);
        const route = wb.selectedRoute;
        wb.routeDetail = answer(directory, "route_detail", [hash, route], () => directoryCap(directory).routeDetail(hash, route));
        const directions = wb.routeDetail?.directions ?? [];
        if (!directions.some((d) => d.direction === wb.selectedDirection)) {
          wb.selectedDirection = directions[0]?.direction ?? null;
          touch("direction");
        }
      },
    },
    {
      // The map follows the most specific selection: a block, else a
      // route — cap-directory answers where it is.
      id: "directory.focus",
      frame: "FocusMap",
      needs: ["document.active", "route", "block"],
      run: async () => {
        const [doc, directory] = [active(), await directoryModule()];
        const target: [EntityKind, string] | null = wb.selectedBlock
          ? ["block", wb.selectedBlock]
          : wb.selectedRoute
            ? ["route", wb.selectedRoute]
            : null;
        if (!doc || !directory || !target) {
          wb.focus = null;
          return;
        }
        const hash = hashIn(directory, doc);
        wb.focus = answer(directory, "focus", [hash, ...target], () => directoryCap(directory).focus(hash, ...target));
      },
    },
    {
      id: "document.compare",
      frame: "",
      needs: ["compare"],
      run: async () => {
        if (wb.compareKey) await readPlan(wb.compareKey, readStoredNetwork);
      },
    },
    {
      id: "analysis.baseline_groups",
      frame: "TypeTable",
      needs: ["document.compare"],
      run: async () => {
        const [doc, analysis] = [baseline(), await analysisModule()];
        if (!doc || !analysis) {
          wb.compareTypes = [];
          wb.compareGroups = [];
          return;
        }
        const cap = analysisCap(analysis);
        const hash = hashIn(analysis, doc);
        wb.compareTypes = answer(analysis, "service_types_overview", [hash], () => cap.serviceTypesOverview(hash));
        wb.compareGroups = answer(analysis, "service_groups", [hash], () => cap.serviceGroups(hash));
      },
    },
    {
      // The selected direction's timetable under the selected type,
      // marked against the baseline plan when there is one.
      id: "directory.timetable",
      frame: "TimetableGrid",
      needs: ["document.active", "document.compare", "route", "direction", "type"],
      run: async () => {
        const [doc, base, directory] = [active(), baseline(), await directoryModule()];
        if (!doc || !directory || !wb.selectedRoute || !wb.selectedType) {
          wb.timetable = null;
          return;
        }
        const dir = directoryCap(directory);
        const args = [
          hashIn(directory, doc),
          wb.selectedRoute,
          wb.selectedDirection,
          wb.selectedType,
          base ? hashIn(directory, base) : null,
        ] as const;
        wb.timetable = answer(directory, "timetable", [...args], () => dir.timetable(...args));
      },
    },
    {
      // What the active plan changes against the baseline under the
      // selected type (none when either side lacks the type).
      id: "analysis.service_type_compare",
      frame: "CompareTable",
      needs: ["document.active", "document.compare", "type"],
      run: async () => {
        const [doc, base, analysis] = [active(), baseline(), await analysisModule()];
        wb.comparison = null;
        if (!doc || !base || !analysis || !wb.selectedType) return;
        const args = [hashIn(analysis, base), hashIn(analysis, doc), wb.selectedType] as const;
        try {
          wb.comparison = answer(analysis, "service_type_compare", [...args], () =>
            analysisCap(analysis).serviceTypeCompare(...args),
          );
        } catch {
          wb.comparison = null;
        }
      },
    },
    {
      // The type's fleet on its representative day — answered only
      // while the fleet panel is open (registering the document in
      // cap-runcut is the cost).
      id: "runcut.blocks",
      frame: "BlocksPanel",
      needs: ["document.active", "analysis.service_type_detail", "fleet"],
      visible: () => wb.fleetOpen,
      hide: () => {
        wb.blocks = null;
      },
      run: async (touch) => {
        const [doc, runcut] = [active(), await runcutModule()];
        const date = wb.typeDetail?.date;
        if (!doc || !runcut || !date) {
          wb.blocks = [];
        } else {
          const hash = hashIn(runcut, doc);
          wb.blocks = answer(runcut, "blocks", [hash, null, date], () => runcutCap(runcut).blocks(hash, null, date));
        }
        if (wb.selectedBlock && !wb.blocks.some((b) => b.block_id === wb.selectedBlock)) {
          wb.selectedBlock = "";
          touch("block");
        }
      },
    },
    {
      id: "runcut.block_trips",
      frame: "BlocksPanel",
      needs: ["document.active", "analysis.service_type_detail", "runcut.blocks", "block", "fleet"],
      visible: () => wb.fleetOpen,
      hide: () => {
        wb.blockChain = null;
      },
      run: async () => {
        const [doc, runcut] = [active(), await runcutModule()];
        const date = wb.typeDetail?.date;
        const block = wb.selectedBlock;
        if (!doc || !runcut || !date || !block) {
          wb.blockChain = null;
          return;
        }
        const hash = hashIn(runcut, doc);
        wb.blockChain = {
          trips: answer(runcut, "block_trips", [hash, null, block, date], () => runcutCap(runcut).blockTrips(hash, null, block, date)),
        };
      },
    },
    {
      id: "analysis.type_coverage",
      frame: "CoverageTable",
      needs: ["document.active", "layer", "coverage-params"],
      run: async () => {
        const [doc, analysis] = [active(), await analysisModule()];
        if (!doc || !analysis || !wb.layer) {
          wb.coverage = null;
          return;
        }
        const p = wb.coverageParams;
        const args = [hashIn(analysis, doc), wb.layer.layer_hash, p.radius_m, p.frequent_secs, p.min_departures] as const;
        wb.coverage = answer(analysis, "type_coverage", [...args], () => analysisCap(analysis).typeCoverage(...args));
      },
    },
    {
      id: "plans.chain",
      frame: "PlanBar",
      needs: ["plans"],
      run: async () => {
        const infos = wb.plans
          .filter((p) => p.id)
          .map((p) => ({ id: p.id, name: p.name, effective_from: p.effectiveFrom }));
        wb.chain = await plansChain(infos, today());
      },
    },
    {
      // The plan's health — last: registering the document in
      // cap-network is the cost, and cap-author already answered it
      // for a document it authored.
      id: "network.validate",
      frame: "PlanBar",
      needs: ["document.active"],
      run: async () => {
        const doc = active();
        if (!doc) return;
        const authored = authoredIssues.get(doc);
        if (authored) {
          wb.issues = authored;
          return;
        }
        const network = await networkModule();
        if (!network) {
          wb.issues = [];
          return;
        }
        const hash = hashIn(network, doc);
        wb.issues = answer(network, "validate", [hash], () => networkCap(network).validate(hash));
      },
    },
  ],
  fail,
);

function planStatus(): void {
  const family = wb.groups.find((g) => g.service_type === wb.selectedType);
  const riders = family && family.members.length > 1 ? ` (+${family.members.length - 1} overlays)` : "";
  wb.status = `${wb.plans.find((p) => p.key === wb.activeKey)?.name}: ${wb.groups.length} service groups${riders}.`;
}

// ---------------------------------------------------------------------
// boot and loaders
// ---------------------------------------------------------------------

/// The plan set restores as stored; a plan is read into the current
/// shape when it is first opened (`document.*` nodes) — a workspace of
/// many forks reads two of them, not all.
export async function boot(): Promise<void> {
  await restorePlans();
  syncPlanRows();
  const first = wb.plans[0]?.key ?? "";
  wb.activeKey = getAllPlans()[getActivePlanId()] ? getActivePlanId() : first;
  if (wb.activeKey) {
    wb.status = `Restored ${wb.plans.length} plan(s).`;
    await openPlan(wb.activeKey);
  } else {
    // A server started with -feed serves the artifact itself.
    const resp = await fetch("/feed").catch(() => null);
    if (resp?.ok) await loadFeedBytes(new Uint8Array(await resp.arrayBuffer()), "server feed");
  }
}

/// A feed becomes a plan: the network capability imports it into an
/// unframed document (the base every fork copies).
export async function loadFeedBytes(bytes: Uint8Array, label: string): Promise<void> {
  const key = await step(`Importing ${label}…`, async () => {
    const [gtfs, network] = [await gtfsModule(), await networkModule()];
    if (!network) throw new Error("network capability unavailable — import needs it");
    const info = gtfs ? gtfsCap(gtfs).feedInfo(gtfsCap(gtfs).loadFeed(bytes).feedHash) : null;
    const cap = networkCap(network);
    const { network: doc, networkHash } = cap.import(cap.loadFeed(bytes).feedHash);
    rememberHash(network, doc, networkHash);
    const key = `plan-${crypto.randomUUID()}`;
    putPlan(key, doc);
    wb.feed = { label, info };
    syncPlanRows();
    return key;
  });
  if (key) await openPlan(key);
}

export async function loadLayerBytes(bytes: Uint8Array): Promise<void> {
  await step("Registering the population layer…", async () => {
    wb.layer = await ensureAnalysisLayer(bytes);
    await request("layer");
    wb.status = wb.layer ? `Coverage against ${wb.layer.total_pop.toLocaleString()} people.` : "Analysis capability unavailable — coverage is off.";
  });
}

// ---------------------------------------------------------------------
// the plan set and the chain
// ---------------------------------------------------------------------

export async function openPlan(key: string): Promise<void> {
  if (!getAllPlans()[key]) return;
  wb.activeKey = key;
  setActivePlan(key);
  if (!wb.compareKey || wb.compareKey === key) {
    wb.compareKey = wb.plans.find((p) => p.key !== key)?.key ?? "";
  }
  wb.warnings = [];
  await step("Answering the plan…", () => request("active", "compare", "plans"));
  planStatus();
}

export async function setCompare(key: string): Promise<void> {
  wb.compareKey = key;
  await request("compare");
}

/// The service-change workflow: the fork is the active document under
/// a new plan frame (a set_plan edit; the host mints the identity).
export async function fork(name: string, effectiveFrom: string): Promise<void> {
  const doc = active();
  if (!doc || !name.trim() || !effectiveFrom) {
    wb.status = "A fork needs a source plan, a name, and an effective date.";
    return;
  }
  const from = wb.activeKey;
  const applied = await apply(doc, [
    { edit: "set_plan", plan: { id: crypto.randomUUID(), name: name.trim(), effective_from: effectiveFrom } },
  ]);
  if (!applied) return;
  const key = `plan-${applied.network.plan.id}`;
  authoredIssues.set(applied.network, applied.issues);
  putPlan(key, applied.network);
  syncPlanRows();
  wb.compareKey = from;
  await openPlan(key);
  wb.warnings = applied.warnings;
  wb.status = `Forked "${name.trim()}" from ${wb.plans.find((p) => p.key === from)?.name}.`;
}

// ---------------------------------------------------------------------
// the selection — each handler sets its input and requests it
// ---------------------------------------------------------------------

export function setCoverageParams(params: CoverageParams): Promise<void> {
  wb.coverageParams = params;
  return request("coverage-params");
}

export function selectType(serviceType: string): Promise<void> {
  wb.selectedType = serviceType;
  return request("type");
}

export function selectRoute(routeId: string): Promise<void> {
  if (wb.selectedRoute !== routeId) wb.selectedStop = "";
  wb.selectedRoute = routeId;
  return request("route");
}

export function selectDirection(direction: string | null): Promise<void> {
  wb.selectedDirection = direction;
  wb.selectedStop = "";
  return request("direction");
}

export function selectStop(stopId: string): void {
  wb.selectedStop = wb.selectedStop === stopId ? "" : stopId;
}

export function selectBlock(blockId: string): Promise<void> {
  wb.selectedBlock = blockId;
  return request("block");
}

/// The fleet panel opened or closed: open answers the type's blocks;
/// closed drops the block selection, so the map returns to the route.
export function setFleetOpen(open: boolean): Promise<void> {
  if (wb.fleetOpen === open) return Promise.resolve();
  wb.fleetOpen = open;
  if (!open) wb.selectedBlock = "";
  return request("fleet", "block");
}

// ---------------------------------------------------------------------
// changes — NetworkEdits through cap-author, the answer adopted
// ---------------------------------------------------------------------

async function apply(doc: Network, edits: NetworkEdit[]) {
  const module = await authorModule();
  if (!module) {
    wb.status = "Authoring capability unavailable — changes are off.";
    return null;
  }
  const cap = authorCap(module);
  try {
    const applied = cap.apply(hashIn(module, doc), edits);
    rememberHash(module, applied.network, applied.networkHash);
    return applied;
  } catch (e) {
    wb.status = `Change refused: ${(e as Error).message}`;
    return null;
  }
}

/// Apply a change to the ACTIVE plan and re-answer everything that
/// reads it. The base import is never edited in place: changing an
/// unframed plan asks for a fork first.
export async function change(edits: NetworkEdit[]): Promise<void> {
  const doc = active();
  if (!doc) return;
  if (!doc.plan?.id) {
    wb.status = "This is the unframed import — fork it into a plan before changing it.";
    return;
  }
  const applied = await apply(doc, edits);
  if (!applied) return;
  authoredIssues.set(applied.network, applied.issues);
  putPlan(wb.activeKey, applied.network);
  await step("Answering the change…", () => request("active"));
  planStatus();
  wb.warnings = applied.warnings;
}

// ---------------------------------------------------------------------
// the timetable's gestures — stated against the selected route,
// direction, and service type (cap-author owns every rule)
// ---------------------------------------------------------------------

/// The edits omit `direction` for the patterns in none.
function direction(): string | undefined {
  return wb.selectedDirection ?? undefined;
}

function where() {
  return { route: wb.selectedRoute, direction: direction(), service_type: wb.selectedType };
}

export const gestures = {
  headway: (from_secs: number, to_secs: number, headway_secs: number) =>
    change([{ edit: "set_headway", ...where(), from_secs, to_secs, headway_secs }]),
  span: (first_secs: number | null, last_secs: number | null) =>
    change([{ edit: "set_span", ...where(), first_secs: first_secs ?? undefined, last_secs: last_secs ?? undefined }]),
  running: (from_secs: number, to_secs: number, delta_secs: number) =>
    change([{ edit: "adjust_running_time", ...where(), from_secs, to_secs, delta_secs }]),
  insertStop: (stop: string, after: string) =>
    change([{ edit: "insert_direction_stop", route: wb.selectedRoute, direction: direction(), stop, after }]),
  removeStop: (stop: string) =>
    change([{ edit: "remove_direction_stop", route: wb.selectedRoute, direction: direction(), stop }]).then(() => {
      wb.selectedStop = "";
    }),
};
