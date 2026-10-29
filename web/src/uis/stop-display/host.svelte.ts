/// The stop display's host layer — the ONLY file of this UI that calls
/// the capability bus. It owns I/O (the published feed, the agency's
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
/// A capability the bus cannot assemble turns its frames OFF (`off`),
/// and they say so — nothing here answers in its place.

import {
  boardCap,
  boardModule,
  directoryCap,
  directoryModule,
  networkCap,
  networkModule,
  rtCap,
  rtModule,
  type Live,
} from "../../capabilities.svelte";
import { answer, answerGraph, hashIn, rememberHash, today, type CapModule } from "../kit";
import type { Boundary } from "../../types/Boundary";
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { EditRecord } from "../../types/EditRecord";
import type { GroupedBoard } from "../../types/GroupedBoard";
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
  live: null as Live | null,
  liveAt: "",
  liveError: "",
  /// The host's clock, seconds since local midnight, for the header.
  nowSecs: nowSecs(),
});

// ---------------------------------------------------------------------
// documents — the network (imported once), the edit document (the sync
// server's), the live snapshots (decoded per poll)
// ---------------------------------------------------------------------

let network: Network | null = null;
let boundary: Boundary | null = null;
let edits: EditRecord[] = [];
let editsText = "[]";
let urls: LiveUrls = { trips: "", vehicles: "", alerts: "" };
const snapshots = { trips: null as string | null, vehicles: null as string | null, alerts: null as string | null };
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

/// The edit document registered once per version (null when empty —
/// schedule-only, the day views' contract), and the live document
/// registered once (its identity asserted equal to rt's).
const editRegs = new WeakMap<EditRecord[], string>();
function editsIn(module: CapModule): string | null {
  if (!network || edits.length === 0) return null;
  let hash = editRegs.get(edits);
  if (!hash) editRegs.set(edits, (hash = boardCap(module).loadEdits(hashIn(module, network), edits).editsHash));
  return hash;
}

const liveSeen = new Set<string>();
function liveIn(module: CapModule): string | null {
  const live = display.live;
  if (!live) return null;
  if (!liveSeen.has(live.liveHash)) {
    const hash = boardCap(module).loadLive(live.live);
    if (hash !== live.liveHash) throw new Error(`live identity disagrees across capabilities: ${hash} != ${live.liveHash}`);
    liveSeen.add(hash);
  }
  return live.liveHash;
}

let boundaryHash: string | null = null;

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
        const directory = await directoryModule();
        display.off.directory = !directory;
        if (!network || !directory) return;
        const hash = hashIn(directory, network);
        display.routes = answer(directory, "routes", [hash], () => directoryCap(directory).routes(hash));
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
        const directory = await directoryModule();
        const text = query.trim();
        if (!network || !directory || !text) {
          display.found = null;
          return;
        }
        const hash = hashIn(directory, network);
        const stops = answer(directory, "stops", [hash, text, FOUND], () => directoryCap(directory).stops(hash, FOUND, text));
        display.found = { text, places: stops.map((s) => ({ id: s.stop_id, name: s.name })) };
      },
    },
    {
      // Which stop this is, as the directory names it.
      id: "directory.stops_by_ids",
      frame: "DisplayHeader",
      needs: ["network", "stop"],
      run: async () => {
        const directory = await directoryModule();
        display.off.directory = !directory;
        if (!network || !directory || !stopId) {
          display.stop = null;
          return;
        }
        const hash = hashIn(directory, network);
        const id = stopId;
        const [stop] = answer(directory, "stops_by_ids", [hash, id], () => directoryCap(directory).stopsByIds(hash, [id]));
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
        const rt = await rtModule();
        display.off.rt = !rt;
        const { trips, vehicles, alerts } = snapshots;
        if (!network || !boundary || !rt || (!trips && !vehicles && !alerts)) {
          display.live = null;
          return;
        }
        const cap = rtCap(rt);
        const networkHash = hashIn(rt, network);
        boundaryHash ??= cap.loadBoundary(boundary);
        const bh = boundaryHash;
        display.live = answer(rt, "live", [networkHash, bh, trips, vehicles, alerts], () => cap.live(networkHash, bh, trips, vehicles, alerts));
      },
    },
    {
      // The groups: each route and destination here, its next three,
      // how long until each leaves; the alerts and detours here.
      id: "board.grouped_board",
      frame: "DisplayGroup",
      needs: ["network", "edits", "rt.live", "stop", "clock"],
      run: async () => {
        const board = await boardModule();
        display.off.board = !board;
        if (!network || !board || !stopId) {
          display.board = null;
          return;
        }
        try {
          const args = [hashIn(board, network), editsIn(board), stopId, today(), nowSecs(), null, liveIn(board)] as const;
          display.board = answer(board, "grouped_board", [...args], () => boardCap(board).groupedBoard(...args));
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
  const resp = await fetch("/feed").catch(() => null);
  if (resp?.ok) {
    await loadFeedBytes(new Uint8Array(await resp.arrayBuffer()), "the agency's feed");
  } else {
    display.busy = false;
    display.status = "This server publishes no feed — load the agency's GTFS zip to begin.";
  }
  setInterval(() => void poll(), POLL_MS);
  // The header's clock reads the host's clock between polls.
  setInterval(() => (display.nowSecs = nowSecs()), 5_000);
}

/// The published feed becomes the network document (and its boundary,
/// the stop_sequence numbering the live feeds name).
export async function loadFeedBytes(bytes: Uint8Array, label: string): Promise<void> {
  display.busy = true;
  display.status = `Loading ${label}…`;
  await new Promise((r) => setTimeout(r, 20));
  try {
    const module = await networkModule();
    if (!module) {
      display.status = "The network capability is unavailable — nothing can be answered without it.";
      return;
    }
    const cap = networkCap(module);
    const imported = cap.import(cap.loadFeed(bytes).feedHash);
    rememberHash(module, imported.network, imported.networkHash);
    network = imported.network;
    boundary = imported.boundary;
    display.feed = network.name || label;
    display.status = "";
    await Promise.all([syncEdits(), fetchLive()]);
    await request("network", "edits", "snapshot", "clock", "query", "stop", "setup");
  } catch (e) {
    fail(e);
  } finally {
    display.busy = false;
  }
}

/// One tick: the edit document, the live feeds, and the clock.
export async function poll(): Promise<void> {
  if (!network || display.busy) return;
  display.nowSecs = nowSecs();
  const changed = ["clock"];
  const [e, l] = await Promise.all([syncEdits(), fetchLive()]);
  if (e) changed.push("edits");
  if (l) changed.push("snapshot");
  await request(...changed);
}

/// The sync server's edit document (detours), read-only. No sync
/// server, or an unreadable answer, keeps the last good document.
/// True when it changed.
async function syncEdits(): Promise<boolean> {
  try {
    const resp = await fetch("/edits");
    if (!resp.ok) return false;
    const text = await resp.text();
    const records = JSON.parse(text) as EditRecord[];
    if (text === editsText) return false;
    editsText = text;
    edits = Array.isArray(records) ? records : [];
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
  const rt = await rtModule();
  display.off.rt = !rt;
  if (!rt) return false;
  let changed = false;
  const errors: string[] = [];
  for (const kind of kinds) {
    try {
      const resp = await fetch(`/rt?url=${encodeURIComponent(urls[kind])}`);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      const hash = rtCap(rt).decode(new Uint8Array(await resp.arrayBuffer())).rtHash;
      if (hash !== snapshots[kind]) changed = true;
      snapshots[kind] = hash;
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
