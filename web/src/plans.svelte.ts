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

/** Set when the stored set is one this build does not read (another
 * kind, or a newer version): the page then never writes over it. */
let storeRefused = false;

/** Persist the plan set — serialized writes, one pending at a time. */
function persistPlans(): void {
  if (storeRefused) return;
  const set: PlanSet = { kind: "plan_set", schema_version: PLAN_SET_VERSION, plans };
  plansWrite = plansWrite
    .then(() => idbSet(PLANS_KEY, set))
    .catch((e) => console.warn("plan persistence failed:", e));
}

/** The plans a stored set holds: the tagged document, or a bare map of
 * plans by store id (the shape written before the set had its tag). A
 * set of another kind or a newer version is not read, and nothing is
 * written over it. */
function storedPlans(stored: unknown): Record<string, unknown> | null {
  if (!stored || typeof stored !== "object") return null;
  const tagged = stored as { kind?: unknown; schema_version?: unknown; plans?: unknown };
  if (tagged.kind === undefined) return stored as Record<string, unknown>;
  if (tagged.kind !== "plan_set" || (tagged.schema_version ?? 1) !== PLAN_SET_VERSION || !tagged.plans || typeof tagged.plans !== "object") {
    storeRefused = true;
    throw new Error(`the stored plans are ${String(tagged.kind)} v${String(tagged.schema_version)}, and this build reads plan_set v${PLAN_SET_VERSION}`);
  }
  return tagged.plans as Record<string, unknown>;
}

/** Restore the plan set on startup (documents cross IN from
 * IndexedDB; JS owns them, so a shape guard keeps stale builds from
 * wedging the workspace). `read` canonicalizes each stored document
 * through a module (the host layer passes its
 * `readStoredNetwork`): a plan saved under older field names comes
 * back in the current shape and is persisted that way. A plan the
 * module refuses is kept as stored and reported, never dropped. */
export async function restorePlans(read?: (stored: unknown) => Promise<Network | null>): Promise<void> {
  try {
    const stored = storedPlans(await idbGet(PLANS_KEY));
    if (stored) {
      const next: Record<string, Network> = {};
      for (const [key, doc] of Object.entries(stored)) {
        try {
          next[key] = (read ? await read(doc) : null) ?? (doc as Network);
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
 * on-demand half of a bare `restorePlans()`: a host that opens few of
 * many plans reads only those (each read is a whole-document round
 * trip through a module). The canonical copy replaces the stored one
 * in memory and is written with the plan set's next persist. A plan
 * the module refuses stays as stored and is reported. Plan metadata
 * (`plan`) reads the same in every stored shape, so a host may list
 * plans before reading them. */
export async function readPlan(id: string, read: (stored: unknown) => Promise<Network | null>): Promise<Network | null> {
  const doc = plans[id];
  if (!doc || readPlans.has(id)) return doc ?? null;
  try {
    const next = await read(doc);
    if (next && plans[id] === doc) plans = { ...plans, [id]: next };
  } catch (e) {
    console.warn(`stored plan ${id} could not be read:`, e);
  }
  readPlans.add(id);
  return plans[id];
}

