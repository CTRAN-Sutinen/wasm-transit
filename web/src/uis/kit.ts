// Only the host layer imports this file. Shells and components never do.

import { idbGet, idbSet } from "../idb";
import { onNewerBuild } from "../versions";
import { previousClock, serviceClock } from "../clock";
import { available, readJournal, startJournal, type CapId } from "../engine";
import { answers, type AnswerNode, type FrameState, type Request } from "./answers";
import { capital, inputs as runInputs, labelOf, ReadFailed, type Answered, type InputDeclaration, type InputState, type Inputs, type Reader } from "./inputs";
import { headAnswer, retryAfterMs } from "./heads";

// `?journal` starts the engine's journal before the host's first ask
// (docs/architecture/web.md, "The journal").
if (typeof location !== "undefined" && new URLSearchParams(location.search).has("journal")) {
  void startJournal();
  (globalThis as { transitJournal?: typeof readJournal }).transitJournal = readJournal;
}

// Each host's manifest input sets it before the page reads the network in
// force on its date. Unknown, the clock reads the device's.
let agencyZone: string | undefined;

export function setAgencyZone(zone: string | null | undefined): void {
  if (zone) agencyZone = zone;
}

export function today(): string {
  return serviceClock(Date.now(), agencyZone).date;
}

export function nowSecs(): number {
  return serviceClock(Date.now(), agencyZone).secs;
}

export function previousDay(): { from_secs: number; length_secs: number } {
  const { from_secs, length_secs } = previousClock(Date.now(), agencyZone);
  return { from_secs, length_secs };
}

export function clockLabel(): string {
  return clockLabelAt(Date.now());
}

export function clockLabelAt(ms: number): string {
  const opts: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit" };
  try {
    return new Date(ms).toLocaleTimeString([], { ...opts, timeZone: agencyZone });
  } catch {
    return new Date(ms).toLocaleTimeString([], opts);
  }
}

// A hidden page makes no requests, and on its return it ticks at once.
function whileVisible(ms: number, tick: () => void): void {
  setInterval(() => {
    if (!document.hidden) tick();
  }, ms);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) tick();
  });
}

export type { AnswerNode, FrameState, Request } from "./answers";

// A sentence for a prop that takes text. "" when the frame answered.
export function reason(state: FrameState | undefined): string {
  if (!state) return "";
  if (state.kind === "failed") return state.message;
  if (state.kind === "unassembled") return "This part of the app did not load.";
  if (state.kind === "unread") return `${capital(state.name)} could not be read: no answer since ${clockLabelAt(state.sinceMs)}.`;
  return `Waiting for ${state.missing}.`;
}

export function answerGraph(nodes: AnswerNode[], frames: Record<string, FrameState>, onerror: (e: unknown) => void, inputs?: Record<string, InputState>): Request {
  return answers(nodes, { frames, onerror, available: (cap) => available(cap as CapId), inputs });
}

export { ReadFailed } from "./inputs";
export type { Answered, InputDeclaration, InputState, Inputs, Reader } from "./inputs";

// `states` is the same record that the host's answer graph reads.
export function inputGraph(declared: InputDeclaration[], readers: Record<string, Reader<any>>, states: Record<string, InputState>, request: Request): Inputs {
  return runInputs(declared, readers, {
    states,
    request,
    now: () => Date.now(),
    random: () => Math.random(),
    after: (ms, fn) => {
      const t = setTimeout(() => void fn(), ms);
      return () => clearTimeout(t);
    },
    isVisible: () => !document.hidden,
    onVisible: (fn) =>
      document.addEventListener("visibilitychange", () => {
        if (!document.hidden) void fn();
      }),
  });
}

export function inputLabel(state: InputState | undefined): string {
  return labelOf(state, clockLabelAt);
}

// For a head with no hash to pin it, such as GET edits. The age is the
// server's (`X-Store-Stale`, `X-Store-As-Of`).
export async function readHead(url: string, signal: AbortSignal, absent: number[] = []): Promise<Answered<string> | null> {
  const resp = await fetchOrFail(url, signal);
  const text = resp.ok ? await resp.text() : "";
  return headAnswer({ status: resp.status, header: (name) => resp.headers.get(name), text }, absent, Date.now());
}

// The device keeps the copy so a staff page boots offline. It
// throws only when the device keeps no copy either.
export async function readHeadKept(ui: string, url: string, signal: AbortSignal, absent: number[] = []): Promise<Answered<string> | null> {
  const key = `held/${ui}/${url}`;
  try {
    const got = await readHead(url, signal, absent);
    if (got) void idbSet(key, { text: got.value, atMs: got.asOfMs ?? Date.now() }).catch(() => {});
    return got;
  } catch (e) {
    const held = (await idbGet(key).catch(() => undefined)) as { text: string; atMs: number } | undefined;
    if (!held) throw e;
    return { value: held.text, key: held.text, asOfMs: held.atMs, why: "the server is not answering" };
  }
}

async function fetchOrFail(url: string, signal: AbortSignal): Promise<Response> {
  try {
    return await fetch(url, { signal, cache: "no-cache" });
  } catch {
    throw new ReadFailed(signal.aborted ? "no answer in time" : "no answer");
  }
}

function failedOf(resp: Response): ReadFailed {
  return new ReadFailed(`HTTP ${resp.status}`, { retryAfterMs: retryAfterMs(resp.headers.get("Retry-After"), Date.now()) });
}

// The service worker (web/service-worker.js, written to dist/sw.js by the
// build) precaches a new build and leaves it waiting. The page
// decides when to apply it: a rider's or staff page at the next cold start
// or navigation, or when the person accepts it, and a display at its
// next poll while the server answers. A page never applies a build over
// unsaved device data.

let registration: ServiceWorkerRegistration | null = null;
let applying = false;
let movedOn = false;

// A build that waited for this navigation applies now, before anything on
// the page is unsaved.
export async function installApp(): Promise<void> {
  if (!("serviceWorker" in navigator)) return;
  try {
    registration = await navigator.serviceWorker.register("sw.js");
  } catch {
    return;
  }
  // A blocked context resolves with no registration.
  if (!registration) return;
  const hadController = !!navigator.serviceWorker.controller;
  navigator.serviceWorker.addEventListener("controllerchange", () => {
    // The first install takes the page without a reload. Another tab's
    // apply leaves this page on the old build, ready to move on.
    if (applying) location.reload();
    else if (hadController) movedOn = true;
  });
  if (registration.waiting && navigator.serviceWorker.controller) applyBuild();
  // A document that another build wrote makes the page look for the
  // deployment's build now.
  onNewerBuild(() => void registration?.update().catch(() => {}));
  registration.addEventListener("updatefound", () => {
    const next = registration?.installing;
    next?.addEventListener("statechange", () => {
      if (next.state === "installed" && navigator.serviceWorker.controller) for (const f of readyHandlers) f();
    });
  });
  void readBuild();
}

const readyHandlers = new Set<() => void>();

export function onBuildReady(handler: () => void): void {
  readyHandlers.add(handler);
}

// Also asks the browser to look for a build, for the next poll.
export function buildWaiting(): boolean {
  if (!registration) return false;
  registration.update().catch(() => {});
  return movedOn || (!!registration.waiting && !!navigator.serviceWorker.controller);
}

export function applyBuild(): void {
  const waiting = registration?.waiting;
  if (waiting) {
    applying = true;
    waiting.postMessage({ type: "apply" });
  } else if (movedOn) {
    location.reload();
  }
}

async function readBuild(): Promise<void> {
  const controller = navigator.serviceWorker.controller;
  if (!controller) return;
  const channel = new MessageChannel();
  const build = await new Promise<string>((resolve) => {
    channel.port1.onmessage = (e) => resolve(String(e.data));
    controller.postMessage({ type: "build" }, [channel.port2]);
  });
  document.documentElement.dataset.build = build;
}

// The deployment's live feeds (live-feeds.json beside the page) are
// fetched directly, because the agency's feeds serve HTTPS with CORS.
// A dev config whose feeds send no CORS header names the dev server's relay
// as `proxy` (`-rt-proxy`, off by default).

export type LiveUrls = { trips: string; vehicles: string; alerts: string };

let feedProxy: string | null = null;

function fetchFeed(url: string, signal?: AbortSignal): Promise<Response> {
  return fetch(feedProxy ? `${feedProxy}?url=${encodeURIComponent(url)}` : url, { signal });
}

// An oracle (docs/architecture/web.md, "External services") is asked on a
// person's action and never polled, so it is not an input.
export async function askOracle<T>(url: string): Promise<T | null> {
  try {
    const resp = await fetch(url, { signal: AbortSignal.timeout(ORACLE_LIMIT_MS) });
    return resp.ok ? ((await resp.json()) as T) : null;
  } catch {
    return null;
  }
}

const ORACLE_LIMIT_MS = 10_000;

export async function readLiveFeeds(signal: AbortSignal): Promise<Answered<Partial<LiveUrls>> | null> {
  const resp = await fetchOrFail("live-feeds.json", signal);
  if (resp.status === 404) return null;
  if (!resp.ok) throw failedOf(resp);
  let config: Partial<LiveUrls> & { proxy?: string };
  try {
    config = (await resp.json()) as Partial<LiveUrls> & { proxy?: string };
  } catch {
    throw new ReadFailed("live-feeds.json is not JSON");
  }
  feedProxy = typeof config.proxy === "string" && config.proxy ? config.proxy : null;
  const urls: Partial<LiveUrls> = {};
  for (const k of ["trips", "vehicles", "alerts"] as const) if (typeof config[k] === "string") urls[k] = config[k];
  return { value: urls, key: JSON.stringify(urls) };
}

export async function readFeed(url: string, signal: AbortSignal): Promise<Uint8Array<ArrayBuffer>> {
  let resp: Response;
  try {
    resp = await fetchFeed(url, signal);
  } catch {
    throw new ReadFailed(signal.aborted ? "no answer in time" : "no answer");
  }
  if (!resp.ok) throw failedOf(resp);
  return new Uint8Array(await resp.arrayBuffer());
}

// A host's reads are its declared inputs, so this only ticks.
export function clockEvery(ms: number, tick: () => void): void {
  whileVisible(ms, tick);
}

