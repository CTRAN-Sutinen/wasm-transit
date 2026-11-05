/// The page's side of the bus worker (src/bus.worker.ts): the one
/// Worker, spawned on first use, and its calls as promises. Only the
/// engine (src/engine.ts) imports this file; it carries messages and
/// decides nothing.

import type { Capability } from "./types/cap_abi/Capability";

export type CapStatus = "unavailable" | "ready";

/// A call to the worker.
export type BusRequest =
  | { type: "init"; base: string }
  /// Assemble capabilities: inside the page's linked assembly when `ui`
  /// names one that links them (the manifest's `assemblies`), each in
  /// its own module otherwise.
  | { type: "ensure"; caps: string[]; ui: string | null }
  /// A JSON op; `load` marks a registration (never preceded by a reset).
  | { type: "text"; cap: string; text: string; load: boolean }
  /// A bulk op over bytes (the module's own op, or a kernel op from 256).
  | { type: "bytes"; cap: string; op: number; bytes: Uint8Array; load: boolean }
  | { type: "inspect"; cap: string }
  | { type: "ceiling"; bytes: number; minResetMs: number };

/// The worker's answer: a value, or the error the call threw. An op's
/// value is the module's outcome text; `reset` says the instance was
/// replaced before it answered (it holds no documents now).
export type BusReply = { ok: unknown; reset?: boolean } | { error: string };

/// Where a capability answers: the module instance it lives in (its
/// key, and the module's pinned hash), its index there (a capability
/// bulk op carries it in the high 16 bits), and whether the module
/// links several (a JSON request then names its capability).
export type Route = { module: string; hash: string; index: number; linked: boolean };

/// What `ensure` answers: per capability, whether it assembled, its
/// manifest entry and its route when it did, and why it did not.
export type Assembly = {
  statuses: Record<string, CapStatus>;
  caps: Record<string, Capability>;
  routes: Record<string, Route>;
  errors: Record<string, string>;
};

let worker: Worker | null = null;
let next = 0;
const pending = new Map<number, (reply: BusReply) => void>();

function bus(): Worker {
  if (!worker) {
    worker = new Worker(new URL("./bus.worker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (event: MessageEvent<{ id: number; reply: BusReply }>) => {
      const resolve = pending.get(event.data.id);
      pending.delete(event.data.id);
      resolve?.(event.data.reply);
    };
    // Asset urls resolve against the page, not the worker's script.
    post({ type: "init", base: document.baseURI });
  }
  return worker;
}

function post(request: BusRequest): Promise<BusReply> {
  const id = next++;
  return new Promise((resolve) => {
    pending.set(id, resolve);
    (worker ?? bus()).postMessage({ id, request });
  });
}

/// One call: its value, or the worker's error thrown.
export async function call(request: BusRequest): Promise<{ ok: unknown; reset: boolean }> {
  bus();
  const reply = await post(request);
  if ("error" in reply) throw new Error(reply.error);
  return { ok: reply.ok, reset: reply.reset ?? false };
}
