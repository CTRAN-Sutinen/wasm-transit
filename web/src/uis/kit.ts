/// The generated UIs' shared host glue — what every UI's host layer
/// needs and none should rewrite. Host layer only: shells and
/// components never import this.
///
/// A generated UI lives in `src/uis/<id>/` (docs/architecture/generated-uis.md): `ui.json` (what it answers), `main.ts` (mount,
/// then boot), `Shell.svelte` (placement only), `host.svelte.ts` (the
/// only file that asks the bus — through the engine, src/engine.ts),
/// and `<id>.html` at the web root.

import { onNewerBuild } from "../versions";
import { previousClock, serviceClock } from "../clock";
import { available, readJournal, startJournal, type CapId } from "../engine";
import { answers, type AnswerNode, type FrameState, type Request } from "./answers";

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

/// The agency's zone: the manifest's `timezone` (readAgencyZone, at
/// boot, before the page picks the network in force on its date), or a
/// loaded zip's (setAgencyZone). Unknown, the clock reads the device's.
let agencyZone: string | undefined;

/// The deployment's manifest, read once per page.
let manifestRead: Promise<Record<string, unknown> | null> | null = null;
export function deploymentManifest<T>(): Promise<T | null> {
  manifestRead ??= fetch("manifest")
    .then((r) => (r.ok ? r.json() : null))
    .catch(() => null);
  return manifestRead as Promise<T | null>;
}

export async function readAgencyZone(): Promise<void> {
  const manifest = await deploymentManifest<{ timezone?: string }>();
  if (manifest?.timezone) agencyZone = manifest.timezone;
}

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
export function whileVisible(ms: number, tick: () => void): void {
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
  return `Waiting for ${state.missing}.`;
}

export function answerGraph(nodes: AnswerNode[], frames: Record<string, FrameState>, onerror: (e: unknown) => void): Request {
  return answers(nodes, { frames, onerror, available: (cap) => available(cap as CapId) });
}

// ---------------------------------------------------------------------
// the app's build — ONE lifecycle for updates and offline (D-63, D-65):
// the service worker (web/service-worker.js, written to dist/sw.js by
// the build) precaches a new build in the background and leaves it
// waiting; the page decides when it applies. A rider's page applies it
// at the next cold start or navigation (`installApp`) or when the
// rider accepts it; a display at its next poll while the server
// answers. Never over unsaved device data — the host says what that
// is.
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
  void readBuild();
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

/// Whether the deployment's server answers now (not the worker's last
/// copy) — a display applies a build only then, so an outage never
/// reloads a working display into an empty one.
export async function serverAnswers(): Promise<boolean> {
  const resp = await fetch("manifest", { cache: "no-cache" }).catch(() => null);
  return !!resp?.ok && resp.headers.get("X-Served-From") !== "last-copy";
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

/// The deployment's feed URLs (and whether a dev relay fetches them);
/// {} when the page has no config.
export async function deploymentFeeds(): Promise<Partial<LiveUrls>> {
  try {
    const resp = await fetch("live-feeds.json");
    if (!resp.ok) return {};
    const config = (await resp.json()) as Partial<LiveUrls> & { proxy?: string };
    feedProxy = typeof config.proxy === "string" && config.proxy ? config.proxy : null;
    const urls: Partial<LiveUrls> = {};
    for (const k of ["trips", "vehicles", "alerts"] as const) if (typeof config[k] === "string") urls[k] = config[k];
    return urls;
  } catch {
    return {};
  }
}

/// Fetch one live feed: directly, or through the dev relay when the
/// config names one.
export function fetchFeed(url: string): Promise<Response> {
  return fetch(feedProxy ? `${feedProxy}?url=${encodeURIComponent(url)}` : url);
}

/// A sync-server answer served from its cache while its store does not
/// answer (D-119) carries X-Store-Stale and X-Store-As-Of: the label a
/// page shows for it — "Detours as of 4:05 PM …" — else "".
export function staleLabel(resp: Response, what: string): string {
  if (resp.headers.get("X-Store-Stale") !== "true") return "";
  const asOf = Number(resp.headers.get("X-Store-As-Of"));
  const when = Number.isFinite(asOf) && asOf > 0 ? ` as of ${clockLabelAt(asOf)}` : "";
  return `${what}${when} — the server's store is not answering.`;
}
