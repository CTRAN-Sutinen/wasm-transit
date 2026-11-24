/// The generated UIs' shared host glue — what every UI's host layer
/// needs and none should rewrite. Host layer only: shells and
/// components never import this.
///
/// A generated UI lives in `src/uis/<id>/` (docs/architecture/generated-uis.md): `ui.json` (what it answers), `main.ts` (mount,
/// then boot), `Shell.svelte` (placement only), `host.svelte.ts` (the
/// only file that asks the bus — through the engine, src/engine.ts),
/// and `<id>.html` at the web root.

import { get as idbGet, set as idbSet } from "idb-keyval";
import { onNewerBuild } from "../versions";
import { previousClock, serviceClock } from "../clock";
import { available, readJournal, startJournal, type CapId } from "../engine";
import { answers, type AnswerNode, type FrameState, type Request } from "./answers";
import { capital, inputs as runInputs, labelOf, ReadFailed, type Answered, type InputDeclaration, type InputState, type Inputs, type Reader } from "./inputs";
import { headAnswer, retryAfterMs } from "./heads";

// The journal, on request. `?journal` in the page's URL starts the
// engine's journal before the host's first ask, and
// `transitJournal()` (the console, or a test) reads it as one file for
// `caps-run replay FILE.json` (docs/architecture/web.md, "The
// journal").
if (typeof location !== "undefined" && new URLSearchParams(location.search).has("journal")) {
  void startJournal();
  (globalThis as { transitJournal?: typeof readJournal }).transitJournal = readJournal;
}

// ---------------------------------------------------------------------
// the clock — hosts own it; ops take it as arguments. Read in the
// AGENCY's zone (src/clock.ts), never the device's.
// ---------------------------------------------------------------------

/// The agency's zone: the manifest's `timezone` (each host's manifest
/// input sets it, before the page reads the network in force on its
/// date), or a loaded zip's. Unknown, the clock reads the device's.
let agencyZone: string | undefined;

export function setAgencyZone(zone: string | null | undefined): void {
  if (zone) agencyZone = zone;
}

/// The agency's date, "YYYY-MM-DD" — dates cross every boundary as
/// strings.
export function today(): string {
  return serviceClock(Date.now(), agencyZone).date;
}

/// Seconds into the agency's service day (from noon minus 12 h, the
/// GTFS anchor).
export function nowSecs(): number {
  return serviceClock(Date.now(), agencyZone).secs;
}

/// The previous service date at this instant, as a board asks for it
/// (cap-board's `previous`): the instant on that date's clock and its
/// length — its after-midnight trips are still leaving.
export function previousDay(): { from_secs: number; length_secs: number } {
  const { from_secs, length_secs } = previousClock(Date.now(), agencyZone);
  return { from_secs, length_secs };
}

/// The agency's wall clock, "4:05 PM", for a status line.
export function clockLabel(): string {
  return clockLabelAt(Date.now());
}

/// The agency's wall clock at an instant (Unix ms), "4:05 PM".
export function clockLabelAt(ms: number): string {
  const opts: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit" };
  try {
    return new Date(ms).toLocaleTimeString([], { ...opts, timeZone: agencyZone });
  } catch {
    return new Date(ms).toLocaleTimeString([], opts);
  }
}

/// Run `tick` every `ms` while the page is visible: a hidden page makes
/// no requests, and on its return it ticks at once, re-reading what it
/// missed.
function whileVisible(ms: number, tick: () => void): void {
  setInterval(() => {
    if (!document.hidden) tick();
  }, ms);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) tick();
  });
}

// ---------------------------------------------------------------------
// The answer graph (src/uis/answers.ts): a host's nodes, scheduled, and
// each frame's state. Here it gets the engine's `available`.
// ---------------------------------------------------------------------

export type { AnswerNode, FrameState, Request } from "./answers";

/// The host's answer graph. `frames` is the host's `$state` record of
/// frame states, so the shell redraws when one changes. `onerror`
/// hears a document read that fails (a node whose frame is "").
/// Why a frame has no answer, as a sentence for a prop that takes text
/// (a block's error, an export's): "" when it answered.
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

// ---------------------------------------------------------------------
// The inputs (src/uis/inputs.ts): a host's declared inputs, read by one
// poll while the page is visible. Here they get the browser's clock,
// timers, and visibility.
// ---------------------------------------------------------------------

export { ReadFailed } from "./inputs";
export type { Answered, InputDeclaration, InputState, Inputs, Reader } from "./inputs";

/// The host's inputs. `declared` is its ui.json's `inputs`; `readers`
/// reads and adopts each; `states` is the host's `$state` record of
/// input states (the same record its answer graph reads); `request` is
/// its answer graph.
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

/// What the page says of an input (inputs.ts `labelOf`), on the
/// agency's clock: its age when the answer is a working copy or old,
/// and since when its reads fail. "" when it is current.
export function inputLabel(state: InputState | undefined): string {
  return labelOf(state, clockLabelAt);
}

/// A head the server serves with no hash to pin it (GET edits): its
/// text, keyed by itself, with the server's age when it answers from
/// its cache (`X-Store-Stale`, `X-Store-As-Of`; D-119). Throws
/// ReadFailed when the server does not answer. `absent` statuses
/// answer null: the server keeps none.
export async function readHead(url: string, signal: AbortSignal, absent: number[] = []): Promise<Answered<string> | null> {
  const resp = await fetchOrFail(url, signal);
  const text = resp.ok ? await resp.text() : "";
  return headAnswer({ status: resp.status, header: (name) => resp.headers.get(name), text }, absent, Date.now());
}

/// A head as `readHead` reads it, kept on the device as this page's
/// working copy (a staff page boots offline, D-189). While the server
/// does not answer, the read answers the device's copy with the time
/// it was confirmed: an old answer that says its age. It throws only
/// when the device keeps no copy either.
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

/// A status other than 2xx, as a failed read with the upstream's
/// Retry-After.
function failedOf(resp: Response): ReadFailed {
  return new ReadFailed(`HTTP ${resp.status}`, { retryAfterMs: retryAfterMs(resp.headers.get("Retry-After"), Date.now()) });
}

// ---------------------------------------------------------------------
// the app's build — ONE lifecycle for updates and offline (D-63, D-65):
// the service worker (web/service-worker.js, written to dist/sw.js by
// the build) precaches a new build in the background and leaves it
// waiting; the page decides when it applies. A rider's or a staff
// page applies it at the next cold start or navigation (`installApp`)
// or when the person accepts it (a staff page says so at once,
// `onBuildReady`; D-189); a display at its next poll while the server
// answers. Never over unsaved device data — the host says what that
// is (a staff page's outbox is on the device, and survives the reload).
// ---------------------------------------------------------------------

let registration: ServiceWorkerRegistration | null = null;
let applying = false;
/// Another page applied a build: this page's worker is the new build's
/// while the page still runs the old one.
let movedOn = false;

/// Register the service worker; a build that waited for this
/// navigation applies now, before anything on the page is unsaved.
/// Without service workers (a dev server, a blocked context) the page
/// runs as it always did.
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
    // An applied build reloads into itself; the first install takes
    // the page without one; another tab's apply leaves this page on
    // the old build, ready to move on.
    if (applying) location.reload();
    else if (hadController) movedOn = true;
  });
  if (registration.waiting && navigator.serviceWorker.controller) applyBuild();
  // A document another build wrote: look for the deployment's build now.
  // The page applies it by its own rule, as for any new build.
  onNewerBuild(() => void registration?.update().catch(() => {}));
  // A new build that finished downloading is said at once (onBuildReady).
  registration.addEventListener("updatefound", () => {
    const next = registration?.installing;
    next?.addEventListener("statechange", () => {
      if (next.state === "installed" && navigator.serviceWorker.controller) for (const f of readyHandlers) f();
    });
  });
  void readBuild();
}

const readyHandlers = new Set<() => void>();

/// Run `handler` when a new build has downloaded and waits: a page that
/// asks the person to apply it shows that at once, not at its next poll.
export function onBuildReady(handler: () => void): void {
  readyHandlers.add(handler);
}

/// Whether a new build has downloaded and waits (asking the browser to
/// look for one, for the next poll).
export function buildWaiting(): boolean {
  if (!registration) return false;
  registration.update().catch(() => {});
  return movedOn || (!!registration.waiting && !!navigator.serviceWorker.controller);
}

/// Apply the waiting build: it takes over, and the page reloads into it
/// (or, when another page already applied it, just reloads).
export function applyBuild(): void {
  const waiting = registration?.waiting;
  if (waiting) {
    applying = true;
    waiting.postMessage({ type: "apply" });
  } else if (movedOn) {
    location.reload();
  }
}

/// The build this page runs, as its worker names it (inspection: the
/// page's `data-build`).
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

// ---------------------------------------------------------------------
// the live feeds — the deployment's (live-feeds.json beside the page,
// D-39), fetched directly: the agency's feeds serve HTTPS with CORS
// (D-67). A dev config whose feeds send no CORS header names the dev
// server's relay as `proxy` (`-rt-proxy`, off by default).
// ---------------------------------------------------------------------

export type LiveUrls = { trips: string; vehicles: string; alerts: string };

/// The relay a dev config names ("/rt"), else null: feeds are fetched
/// directly.
let feedProxy: string | null = null;

/// Fetch one live feed: directly, or through the dev relay when the
/// config names one.
function fetchFeed(url: string, signal?: AbortSignal): Promise<Response> {
  return fetch(feedProxy ? `${feedProxy}?url=${encodeURIComponent(url)}` : url, { signal });
}

/// Ask an oracle the manifest lists (the geocoder; docs/architecture/
/// web.md, "External services") at `url`: its JSON answer, or null when
/// it does not answer. It is asked on a person's action, never polled,
/// so it is not an input.
export async function askOracle<T>(url: string): Promise<T | null> {
  try {
    const resp = await fetch(url, { signal: AbortSignal.timeout(ORACLE_LIMIT_MS) });
    return resp.ok ? ((await resp.json()) as T) : null;
  } catch {
    return null;
  }
}

/// The time limit of an oracle's answer.
const ORACLE_LIMIT_MS = 10_000;

/// The deployment's feed URLs, as an input reads them
/// (live-feeds.json beside the page): null when the page has none
/// (404). Throws ReadFailed when the read fails.
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

/// One live feed's bytes, as an input reads them (directly, or through
/// the dev relay). Throws ReadFailed when the feed does not answer, or
/// answers a status other than 2xx.
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

/// Run `tick` every `ms` while the page is visible, and at once when it
/// shows again. The clock only: it reads nothing. A host's reads are
/// its declared inputs.
export function clockEvery(ms: number, tick: () => void): void {
  whileVisible(ms, tick);
}

