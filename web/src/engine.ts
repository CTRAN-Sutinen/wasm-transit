// The design is in docs/architecture/web.md, "The engine". It knows the ABI
// (which op loads which kind) and nothing of the domain. A departure, a
// trip, or a stop here is a leak.

import { noteRefusal } from "./versions";
import { Refused, unknownDocument, type ModuleError } from "./refusals";
import { ReadFailed } from "./uis/inputs";
import { call, type Assembly, type Route } from "./bus";
import type { Capability } from "./types/cap_abi/Capability";
import type { Evicted } from "./types/cap_abi/Evicted";
import type { Holding } from "./types/cap_abi/Holding";
import type { Caps } from "./types/caps";
import type { Boundary } from "./types/Boundary";
import type { Network } from "./types/Network";
import type { PublishedPlan } from "./types/cap_plans/PublishedPlan";
import type { PublishedPlans } from "./types/cap_plans/PublishedPlans";
import type { WalkTable } from "./types/WalkTable";

export type CapId = keyof Caps;
export type OpOf<C extends CapId> = Caps[C][0]["op"];
type RequestOf<C extends CapId, O> = Extract<Caps[C][0], { op: O }>;
export type Answer<C extends CapId, O> = Extract<Caps[C][1], { op: O }>;
type DocFields<T> = { [K in keyof T]: K extends `${string}_hash` ? Doc | Exclude<T[K], string> : T[K] };
export type Args<C extends CapId, O> = DocFields<Omit<RequestOf<C, O>, "op">>;

// The id is the kind's line in crates/cap-abi/src/kinds.rs.
export type Kind = string;

export class Doc<V = unknown> {
  readonly kind: Kind;
  // Undefined for a document that crossed in as bytes only. The page never
  // parses those.
  readonly value: V | undefined;
  readonly over: Doc | null;
  readonly pin: string | null;
  #bytes: Uint8Array | null;
  #hash: string | null = null;
  readonly #loads = new Map<string, Promise<LoadAnswer>>();
  // For a document the page never received: the ask that creates it again
  // where an instance dropped it.
  remint: (() => Promise<unknown>) | null = null;

  // `{ minted: true }` is a document that an op created and the page
  // declined to receive. It lives only in the instance that created it.
  constructor(kind: Kind, source: { value: V } | { bytes: Uint8Array } | { minted: true }, opts: { over?: Doc; pin?: string | null } = {}) {
    this.kind = kind;
    this.value = "value" in source ? source.value : undefined;
    this.#bytes = "bytes" in source ? source.bytes : null;
    this.over = opts.over ?? null;
    this.pin = opts.pin ?? null;
  }

  get hasBytes(): boolean {
    return this.#bytes !== null;
  }

  get hash(): string | null {
    return this.#hash;
  }

  bytes(): Uint8Array {
    if (this.#bytes === null && this.value === undefined) {
      throw new Error(`the ${this.kind} document never crossed to the page — it cannot be registered from here`);
    }
    this.#bytes ??= new TextEncoder().encode(JSON.stringify(this.value));
    return this.#bytes;
  }

  witness(cap: string, hash: string): void {
    if (this.pin && hash !== this.pin) {
      throw new Error(`the served ${this.kind} registers as ${hash}, the server pins ${this.pin}`);
    }
    if (this.#hash && hash !== this.#hash) {
      throw new Error(`the ${this.kind} document registers as ${hash} in ${cap}, but as ${this.#hash} elsewhere`);
    }
    this.#hash = hash;
  }

  loadIn(slot: string, run: () => Promise<LoadAnswer>): Promise<LoadAnswer> {
    let load = this.#loads.get(slot);
    if (!load) {
      load = run();
      this.#loads.set(slot, load);
      load.catch(() => this.#loads.delete(slot));
    }
    return load;
  }

  forget(slot: string): void {
    this.#loads.delete(slot);
  }
}

export type LoadAnswer = { hash: string; [field: string]: unknown };

export class Unavailable extends Error {
  readonly cap: CapId;
  constructor(cap: CapId) {
    super(`The ${cap} capability did not load.`);
    this.cap = cap;
  }
}

// Each kind's bulk op code. The worker answers it from the manifest with
// the first assembly.
let kernelLoads: Record<string, number> | null = null;

let page: string | null = null;

export function assemble(ui: string): void {
  if (assembly.size > 0 && page !== ui) throw new Error(`assemble("${ui}") after the first ask — name the page first`);
  page = ui;
}

type Placed = Capability & { route: Route };

const assembly = new Map<string, Promise<Placed | null>>();

async function capOf(cap: CapId): Promise<Placed> {
  let got = assembly.get(cap);
  if (!got) {
    got = call({ type: "ensure", caps: [cap], ui: page }).then(({ ok }) => {
      const answer = ok as Assembly;
      kernelLoads ??= answer.loads;
      const entry = answer.caps[cap];
      return entry ? { ...entry, route: answer.routes[cap] } : null;
    });
    assembly.set(cap, got);
  }
  const entry = await got;
  if (!entry) throw new Unavailable(cap);
  return entry;
}

export async function together(...caps: CapId[]): Promise<boolean> {
  const placed = await Promise.all(caps.map((cap) => capOf(cap).then((p) => p.route.module, () => null)));
  return placed.every((m) => m !== null && m === placed[0]);
}

export async function available(cap: CapId): Promise<boolean> {
  return capOf(cap).then(
    () => true,
    () => false,
  );
}

// The generation of each module instance rises when the worker replaces
// the instance, which voids every registration memo of the old generation
// for every capability in it.
const generations = new Map<string, number>();
const modules = new Map<string, string>();
const slot = (cap: string): string => {
  const module = modules.get(cap) ?? `cap:${cap}`;
  return `${module}@${generations.get(module) ?? 0}`;
};

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
  const outcome = JSON.parse(ok as string) as { ok: unknown } | { error: ModuleError };
  if ("error" in outcome) {
    const refused = new Refused(outcome.error);
    // A document that another build wrote asks for the deployment's build
    // (src/versions.ts).
    noteRefusal(refused);
    throw refused;
  }
  return outcome.ok;
}

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

// Retries at most once per document, so a module that keeps refusing
// surfaces its error.
async function recovering<T>(cap: CapId, docs: Doc[], attempt: () => Promise<T>): Promise<T> {
  for (let tries = 0; ; tries++) {
    try {
      return await attempt();
    } catch (error) {
      const hash = unknownDocument(error);
      const dropped = hash ? docs.find((d) => d.hash === hash) : undefined;
      if (!dropped || tries > docs.length) throw error;
      dropped.forget(slot(cap));
    }
  }
}

export function load(cap: CapId, doc: Doc): Promise<LoadAnswer> {
  return recovering(cap, docsOf([doc.over]), () => register(cap, doc));
}

function register(cap: CapId, doc: Doc): Promise<LoadAnswer> {
  return doc.loadIn(slot(cap), async () => {
    if (doc.value === undefined && !doc.hasBytes) {
      if (!doc.remint || !doc.hash) throw new Error(`the ${doc.kind} document never crossed to the page — it cannot be registered in ${cap}`);
      await doc.remint();
      return { hash: doc.hash };
    }
    await capOf(cap);
    const code = kernelLoads?.[doc.kind];
    if (code === undefined) throw new Error(`the manifest has no kernel load for a ${doc.kind} document`);
    const answer = (await invoke(cap, { op: code, bytes: doc.bytes() }, true)) as Record<string, unknown>;
    // A load answers the identity in one `<name>_hash` field
    // (`transit_version::Load::answer`).
    const hashes = Object.keys(answer).filter((field) => field.endsWith("_hash"));
    const hash = hashes.length === 1 ? answer[hashes[0]] : undefined;
    if (typeof hash !== "string") throw new Error(`${cap} answered no one hash for a ${doc.kind} document`);
    doc.witness(cap, hash);
    return { ...answer, hash };
  });
}

// The module that answered the document, and every capability linked
// beside it, already holds it under `hash`, so it is not registered there
// again while that instance lives. `remint` is asked again past the answer
// cache when an instance drops a document that the page declined to receive.
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
    // A registration, so it never resets the instance that it creates in.
    doc.remint = () => asked(cap, op, args, false, true);
  }
  void doc.loadIn(slot(cap), () => Promise.resolve({ hash }));
  return doc;
}

// Ordered by use, most recent last.
const answers = new Map<string, unknown>();
const ANSWERS_KEPT = 256;

const lastAsked = new Map<string, { op: string; args: Record<string, unknown> }>();

export async function ask<C extends CapId, O extends OpOf<C>>(cap: C, op: O, args: Args<C, O>): Promise<Answer<C, O>> {
  return (await asked(cap, op, args as Record<string, unknown>, true)) as Answer<C, O>;
}

async function asked(cap: CapId, op: string, args: Record<string, unknown>, cached: boolean, load = false): Promise<unknown> {
  const { route } = await capOf(cap);
  const moduleHash = route.hash;
  lastAsked.set(cap, { op, args });
  return recovering(cap, docsOf(Object.values(args)), async () => {
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

// Start the journal before the first ask. It is off by default because it
// keeps the bytes of every request.
export async function startJournal(): Promise<void> {
  await call({ type: "journal", run: `${new Date().toISOString()}-page` });
}

export async function readJournal(): Promise<{ journal: string; blobs: Record<string, string> }> {
  const { ok } = await call({ type: "journal-read" });
  const { journal, blobs } = ok as { journal: string; blobs: Record<string, Uint8Array> };
  const base64 = (bytes: Uint8Array): string => {
    let text = "";
    for (let i = 0; i < bytes.length; i += 0x8000) text += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return btoa(text);
  };
  return { journal, blobs: Object.fromEntries(Object.entries(blobs).map(([hash, bytes]) => [hash, base64(bytes)])) };
}

const OP_EVICT = 257;
const OP_HELD = 258;

export async function keep(docs: (Doc | null | undefined)[]): Promise<void> {
  const hashes = docsOf(docs.filter((d): d is Doc => d instanceof Doc))
    .map((d) => d.hash)
    .filter((h): h is string => h !== null);
  const bytes = new TextEncoder().encode(JSON.stringify({ keep: hashes }));
  // A linked assembly holds every capability's documents in one registry.
  const byModule = new Map<string, CapId>();
  for (const cap of assembly.keys()) {
    const placed = await assembly.get(cap);
    if (placed && !byModule.has(placed.route.module)) byModule.set(placed.route.module, cap as CapId);
  }
  await Promise.all([...byModule.values()].map(async (cap) => (await invoke(cap, { op: OP_EVICT, bytes }, true)) as Evicted));
}

const inspection = {
  caps: async (): Promise<CapId[]> => {
    const ids = [...assembly.keys()] as CapId[];
    const ok = await Promise.all(ids.map((cap) => available(cap)));
    return ids.filter((_, i) => ok[i]);
  },
  held: async (cap: CapId): Promise<Holding> => (await invoke(cap, { op: OP_HELD, bytes: new Uint8Array() }, true)) as Holding,
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

// The sync server and a static host serve one layout (D-131). Paths are
// relative to the page, so a deployment may live under a prefix. Every pin
// is the manifest's, never a response header's.
export type DeploymentManifest = {
  network_hash?: string;
  boundary_hash?: string;
  plans_hash?: string;
  [pin: `${string}_hash`]: string | undefined;
  feed_hash?: string;
  auth?: { issuer: string; client_id: string; scope: string };
  streets_hash?: string;
  walks_hash?: string;
  feed_url?: string;
  timezone?: string;
};

// A read answers null when the deployment keeps no document, and throws
// ReadFailed when the read fails (QAF-131). Each read takes the manifest
// that the page holds, so one manifest read names every pin of one poll.

export type ReadOptions = { signal?: AbortSignal; manifest?: DeploymentManifest };

function retryAfterOf(resp: Response): number | null {
  const v = resp.headers.get("Retry-After");
  if (!v) return null;
  const secs = Number(v);
  if (Number.isFinite(secs)) return Math.max(0, secs * 1000);
  const at = Date.parse(v);
  return Number.isFinite(at) ? Math.max(0, at - Date.now()) : null;
}

async function getOk(url: string, signal: AbortSignal | undefined, { absent = [] as number[], cache }: { absent?: number[]; cache?: RequestCache } = {}): Promise<Response | null> {
  let resp: Response;
  try {
    resp = await fetch(url, { signal, cache });
  } catch {
    throw new ReadFailed(signal?.aborted ? "no answer in time" : "no answer");
  }
  if (resp.ok) return resp;
  if (absent.includes(resp.status)) return null;
  throw new ReadFailed(`HTTP ${resp.status}`, { retryAfterMs: retryAfterOf(resp) });
}

// `asOfMs` is when the upstream last confirmed the manifest, and is set
// only when the answer is a working copy: the server's (`X-Store-Stale`,
// `X-Store-As-Of`) or the service worker's last copy (`X-Served-From:
// last-copy`, its `Date`). `answered` is false when the worker answered.
export async function readManifest(signal?: AbortSignal): Promise<{ manifest: DeploymentManifest; asOfMs: number | null; why: string | null; answered: boolean }> {
  const resp = (await getOk("manifest", signal, { cache: "no-cache" }))!;
  let manifest: DeploymentManifest;
  try {
    manifest = (await resp.json()) as DeploymentManifest;
  } catch {
    throw new ReadFailed("the manifest is not JSON");
  }
  const lastCopy = resp.headers.get("X-Served-From") === "last-copy";
  let asOfMs: number | null = null;
  if (lastCopy) {
    const date = Date.parse(resp.headers.get("Date") ?? "");
    asOfMs = Number.isFinite(date) ? date : 0;
  } else if (resp.headers.get("X-Store-Stale") === "true") {
    const asOf = Number(resp.headers.get("X-Store-As-Of"));
    asOfMs = Number.isFinite(asOf) && asOf > 0 ? asOf : 0;
  }
  const why = lastCopy ? "the server is not answering" : asOfMs != null ? "the server's store is not answering" : null;
  return { manifest, asOfMs, why, answered: !lastCopy };
}

async function manifestOf(opts: ReadOptions): Promise<DeploymentManifest> {
  return opts.manifest ?? (await readManifest(opts.signal)).manifest;
}

// With no server to import the feed (D-132), the page fetches the zip,
// imports it in its own assembly, and checks the URL once a day. Cache
// Storage keeps the last zip, so a device boots offline on the schedule it
// had.

const FEED_CHECK_MS = 24 * 3600_000;
const FEED_RETRY_MS = 3600_000;
const FEED_CACHE = "gtfs-feed";

let followed: { url: string; checkedAt: number; zipHash: string; networkHash: string | null; pending: Uint8Array<ArrayBuffer> | null } | null = null;

async function sha256Hex(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

async function feedZip(url: string, signal?: AbortSignal): Promise<{ bytes: Uint8Array<ArrayBuffer>; fresh: boolean }> {
  const kept = typeof caches === "undefined" ? null : await caches.open(FEED_CACHE).catch(() => null);
  let failure = "no answer";
  try {
    const resp = await fetch(url, { cache: "no-cache", signal });
    if (resp.ok) {
      await kept?.put(url, resp.clone()).catch(() => {});
      return { bytes: new Uint8Array(await resp.arrayBuffer()), fresh: true };
    }
    failure = `HTTP ${resp.status}`;
  } catch {
  }
  const hit = await kept?.match(url).catch(() => undefined);
  if (hit) return { bytes: new Uint8Array(await hit.arrayBuffer()), fresh: false };
  throw new ReadFailed(`the agency's GTFS URL: ${failure}`);
}

// A changed zip answers a pin that no network holds (`feed:<zip hash>`),
// so the page adopts `servedNetwork`'s import of it.
async function followedPin(url: string, signal?: AbortSignal): Promise<string> {
  const now = Date.now();
  if (followed?.url === url && followed.networkHash && now - followed.checkedAt < FEED_CHECK_MS) return followed.networkHash;
  let got: { bytes: Uint8Array<ArrayBuffer>; fresh: boolean };
  try {
    got = await feedZip(url, signal);
  } catch (e) {
    if (followed?.url === url && followed.networkHash) {
      followed.checkedAt = now - FEED_CHECK_MS + FEED_RETRY_MS;
      return followed.networkHash;
    }
    throw e;
  }
  const checkedAt = got.fresh ? now : now - FEED_CHECK_MS + FEED_RETRY_MS;
  const zipHash = await sha256Hex(got.bytes);
  const same = followed?.url === url && followed.zipHash === zipHash;
  followed = { url, checkedAt, zipHash, networkHash: same ? followed!.networkHash : null, pending: same ? null : got.bytes };
  return followed.networkHash ?? `feed:${zipHash}`;
}

async function importFollowed(url: string, signal?: AbortSignal): Promise<{ network: Doc<Network>; boundary: Doc<Boundary>; timezone?: string }> {
  let bytes = followed?.url === url ? followed.pending : null;
  if (!bytes) {
    const got = await feedZip(url, signal);
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

// A hash that the manifest pins is there, so any other answer is a failed read.
async function blobGz(hash: string, signal?: AbortSignal): Promise<Uint8Array<ArrayBuffer>> {
  const resp = (await getOk(`blobs/${hash}.gz`, signal))!;
  return new Uint8Array(await resp.arrayBuffer());
}

async function blobJson<T>(hash: string, signal?: AbortSignal): Promise<T> {
  const resp = (await getOk(`blobs/${hash}`, signal))!;
  return (await resp.json()) as T;
}

// Throws Unavailable when the deployment publishes picks and the plans
// module does not assemble, because a page never guesses which schedule runs.
async function pickInForce(date: string, manifest: DeploymentManifest, signal?: AbortSignal): Promise<PublishedPlan | null> {
  const plansHash = manifest.plans_hash;
  if (!plansHash) return null;
  if (!(await available("plans"))) throw new Unavailable("plans");
  const published = await blobJson<PublishedPlans>(plansHash, signal);
  return (await ask("plans", "in_force", { published, date })).plan;
}

// An open page compares this with its network on each poll and re-pins
// when it moved: a republish, a new pick, or the date crossing a pick's
// effective date (D-63).
export async function servedNetworkPin(date: string, opts: ReadOptions = {}): Promise<string | null> {
  const manifest = await manifestOf(opts);
  const pick = await pickInForce(date, manifest, opts.signal);
  if (pick) return pick.network_hash;
  if (manifest.network_hash) return manifest.network_hash;
  return manifest.feed_url ? followedPin(manifest.feed_url, opts.signal) : null;
}

// The network arrives gzipped and the page never unzips or parses it.
// The pin is checked in the first reader that assembles, because
// registering the network in a module that the page never asks would hold
// a whole copy only to check it. One manifest read names the network and
// its boundary, so the boundary belongs to the network.
export async function servedNetwork(
  date: string,
  readers: CapId[],
  opts: ReadOptions = {},
): Promise<{ network: Doc<Network>; boundary: Doc<Boundary>; timezone?: string } | null> {
  const candidates: CapId[] = readers.length ? readers : ["network"];
  let cap: CapId | null = null;
  for (const c of candidates) if (!cap && (await available(c))) cap = c;
  if (!cap) throw new Unavailable(candidates[0]);
  const manifest = await manifestOf(opts);
  const pick = await pickInForce(date, manifest, opts.signal);
  // The answer carries the feed's `timezone` for the host to adopt.
  if (!pick && !manifest.network_hash && manifest.feed_url) return importFollowed(manifest.feed_url, opts.signal);
  const pin = pick?.network_hash ?? manifest.network_hash;
  if (!pin) return null;
  let boundaryValue: Boundary = { kind: "boundary", schema_version: 1 };
  if (!pick) {
    if (!manifest.boundary_hash) throw new ReadFailed("the manifest names a network and no boundary");
    boundaryValue = await blobJson<Boundary>(manifest.boundary_hash, opts.signal);
  }
  const network = new Doc<Network>("network", { bytes: await blobGz(pin, opts.signal) }, { pin });
  const boundary = new Doc<Boundary>("boundary", { value: boundaryValue });
  await load(cap, network);
  return { network, boundary };
}

// Writes need a token and reads stay public (D-58).
export async function deploymentAuth(opts: ReadOptions = {}): Promise<{ issuer: string; client_id: string; scope: string } | null> {
  return (await manifestOf(opts)).auth ?? null;
}

// The id is the zip's hash (`feed_hash`), which the stops page compares
// the stops record with (D-150).
export async function servedFeed(readers: CapId[], opts: ReadOptions = {}): Promise<{ network: Doc<Network>; id: string } | null> {
  let cap: CapId | null = null;
  for (const c of readers.length ? readers : (["network"] as CapId[])) if (!cap && (await available(c))) cap = c;
  if (!cap) return null;
  const manifest = await manifestOf(opts);
  const pin = manifest.network_hash;
  if (!pin || !manifest.feed_hash) return null;
  const network = new Doc<Network>("network", { bytes: await blobGz(pin, opts.signal) }, { pin });
  await load(cap, network);
  return { network, id: manifest.feed_hash };
}

export async function servedNetworkValue(signal?: AbortSignal): Promise<{
  network: Doc<Network>;
  boundary: Doc<Boundary>;
} | null> {
  const resp = await getOk("/network", signal, { absent: [404] });
  if (!resp) return null;
  const served = (await resp.json()) as { network: Network; boundary: Boundary; network_hash: string };
  const network = new Doc<Network>("network", { value: served.network }, { pin: served.network_hash });
  await load("network", network);
  return {
    network,
    boundary: new Doc<Boundary>("boundary", { value: served.boundary }),
  };
}

// Every published pick, earlier, in force, and later alike (D-139).
export async function publishedPlanSet(opts: ReadOptions = {}): Promise<PublishedPlans | null> {
  const plansHash = (await manifestOf(opts)).plans_hash;
  return plansHash ? blobJson<PublishedPlans>(plansHash, opts.signal) : null;
}

export async function publishedPickValue(pick: PublishedPlan, signal?: AbortSignal): Promise<Doc<Network>> {
  const resp = (await getOk(`blobs/${pick.network_hash}.gz`, signal))!;
  if (!resp.body) throw new ReadFailed("the pick's network is empty");
  const text = await new Response(resp.body.pipeThrough(new DecompressionStream("gzip"))).text();
  const network = new Doc<Network>("network", { value: JSON.parse(text) as Network }, { pin: pick.network_hash });
  await load("network", network);
  return network;
}

export async function served(kind: Kind, opts: ReadOptions = {}): Promise<Doc | null> {
  const pin = (await manifestOf(opts))[`${kind}_hash`];
  if (typeof pin !== "string") return null;
  return new Doc(kind, { bytes: await blobGz(pin, opts.signal) }, { pin });
}

export async function servedStreets(opts: ReadOptions = {}): Promise<Doc | null> {
  const doc = await served("streets", opts);
  if (doc) await load("streets", doc);
  return doc;
}

export async function servedWalks(opts: ReadOptions = {}): Promise<Doc<WalkTable> | null> {
  const pin = (await manifestOf(opts)).walks_hash;
  if (!pin) return null;
  const doc = new Doc<WalkTable>("walks", { value: await blobJson<WalkTable>(pin, opts.signal) }, { pin });
  await load("router", doc);
  return doc;
}
