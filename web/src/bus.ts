// Only the engine (src/engine.ts) imports this file.

import type { Capability } from "./types/cap_abi/Capability";

export type CapStatus = "unavailable" | "ready";

export type BusRequest =
  | { type: "init"; base: string }
  | { type: "ensure"; caps: string[]; ui: string | null }
  // `load` marks a registration, which is never preceded by a reset.
  | { type: "text"; cap: string; text: string; load: boolean }
  // Kernel ops start at 256.
  | { type: "bytes"; cap: string; op: number; bytes: Uint8Array; load: boolean }
  | { type: "inspect"; cap: string }
  | { type: "ceiling"; bytes: number; minResetMs: number }
  | { type: "journal"; run: string }
  | { type: "journal-read" };

// `reset` says the instance was replaced before it answered, so it holds
// no documents.
export type BusReply = { ok: unknown; reset?: boolean } | { error: string };

// A capability's bulk op carries `index` in its high 16 bits. A JSON
// request names its capability when the module is `linked`.
export type Route = { module: string; hash: string; index: number; linked: boolean };

export type Assembly = {
  statuses: Record<string, CapStatus>;
  caps: Record<string, Capability>;
  routes: Record<string, Route>;
  errors: Record<string, string>;
  loads: Record<string, number>;
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

export async function call(request: BusRequest): Promise<{ ok: unknown; reset: boolean }> {
  bus();
  const reply = await post(request);
  if ("error" in reply) throw new Error(reply.error);
  return { ok: reply.ok, reset: reply.reset ?? false };
}
