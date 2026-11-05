/// THE BUS WORKER — the page's one Worker, and the only wasm loader
/// (docs/ARCHITECTURE.md, web section, "The bus worker").
///
/// Every module a page assembles lives here, off the thread the UI
/// renders on: the manifest is validated at the seam, each module's
/// bytes are verified against its pinned hash, instantiated, and answer
/// ops posted from the page. A generated UI's page runs ONE module —
/// its linked assembly (the manifest's `assemblies`, D-77): every
/// capability it uses over one kernel registry, so a document registers
/// once per page. A capability outside it (or every one, when the
/// assembly does not load) runs in its own module.
///
/// It answers nothing of its own: the engine (src/engine.ts, through
/// src/bus.ts) is the one caller, and decides what every call means.
/// What lives here is the runtime: instances, and the memory ceiling —
/// wasm linear memory never shrinks, so an instance whose memory
/// passed the ceiling is dropped and instantiated fresh before its
/// next op, and the engine registers the documents it needs again
/// (modules are pure, so the fresh instance answers the same bytes).

/// <reference lib="webworker" />

import type { CapManifest } from "./types/cap_abi/CapManifest";
import type { Capability } from "./types/cap_abi/Capability";
import type { LinkedAssembly } from "./types/cap_abi/LinkedAssembly";
import type { BusRequest, BusReply, CapStatus, Route } from "./bus";

/// The flat ABI every capability module exports: four functions, no
/// imports (a capability differs in which ops it answers, never in
/// how it is called).
type CapExports = {
  memory: WebAssembly.Memory;
  transit_alloc(len: number): number;
  transit_free(ptr: number, len: number): void;
  transit_invoke(ptr: number, len: number): number;
  transit_invoke_bytes(op: number, ptr: number, len: number): number;
};

const KERNEL_VERSION = 1;

/// One live instance of a verified module.
class Instance {
  readonly #exports: CapExports;
  /// Ops answered since instantiation (registrations and refusals
  /// excluded): a fresh instance always answers one op before it may
  /// be reset — its first op may be refused for the documents the reset
  /// dropped, and registering them must not reset it again — so a
  /// ceiling below its working set cannot loop.
  ops = 0;

  constructor(exports: WebAssembly.Exports) {
    this.#exports = exports as unknown as CapExports;
  }

  get memory(): number {
    return this.#exports.memory.buffer.byteLength;
  }

  text(text: string): string {
    return this.#call(new TextEncoder().encode(text), (ptr, len) => this.#exports.transit_invoke(ptr, len));
  }

  bytes(op: number, payload: Uint8Array): string {
    return this.#call(payload, (ptr, len) => this.#exports.transit_invoke_bytes(op, ptr, len));
  }

  #call(payload: Uint8Array, enter: (ptr: number, len: number) => number): string {
    const wasm = this.#exports;
    const ptr = wasm.transit_alloc(payload.length);
    new Uint8Array(wasm.memory.buffer, ptr, payload.length).set(payload);
    const outPtr = enter(ptr, payload.length);
    wasm.transit_free(ptr, payload.length);
    const len = new DataView(wasm.memory.buffer).getUint32(outPtr, true);
    const text = new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, outPtr + 4, len));
    wasm.transit_free(outPtr, len + 4);
    return text;
  }
}

/// One assembled module (a linked assembly or a capability alone): its
/// verified, compiled bytes (kept, so a reset re-instantiates without
/// fetching) and its live instance.
type Assembled = { key: string; module: WebAssembly.Module; instance: Instance; resets: number; resetAt: number };

/// The memory ceiling per instance, and the least time between two
/// resets of one module (a working set above the ceiling must not
/// thrash). The defaults sit far above one agency's working set and
/// far below the 4 GB address space; the engine may lower them (the
/// e2e forces a reset).
let ceiling = 256 * 1024 * 1024;
let minResetMs = 60_000;

let base = "";
let manifest: Promise<CapManifest> | null = null;
/// Modules by key ("ui:<id>" for a linked assembly, "cap:<id>" alone).
const assembled = new Map<string, Promise<Assembled | null>>();
/// Where each assembled capability answers.
const routes = new Map<string, Route>();
const errors: Record<string, string> = {};

/// sha256 of bytes as "sha256:<hex>" — the manifest's format, the
/// same formula the generator and the Rust side use.
async function contentHash(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const hex = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
  return "sha256:" + hex;
}

/// Load the manifest and validate it AT THE SEAM: wrong kernel, no
/// content hash, or duplicate ids are assembly errors, not runtime
/// surprises.
async function loadManifest(): Promise<CapManifest> {
  const loaded = (await (await fetch(new URL("caps/manifest.json", base))).json()) as CapManifest;
  if (loaded.kernel !== KERNEL_VERSION) {
    throw new Error(`manifest kernel v${loaded.kernel} != host kernel v${KERNEL_VERSION}`);
  }
  const ids = new Set<string>();
  for (const cap of loaded.capabilities) {
    if (ids.has(cap.id)) throw new Error(`duplicate capability id ${cap.id}`);
    if (!cap.hash.startsWith("sha256:")) throw new Error(`capability ${cap.id} has no content hash`);
    ids.add(cap.id);
  }
  return loaded;
}

/// One module, demand-driven and memoized: fetched by manifest url,
/// VERIFIED against the pinned hash, compiled and instantiated. A
/// failure resolves null — the assembly degrades, never fails
/// wholesale.
function assemble(key: string, entry: { url: string; hash: string }): Promise<Assembled | null> {
  let pending = assembled.get(key);
  if (!pending) {
    pending = (async () => {
      try {
        const bytes = new Uint8Array(await (await fetch(new URL(entry.url, base))).arrayBuffer());
        // Integrity at the seam: the manifest pins the exact bytes.
        const hash = await contentHash(bytes);
        if (hash !== entry.hash) {
          throw new Error(`module ${key} hash mismatch: fetched ${hash}, manifest pins ${entry.hash}`);
        }
        const module = await WebAssembly.compile(bytes);
        const instance = new Instance(await WebAssembly.instantiate(module, {}).then((i) => i.exports));
        return { key, module, instance, resets: 0, resetAt: 0 };
      } catch (error) {
        errors[key] = error instanceof Error ? error.message : String(error);
        return null;
      }
    })();
    assembled.set(key, pending);
  }
  return pending;
}

/// The module a capability answers in: the page's linked assembly when
/// it links the capability and loads, the capability alone otherwise.
async function place(cap: Capability, linked: LinkedAssembly | undefined): Promise<Route | null> {
  if (linked?.capabilities.includes(cap.id)) {
    const key = `ui:${linked.ui}`;
    if (await assemble(key, linked)) {
      return { module: key, hash: linked.hash, index: linked.capabilities.indexOf(cap.id), linked: linked.capabilities.length > 1 };
    }
  }
  const key = `cap:${cap.id}`;
  return (await assemble(key, cap)) ? { module: key, hash: cap.hash, index: 0, linked: false } : null;
}

/// The module instance a capability answers in.
async function assembledOf(cap: string): Promise<Assembled> {
  const route = routes.get(cap);
  const got = route && (await assembled.get(route.module));
  if (!got) throw new Error(`capability ${cap} is not assembled`);
  return got;
}

/// Before an op (never a registration): an instance past the ceiling
/// that has answered since it was made, and was not reset too
/// recently, is dropped for a fresh one. True when it was.
async function reclaim(a: Assembled): Promise<boolean> {
  const now = Date.now();
  if (a.instance.memory <= ceiling || a.instance.ops === 0 || now - a.resetAt < minResetMs) return false;
  a.instance = new Instance((await WebAssembly.instantiate(a.module, {})).exports);
  a.resets += 1;
  a.resetAt = now;
  return true;
}

async function handle(request: BusRequest): Promise<BusReply> {
  switch (request.type) {
    case "init":
      base = request.base;
      return { ok: null };
    case "ensure": {
      manifest ??= loadManifest();
      const loaded = await manifest;
      const byId = new Map(loaded.capabilities.map((c) => [c.id, c]));
      const linked = request.ui ? loaded.assemblies?.find((a) => a.ui === request.ui) : undefined;
      const statuses: Record<string, CapStatus> = {};
      const caps: Record<string, Capability> = {};
      const placed: Record<string, Route> = {};
      for (const id of request.caps) {
        const cap = byId.get(id);
        if (!cap) throw new Error(`manifest has no capability ${id}`);
        const route = routes.get(id) ?? (await place(cap, linked));
        statuses[id] = route ? "ready" : "unavailable";
        if (route) {
          routes.set(id, route);
          caps[id] = cap;
          placed[id] = route;
        }
      }
      return { ok: { statuses, caps, routes: placed, errors: { ...errors } } };
    }
    case "text": {
      const a = await assembledOf(request.cap);
      const reset = !request.load && (await reclaim(a));
      const out = a.instance.text(request.text);
      if (!request.load && out.startsWith('{"ok"')) a.instance.ops += 1;
      return { ok: out, reset };
    }
    case "bytes": {
      const a = await assembledOf(request.cap);
      const reset = !request.load && (await reclaim(a));
      const out = a.instance.bytes(request.op, request.bytes);
      if (!request.load && out.startsWith('{"ok"')) a.instance.ops += 1;
      return { ok: out, reset };
    }
    case "inspect": {
      const a = await assembledOf(request.cap);
      return { ok: { memory: a.instance.memory, resets: a.resets } };
    }
    case "ceiling":
      ceiling = request.bytes;
      minResetMs = request.minResetMs;
      return { ok: null };
  }
}

self.onmessage = (event: MessageEvent<{ id: number; request: BusRequest }>) => {
  const { id, request } = event.data;
  handle(request).then(
    (reply) => self.postMessage({ id, reply }),
    (error: unknown) => self.postMessage({ id, reply: { error: error instanceof Error ? error.message : String(error) } }),
  );
};
