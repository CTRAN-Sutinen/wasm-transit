/// THE ENGINE — the one piece of host code that calls a module op
/// (docs/units/host-discipline.md). A host layer asks questions as
/// data — `ask(capability, op, args)` — and holds DOCUMENTS, never
/// modules or hashes:
///
/// - A `Doc` is one document version: its kind, its bytes or its value
///   (JS owns the documents), and what it depends on (an edit document
///   is validated over a network). The host makes one when a document
///   crosses in (a fetch, a file, IndexedDB) and passes it wherever an
///   op argument is a `*_hash`; the engine registers it in that module
///   on first use, through the kind's load op, and substitutes its hash.
/// - The engine is the hash authority's witness: the module answers a
///   document's identity, and every module must answer the same one (a
///   served document must answer the hash its server pins). A
///   disagreement is an error, never a quiet second identity — so
///   "the same document" holds by construction, not by discipline.
/// - An answer is cached under the journal's key: the module's bytes
///   (its pinned hash) and the exact request bytes, which name every
///   input by hash. Modules are pure, so a hit is the same answer.
/// - The modules live on the bus worker (src/bus.worker.ts), off the
///   thread the page renders on; every call here is async data.
/// - A module may drop what it holds: an ephemeral kind bounds itself
///   (rt snapshots and live documents keep their last few), the host
///   names what survives (`keep`), and the worker replaces an instance
///   whose memory passed its ceiling. Asking for a dropped document is
///   the module's unknown-hash error; the engine registers the
///   document again and asks once more, so a host never sees it.
///
/// It knows the ABI (which op loads which kind) and nothing of the
/// domain: if a departure, a trip, or a stop appears here, it leaked.

import { call, type Assembly } from "./bus";
import type { Capability } from "./types/cap_abi/Capability";
import type { Evicted } from "./types/cap_abi/Evicted";
import type { Holding } from "./types/cap_abi/Holding";
import type { Request as AnalysisRequest } from "./types/cap_analysis/Request";
import type { Response as AnalysisResponse } from "./types/cap_analysis/Response";
import type { Request as AuthorRequest } from "./types/cap_author/Request";
import type { Response as AuthorResponse } from "./types/cap_author/Response";
import type { Request as BoardRequest } from "./types/cap_board/Request";
import type { Response as BoardResponse } from "./types/cap_board/Response";
import type { Request as DirectoryRequest } from "./types/cap_directory/Request";
import type { Response as DirectoryResponse } from "./types/cap_directory/Response";
import type { Request as GtfsRequest } from "./types/cap_gtfs/Request";
import type { Response as GtfsResponse } from "./types/cap_gtfs/Response";
import type { Request as NetworkRequest } from "./types/cap_network/Request";
import type { Response as NetworkResponse } from "./types/cap_network/Response";
import type { Request as PlansRequest } from "./types/cap_plans/Request";
import type { Response as PlansResponse } from "./types/cap_plans/Response";
import type { Request as RouterRequest } from "./types/cap_router/Request";
import type { Response as RouterResponse } from "./types/cap_router/Response";
import type { Request as RtRequest } from "./types/cap_rt/Request";
import type { Response as RtResponse } from "./types/cap_rt/Response";
import type { Request as StreetsRequest } from "./types/cap_streets/Request";
import type { Response as StreetsResponse } from "./types/cap_streets/Response";
import type { Boundary } from "./types/Boundary";
import type { Network } from "./types/Network";
import type { WalkTable } from "./types/WalkTable";

// ---------------------------------------------------------------------
// the contract: capabilities, ops, and documents
// ---------------------------------------------------------------------

/// The capabilities the generated UIs assemble, each with its
/// generated request and response types.
type Caps = {
  analysis: [AnalysisRequest, AnalysisResponse];
  author: [AuthorRequest, AuthorResponse];
  board: [BoardRequest, BoardResponse];
  directory: [DirectoryRequest, DirectoryResponse];
  gtfs: [GtfsRequest, GtfsResponse];
  network: [NetworkRequest, NetworkResponse];
  plans: [PlansRequest, PlansResponse];
  router: [RouterRequest, RouterResponse];
  rt: [RtRequest, RtResponse];
  streets: [StreetsRequest, StreetsResponse];
};
export type CapId = keyof Caps;
export type OpOf<C extends CapId> = Caps[C][0]["op"];
type RequestOf<C extends CapId, O> = Extract<Caps[C][0], { op: O }>;
/// The answer to an op: tagged like its request (one op answers under
/// another name — `migrate_watchlist` answers `migrated_watchlist`).
type Tag<O> = O extends "migrate_watchlist" ? "migrated_watchlist" : O;
export type Answer<C extends CapId, O> = Extract<Caps[C][1], { op: Tag<O> }>;
/// An op's arguments: the request without its tag, every `*_hash`
/// field taking a document instead of a hash.
type DocFields<T> = { [K in keyof T]: K extends `${string}_hash` ? Doc | Exclude<T[K], string> : T[K] };
export type Args<C extends CapId, O> = DocFields<Omit<RequestOf<C, O>, "op">>;

/// The document kinds that cross into modules by a load op.
export type Kind =
  | "network"
  | "feed"
  | "edits"
  | "live"
  | "boundary"
  | "walks"
  | "streets"
  | "watchlist"
  | "layer"
  | "gtfs_rt";

/// One document version. Immutable: a new version is a new Doc.
export class Doc<V = unknown> {
  readonly kind: Kind;
  /// The document as JS holds it (absent for a document that crossed
  /// in as bytes only — the page never parses those).
  readonly value: V | undefined;
  /// The document this one is read over (an edit document's network).
  readonly over: Doc | null;
  /// The hash a server pins for it; the first registration must answer it.
  readonly pin: string | null;
  #bytes: Uint8Array | null;
  /// The identity, once a module answered it.
  #hash: string | null = null;
  /// Per capability: the load op's answer (the hash plus what the load
  /// reported — an edit document's reports, a feed's summary).
  readonly #loads = new Map<string, Promise<LoadAnswer>>();

  constructor(kind: Kind, source: { value: V } | { bytes: Uint8Array }, opts: { over?: Doc; pin?: string | null } = {}) {
    this.kind = kind;
    this.value = "value" in source ? source.value : undefined;
    this.#bytes = "bytes" in source ? source.bytes : null;
    this.over = opts.over ?? null;
    this.pin = opts.pin ?? null;
  }

  /// The document's identity, once any module has answered it (null
  /// before its first registration).
  get hash(): string | null {
    return this.#hash;
  }

  /// The document's bytes: as they crossed in, or its value's JSON
  /// (encoded once, however many modules register it).
  bytes(): Uint8Array {
    this.#bytes ??= new TextEncoder().encode(JSON.stringify(this.value));
    return this.#bytes;
  }

  /// Record the identity a module answered — the pin first, then the
  /// identity every other module answered.
  witness(cap: string, hash: string): void {
    if (this.pin && hash !== this.pin) {
      throw new Error(`the served ${this.kind} registers as ${hash}, the server pins ${this.pin}`);
    }
    if (this.#hash && hash !== this.#hash) {
      throw new Error(`the ${this.kind} document registers as ${hash} in ${cap}, but as ${this.#hash} elsewhere`);
    }
    this.#hash = hash;
  }

  /// The registration in one module instance (memoized by `slot`: the
  /// capability and its instance generation).
  loadIn(slot: string, run: () => Promise<LoadAnswer>): Promise<LoadAnswer> {
    let load = this.#loads.get(slot);
    if (!load) {
      load = run();
      this.#loads.set(slot, load);
      load.catch(() => this.#loads.delete(slot));
    }
    return load;
  }

  /// The module dropped it: the next use registers it again.
  forget(slot: string): void {
    this.#loads.delete(slot);
  }
}

/// A load op's answer: the document's hash in that module, and the
/// rest of what the load reported.
export type LoadAnswer = { hash: string; [field: string]: unknown };

/// A frame's capability did not assemble (it failed to fetch, verify,
/// or instantiate): the frame says so and every other frame answers.
export class Unavailable extends Error {
  readonly cap: CapId;
  constructor(cap: CapId) {
    super(`The ${cap} capability did not load.`);
    this.cap = cap;
  }
}

// ---------------------------------------------------------------------
// the ABI: how each kind is registered
// ---------------------------------------------------------------------

/// The kernel's bulk op (cap_abi::OP_LOAD_NETWORK): the bare network
/// document's bytes, the same op and hash in every consumer.
const OP_LOAD_NETWORK = 256;

/// How a kind crosses into a module: a bulk op over its bytes (the
/// module's own op code), or a JSON op carrying its value — and the
/// answer's field that names its hash.
type Loader =
  | { bytes: (cap: CapId) => number; hash: string }
  | { json: (doc: Doc, over: string | null) => Record<string, unknown>; hash: string };

const LOADERS: Record<Kind, Loader> = {
  network: { bytes: () => OP_LOAD_NETWORK, hash: "network_hash" },
  // gtfs and network both read a GTFS zip on their op 0.
  feed: { bytes: () => 0, hash: "feed_hash" },
  edits: { json: (d, over) => ({ op: "load_edits", network_hash: over, records: d.value }), hash: "edits_hash" },
  live: { json: (d) => ({ op: "load_live", live: d.value }), hash: "live_hash" },
  boundary: { json: (d) => ({ op: "load_boundary", boundary: d.value }), hash: "boundary_hash" },
  walks: { json: (d) => ({ op: "load_walks", walks: d.value }), hash: "walks_hash" },
  streets: { bytes: () => 1, hash: "streets_hash" }, // cap-streets' OP_LOAD_STREETS
  watchlist: { json: (d) => ({ op: "load_watchlist", document: d.value }), hash: "watchlist_hash" },
  layer: { bytes: () => 0, hash: "layer_hash" }, // cap-analysis' OP_LOAD_LAYER
  gtfs_rt: { bytes: () => 0, hash: "rt_hash" }, // cap-rt's OP_RT_DECODE
};

// ---------------------------------------------------------------------
// the calls
// ---------------------------------------------------------------------

/// The capabilities asked for so far: their manifest entries (null when
/// one did not assemble).
const assembly = new Map<string, Promise<Capability | null>>();

/// The capability's manifest entry once the worker assembled it, or
/// Unavailable.
async function capOf(cap: CapId): Promise<Capability> {
  let got = assembly.get(cap);
  if (!got) {
    got = call({ type: "ensure", caps: [cap] }).then(({ ok }) => (ok as Assembly).caps[cap] ?? null);
    assembly.set(cap, got);
  }
  const entry = await got;
  if (!entry) throw new Unavailable(cap);
  return entry;
}

/// Whether a capability assembles (a frame's `off` flag reads this).
export async function available(cap: CapId): Promise<boolean> {
  return capOf(cap).then(
    () => true,
    () => false,
  );
}

/// Per capability, its instance generation: the worker replaced the
/// instance (the memory ceiling) when it rises, and every registration
/// memo of the old generation is void.
const generations = new Map<string, number>();
const slot = (cap: string): string => `${cap}@${generations.get(cap) ?? 0}`;

/// One call into a module: its outcome's value, or its error thrown. A
/// reset instance voids the generation's registrations first.
async function invoke(
  cap: CapId,
  call_: { text: string } | { op: number; bytes: Uint8Array },
  load: boolean,
): Promise<unknown> {
  await capOf(cap);
  const { ok, reset } = await call(
    "text" in call_ ? { type: "text", cap, text: call_.text, load } : { type: "bytes", cap, op: call_.op, bytes: call_.bytes, load },
  );
  if (reset) generations.set(cap, (generations.get(cap) ?? 0) + 1);
  const outcome = JSON.parse(ok as string) as { ok: unknown } | { error: string };
  if ("error" in outcome) throw new Error(outcome.error);
  return outcome.ok;
}

/// The module's unknown-hash error (`unknown <kind> <hash>: <load op>
/// first`, cap_abi::registry): the hash it no longer holds.
function unknownHash(error: unknown): string | null {
  const match = /^unknown .+ (sha256:[0-9a-f]{64}): .+ first$/.exec(error instanceof Error ? error.message : "");
  return match ? match[1] : null;
}

/// The documents a call names, and what each is read over.
function docsOf(values: unknown[]): Doc[] {
  const out: Doc[] = [];
  for (let value of values) {
    while (value instanceof Doc && !out.includes(value)) {
      out.push(value);
      value = value.over;
    }
  }
  return out;
}

/// Run `attempt` (which registers `docs` and asks): when the module
/// answers that it no longer holds one of them, the document is
/// registered again and the call made once more — at most once per
/// document, so a module that keeps refusing surfaces its error.
async function recovering<T>(cap: CapId, docs: Doc[], attempt: () => Promise<T>): Promise<T> {
  for (let tries = 0; ; tries++) {
    try {
      return await attempt();
    } catch (error) {
      const hash = unknownHash(error);
      const dropped = hash ? docs.find((d) => d.hash === hash) : undefined;
      if (!dropped || tries > docs.length) throw error;
      dropped.forget(slot(cap));
    }
  }
}

/// Register a document in a capability (once per document version and
/// module instance), answering the load op's whole answer — its hash,
/// and what the load reported.
export function load(cap: CapId, doc: Doc): Promise<LoadAnswer> {
  return recovering(cap, docsOf([doc.over]), () => register(cap, doc));
}

function register(cap: CapId, doc: Doc): Promise<LoadAnswer> {
  return doc.loadIn(slot(cap), async () => {
    const loader = LOADERS[doc.kind];
    const over = doc.over ? (await register(cap, doc.over)).hash : null;
    const answer = (await invoke(
      cap,
      "bytes" in loader ? { op: loader.bytes(cap), bytes: doc.bytes() } : { text: JSON.stringify(loader.json(doc, over)) },
      true,
    )) as Record<string, unknown>;
    const hash = answer[loader.hash];
    if (typeof hash !== "string") throw new Error(`${cap} answered no ${loader.hash} for a ${doc.kind} document`);
    doc.witness(cap, hash);
    return { ...answer, hash };
  });
}

/// A document an op produced (an import's network, an applied edit's
/// next network, the live document): the module that answered it
/// already holds it under `hash`, so it is never registered there
/// again (while that instance lives).
export function produced<V>(kind: Kind, value: V, cap: CapId, hash: string, opts: { over?: Doc } = {}): Doc<V> {
  const doc = new Doc<V>(kind, { value }, opts);
  doc.witness(cap, hash);
  void doc.loadIn(slot(cap), () => Promise.resolve({ hash }));
  return doc;
}

/// The answers kept, by journal key — most recently used last.
const answers = new Map<string, unknown>();
const ANSWERS_KEPT = 256;

/// The last op asked of each capability (the e2e replays it).
const lastAsked = new Map<string, { op: string; args: Record<string, unknown> }>();

/// Ask a capability one op. Every `*_hash` argument may be a document
/// (registered in that module first); the answer is cached by the
/// module's bytes and the request's bytes. Throws Unavailable when the
/// capability did not assemble, and the module's error when it
/// refuses.
export async function ask<C extends CapId, O extends OpOf<C>>(cap: C, op: O, args: Args<C, O>): Promise<Answer<C, O>> {
  return (await asked(cap, op, args as Record<string, unknown>, true)) as Answer<C, O>;
}

async function asked(cap: CapId, op: string, args: Record<string, unknown>, cached: boolean): Promise<unknown> {
  const { hash: moduleHash } = await capOf(cap);
  lastAsked.set(cap, { op, args });
  return recovering(cap, docsOf(Object.values(args)), async () => {
    const request: Record<string, unknown> = { op };
    for (const [field, value] of Object.entries(args)) {
      request[field] = value instanceof Doc ? (await register(cap, value)).hash : value;
    }
    const text = JSON.stringify(request);
    const key = `${moduleHash}\n${text}`;
    if (cached && answers.has(key)) {
      const hit = answers.get(key);
      answers.delete(key);
      answers.set(key, hit);
      return hit;
    }
    const answer = await invoke(cap, { text }, false);
    answers.set(key, answer);
    if (answers.size > ANSWERS_KEPT) answers.delete(answers.keys().next().value as string);
    return answer;
  });
}

/// The kernel's registry ops (cap_abi::registry), answered by every
/// module.
const OP_EVICT = 257;
const OP_HELD = 258;

/// Name what survives: every module this page assembled drops every
/// document not named here (nor read over by one named — an edit
/// document keeps its network). A host calls it when it lets documents
/// go (a new network replaces the old); the kinds that bound
/// themselves (rt snapshots, live documents) need no call. A dropped
/// document a host still holds is registered again on its next use.
export async function keep(docs: (Doc | null | undefined)[]): Promise<void> {
  const hashes = docsOf(docs.filter((d): d is Doc => d instanceof Doc))
    .map((d) => d.hash)
    .filter((h): h is string => h !== null);
  const bytes = new TextEncoder().encode(JSON.stringify({ keep: hashes }));
  await Promise.all(
    [...assembly.keys()].map(async (cap) => {
      if (!(await available(cap as CapId))) return;
      (await invoke(cap as CapId, { op: OP_EVICT, bytes }, true)) as Evicted;
    }),
  );
}

/// Inspection the e2e and the perf probe read (window.__bus): the
/// capabilities asked for, what a module holds, its linear memory and
/// resets, the ceiling, and the last op asked of a capability asked
/// again past the answer cache.
const inspection = {
  caps: async (): Promise<CapId[]> => {
    const ids = [...assembly.keys()] as CapId[];
    const ok = await Promise.all(ids.map((cap) => available(cap)));
    return ids.filter((_, i) => ok[i]);
  },
  held: async (cap: CapId): Promise<Holding> => (await invoke(cap, { op: OP_HELD, bytes: new Uint8Array() }, true)) as Holding,
  memory: async (cap: CapId): Promise<{ memory: number; resets: number }> => {
    await capOf(cap);
    return (await call({ type: "inspect", cap })).ok as { memory: number; resets: number };
  },
  ceiling: async (bytes: number, minResetMs: number): Promise<void> => {
    await call({ type: "ceiling", bytes, minResetMs });
  },
  replay: async (cap: CapId): Promise<string> => {
    const last = lastAsked.get(cap);
    if (!last) throw new Error(`nothing was asked of ${cap}`);
    return JSON.stringify(await asked(cap, last.op, last.args, false));
  },
};
if (typeof window !== "undefined") window.__bus = inspection;
export type Inspection = typeof inspection;

// ---------------------------------------------------------------------
// the deployment's published documents
// ---------------------------------------------------------------------

/// The hash a published document's ETag pins (`"<hash>"`, weakened or
/// not by a proxy).
function etag(resp: Response): string | null {
  return resp.headers.get("ETag")?.replace(/^W\//, "").replace(/"/g, "") || null;
}

/// The deployment's published network: `GET /network`, the server's
/// import of its GTFS zip (cap-network, the same module) — registered
/// in `cap` now, so a network that does not answer the hash the server
/// pins is refused before anything is asked of it. Null when the
/// server publishes none.
export async function servedNetwork(cap: CapId = "network"): Promise<{ network: Doc<Network>; boundary: Doc<Boundary> } | null> {
  const resp = await fetch("/network").catch(() => null);
  if (!resp?.ok) return null;
  const served = (await resp.json()) as { network: Network; boundary: Boundary; network_hash: string };
  const network = new Doc<Network>("network", { value: served.network }, { pin: served.network_hash });
  await load(cap, network);
  return { network, boundary: new Doc<Boundary>("boundary", { value: served.boundary }) };
}

/// The deployment's published streets: `GET /streets`, the server's
/// import of its street layer — bytes the page never parses,
/// registered in cap-streets and refused unless it answers the hash
/// the server pins. Null when the server publishes none.
export async function servedStreets(): Promise<Doc | null> {
  const resp = await fetch("/streets").catch(() => null);
  if (!resp?.ok) return null;
  const doc = new Doc("streets", { bytes: new Uint8Array(await resp.arrayBuffer()) }, { pin: etag(resp) });
  await load("streets", doc);
  return doc;
}

/// The deployment's walk table: `GET /walks`, derived on the server
/// from its streets and network, registered in the router and refused
/// unless it answers the hash the server pins. Null when the server
/// publishes none.
export async function servedWalks(): Promise<Doc<WalkTable> | null> {
  const resp = await fetch("/walks").catch(() => null);
  if (!resp?.ok) return null;
  const doc = new Doc<WalkTable>("walks", { value: (await resp.json()) as WalkTable }, { pin: etag(resp) });
  await load("router", doc);
  return doc;
}
