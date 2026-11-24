/// The stop display's host layer — the ONLY file of this UI that calls
/// the capability bus. It owns I/O, the clock, and the selection (the
/// stop and the layout); every group, time, status, and alert it writes
/// into `display` is an op's answer, adopted as-is.
///
/// The questions it answers (its ui.json): which
/// stop this is, what leaves next on each route to each destination
/// and how long until it does, live or scheduled, from which bay, which
/// routes have stopped for the day, and the alerts and detours here.
///
/// It asks every question through the engine (src/engine.ts) and holds
/// documents, never modules or hashes. Its inputs are declared in its
/// ui.json and read by the kit (src/uis/inputs.ts): the deployment's
/// manifest, the network it pins, the edit document, and the live
/// feeds. Each input's state (`display.inputs`) says what the page
/// holds of it. The answer graph keeps why a frame has no answer
/// (`display.frames`; src/uis/answers.ts): a capability the bus cannot
/// assemble turns its frames off, and they say so. Nothing here
/// answers in its place.

import { ask, assemble, available, keep, load, produced, readManifest, servedNetwork, servedNetworkPin, together, Doc, type DeploymentManifest } from "../../engine";
import {
  answerGraph,
  applyBuild,
  buildWaiting,
  clockEvery,
  inputGraph,
  inputLabel,
  installApp,
  nowSecs,
  previousDay,
  readFeed,
  readHead,
  readLiveFeeds,
  setAgencyZone,
  today,
  type FrameState,
  type InputDeclaration,
  type InputState,
  type LiveUrls,
  type Reader,
} from "../kit";
import descriptor from "./ui.json";

// This page's capabilities answer inside its linked assembly — one
// module, one registry: each document registers once (D-77).
assemble("stop-display");
import type { Boundary } from "../../types/Boundary";
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { EditDocument } from "../../types/EditDocument";
import type { GroupedBoard } from "../../types/GroupedBoard";
import type { LiveDay } from "../../types/LiveDay";
import type { Network } from "../../types/Network";

/// The three layouts — placements of one answer, any of them at any
/// stop: a transit center's bays, one route large, several routes.
export type Layout = "center" | "route" | "routes";
export type Place = { id: string; name: string };

const LAYOUTS: Layout[] = ["center", "route", "routes"];
/// How often the board answers again on the clock, and the header's
/// clock moves.
const BOARD_MS = 20_000;
const FACE_MS = 5_000;
const FOUND = 25;
/// How long a board whose op threw stays on screen (marked) before the
/// error replaces it: its waits were counted from the moment it was
/// answered, so it is right only briefly.
const STALE_BOARD_MS = 2 * BOARD_MS;
/// The modules that read the network, in order: its pin is checked in
/// the first that assembles.
const NETWORK_READERS = ["directory", "board", "rt"] as const;

export const display = $state({
  /// A message that replaces the page's answers (a failed boot step).
  status: "",
  busy: true,
  feed: "",
  /// Each frame that shows no answer, and why (the answer graph's).
  frames: {} as Record<string, FrameState>,
  /// What the page holds of each input (the kit's).
  inputs: {} as Record<string, InputState>,
  layout: "routes" as Layout,
  /// The stop on screen, as the directory names it; null shows the
  /// setup.
  stop: null as Place | null,
  setup: true,
  found: null as { text: string; places: Place[] } | null,
  routes: [] as DirectoryRoute[],
  board: null as GroupedBoard | null,
  /// An answer that failed after the board was up: shown beside the
  /// board, never instead of it.
  problem: "",
  /// The agency's clock, seconds into the service day, for the header.
  nowSecs: nowSecs(),
});

// ---------------------------------------------------------------------
// documents — the manifest, the network it pins (or a zip loaded by
// hand), the edit document, the live snapshots
// ---------------------------------------------------------------------

type Served = { network: Doc<Network>; boundary: Doc<Boundary>; pin: string };

let manifest: DeploymentManifest | null = null;
let network: Doc<Network> | null = null;
let boundary: Doc<Boundary> | null = null;
/// A network loaded from a zip by hand: the page stops following the
/// deployment.
let byHand: Served | null = null;
/// The edit document over the network — null when empty (schedule
/// only, the day views' contract).
let edits: Doc<EditDocument> | null = null;
/// THE live document (rt's answer over the snapshots), by which every
/// board reads the live feeds.
let live: Doc<LiveDay> | null = null;
let urls: Partial<LiveUrls> = {};
/// The decoded live feeds, one document per read's bytes.
const snapshots = { trips: null as Doc | null, vehicles: null as Doc | null, alerts: null as Doc | null };
/// The stop asked for, by id (the URL's), until the directory names it.
let stopId: string | null = null;
let query = "";

/// When the board last answered (ms), for STALE_BOARD_MS.
let boardAt = 0;

/// Whether a frame's capability did not assemble: its component says so.
export function off(frame: string): boolean {
  return display.frames[frame]?.kind === "unassembled";
}

/// A frame's failure, as text ("" when it answered).
export function failure(frame: string): string {
  const state = display.frames[frame];
  return state?.kind === "failed" ? state.message : "";
}

/// What the page says of an input ("" when it is current).
export function label(input: string): string {
  return inputLabel(display.inputs[input]);
}

/// The page's status line: a failed step, else what it holds of the
/// schedule. "" when the schedule is current.
export function status(): string {
  if (display.status) return display.status;
  const state = display.inputs.network;
  if (!state || state.kind === "reading") return network ? "" : "Loading the agency's network…";
  if (state.kind === "none" && !network) return "This server publishes no network — load the agency's GTFS zip to begin.";
  if (state.kind === "failed" && !state.held && !network) return `${inputLabel(state)} This screen will try again.`;
  return "";
}

/// A failed answer. Before the network is up it is the page's status;
/// after, it sits beside the board — a display never blanks over one
/// failed answer.
function fail(e: unknown): void {
  const message = `Something went wrong: ${(e as Error)?.message ?? e}`;
  if (network) display.problem = message;
  else display.status = message;
  console.error(e);
}

// ---------------------------------------------------------------------
// This UI's answer graph (src/uis/kit.ts, "the answer graph").
// Inputs: "network",
// "edits" (the edit document), "snapshot" (the decoded live feeds),
// "clock" (the board's tick), "query" / "stop" (the selection),
// "setup" (whether the setup is on screen).
// ---------------------------------------------------------------------

const request = answerGraph(
  [
    {
      // The route names the alerts slot reads.
      id: "directory.routes",
      frame: "DisplayAlerts",
      needs: ["network"],
      caps: ["directory"],
      run: async () => {
        if (!network) return;
        display.routes = (await ask("directory", "routes", { network_hash: network })).routes;
      },
    },
    {
      // The setup's stop picker: every word typed in the name or id.
      id: "directory.stops?query",
      frame: "DisplaySetup",
      needs: ["network", "query", "setup"],
      visible: () => display.setup,
      caps: ["directory"],
      clear: () => (display.found = null),
      run: async () => {
        const text = query.trim();
        if (!network || !text) {
          display.found = null;
          return;
        }
        const { stops } = await ask("directory", "stops", { network_hash: network, limit: FOUND, query: text });
        display.found = { text, places: stops.map((s) => ({ id: s.stop_id, name: s.name })) };
      },
    },
    {
      // Which stop this is, as the directory names it.
      id: "directory.stops_by_ids",
      frame: "DisplayHeader",
      needs: ["network", "stop"],
      caps: ["directory"],
      clear: () => (display.stop = null),
      run: async () => {
        if (!network || !stopId) {
          display.stop = null;
          return;
        }
        const id = stopId;
        const [stop] = (await ask("directory", "stops_by_ids", { network_hash: network, ids: [id] })).stops;
        display.stop = stop ? { id: stop.stop_id, name: stop.name } : { id, name: id };
      },
    },
    {
      // THE live document over whichever feeds were fetched; the
      // board reads it by hash.
      id: "rt.live",
      frame: "Live",
      needs: ["network", "snapshot"],
      caps: ["rt"],
      clear: () => (live = null),
      run: async () => {
        const { trips, vehicles, alerts } = snapshots;
        if (!network || !boundary || (!trips && !vehicles && !alerts)) {
          live = null;
          return;
        }
        // The display never reads the live document: linked beside rt,
        // the board reads it by hash where rt minted it, and it never
        // crosses to the page (QAF-49).
        const args = {
          network_hash: network,
          boundary_hash: boundary,
          rt_hash: trips ?? undefined,
          vp_hash: vehicles ?? undefined,
          alerts_hash: alerts ?? undefined,
          with_document: !(await together("rt", "board")),
        };
        const answer = await ask("rt", "live", args);
        live = produced("live", answer.live ?? undefined, "rt", answer.live_hash, { remint: { op: "live", args } });
      },
    },
    {
      // The groups: each route and destination here, its next three,
      // how long until each leaves; the alerts and detours here.
      id: "board.grouped_board",
      frame: "DisplayGroup",
      needs: ["network", "edits", "rt.live", "stop", "clock"],
      caps: ["board"],
      clear: () => (display.board = null),
      // The last board stays, marked, while its waits are still near
      // right; then the error replaces it.
      keepOnFailure: true,
      run: async () => {
        if (!network || !stopId) {
          display.board = null;
          return;
        }
        try {
          const board = (
            await ask("board", "grouped_board", {
              network_hash: network,
              edits_hash: edits ?? undefined,
              stop_id: stopId,
              date: today(),
              from_secs: nowSecs(),
              // The previous date's after-midnight trips still leaving.
              previous: previousDay(),
              live_hash: live ?? undefined,
            })
          ).board;
          display.board = board;
          display.problem = "";
          boardAt = Date.now();
        } catch (e) {
          if (Date.now() - boardAt > STALE_BOARD_MS) display.board = null;
          throw e;
        }
      },
    },
  ],
  display.frames,
  fail,
  display.inputs,
);

// ---------------------------------------------------------------------
// The inputs (its ui.json's `inputs`, read by the kit): each reader
// answers its document, null for none, or throws when the read fails.
// ---------------------------------------------------------------------

/// Name what the modules keep: the documents this page holds now —
/// everything else (a replaced network, a superseded edit document)
/// is dropped from every module (the engine's `keep`). The live
/// documents and rt snapshots bound themselves between calls.
function release(): Promise<void> {
  return keep([network, boundary, edits, live, snapshots.trips, snapshots.vehicles, snapshots.alerts]);
}

/// One live feed: its bytes decoded into a snapshot (cap-rt registers
/// it), keyed by its hash. None when the deployment names no such feed
/// or rt did not assemble.
function liveFeed(kind: keyof typeof snapshots): Reader<Doc> {
  return {
    read: async ({ signal }) => {
      const url = urls[kind];
      if (!url || !(await available("rt"))) return null;
      const snapshot = new Doc("rt", { bytes: await readFeed(url, signal) });
      const { hash } = await load("rt", snapshot);
      return { value: snapshot, key: hash };
    },
    adopt: (snapshot) => void (snapshots[kind] = snapshot),
  };
}

const readers: Record<string, Reader<any>> = {
  manifest: {
    read: async ({ signal }) => {
      const read = await readManifest(signal);
      return { value: read.manifest, key: JSON.stringify(read.manifest), asOfMs: read.asOfMs ?? undefined, why: read.why ?? undefined };
    },
    adopt: (m: DeploymentManifest | null) => {
      manifest = m;
      // The agency's zone, before the network in force on its date is
      // picked.
      setAgencyZone(m?.timezone);
    },
  },
  "live-feeds": {
    read: ({ signal }) => readLiveFeeds(signal),
    adopt: (u: Partial<LiveUrls> | null) => void (urls = u ?? {}),
  },
  network: {
    // The deployment's network: the pick in force today, else its
    // import. It follows the deployment as it moves (a republish, a new
    // pick, today crossing a pick's date; D-63): the pin is checked on
    // each read, and only a moved pin reads the network again.
    read: async ({ signal, held }: { signal: AbortSignal; held: Served | null }) => {
      if (byHand) return { value: byHand, key: byHand.pin };
      if (!manifest) throw new Error("the deployment's manifest is not read");
      const pin = await servedNetworkPin(today(), { manifest, signal });
      if (!pin) return null;
      if (held?.pin === pin) return { value: held, key: pin };
      const served = await servedNetwork(today(), [...NETWORK_READERS], { manifest, signal });
      if (!served) return null;
      // A deployment that names its GTFS URL: the zip's zone (D-132).
      setAgencyZone(served.timezone);
      // The pin of an imported zip is the network the page now holds.
      const now = (await servedNetworkPin(today(), { manifest, signal })) ?? pin;
      return { value: { network: served.network, boundary: served.boundary, pin: now }, key: now };
    },
    adopt: async (got: Served | null) => {
      network = got?.network ?? null;
      boundary = got?.boundary ?? null;
      // The zone may have just become known (a loaded zip's).
      display.nowSecs = nowSecs();
      // The heading: the network's name, which the directory answers
      // (a published network is bytes this page never parsed).
      const about = network && (await available("directory")) ? await ask("directory", "about", { network_hash: network }).catch(() => null) : null;
      display.feed = network ? about?.name || network.value?.name || "the agency's network" : "";
      display.status = "";
      await release();
    },
  },
  edits: {
    // The edit document (detours), read-only, over the network the page
    // holds: a new network reads it again. The modules refuse any
    // version they do not read. No sync server keeps none (404).
    read: async ({ signal }) => {
      if (!network) return null;
      const got = await readHead("edits", signal, [404]);
      if (!got) return null;
      let document: EditDocument;
      try {
        document = JSON.parse(got.value) as EditDocument;
      } catch {
        throw new Error("the edit document is not JSON");
      }
      return { ...got, value: { document, over: network }, key: `${network.hash ?? network.pin}:${got.key}` };
    },
    adopt: async (got: { document: EditDocument; over: Doc<Network> } | null) => {
      edits = got && got.document?.records?.length > 0 ? new Doc("edits", { value: got.document }, { over: got.over }) : null;
      await release();
    },
  },
  trips: liveFeed("trips"),
  vehicles: liveFeed("vehicles"),
  alerts: liveFeed("alerts"),
};

const inputs = inputGraph(descriptor.inputs as InputDeclaration[], readers, display.inputs, request);

// ---------------------------------------------------------------------
// boot, and the clock
// ---------------------------------------------------------------------

export async function boot(): Promise<void> {
  // The app's build: a build that waited for this load applies now.
  void installApp();
  readUrl();
  try {
    await inputs.start();
    await request("clock", "query", "stop", "setup");
  } catch (e) {
    fail(e);
  } finally {
    display.busy = false;
  }
  // The board answers again on the clock; the header's clock moves
  // between.
  clockEvery(BOARD_MS, () => {
    display.nowSecs = nowSecs();
    if (applyWaitingBuild()) return;
    void request("clock");
  });
  clockEvery(FACE_MS, () => (display.nowSecs = nowSecs()));
}

/// A GTFS zip loaded here becomes the network document (and its
/// boundary, the stop_sequence numbering the live feeds name). The
/// page stops following the deployment.
export async function loadFeedBytes(bytes: Uint8Array, label: string): Promise<void> {
  display.busy = true;
  display.status = `Loading ${label}…`;
  try {
    // The import is the network module's: the only time this page
    // assembles it.
    if (!(await available("network"))) {
      display.status = "The network capability is unavailable — a GTFS zip cannot be imported without it.";
      return;
    }
    const feed = new Doc("feed", { bytes });
    const imported = await ask("network", "import", { feed_hash: feed });
    setAgencyZone(imported.timezone);
    byHand = {
      network: produced("network", imported.network, "network", imported.network_hash),
      boundary: new Doc<Boundary>("boundary", { value: imported.boundary }),
      pin: `zip:${imported.network_hash}`,
    };
    display.status = "";
    await inputs.read("network", "edits");
  } catch (e) {
    fail(e);
  } finally {
    display.busy = false;
  }
}

/// A new build waits (D-63): nobody is at a display to tap, so it
/// applies itself — only while the server answers (an outage never
/// reloads a working display into an empty one: the manifest's last
/// read answered, and not from a working copy), and never while the
/// setup is open (someone is configuring the screen). True when it
/// applies. The page then reads nothing more: the old build's worker
/// hands over only when no request is in flight, and the page reloads
/// into the new build.
function applyWaitingBuild(): boolean {
  const m = display.inputs.manifest;
  const answers = m?.kind === "answered" && m.asOfMs == null;
  if (display.setup || !answers || !buildWaiting()) return false;
  inputs.stop();
  applyBuild();
  return true;
}

// ---------------------------------------------------------------------
// the selection — the stop and the layout, kept in the page's URL so a
// screen reopens on its stop
// ---------------------------------------------------------------------

function readUrl(): void {
  const params = new URLSearchParams(location.search);
  const layout = params.get("layout") as Layout | null;
  if (layout && LAYOUTS.includes(layout)) display.layout = layout;
  stopId = params.get("stop");
  display.setup = !stopId;
}

// A configured screen's URL is persisted data (docs/FORMATS.md,
// "Versions"): a parameter this build does not know is kept.
function writeUrl(): void {
  const params = new URLSearchParams(location.search);
  if (stopId) params.set("stop", stopId);
  else params.delete("stop");
  params.set("layout", display.layout);
  history.replaceState(null, "", `${location.pathname}?${params}`);
}

export function typed(text: string): Promise<void> {
  query = text;
  return request("query");
}

export function setLayout(layout: Layout): void {
  display.layout = layout;
  writeUrl();
}

export function pickStop(place: Place): Promise<void> {
  stopId = place.id;
  display.stop = place;
  display.board = null;
  display.setup = false;
  query = "";
  writeUrl();
  return request("stop", "query", "setup");
}

export function openSetup(): Promise<void> {
  display.setup = true;
  return request("setup");
}

export function closeSetup(): Promise<void> {
  display.setup = !stopId;
  return request("setup");
}
