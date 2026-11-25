// Only this file of the UI calls the bus. Every group, time, status, and alert
// that it writes into `display` is an op's answer, adopted as given. A
// capability that the bus cannot assemble turns its frames off, and nothing
// here answers in its place.

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

// Each document registers once in the page's one linked module.
assemble("stop-display");
import type { Boundary } from "../../types/Boundary";
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { EditDocument } from "../../types/EditDocument";
import type { GroupedBoard } from "../../types/GroupedBoard";
import type { LiveDay } from "../../types/LiveDay";
import type { Network } from "../../types/Network";

export type Layout = "center" | "route" | "routes";
export type Place = { id: string; name: string };

const LAYOUTS: Layout[] = ["center", "route", "routes"];
const BOARD_MS = 20_000;
const FACE_MS = 5_000;
const FOUND = 25;
/// A board whose op threw stays on screen, marked, for this long. Its waits were counted from when it was answered, so it is right only briefly.
const STALE_BOARD_MS = 2 * BOARD_MS;
/// The first module that assembles checks the network's pin.
const NETWORK_READERS = ["directory", "board", "rt"] as const;

export const display = $state({
  status: "",
  busy: true,
  feed: "",
  frames: {} as Record<string, FrameState>,
  inputs: {} as Record<string, InputState>,
  layout: "routes" as Layout,
  stop: null as Place | null,
  setup: true,
  found: null as { text: string; places: Place[] } | null,
  routes: [] as DirectoryRoute[],
  board: null as GroupedBoard | null,
  /// An answer that failed after the board was up shows beside the board, never instead of it.
  problem: "",
  nowSecs: nowSecs(),
});

type Served = { network: Doc<Network>; boundary: Doc<Boundary>; pin: string };

let manifest: DeploymentManifest | null = null;
let network: Doc<Network> | null = null;
let boundary: Doc<Boundary> | null = null;
let byHand: Served | null = null;
/// Null when empty. The day views then read the schedule only.
let edits: Doc<EditDocument> | null = null;
let live: Doc<LiveDay> | null = null;
let urls: Partial<LiveUrls> = {};
const snapshots = { trips: null as Doc | null, vehicles: null as Doc | null, alerts: null as Doc | null };
let stopId: string | null = null;
let query = "";

let boardAt = 0;

export function off(frame: string): boolean {
  return display.frames[frame]?.kind === "unassembled";
}

export function failure(frame: string): string {
  const state = display.frames[frame];
  return state?.kind === "failed" ? state.message : "";
}

export function label(input: string): string {
  return inputLabel(display.inputs[input]);
}

export function status(): string {
  if (display.status) return display.status;
  const state = display.inputs.network;
  if (!state || state.kind === "reading") return network ? "" : "Loading the agency's network…";
  if (state.kind === "none" && !network) return "This server publishes no network — load the agency's GTFS zip to begin.";
  if (state.kind === "failed" && !state.held && !network) return `${inputLabel(state)} This screen will try again.`;
  return "";
}

/// After the network is up, a failed answer sits beside the board. A display never blanks over one.
function fail(e: unknown): void {
  const message = `Something went wrong: ${(e as Error)?.message ?? e}`;
  if (network) display.problem = message;
  else display.status = message;
  console.error(e);
}


const request = answerGraph(
  [
    {
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
        // The display never reads the live document. The board reads it by hash because rt created it in the linked module, so it never crosses to the page (QAF-49).
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
      id: "board.grouped_board",
      frame: "DisplayGroup",
      needs: ["network", "edits", "rt.live", "stop", "clock"],
      caps: ["board"],
      clear: () => (display.board = null),
      // The last board stays, marked, while its waits are still near right.
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

/// Documents not listed are dropped from every module (the engine's `keep`). The live documents and rt snapshots bound themselves between calls.
function release(): Promise<void> {
  return keep([network, boundary, edits, live, snapshots.trips, snapshots.vehicles, snapshots.alerts]);
}

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
      // The agency's zone is set before the network in force on its date is picked.
      setAgencyZone(m?.timezone);
    },
  },
  "live-feeds": {
    read: ({ signal }) => readLiveFeeds(signal),
    adopt: (u: Partial<LiveUrls> | null) => void (urls = u ?? {}),
  },
  network: {
    // Follows the deployment (a republish, a new pick, today crossing a pick's date). Only a moved pin reads the network again.
    read: async ({ signal, held }: { signal: AbortSignal; held: Served | null }) => {
      if (byHand) return { value: byHand, key: byHand.pin };
      if (!manifest) throw new Error("the deployment's manifest is not read");
      const pin = await servedNetworkPin(today(), { manifest, signal });
      if (!pin) return null;
      if (held?.pin === pin) return { value: held, key: pin };
      const served = await servedNetwork(today(), [...NETWORK_READERS], { manifest, signal });
      if (!served) return null;
      // A deployment that names its GTFS URL: the zip's zone.
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
      // The directory answers the name, because a published network is bytes that this page never parsed.
      const about = network && (await available("directory")) ? await ask("directory", "about", { network_hash: network }).catch(() => null) : null;
      display.feed = network ? about?.name || network.value?.name || "the agency's network" : "";
      display.status = "";
      await release();
    },
  },
  edits: {
    // Read-only: this page never writes edits. The modules refuse any version they do not read. A server with no sync answers 404.
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

export async function boot(): Promise<void> {
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
  clockEvery(BOARD_MS, () => {
    display.nowSecs = nowSecs();
    if (applyWaitingBuild()) return;
    void request("clock");
  });
  clockEvery(FACE_MS, () => (display.nowSecs = nowSecs()));
}

/// The boundary document holds the stop_sequence numbering that the live feeds name.
export async function loadFeedBytes(bytes: Uint8Array, label: string): Promise<void> {
  display.busy = true;
  display.status = `Loading ${label}…`;
  try {
    // This is the only time that the page assembles the network module.
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

/// Nobody is at a display to tap, so a waiting build applies itself. It applies only while the server answers (an outage never reloads a working display into an empty one) and never while the setup is open. It returns true when it applies. The page then reads nothing more, because the old build's worker hands over only when no request is in flight.
function applyWaitingBuild(): boolean {
  const m = display.inputs.manifest;
  const answers = m?.kind === "answered" && m.asOfMs == null;
  if (display.setup || !answers || !buildWaiting()) return false;
  inputs.stop();
  applyBuild();
  return true;
}

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
