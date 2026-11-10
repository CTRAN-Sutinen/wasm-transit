/// THE ENGINE — the one piece of host code that calls a module op
/// (docs/units/host-discipline.md). A host layer asks questions as
/// data — `ask(capability, op, args)` — and holds DOCUMENTS, never
/// modules or hashes:
///
/// - A `Doc` is one document version: its kind, its bytes or its value
///   (JS owns the documents), and what it is read over (an edit
///   document's network — kept with it). The host makes one when a
///   document crosses in (a fetch, a file, IndexedDB) and passes it
///   wherever an op argument is a `*_hash`; the engine registers it in
///   that module instance on first use, through the kernel's load of
///   its kind, and substitutes its hash.
/// - A generated UI's page runs ONE module, its linked assembly (D-77;
///   `assemble(ui)` names it before the first ask): every capability it
///   uses over one kernel registry, so a document registers ONCE per
///   page however many capabilities read it, and a document a
///   capability mints there (the live document) is read by hash
///   without crossing back. A capability outside it runs alone.
/// - The engine is the hash authority's witness: the module answers a
///   document's identity, and every module must answer the same one (a
///   served document must answer the hash its server pins; the identity
///   check holds between instances — a page's assembly against the
///   server's pin). A disagreement is an error, never a quiet second
///   identity — so "the same document" holds by construction, not by
///   discipline.
/// - An answer is cached under the journal's key: the module's bytes
///   (its pinned hash — the assembly's, in a linked page) and the exact
///   request bytes, which name the capability and every input by hash.
///   Modules are pure, so a hit is the same answer.
/// - The modules live on the bus worker (src/bus.worker.ts), off the
///   thread the page renders on; every call here is async data.
/// - A module may drop what it holds: an ephemeral kind bounds itself
///   (rt snapshots and live documents keep their last few), the host
///   names what survives (`keep`), and the worker replaces an instance
///   whose memory passed its ceiling (every capability's state in it
///   together). Asking for a dropped document is the module's
///   unknown-hash error; the engine registers the document again (or
///   mints it again, for one the page never received) and asks once
///   more, so a host never sees it.
///
/// It knows the ABI (which op loads which kind) and nothing of the
/// domain: if a departure, a trip, or a stop appears here, it leaked.

import { call, type Assembly, type Route } from "./bus";
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
import type { Request as MigrateRequest } from "./types/cap_migrate/Request";
import type { Response as MigrateResponse } from "./types/cap_migrate/Response";
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
import type { PublishedPlan } from "./types/cap_plans/PublishedPlan";
import type { PublishedPlans } from "./types/cap_plans/PublishedPlans";
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
  migrate: [MigrateRequest, MigrateResponse];
  network: [NetworkRequest, NetworkResponse];
  plans: [PlansRequest, PlansResponse];
  router: [RouterRequest, RouterResponse];
  rt: [RtRequest, RtResponse];
  streets: [StreetsRequest, StreetsResponse];
};
export type CapId = keyof Caps;
export type OpOf<C extends CapId> = Caps[C][0]["op"];
type RequestOf<C extends CapId, O> = Extract<Caps[C][0], { op: O }>;
/// The answer to an op: tagged like its request.
export type Answer<C extends CapId, O> = Extract<Caps[C][1], { op: O }>;
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
  /// Per module instance: the load's answer (the hash plus what the
  /// load reported — a feed's summary, a layer's headcount).
  readonly #loads = new Map<string, Promise<LoadAnswer>>();
  /// For a document the page never received (minted by an op, declined
  /// as an answer): the ask that mints it again where it was dropped.
  remint: (() => Promise<unknown>) | null = null;

  /// `{ minted: true }`: a document an op minted that the page declined
  /// to receive — it lives only in the instance that minted it.
  constructor(kind: Kind, source: { value: V } | { bytes: Uint8Array } | { minted: true }, opts: { over?: Doc; pin?: string | null } = {}) {
    this.kind = kind;
    this.value = "value" in source ? source.value : undefined;
    this.#bytes = "bytes" in source ? source.bytes : null;
    this.over = opts.over ?? null;
    this.pin = opts.pin ?? null;
  }

  /// Whether the document crossed in as bytes.
  get hasBytes(): boolean {
    return this.#bytes !== null;
  }

  /// The document's identity, once any module has answered it (null
  /// before its first registration).
  get hash(): string | null {
    return this.#hash;
  }

  /// The document's bytes: as they crossed in, or its value's JSON
  /// (encoded once, however many modules register it).
  bytes(): Uint8Array {
    if (this.#bytes === null && this.value === undefined) {
      throw new Error(`the ${this.kind} document never crossed to the page — it cannot be registered from here`);
    }
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
  /// module and its instance generation).
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

/// How a kind registers: the kernel's load (cap_abi::kernel — the same
/// op and the same hash in every module whose capabilities read the
/// kind), on its bulk op over the document's bytes or as its JSON op
/// carrying its value — and the answer's field that names its hash.
type Loader = { bytes: number; hash: string } | { json: (doc: Doc) => Record<string, unknown>; hash: string };

const LOADERS: Record<Kind, Loader> = {
  network: { bytes: 256, hash: "network_hash" },
  edits: { json: (d) => ({ op: "load_edits", edits: d.value }), hash: "edits_hash" },
  live: { json: (d) => ({ op: "load_live", live: d.value }), hash: "live_hash" },
  boundary: { json: (d) => ({ op: "load_boundary", boundary: d.value }), hash: "boundary_hash" },
  walks: { json: (d) => ({ op: "load_walks", walks: d.value }), hash: "walks_hash" },
  streets: { bytes: 263, hash: "streets_hash" },
  watchlist: { json: (d) => ({ op: "load_watchlist", document: d.value }), hash: "watchlist_hash" },
  layer: { bytes: 265, hash: "layer_hash" },
  feed: { bytes: 266, hash: "feed_hash" },
  gtfs_rt: { bytes: 267, hash: "rt_hash" },
};

// ---------------------------------------------------------------------
// the calls
// ---------------------------------------------------------------------

/// The generated UI this page is (its linked assembly answers every
/// capability it links).
let page: string | null = null;

/// Name the page's generated UI, before its first ask: its capabilities
/// answer inside its linked assembly (the manifest's `assemblies`).
export function assemble(ui: string): void {
  if (assembly.size > 0 && page !== ui) throw new Error(`assemble("${ui}") after the first ask — name the page first`);
  page = ui;
}

/// A capability as the worker assembled it: its manifest entry and
/// where it answers.
type Placed = Capability & { route: Route };

/// The capabilities asked for so far (null when one did not assemble).
const assembly = new Map<string, Promise<Placed | null>>();

/// The capability as the worker assembled it, or Unavailable.
async function capOf(cap: CapId): Promise<Placed> {
  let got = assembly.get(cap);
  if (!got) {
    got = call({ type: "ensure", caps: [cap], ui: page }).then(({ ok }) => {
      const answer = ok as Assembly;
      const entry = answer.caps[cap];
      return entry ? { ...entry, route: answer.routes[cap] } : null;
    });
    assembly.set(cap, got);
  }
  const entry = await got;
  if (!entry) throw new Unavailable(cap);
  return entry;
}

/// Whether the capabilities answer in one module instance (the page's
/// linked assembly): a document one of them mints is read by the others
/// by hash, so the page need not receive it.
export async function together(...caps: CapId[]): Promise<boolean> {
  const placed = await Promise.all(caps.map((cap) => capOf(cap).then((p) => p.route.module, () => null)));
  return placed.every((m) => m !== null && m === placed[0]);
}

/// Whether a capability assembles (a frame's `off` flag reads this).
export async function available(cap: CapId): Promise<boolean> {
  return capOf(cap).then(
    () => true,
    () => false,
  );
}

/// Per module instance, its generation: the worker replaced the
/// instance (the memory ceiling) when it rises, and every registration
/// memo of the old generation is void — for every capability in it.
const generations = new Map<string, number>();
const modules = new Map<string, string>();
const slot = (cap: string): string => {
  const module = modules.get(cap) ?? `cap:${cap}`;
  return `${module}@${generations.get(module) ?? 0}`;
};

/// One call into a module: its outcome's value, or its error thrown. A
/// reset instance voids the generation's registrations first.
async function invoke(
  cap: CapId,
  call_: { text: string } | { op: number; bytes: Uint8Array },
  load: boolean,
): Promise<unknown> {
  const { route } = await capOf(cap);
  modules.set(cap, route.module);
  const { ok, reset } = await call(
    "text" in call_ ? { type: "text", cap, text: call_.text, load } : { type: "bytes", cap, op: call_.op, bytes: call_.bytes, load },
  );
  if (reset) generations.set(route.module, (generations.get(route.module) ?? 0) + 1);
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
/// registered again — or minted again, for one the page never received
/// — and the call made once more, at most once per document, so a
/// module that keeps refusing surfaces its error.
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
    // A document the page never received is minted again where it is
    // needed — the module that minted it registers it there.
    if (doc.value === undefined && !doc.hasBytes) {
      if (!doc.remint || !doc.hash) throw new Error(`the ${doc.kind} document never crossed to the page — it cannot be registered in ${cap}`);
      await doc.remint();
      return { hash: doc.hash };
    }
    const loader = LOADERS[doc.kind];
    const answer = (await invoke(
      cap,
      "bytes" in loader ? { op: loader.bytes, bytes: doc.bytes() } : { text: JSON.stringify(loader.json(doc)) },
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
/// already holds it under `hash` — and so does every capability linked
/// beside it — so it is never registered there again (while that
/// instance lives). A document the page declined to receive (`value`
/// undefined) carries the ask that mints it: `remint` names it, asked
/// again past the answer cache when an instance dropped it.
export function produced<V>(
  kind: Kind,
  value: V | undefined,
  cap: CapId,
  hash: string,
  opts: { over?: Doc; remint?: { op: string; args: Record<string, unknown> } } = {},
): Doc<V> {
  const doc = new Doc<V>(kind, value === undefined ? { minted: true } : { value }, opts);
  doc.witness(cap, hash);
  if (opts.remint) {
    const { op, args } = opts.remint;
    // A registration, not an op: it never resets the instance it mints in.
    doc.remint = () => asked(cap, op, args, false, true);
  }
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

async function asked(cap: CapId, op: string, args: Record<string, unknown>, cached: boolean, load = false): Promise<unknown> {
  const { route } = await capOf(cap);
  const moduleHash = route.hash;
  lastAsked.set(cap, { op, args });
  return recovering(cap, docsOf(Object.values(args)), async () => {
    // A module linking several capabilities is told which one is asked.
    const request: Record<string, unknown> = route.linked ? { cap, op } : { op };
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
    const answer = await invoke(cap, { text }, load);
    answers.set(key, answer);
    if (answers.size > ANSWERS_KEPT) answers.delete(answers.keys().next().value as string);
    return answer;
  });
}

/// The kernel's registry ops (cap_abi::registry), answered by every
/// module.
const OP_EVICT = 257;
const OP_HELD = 258;

/// Name what survives: every module instance this page assembled drops
/// every document not named here (nor read over by one named — an edit
/// document keeps its network). A host calls it when it lets documents
/// go (a new network replaces the old); the kinds that bound
/// themselves (rt snapshots, live documents) need no call. A dropped
/// document a host still holds is registered again on its next use.
export async function keep(docs: (Doc | null | undefined)[]): Promise<void> {
  const hashes = docsOf(docs.filter((d): d is Doc => d instanceof Doc))
    .map((d) => d.hash)
    .filter((h): h is string => h !== null);
  const bytes = new TextEncoder().encode(JSON.stringify({ keep: hashes }));
  // One call per module instance: a linked assembly holds every
  // capability's documents in one registry.
  const byModule = new Map<string, CapId>();
  for (const cap of assembly.keys()) {
    const placed = await assembly.get(cap);
    if (placed && !byModule.has(placed.route.module)) byModule.set(placed.route.module, cap as CapId);
  }
  await Promise.all([...byModule.values()].map(async (cap) => (await invoke(cap, { op: OP_EVICT, bytes }, true)) as Evicted));
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
  /// The module instance a capability answers in ("ui:<id>" for the
  /// page's linked assembly, "cap:<id>" alone).
  module: async (cap: CapId): Promise<string> => (await capOf(cap)).route.module,
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

/// The deployment's manifest and published documents live at one
/// layout, whoever serves it — the sync server or a static host
/// (D-131; docs/ARCHITECTURE.md, "Static deployment"): `manifest`, and
/// each document by its hash at `blobs/<hash>` (its JSON) and
/// `blobs/<hash>.gz` (the same bytes gzipped — fetched as bytes, so a
/// module registers them compressed and the text never exists whole on
/// the page). Paths are relative to the page, so a deployment may live
/// under a prefix. Every pin is the manifest's, never a response
/// header's.
type DeploymentManifest = {
  network_hash?: string;
  boundary_hash?: string;
  plans_hash?: string;
  streets_hash?: string;
  walks_hash?: string;
  /// A serverless deployment that follows the agency's GTFS URL (D-132).
  feed_url?: string;
};

// ---------------------------------------------------------------------
// a deployment that names its GTFS URL (D-132): with no server to
// import it, the page fetches the zip, imports it in its own assembly
// (network `import`), and checks the URL once a day — a changed zip is
// imported and adopted, an unchanged one is not. The last zip is kept
// in Cache Storage, so a device boots offline on the schedule it had.
// ---------------------------------------------------------------------

const FEED_CHECK_MS = 24 * 3600_000;
/// A check that found the URL unanswered (and read the kept copy, or
/// nothing) retries after this, not at the next poll nor a day later.
const FEED_RETRY_MS = 3600_000;
const FEED_CACHE = "gtfs-feed";

/// What this page knows of the deployment's GTFS URL: when it last
/// checked it, the zip's hash then, the network it imported from that
/// zip (null until imported), and a fetched zip not yet imported.
let followed: { url: string; checkedAt: number; zipHash: string; networkHash: string | null; pending: Uint8Array<ArrayBuffer> | null } | null = null;

async function sha256Hex(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

/// The zip at `url` — fetched (`fresh`), and kept as the last copy;
/// when the URL does not answer, the last copy this device kept; null
/// when neither.
async function feedZip(url: string): Promise<{ bytes: Uint8Array<ArrayBuffer>; fresh: boolean } | null> {
  const kept = typeof caches === "undefined" ? null : await caches.open(FEED_CACHE).catch(() => null);
  try {
    const resp = await fetch(url, { cache: "no-cache" });
    if (resp.ok) {
      await kept?.put(url, resp.clone()).catch(() => {});
      return { bytes: new Uint8Array(await resp.arrayBuffer()), fresh: true };
    }
  } catch {
    // unreachable: the last copy below
  }
  const hit = await kept?.match(url).catch(() => undefined);
  return hit ? { bytes: new Uint8Array(await hit.arrayBuffer()), fresh: false } : null;
}

/// Check the URL (at most once a day): the hash of the network the
/// page should hold — the one it imported, while the zip is unchanged;
/// for a changed zip, a pin no network holds (`feed:<zip hash>`), so the
/// page adopts `servedNetwork`'s import of it. Null when the URL does
/// not answer and nothing was imported yet.
async function followedPin(url: string): Promise<string | null> {
  const now = Date.now();
  if (followed?.url === url && now - followed.checkedAt < FEED_CHECK_MS) return followed.networkHash;
  const got = await feedZip(url);
  // The URL did not answer: check again in an hour, not a day.
  const checkedAt = got?.fresh ? now : now - FEED_CHECK_MS + FEED_RETRY_MS;
  if (!got) {
    if (followed?.url === url) followed.checkedAt = checkedAt;
    return followed?.url === url ? followed.networkHash : null;
  }
  const zipHash = await sha256Hex(got.bytes);
  const same = followed?.url === url && followed.zipHash === zipHash;
  followed = { url, checkedAt, zipHash, networkHash: same ? followed!.networkHash : null, pending: same ? null : got.bytes };
  return followed.networkHash ?? `feed:${zipHash}`;
}

/// Import the deployment's GTFS: the zip the last check fetched, else
/// fetched now. The import is the network module's; its network is
/// registered where it was made (`produced`).
async function importFollowed(url: string): Promise<{ network: Doc<Network>; boundary: Doc<Boundary>; timezone?: string } | null> {
  let bytes = followed?.url === url ? followed.pending : null;
  if (!bytes) {
    const got = await feedZip(url);
    if (!got) return null;
    bytes = got.bytes;
    const checkedAt = got.fresh ? Date.now() : Date.now() - FEED_CHECK_MS + FEED_RETRY_MS;
    followed = { url, checkedAt, zipHash: await sha256Hex(bytes), networkHash: null, pending: bytes };
  }
  if (!(await available("network"))) throw new Unavailable("network");
  const imported = await ask("network", "import", { feed_hash: new Doc("feed", { bytes }) });
  followed = { ...followed!, networkHash: imported.network_hash, pending: null };
  return {
    network: produced("network", imported.network, "network", imported.network_hash),
    boundary: new Doc<Boundary>("boundary", { value: imported.boundary }),
    timezone: imported.timezone ?? undefined,
  };
}

/// A published document's bytes, gzipped, by its hash.
function blobGz(hash: string): Promise<Response | null> {
  return fetch(`blobs/${hash}.gz`).catch(() => null);
}

/// The published pick in force on `date`: the manifest's `plans_hash`
/// names the published plan set (`blobs/<hash>`), and the plans
/// module answers which pick governs the date and its network's hash.
/// Null when the deployment publishes no picks or none has taken
/// effect yet; throws Unavailable when it publishes picks and the plans
/// module does not assemble — a page never guesses which schedule runs.
async function pickInForce(date: string, manifest?: DeploymentManifest | null): Promise<PublishedPlan | null> {
  manifest ??= await deploymentManifestNow();
  const plansHash = manifest?.plans_hash;
  if (!plansHash) return null;
  if (!(await available("plans"))) throw new Unavailable("plans");
  const set = await fetch(`blobs/${plansHash}`).catch(() => null);
  if (!set?.ok) return null;
  const published = (await set.json()) as PublishedPlans;
  return (await ask("plans", "in_force", { published, date })).plan;
}

/// The deployment's manifest as it stands now — revalidated on every
/// read. Null when the deployment does not answer.
async function deploymentManifestNow(): Promise<DeploymentManifest | null> {
  const resp = await fetch("manifest", { cache: "no-cache" }).catch(() => null);
  return resp?.ok ? ((await resp.json()) as DeploymentManifest) : null;
}

/// The hash of the network the deployment serves on `date` — the pick
/// in force, else its import: what `servedNetwork` would load now. An
/// open page compares it with the network it holds on each poll and
/// re-pins when it moved (a republish, a new pick, or the date crossing
/// a pick's effective date — D-63). Null when the deployment does not
/// answer or publishes none: the page keeps what it holds.
export async function servedNetworkPin(date: string): Promise<string | null> {
  const manifest = await deploymentManifestNow();
  if (!manifest) return null;
  const pick = await pickInForce(date, manifest);
  if (pick) return pick.network_hash;
  if (manifest.network_hash) return manifest.network_hash;
  return manifest.feed_url ? followedPin(manifest.feed_url) : null;
}

/// The deployment's published network on `date`, for a page that READS
/// it. When the deployment publishes picks (docs/ROADMAP.md 1), the
/// pick in force on the date (`pickInForce`): its network by hash,
/// with the default boundary — a published pick is an authored
/// document, numbered 1..n the way its export is. Otherwise, and before
/// the first pick takes effect, the manifest's `network_hash`: the
/// import of the deployment's GTFS zip (cap-network, the same module),
/// with its boundary (`boundary_hash`) — one manifest read names both,
/// so the boundary always belongs to the network. A deployment with no
/// network but the agency's GTFS URL (`feed_url`, D-132) has the page
/// import the zip itself (`importFollowed`); its answer carries the
/// feed's `timezone` for the host to adopt. Either way the
/// network arrives gzipped and the page never unzips or parses it —
/// every module registers it compressed (kernel op 256 decodes as it
/// parses). Registered in a reader now, so a network that does not
/// answer its pin is refused before anything is asked of it. A page
/// names the modules that read it, in order, and the pin is checked in
/// the first that assembles — registering it in a module the page never
/// asks would hold a whole copy only to check it. The network's name is
/// a question for the directory (`about`). Throws Unavailable when none
/// assembles; null when the deployment publishes none.
export async function servedNetwork(
  date: string,
  ...readers: CapId[]
): Promise<{ network: Doc<Network>; boundary: Doc<Boundary>; timezone?: string } | null> {
  const candidates: CapId[] = readers.length ? readers : ["network"];
  let cap: CapId | null = null;
  for (const c of candidates) if (!cap && (await available(c))) cap = c;
  if (!cap) throw new Unavailable(candidates[0]);
  const manifest = await deploymentManifestNow();
  const pick = await pickInForce(date, manifest);
  // No network published, but the agency's GTFS URL named: the page
  // imports it (D-132; its `timezone` is the hosts' to adopt).
  if (!pick && !manifest?.network_hash && manifest?.feed_url) return importFollowed(manifest.feed_url);
  const pin = pick?.network_hash ?? manifest?.network_hash;
  if (!pin) return null;
  let boundaryValue: Boundary = { kind: "boundary", schema_version: 1 };
  if (!pick) {
    if (!manifest?.boundary_hash) return null;
    const bnd = await fetch(`blobs/${manifest.boundary_hash}`).catch(() => null);
    if (!bnd?.ok) return null;
    boundaryValue = (await bnd.json()) as Boundary;
  }
  const doc = await blobGz(pin);
  if (!doc?.ok) return null;
  const network = new Doc<Network>("network", { bytes: new Uint8Array(await doc.arrayBuffer()) }, { pin });
  const boundary = new Doc<Boundary>("boundary", { value: boundaryValue });
  await load(cap, network);
  return { network, boundary };
}

/// The deployment's published network as a VALUE, for a page that
/// edits it (the planner forks it into a plan; JS owns the documents
/// it authors): `GET /network`, the import's whole answer, registered
/// in the network module under the pin it serves. Null when the server
/// publishes none.
export async function servedNetworkValue(): Promise<{ network: Doc<Network>; boundary: Doc<Boundary> } | null> {
  const resp = await fetch("/network").catch(() => null);
  if (!resp?.ok) return null;
  const served = (await resp.json()) as { network: Network; boundary: Boundary; network_hash: string };
  const network = new Doc<Network>("network", { value: served.network }, { pin: served.network_hash });
  await load("network", network);
  return { network, boundary: new Doc<Boundary>("boundary", { value: served.boundary }) };
}

/// The deployment's published streets: the manifest's `streets_hash`,
/// the server's import of its street layer — bytes the page never
/// parses, registered in cap-streets and refused unless they answer
/// the pin. Null when the deployment publishes none.
export async function servedStreets(): Promise<Doc | null> {
  const pin = (await deploymentManifestNow())?.streets_hash;
  if (!pin) return null;
  const resp = await blobGz(pin);
  if (!resp?.ok) return null;
  const doc = new Doc("streets", { bytes: new Uint8Array(await resp.arrayBuffer()) }, { pin });
  await load("streets", doc);
  return doc;
}

/// The deployment's walk table: the manifest's `walks_hash`, derived
/// from its streets and network, registered in the router and refused
/// unless it answers the pin. Null when the deployment publishes none.
export async function servedWalks(): Promise<Doc<WalkTable> | null> {
  const pin = (await deploymentManifestNow())?.walks_hash;
  if (!pin) return null;
  const resp = await fetch(`blobs/${pin}`).catch(() => null);
  if (!resp?.ok) return null;
  const doc = new Doc<WalkTable>("walks", { value: (await resp.json()) as WalkTable }, { pin });
  await load("router", doc);
  return doc;
}
