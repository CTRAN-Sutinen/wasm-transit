/// The capability bus: the host-side resolver — the ONLY wasm loader.
///
/// Capabilities advertise (the manifest); this module assembles them
/// on demand: manifest validation at the seam, byte verification
/// against each module's pinned hash, instantiation, and a status per
/// capability, so an assembly degrades per capability and never fails
/// wholesale. It answers nothing: the engine (src/engine.ts) is the
/// one caller of a module's ops, and the only importer of this file.

import type { CapManifest } from "./types/cap_abi/CapManifest";
import type { Capability } from "./types/cap_abi/Capability";

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

export type CapStatus = "unavailable" | "loading" | "ready";

const KERNEL_VERSION = 1;

/// One loaded capability module: the bytes were fetched, verified
/// against the manifest's hash, and instantiated. Its two calls are
/// the ABI's — a JSON op, or a bulk op over bytes — and both answer
/// the outcome's `ok` or throw its `error`.
export class CapModule {
  readonly cap: Capability;
  readonly #exports: CapExports;

  constructor(cap: Capability, exports: WebAssembly.Exports) {
    this.cap = cap;
    this.#exports = exports as unknown as CapExports;
  }

  /// One JSON op, the request already serialized (the engine's
  /// journal key is these bytes, so it serializes once).
  invokeText(text: string): unknown {
    return this.#call(new TextEncoder().encode(text), (ptr, len) => this.#exports.transit_invoke(ptr, len));
  }

  /// One bulk op: the module's own op code (or a kernel op, from
  /// 256) over raw bytes.
  invokeBytes(op: number, bytes: Uint8Array): unknown {
    return this.#call(bytes, (ptr, len) => this.#exports.transit_invoke_bytes(op, ptr, len));
  }

  #call(payload: Uint8Array, enter: (ptr: number, len: number) => number): unknown {
    const wasm = this.#exports;
    const ptr = wasm.transit_alloc(payload.length);
    new Uint8Array(wasm.memory.buffer, ptr, payload.length).set(payload);
    const outPtr = enter(ptr, payload.length);
    wasm.transit_free(ptr, payload.length);
    const len = new DataView(wasm.memory.buffer).getUint32(outPtr, true);
    const text = new TextDecoder().decode(new Uint8Array(wasm.memory.buffer, outPtr + 4, len));
    wasm.transit_free(outPtr, len + 4);
    const outcome = JSON.parse(text) as { ok: unknown } | { error: string };
    if ("error" in outcome) throw new Error(outcome.error);
    return outcome.ok;
  }
}

/// sha256 of bytes as "sha256:<hex>" — the manifest's format, the
/// same formula the generator and the Rust side use.
async function contentHash(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  const hex = Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
  return "sha256:" + hex;
}

/// The assembly state; the resolver is the only writer.
export const caps = $state({
  manifest: null as CapManifest | null,
  statuses: {} as Record<string, CapStatus>,
  errors: {} as Record<string, string>,
  modules: {} as Record<string, CapModule>,
});

/// Load the manifest and validate it AT THE SEAM: wrong kernel, no
/// content hash, or duplicate ids are assembly errors, not runtime
/// surprises.
async function loadManifest(): Promise<CapManifest> {
  const manifest = (await (await fetch("caps/manifest.json")).json()) as CapManifest;
  if (manifest.kernel !== KERNEL_VERSION) {
    throw new Error(`manifest kernel v${manifest.kernel} != host kernel v${KERNEL_VERSION}`);
  }
  const ids = new Set<string>();
  for (const cap of manifest.capabilities) {
    if (ids.has(cap.id)) throw new Error(`duplicate capability id ${cap.id}`);
    if (!cap.hash.startsWith("sha256:")) throw new Error(`capability ${cap.id} has no content hash`);
    ids.add(cap.id);
    caps.statuses[cap.id] = "unavailable";
  }
  caps.manifest = manifest;
  return manifest;
}

let manifestLoad: Promise<CapManifest> | null = null;
const loading = new Map<string, Promise<CapModule | null>>();

/// Ensure a set of capabilities, demand-driven and memoized. Fetches
/// by manifest url, VERIFIES the bytes against the pinned hash, and
/// instantiates. A failed capability resolves null and is marked
/// unavailable — the assembly degrades per capability, never fails
/// wholesale.
export async function ensure(ids: string[]): Promise<Record<string, CapModule | null>> {
  if (!caps.manifest) {
    manifestLoad ??= loadManifest();
    await manifestLoad;
  }
  const byId = new Map((caps.manifest?.capabilities ?? []).map((c) => [c.id, c]));
  const out: Record<string, CapModule | null> = {};
  for (const id of ids) {
    const cap = byId.get(id);
    if (!cap) throw new Error(`manifest has no capability ${id}`);
    out[id] = await ensureOne(cap);
  }
  return out;
}

function ensureOne(cap: Capability): Promise<CapModule | null> {
  const existing = caps.modules[cap.id];
  if (existing) return Promise.resolve(existing);
  const pending = loading.get(cap.id);
  if (pending) return pending;

  const load = (async () => {
    caps.statuses[cap.id] = "loading";
    try {
      const bytes = new Uint8Array(await (await fetch(cap.url)).arrayBuffer());
      // Integrity at the seam: the manifest pins the exact bytes.
      const hash = await contentHash(bytes);
      if (hash !== cap.hash) {
        throw new Error(`capability ${cap.id} hash mismatch: fetched ${hash}, manifest pins ${cap.hash}`);
      }
      const { instance } = await WebAssembly.instantiate(bytes, {});
      const module = new CapModule(cap, instance.exports);
      caps.modules[cap.id] = module;
      caps.statuses[cap.id] = "ready";
      return module;
    } catch (error) {
      caps.statuses[cap.id] = "unavailable";
      caps.errors[cap.id] = error instanceof Error ? error.message : String(error);
      return null;
    } finally {
      loading.delete(cap.id);
    }
  })();
  loading.set(cap.id, load);
  return load;
}
