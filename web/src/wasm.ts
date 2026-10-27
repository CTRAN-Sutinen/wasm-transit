/// The browser host: a thin wrapper over the wasm artifact's flat C
/// ABI (see crates/transit-wasm). The artifact has zero imports, so
/// instantiation is trivial — no wasm-bindgen glue anywhere. The
/// payload types are GENERATED from the Rust model (src/types, via
/// ts-rs) — the TS contract is the artifact's contract, never
/// hand-written.
///
/// Protocol notes (crates/transit-wasm/src/lib.rs has the full
/// contract): requests are JSON, results are
/// `[u32 LE length][payload]` buffers that the host reads then frees;
/// outcomes are `{"ok": ...}` or `{"error": ...}`.

import wasmUrl from "../pkg/transit_wasm.wasm?url";
import type { Network } from "./types/Network";
import type { PlanInfo } from "./types/PlanInfo";
import type { PlanIssue } from "./types/PlanIssue";
import type { PlanPeriod } from "./types/PlanPeriod";


/// The flat C ABI the artifact exports.
type WasmExports = {
  memory: WebAssembly.Memory;
  transit_alloc(len: number): number;
  transit_free(ptr: number, len: number): void;
  transit_invoke(ptr: number, len: number): number;
  transit_invoke_bytes(op: number, ptr: number, len: number): number;
};

let instance: WebAssembly.Instance | null = null;

function exports(): WasmExports {
  return instance?.exports as unknown as WasmExports;
}

/// Instantiate the artifact once (fetch is JS-owned I/O, per the
/// project's boundary rules).
async function instantiate(): Promise<WebAssembly.Instance> {
  if (instance) return instance;
  const bytes = await (await fetch(wasmUrl)).arrayBuffer();
  // Zero imports: every host is this easy.
  const { instance: inst } = await WebAssembly.instantiate(bytes, {});
  instance = inst;
  return inst;
}

type Ok = { ok: unknown };

function readResult(outPtr: number): unknown {
  const wasm = exports();
  const buffer = wasm.memory.buffer;
  const len = new DataView(buffer).getUint32(outPtr, true);
  const payload = new Uint8Array(buffer, outPtr + 4, len);
  const outcome = JSON.parse(new TextDecoder().decode(payload)) as
    | Ok
    | { error: string };
  wasm.transit_free(outPtr, len + 4);
  if ("error" in outcome) throw new Error(outcome.error);
  return outcome.ok;
}

function invoke(request: unknown): unknown {
  const wasm = exports();
  const payload = new TextEncoder().encode(JSON.stringify(request));
  const ptr = wasm.transit_alloc(payload.length);
  new Uint8Array(wasm.memory.buffer, ptr, payload.length).set(payload);
  const outPtr = wasm.transit_invoke(ptr, payload.length);
  wasm.transit_free(ptr, payload.length);
  return readResult(outPtr);
}


// ---------------------------------------------------------------
// Street networks (detour authoring) live in the cap-streets
// capability now — see capabilities.svelte.ts (streetsCap).
// ---------------------------------------------------------------

/// Which service plan is in force on a date (the latest pick at or
/// before it; unversioned plans never resolve).
export function plans_resolve(plans: PlanInfo[], date: string): string | null {
  return (invoke({ op: "plans_resolve", plans, date }) as { plan: string | null }).plan;
}

/// Chain validity: unique plan ids, versioned plans effective-dated,
/// effective dates unique.
export function plans_validate(plans: PlanInfo[]): PlanIssue[] {
  return (invoke({ op: "plans_validate", plans }) as { issues: PlanIssue[] }).issues;
}

/// Every pick's governing period — [effective_from, until the next
/// pick) — derived, never stored.
export function plans_periods(plans: PlanInfo[]): PlanPeriod[] {
  return (invoke({ op: "plans_periods", plans }) as { periods: PlanPeriod[] }).periods;
}

/// The API surface `loadWasm()` hands back: the plans chain (the
/// The API surface `loadWasm()` hands back: the plans chain (the
/// artifact's remaining surface — every feed-side read answers
/// through the capability bus now).
export type WasmApi = {
  plans_resolve: typeof plans_resolve;
  plans_validate: typeof plans_validate;
  plans_periods: typeof plans_periods;
};

/// The entry point main.js awaits: instantiate the artifact, then
/// hand back the API surface the app expects.
export async function loadWasm(): Promise<WasmApi> {
  await instantiate();
  return {
    plans_resolve,
    plans_validate,
    plans_periods,
  };
}
