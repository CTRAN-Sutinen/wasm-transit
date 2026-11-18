/// The service plans: the authored network documents (transit-network
/// `Network`) the service-design UI keeps on this device. JS owns
/// them — plain data, wholesale replacement, never mutated in place.
/// A **service plan** is one of these documents framed by its `plan`
/// metadata (identity, name, effective date); the plan set is keyed
/// by a store id (the sync identity inside the document is `plan.id`
/// — empty on unversioned imports). wasm only ever sees a document as
/// an op argument. Persisted to IndexedDB as one plan set document
/// (transit-network `PlanSet`: its kind, its version, and the plans by
/// store id; D-115), so plans survive reloads — service changes span
/// weeks.

import { get as idbGet, set as idbSet } from "idb-keyval";
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

/** Drop a plan from the set (and from IndexedDB with the next
 * persist). The active plan id is the host's to move. */
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
/** The plans whose in-memory copy is in the current shape: put by a
 * host (a module answered it) or read back through `readPlan`. */
const readPlans = new Set<string>();

/** The plan set version this build writes (transit-network
 * `PLAN_SET_VERSION`). */
const PLAN_SET_VERSION = 1;

/** A stored plan read through a module: its document in the current
 * shape, and whether a migration brought it forward. */
export type ReadPlan = { network: Network; migrated: boolean };

/** The set as IndexedDB held it at restore, kept once in the device's
 * document store (`plan_set`) before a migrated plan is written over it. */
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

/** Set when the stored set is one this build does not read (another
 * kind, or a newer version, as the module's `stored_plans` answers):
 * the page then never writes over it. */
let storeRefused = false;

/** Persist the plan set — serialized writes, one pending at a time. */
function persistPlans(): void {
  if (storeRefused) return;
  const set: PlanSet = { kind: "plan_set", schema_version: PLAN_SET_VERSION, plans };
  plansWrite = plansWrite
    .then(() => idbSet(PLANS_KEY, set))
    .catch((e) => console.warn("plan persistence failed:", e));
}

/** Restore the plan set on startup (documents cross IN from
 * IndexedDB; JS owns them). `readSet` reads the stored set through a
 * module (the host passes network `stored_plans`): it answers the plans
 * by store id, or refuses a set of another kind or a newer version, and
 * then nothing is written over it. `read` canonicalizes each stored
 * document through a module (the host layer passes its
 * `readStoredNetwork`): a plan saved under older field names comes
 * back in the current shape and is persisted that way. When a migration
 * brought a plan forward, the set as stored is kept first, in the
 * device's document store (`plan_set`). A plan the module refuses is
 * kept as stored and reported, never dropped. */
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

/** One stored plan read into the current shape on first use — the
 * on-demand half of `restorePlans` without `read`: a host that opens few of
 * many plans reads only those (each read is a whole-document round
 * trip through a module). The canonical copy replaces the stored one
 * in memory and is written with the plan set's next persist. A plan
 * the module refuses stays as stored and is reported. Plan metadata
 * (`plan`) reads the same in every stored shape, so a host may list
 * plans before reading them. */
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

