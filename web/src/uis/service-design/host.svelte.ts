/// The service-design UI's host layer — the ONLY file of this UI that calls the
/// capability bus. It owns I/O (feed and layer bytes), the plan set
/// (shared with every other shell through state.svelte.ts: JS owns the
/// documents), and the selection; every number, list, and line it
/// writes into `wb` is an op's answer, adopted as-is. The shell's
/// components take these as props and emit events back here.
///
/// The questions it answers, in the order they are asked (the
/// 2026-10-27 service-design dogfood session, docs/QA-FINDINGS.md): which
/// plan, what each service type costs and covers, which routes run
/// under a type, what one route is and where it runs, what a change
/// does to the type and the fleet.

import { getAllPlans, getActivePlanId, putPlan, setActivePlan, restorePlans } from "../../state.svelte";
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
import { hashIn, rememberHash, today } from "../kit";
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
import type { ServiceTypeDetail } from "../../types/ServiceTypeDetail";
import type { ServiceTypeSummary } from "../../types/ServiceTypeSummary";
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
  compareTypes: [] as ServiceTypeSummary[],
  layer: null as AnalysisLayerHandle | null,
  coverageParams: { radius_m: 400, frequent_secs: 900, min_departures: 2 } as CoverageParams,
  coverage: null as TypeCoverage[] | null,
  selectedType: "",
  typeDetail: null as ServiceTypeDetail | null,
  selectedRoute: "",
  routeDetail: null as RouteDetail | null,
  selectedSchedule: "",
  starts: null as number[] | null,
  blocks: [] as DayBlock[],
  selectedBlock: "",
  blockChain: null as { trips: BlockChainTrip[]; agencyTripIds: string[][] } | null,
  focus: null as EntityFocus | null,
});

// ---------------------------------------------------------------------
// documents — registered once per module per document version
// ---------------------------------------------------------------------

function active(): Network | null {
  return getAllPlans()[wb.activeKey] ?? null;
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

function syncPlanRows(): void {
  wb.plans = Object.entries(getAllPlans()).map(([key, d]) => ({
    key,
    id: d.plan?.id ?? "",
    name: d.plan?.name || "(unframed import)",
    effectiveFrom: d.plan?.effective_from ?? null,
  }));
}

// ---------------------------------------------------------------------
// boot and loaders
// ---------------------------------------------------------------------

export async function boot(): Promise<void> {
  await restorePlans(readStoredNetwork);
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
  await step(`Importing ${label}…`, async () => {
    const [gtfs, network] = [await gtfsModule(), await networkModule()];
    if (!network) throw new Error("network capability unavailable — import needs it");
    const info = gtfs ? gtfsCap(gtfs).feedInfo(gtfsCap(gtfs).loadFeed(bytes).feedHash) : null;
    const cap = networkCap(network);
    const { network: doc } = cap.import(cap.loadFeed(bytes).feedHash);
    const key = `plan-${crypto.randomUUID()}`;
    putPlan(key, doc);
    wb.feed = { label, info };
    syncPlanRows();
    await openPlan(key);
  });
}

export async function loadLayerBytes(bytes: Uint8Array): Promise<void> {
  await step("Registering the population layer…", async () => {
    wb.layer = await ensureAnalysisLayer(bytes);
    await refreshCoverage();
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
  await refreshPlan();
}

export async function setCompare(key: string): Promise<void> {
  wb.compareKey = key;
  await refreshCompare();
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
  putPlan(key, applied.network);
  syncPlanRows();
  wb.compareKey = from;
  await openPlan(key);
  wb.warnings = applied.warnings;
  wb.status = `Forked "${name.trim()}" from ${wb.plans.find((p) => p.key === from)?.name}.`;
}

async function refreshChain(): Promise<void> {
  const infos = wb.plans
    .filter((p) => p.id)
    .map((p) => ({ id: p.id, name: p.name, effective_from: p.effectiveFrom }));
  wb.chain = await plansChain(infos, today());
}

// ---------------------------------------------------------------------
// answers over the active plan
// ---------------------------------------------------------------------

async function refreshPlan(): Promise<void> {
  const doc = active();
  if (!doc) return;
  await step("Answering the plan…", async () => {
    const [analysis, network] = [await analysisModule(), await networkModule()];
    if (!analysis) throw new Error("analysis capability unavailable");
    wb.types = analysisCap(analysis).serviceTypesOverview(hashIn(analysis, doc));
    wb.issues = network ? networkCap(network).validate(hashIn(network, doc)) : [];
    if (!wb.types.some((t) => t.service_type === wb.selectedType)) {
      wb.selectedType = wb.types[0]?.service_type ?? "";
    }
    await Promise.all([refreshChain(), refreshCompare(), refreshType()]);
    refreshCoverage();
    wb.status = `${wb.plans.find((p) => p.key === wb.activeKey)?.name}: ${wb.types.length} service types.`;
  });
}

async function refreshCompare(): Promise<void> {
  const doc = getAllPlans()[wb.compareKey];
  const analysis = await analysisModule();
  wb.compareTypes = doc && analysis ? analysisCap(analysis).serviceTypesOverview(hashIn(analysis, doc)) : [];
}

export function setCoverageParams(params: CoverageParams): void {
  wb.coverageParams = params;
  refreshCoverage();
}

async function refreshCoverage(): Promise<void> {
  const doc = active();
  const analysis = await analysisModule();
  if (!doc || !analysis || !wb.layer) {
    wb.coverage = null;
    return;
  }
  const p = wb.coverageParams;
  wb.coverage = analysisCap(analysis).typeCoverage(
    hashIn(analysis, doc),
    wb.layer.layer_hash,
    p.radius_m,
    p.frequent_secs,
    p.min_departures,
  );
}

export async function selectType(serviceType: string): Promise<void> {
  wb.selectedType = serviceType;
  await refreshType();
}

/// A type's drill-down: its routes (analysis) and its fleet on the
/// representative day (runcut) — the type's own date, never a
/// calendar nobody picked.
async function refreshType(): Promise<void> {
  const doc = active();
  if (!doc || !wb.selectedType) return;
  const [analysis, runcut] = [await analysisModule(), await runcutModule()];
  wb.typeDetail = analysis ? analysisCap(analysis).serviceTypeDetail(hashIn(analysis, doc), wb.selectedType) : null;
  const date = wb.typeDetail?.date;
  wb.blocks = runcut && date ? runcutCap(runcut).blocks(hashIn(runcut, doc), null, date) : [];
  if (!wb.blocks.some((b) => b.block_id === wb.selectedBlock)) {
    wb.selectedBlock = "";
    wb.blockChain = null;
  } else {
    await selectBlock(wb.selectedBlock);
  }
  if (wb.selectedRoute) await selectRoute(wb.selectedRoute);
}

export async function selectRoute(routeId: string): Promise<void> {
  const doc = active();
  const directory = await directoryModule();
  if (!doc || !directory) return;
  const dir = directoryCap(directory);
  const hash = hashIn(directory, doc);
  wb.selectedRoute = routeId;
  wb.routeDetail = dir.routeDetail(hash, routeId);
  const stillThere = wb.routeDetail?.pattern_trips_by_type.some((g) => g.pattern_trips.some((s) => s.id === wb.selectedSchedule));
  if (!stillThere) wb.selectedSchedule = "";
  await refreshFocus();
  readStarts();
}

export async function selectSchedule(scheduleId: string): Promise<void> {
  wb.selectedSchedule = scheduleId;
  readStarts();
  await refreshFocus();
}

/// A schedule's departures as the document holds them (exact times
/// only — a headway schedule's starts are its bands, not a list).
function readStarts(): void {
  const s = active()?.pattern_trips[wb.selectedSchedule]?.departures;
  wb.starts = s?.kind === "exact_times" ? s.starts : null;
}

export async function selectBlock(blockId: string): Promise<void> {
  const doc = active();
  const [runcut, network] = [await runcutModule(), await networkModule()];
  const date = wb.typeDetail?.date;
  wb.selectedBlock = blockId;
  if (!doc || !runcut || !date || !blockId) {
    wb.blockChain = null;
    return;
  }
  const trips = runcutCap(runcut).blockTrips(hashIn(runcut, doc), null, blockId, date);
  const agencyTripIds = network ? networkCap(network).agencyTripIds(hashIn(network, doc), trips) : trips.map(() => []);
  wb.blockChain = { trips, agencyTripIds };
  await refreshFocus();
}

/// The map follows the most specific selection: a block, else a
/// schedule, else a route — cap-directory answers where it is.
async function refreshFocus(): Promise<void> {
  const doc = active();
  const directory = await directoryModule();
  const target: [EntityKind, string] | null = wb.selectedBlock
    ? ["block", wb.selectedBlock]
    : wb.selectedSchedule
      ? ["pattern_trips", wb.selectedSchedule]
      : wb.selectedRoute
        ? ["route", wb.selectedRoute]
        : null;
  wb.focus = doc && directory && target ? directoryCap(directory).focus(hashIn(directory, doc), ...target) : null;
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
  putPlan(wb.activeKey, applied.network);
  await refreshPlan();
  wb.warnings = applied.warnings;
  wb.issues = applied.issues;
}
