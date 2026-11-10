/// The generated UIs' shared host glue — what every UI's host layer
/// needs and none should rewrite. Host layer only: shells and
/// components never import this.
///
/// A generated UI lives in `src/uis/<id>/` (docs/ARCHITECTURE.md,
/// "Generated UIs"): `ui.json` (what it answers), `main.ts` (mount,
/// then boot), `Shell.svelte` (placement only), `host.svelte.ts` (the
/// only file that asks the bus — through the engine, src/engine.ts),
/// and `<id>.html` at the web root.

import { previousClock, serviceClock } from "../clock";

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
// TRANSIENT — the answer graph, hand-built ahead of flow maps.
//
// The engine (src/engine.ts) makes every op call;
// docs/units/host-discipline.md has it interpret flow maps next (data
// in the UI manifest, docs/units/wire-time-assembly.md). Until then,
// a host declares what it answers as nodes and this runner schedules
// them. It is shaped to be REPLACED by flow maps, not grown into them:
//
// - a node is what a flow-map entry will be — the inputs it reads,
//   the frame that renders it, when that frame is on screen, and
//   the op calls — so a host's node list is the first draft of its
//   flow map (the engine's answers are already cached by the
//   journal's key);
// - it knows nothing about transit. If a node name, a field,
//   or a rule of the domain appears in this section, it leaked.
//
// Do not add features here; add them to the unit.
// ---------------------------------------------------------------------

/// One answer a host keeps: an op (or a short run of ops over the
/// same inputs) and the host state it writes.
export type AnswerNode = {
  /// The op(s) it answers ("analysis.service_groups"), or
  /// "document.<name>" for a document read before any op sees it.
  id: string;
  /// The component that renders the answer ("" for a document read).
  frame: string;
  /// The inputs (selection keys, document keys) and earlier nodes'
  /// ids it reads: a change to any of them re-answers it. A node may
  /// only name nodes declared before it.
  needs: string[];
  /// False while its frame is off screen: the node is not answered,
  /// `hide` clears what it wrote, and its dependents count as
  /// changed. Name the visibility input in `needs` so showing the
  /// frame answers it.
  visible?: () => boolean;
  hide?: () => void;
  /// Answer, write the host state, and `touch` any input it repaired
  /// (a selection the new answer no longer holds) — later nodes that
  /// read that input re-answer in the same pass.
  run: (touch: (input: string) => void) => Promise<void>;
};

const yieldToPaint = () => new Promise<void>((r) => setTimeout(r, 0));

/// The runner: `request(...inputs)` marks inputs changed and answers
/// every node downstream of them, in declared order, yielding between
/// nodes so each frame paints as its answer lands. A request made
/// while a pass runs supersedes it: the pass stops at the next node
/// and one merged pass starts from the top (nodes already answered
/// hit `answer`'s cache). The returned promise settles when no
/// request is left.
export function answerGraph(nodes: AnswerNode[], onerror: (e: unknown) => void) {
  const declared = new Set<string>();
  const ids = new Set(nodes.map((n) => n.id));
  for (const node of nodes) {
    for (const need of node.needs) {
      if (ids.has(need) && !declared.has(need)) throw new Error(`answer node ${node.id} reads ${need} before it is declared`);
    }
    declared.add(node.id);
  }

  let pending = new Set<string>();
  let draining: Promise<void> | null = null;

  async function pass(changed: Set<string>): Promise<void> {
    for (const node of nodes) {
      if (!node.needs.some((n) => changed.has(n))) continue;
      if (node.visible && !node.visible()) {
        node.hide?.();
        changed.add(node.id);
        continue;
      }
      await yieldToPaint();
      if (pending.size) {
        for (const c of changed) pending.add(c);
        return;
      }
      try {
        await node.run((input) => changed.add(input));
      } catch (e) {
        onerror(e);
      }
      changed.add(node.id);
    }
  }

  return function request(...inputs: string[]): Promise<void> {
    for (const input of inputs) pending.add(input);
    draining ??= (async () => {
      try {
        while (pending.size) {
          const changed = pending;
          pending = new Set();
          await pass(changed);
        }
      } finally {
        draining = null;
      }
    })();
    return draining;
  };
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
