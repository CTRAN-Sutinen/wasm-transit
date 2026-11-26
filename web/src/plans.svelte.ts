// The plan set is keyed by a store id. The sync identity inside a document
// is `plan.id`, which is empty on unversioned imports. IndexedDB holds the
// set as one `PlanSet` document, so plans survive reloads.

import { idbGet, idbSet } from "./idb";
import { keepRecord } from "./documents.svelte";
import type { Network } from "./types/Network";
import type { PlanSet } from "./types/PlanSet";

let plans = $state.raw<Record<string, Network>>({});
let activePlanId = $state.raw<string>("");

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

/** The active plan id is the host's to move. */
export function removePlan(id: string): void {
  if (!(id in plans)) return;
  const { [id]: _, ...rest } = plans;
  plans = rest;
  readPlans.delete(id);
  void persistPlans();
}

export function setActivePlan(id: string): void {
  activePlanId = id;
  localStorage.setItem(ACTIVE_PLAN_KEY, id);
}


const PLANS_KEY = "transit:plans";
const ACTIVE_PLAN_KEY = "transit:active-plan";
let plansWrite: Promise<unknown> = Promise.resolve();
const readPlans = new Set<string>();

/** Matches transit-network `PLAN_SET_VERSION`. */
const PLAN_SET_VERSION = 1;

export type ReadPlan = { network: Network; migrated: boolean };

let storedSet: unknown = undefined;
let storedSetKept = false;

async function keepStoredSet(): Promise<void> {
  if (storedSetKept || storedSet === undefined) return;
  storedSetKept = true;
  try {
    await keepRecord("plan_set", JSON.stringify(storedSet), PLAN_SET_VERSION);
  } catch (e) {
    console.warn("the plan set from before the migration was not kept:", e);
  }
}

/** Set when the stored set is of another kind or a newer version. The page
 * then never writes over it. */
let storeRefused = false;

function persistPlans(): void {
  if (storeRefused) return;
  const set: PlanSet = { kind: "plan_set", schema_version: PLAN_SET_VERSION, plans };
  plansWrite = plansWrite
    .then(() => idbSet(PLANS_KEY, set))
    .catch((e) => console.warn("plan persistence failed:", e));
}

/** `readSet` reads the stored set through network `stored_plans`. `read`
 * canonicalizes each stored plan, so a plan saved under older field names
 * is persisted in the current shape. When a migration brought a plan
 * forward, the set as stored is kept first in the `plan_set` document
 * record. A plan that the module refuses is kept as stored. */
export async function restorePlans(
  readSet: (stored: unknown) => Promise<Record<string, unknown>>,
  read?: (stored: unknown) => Promise<ReadPlan | null>,
): Promise<void> {
  try {
    const raw = await idbGet(PLANS_KEY);
    storedSet = raw;
    let stored: Record<string, unknown> | null = null;
    if (raw !== undefined) {
      try {
        stored = await readSet(raw);
      } catch (e) {
        storeRefused = true;
        throw e;
      }
    }
    if (stored) {
      const next: Record<string, Network> = {};
      for (const [key, doc] of Object.entries(stored)) {
        try {
          const answer = read ? await read(doc) : null;
          if (answer?.migrated) await keepStoredSet();
          next[key] = answer?.network ?? (doc as Network);
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

/** The on-demand half of `restorePlans` without `read`. Each read is a
 * whole-document round trip through a module, so a host that opens few of
 * many plans reads only those. Plan metadata (`plan`) reads the same in
 * every stored shape, so a host may list plans before reading them. */
export async function readPlan(id: string, read: (stored: unknown) => Promise<ReadPlan | null>): Promise<Network | null> {
  const doc = plans[id];
  if (!doc || readPlans.has(id)) return doc ?? null;
  try {
    const answer = await read(doc);
    if (answer?.migrated) await keepStoredSet();
    if (answer && plans[id] === doc) plans = { ...plans, [id]: answer.network };
  } catch (e) {
    console.warn(`stored plan ${id} could not be read:`, e);
  }
  readPlans.add(id);
  return plans[id];
}

