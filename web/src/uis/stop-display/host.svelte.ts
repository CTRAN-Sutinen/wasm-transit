/// The stop display's host layer — the ONLY file of this UI that calls
/// the capability bus. It owns I/O (the published network, the agency's
/// live feeds through the server's /rt proxy, the sync server's edit
/// document, the page's URL), the clock, and the selection (the stop
/// and the layout); every group, time, status, and alert it writes
/// into `display` is an op's answer, adopted as-is.
///
/// The questions it answers (docs/units/ui-stop-display.md): which
/// stop this is, what leaves next on each route to each destination
/// and how long until it does, live or scheduled, from which bay, which
/// routes have stopped for the day, and the alerts and detours here.
///
/// It asks every question through the engine (src/engine.ts) and holds
/// documents, never modules or hashes. A capability the bus cannot
/// assemble turns its frames OFF (`off`), and they say so — nothing
/// here answers in its place.

import { ask, assemble, available, keep, load, produced, servedNetwork, together, Doc } from "../../engine";
import { answerGraph, today } from "../kit";

// This page's capabilities answer inside its linked assembly — one
// module, one registry: each document registers once (D-77).
assemble("stop-display");
import type { Boundary } from "../../types/Boundary";
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { EditRecord } from "../../types/EditRecord";
import type { GroupedBoard } from "../../types/GroupedBoard";
import type { LiveDay } from "../../types/LiveDay";
import type { Network } from "../../types/Network";

/// The three layouts — placements of one answer, any of them at any
/// stop: a transit center's bays, one route large, several routes.
export type Layout = "center" | "route" | "routes";
export type Place = { id: string; name: string };
type LiveUrls = { trips: string; vehicles: string; alerts: string };

const LAYOUTS: Layout[] = ["center", "route", "routes"];
/// How often the host re-reads the live feeds, the edit document, and
/// the clock (one timer: the polled input,
/// docs/units/host-discipline.md).
const POLL_MS = 20_000;
const FOUND = 25;

export const display = $state({
  status: "Loading the agency's feed…",
  busy: true,
  feed: "",
  /// Frames whose capability the bus could not assemble.
  off: { directory: false, board: false, rt: false },
  layout: "routes" as Layout,
  /// The stop on screen, as the directory names it; null shows the
  /// setup.
  stop: null as Place | null,
  setup: true,
  found: null as { text: string; places: Place[] } | null,
  routes: [] as DirectoryRoute[],
  board: null as GroupedBoard | null,
  boardError: "",
  /// Whether the live document answered (the header's live mark).
  live: false,
  liveAt: "",
  liveError: "",
  /// The host's clock, seconds since local midnight, for the header.
  nowSecs: nowSecs(),
});

// ---------------------------------------------------------------------
// documents — the network (imported once), the edit document (the sync
// server's), the live snapshots (decoded per poll)
// ---------------------------------------------------------------------

let network: Doc<Network> | null = null;
let boundary: Doc<Boundary> | null = null;
/// The edit document over the network — null when empty (schedule
/// only, the day views' contract).
let edits: Doc<EditRecord[]> | null = null;
let editsText = "[]";
/// THE live document (rt's answer over the snapshots), by which every
/// board reads the live feeds.
let live: Doc<LiveDay> | null = null;
let urls: LiveUrls = { trips: "", vehicles: "", alerts: "" };
/// The decoded live feeds, one document per poll's bytes.
const snapshots = { trips: null as Doc | null, vehicles: null as Doc | null, alerts: null as Doc | null };
/// The stop asked for, by id (the URL's), until the directory names it.
let stopId: string | null = null;
let query = "";

/// The clock: seconds since local midnight. Hosts own the clock; ops
/// take it as arguments.
function nowSecs(): number {
  const d = new Date();
  return d.getHours() * 3600 + d.getMinutes() * 60 + d.getSeconds();
}

function clockLabel(): string {
  return new Date().toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function fail(e: unknown): void {
  display.status = `Something went wrong: ${(e as Error)?.message ?? e}`;
  console.error(e);
}

// ---------------------------------------------------------------------
// TRANSIENT — this UI's answer graph (src/uis/kit.ts, "the answer
// graph"): the first draft of its flow map. Inputs: "network",
// "edits" (the sync server's edit document), "snapshot" (the decoded
// live feeds), "clock" (the poll's tick), "query" / "stop" (the
// selection), "setup" (whether the setup is on screen).
// ---------------------------------------------------------------------

const request = answerGraph(
  [
    {
      // The route names the alerts slot reads.
      id: "directory.routes",
      frame: "DisplayAlerts",
      needs: ["network"],
      run: async () => {
        display.off.directory = !(await available("directory"));
        if (!network || display.off.directory) return;
        display.routes = (await ask("directory", "routes", { network_hash: network })).routes;
      },
    },
    {
      // The setup's stop picker: every word typed in the name or id.
      id: "directory.stops?query",
      frame: "DisplaySetup",
      needs: ["network", "query", "setup"],
      visible: () => display.setup,
      hide: () => {
        display.found = null;
      },
      run: async () => {
        const text = query.trim();
        if (!network || !text || !(await available("directory"))) {
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
      run: async () => {
        display.off.directory = !(await available("directory"));
        if (!network || display.off.directory || !stopId) {
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
      frame: "DisplayHeader",
      needs: ["network", "snapshot"],
      run: async () => {
        display.off.rt = !(await available("rt"));
        const { trips, vehicles, alerts } = snapshots;
        if (!network || !boundary || display.off.rt || (!trips && !vehicles && !alerts)) {
          live = null;
          display.live = false;
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
        display.live = true;
      },
    },
    {
      // The groups: each route and destination here, its next three,
      // how long until each leaves; the alerts and detours here.
      id: "board.grouped_board",
      frame: "DisplayGroup",
      needs: ["network", "edits", "rt.live", "stop", "clock"],
      run: async () => {
        display.off.board = !(await available("board"));
        if (!network || display.off.board || !stopId) {
          display.board = null;
          return;
        }
        try {
          display.board = (
            await ask("board", "grouped_board", {
              network_hash: network,
              edits_hash: edits ?? undefined,
              stop_id: stopId,
              date: today(),
              from_secs: nowSecs(),
              live_hash: live ?? undefined,
            })
          ).board;
          display.boardError = "";
        } catch (e) {
          console.error(e);
          display.board = null;
          display.boardError = (e as Error).message ?? String(e);
        }
      },
    },
  ],
  fail,
);

// ---------------------------------------------------------------------
// boot, the feed, and the poll
// ---------------------------------------------------------------------

export async function boot(): Promise<void> {
  readUrl();
  await restoreUrls();
  await adoptNetwork("the agency's network", async () => {
    // The pick in force today when the deployment publishes picks,
    // else its import. Its pin is checked in a module this page reads it with (the
    // directory answers the first question; the board and rt when the
    // directory did not assemble): none holds a copy only to check it.
    const served = await servedNetwork(today(), "directory", "board", "rt");
    if (served) return served;
    display.status = "This server publishes no network — load the agency's GTFS zip to begin.";
    return null;
  });
  setInterval(() => void poll(), POLL_MS);
  // The header's clock reads the host's clock between polls.
  setInterval(() => (display.nowSecs = nowSecs()), 5_000);
}

/// A GTFS zip loaded here becomes the network document (and its
/// boundary, the stop_sequence numbering the live feeds name).
export function loadFeedBytes(bytes: Uint8Array, label: string): Promise<void> {
  return adoptNetwork(label, async () => {
    // The import is the network module's: the only time this page
    // assembles it.
    if (!(await available("network"))) {
      display.status = "The network capability is unavailable — a GTFS zip cannot be imported without it.";
      return null;
    }
    const feed = new Doc("feed", { bytes });
    const imported = await ask("network", "import", { feed_hash: feed });
    return {
      network: produced("network", imported.network, "network", imported.network_hash),
      boundary: new Doc<Boundary>("boundary", { value: imported.boundary }),
    };
  });
}

/// Adopt the network (and boundary) `obtain` answers, then answer
/// everything over it.
async function adoptNetwork(
  label: string,
  obtain: () => Promise<{ network: Doc<Network>; boundary: Doc<Boundary> } | null>,
): Promise<void> {
  display.busy = true;
  display.status = `Loading ${label}…`;
  await new Promise((r) => setTimeout(r, 20));
  try {
    const got = await obtain();
    if (!got) return;
    network = got.network;
    boundary = got.boundary;
    edits = null;
    editsText = "[]";
    // The heading: the network's name, which the directory answers
    // (a published network is bytes this page never parsed).
    const about = (await available("directory")) ? await ask("directory", "about", { network_hash: network }) : null;
    display.feed = about?.name || network.value?.name || label;
    display.status = "";
    await Promise.all([syncEdits(), fetchLive()]);
    await release();
    await request("network", "edits", "snapshot", "clock", "query", "stop", "setup");
  } catch (e) {
    fail(e);
  } finally {
    display.busy = false;
  }
}

/// Name what the modules keep: the documents this page holds now —
/// everything else (a replaced network, a superseded edit document)
/// is dropped from every module (the engine's `keep`). The live
/// documents and rt snapshots bound themselves between calls.
function release(): Promise<void> {
  return keep([network, boundary, edits, live, snapshots.trips, snapshots.vehicles, snapshots.alerts]);
}

/// One tick: the edit document, the live feeds, and the clock.
export async function poll(): Promise<void> {
  if (!network || display.busy) return;
  display.nowSecs = nowSecs();
  const changed = ["clock"];
  const [e, l] = await Promise.all([syncEdits(), fetchLive()]);
  if (e) {
    changed.push("edits");
    await release();
  }
  if (l) changed.push("snapshot");
  await request(...changed);
}

/// The sync server's edit document (detours), read-only. No sync
/// server, or an unreadable answer, keeps the last good document.
/// True when it changed.
async function syncEdits(): Promise<boolean> {
  if (!network) return false;
  try {
    const resp = await fetch("/edits");
    if (!resp.ok) return false;
    const text = await resp.text();
    const records = JSON.parse(text) as EditRecord[];
    if (text === editsText) return false;
    editsText = text;
    edits = Array.isArray(records) && records.length > 0 ? new Doc("edits", { value: records }, { over: network }) : null;
    return true;
  } catch (e) {
    console.warn("edit sync failed:", e);
    return false;
  }
}

/// Fetch each configured live feed through the server's proxy and
/// decode it (cap-rt registers the snapshot). A failed fetch keeps the
/// last good snapshot. True when any snapshot changed.
async function fetchLive(): Promise<boolean> {
  const kinds = (["trips", "vehicles", "alerts"] as const).filter((k) => urls[k]);
  if (kinds.length === 0) return false;
  display.off.rt = !(await available("rt"));
  if (display.off.rt) return false;
  let changed = false;
  const errors: string[] = [];
  for (const kind of kinds) {
    try {
      const resp = await fetch(`/rt?url=${encodeURIComponent(urls[kind])}`);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const snapshot = new Doc("gtfs_rt", { bytes: new Uint8Array(await resp.arrayBuffer()) });
      const { hash } = await load("rt", snapshot);
      if (hash !== snapshots[kind]?.hash) {
        changed = true;
        snapshots[kind] = snapshot;
      }
    } catch (e) {
      errors.push(`${kind}: ${(e as Error).message}`);
    }
  }
  display.liveError = errors.join("; ");
  if (errors.length < kinds.length) display.liveAt = clockLabel();
  return changed;
}

/// The deployment's live feeds (live-feeds.json beside the page).
async function restoreUrls(): Promise<void> {
  try {
    const resp = await fetch("live-feeds.json");
    if (resp.ok) urls = { ...urls, ...((await resp.json()) as Partial<LiveUrls>) };
  } catch {
    // no deployment config: the schedule
  }
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

function writeUrl(): void {
  const params = new URLSearchParams();
  if (stopId) params.set("stop", stopId);
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
