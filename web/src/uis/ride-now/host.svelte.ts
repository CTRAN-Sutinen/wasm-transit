/// The ride-now UI's host layer — the ONLY file of this UI that calls the
/// capability bus. It owns I/O (the published network, the agency's live
/// feeds through the server's /rt proxy, the sync server's edit
/// document, the watchlist document, the device's location, the
/// rider's recent stops), the clock, and the selection; every row,
/// time, status, line, and notice it writes into `ride` is an op's
/// answer, adopted as-is.
///
/// The questions it answers (docs/units/ui-ride-now.md): which stop is
/// mine (typed, near me, or tapped on the map), what leaves next from
/// it live, is my bus on time, how do I get there (now, at a time, by
/// a time), when is the last bus, is a detour or alert in my way, and
/// what leaves the stops I saved — and, around them, the network:
/// where every bus is now and where one is going, and what each route
/// is (its directions and their stops).
///
/// A capability the bus cannot assemble turns its frames OFF (`off`),
/// and they say so — nothing here answers in its place.

import {
  boardCap,
  boardModule,
  currentDocument,
  directoryCap,
  directoryModule,
  networkCap,
  networkModule,
  putDocument,
  restoreDocuments,
  routerCap,
  routerModule,
  rtCap,
  rtModule,
  type Live,
} from "../../capabilities.svelte";
import { answer, answerGraph, hashIn, rememberHash, servedNetwork, today, type CapModule } from "../kit";
import type { Boundary } from "../../types/Boundary";
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { DirectoryStop } from "../../types/cap_directory/DirectoryStop";
import type { EntityRow } from "../../types/EntityRow";
import type { RouteDetail } from "../../types/RouteDetail";
import type { TripProgress } from "../../types/TripProgress";
import type { EditRecord } from "../../types/EditRecord";
import type { EntityFocus } from "../../types/EntityFocus";
import type { Itinerary } from "../../types/Itinerary";
import type { LegPath } from "../../types/LegPath";
import type { Network } from "../../types/Network";
import type { StopBoard } from "../../types/StopBoard";
import type { WatchBoardStop } from "../../types/cap_board/WatchBoardStop";
import type { Watchlist } from "../../types/cap_board/Watchlist";

/// A stop as the directory answers it, wherever it was picked from
/// (typed, near a point, tapped on the map): enough to name it, put it
/// on the map, and ask its board.
export type Place = { id: string; name: string; lat?: number; lon?: number; distance_m?: number };
export type Field = "search" | "from" | "to";
export type View = "home" | "stop" | "trip" | "alerts" | "routes" | "route" | "bus";
/// A bus picked on the map, by the live document's key.
export type Picked = { key: string; trip: string | null; route: string | null };
export type When = { mode: "now" | "depart" | "arrive" | "last"; secs: number | null };
export type LiveUrls = { trips: string; vehicles: string; alerts: string };

/// How often the host re-reads the live feeds, the edit document, and
/// the clock (one timer: the polled input,
/// docs/units/host-discipline.md).
const POLL_MS = 30_000;
const BOARD_ROWS = 10;
const CARD_ROWS = 3;
const NEARBY = 5;
const FOUND = 25;
const MAX_LEGS = 4;
const URLS_KEY = "ride-now:live-urls";
const RECENT_KEY = "ride-now:recent";

export const ride = $state({
  status: "Loading the agency's feed…",
  busy: true,
  feed: "",
  /// Frames whose capability the bus could not assemble.
  off: { directory: false, board: false, router: false, rt: false },
  view: "home" as View,
  routes: [] as DirectoryRoute[],
  /// Every stop, for the map (directory `stops`, no query).
  stops: [] as DirectoryStop[],
  /// The type-ahead: which field is asking, and the directory's answer.
  query: null as { field: Field; text: string } | null,
  found: null as { field: Field; text: string; places: Place[] } | null,
  /// The device's location, once the rider shares it.
  here: null as { lat: number; lon: number } | null,
  locating: false,
  /// The point the nearby list answers: the rider, or a spot tapped
  /// on the map.
  near: null as { lat: number; lon: number; mine: boolean } | null,
  nearby: null as Place[] | null,
  /// Each nearby place's board; null where asking it failed.
  nearbyBoards: {} as Record<string, StopBoard | null>,
  stop: null as Place | null,
  board: null as StopBoard | null,
  boardError: "",
  /// A departure picked on the board: its route on the map, its bus
  /// marked.
  departure: null as { trip: string; start_secs: number; route: string } | null,
  routeFocus: null as EntityFocus | null,
  /// Which route `routeFocus` draws.
  focusRoute: null as string | null,
  urls: { trips: "", vehicles: "", alerts: "" } as LiveUrls,
  live: null as Live | null,
  liveAt: "",
  liveError: "",
  from: null as Place | null,
  to: null as Place | null,
  when: { mode: "now", secs: null } as When,
  plan: null as { itinerary: Itinerary | null; notice: string | null; error?: string } | null,
  paths: null as LegPath[] | null,
  /// The network: the routes list (filtered as typed), one route (its
  /// directions and the variants a rider rides), one bus (its trip's
  /// progress, its stops named).
  routeQuery: "",
  routeRows: null as EntityRow[] | null,
  route: null as string | null,
  /// Asked for the open route; `detail` null when the route is unknown.
  routeInfo: null as { detail: RouteDetail | null } | null,
  vehicle: null as Picked | null,
  progress: null as { progress: TripProgress | null; names: DirectoryStop[] } | null,
  /// The map keeps the picked bus in view as it moves.
  follow: true,
  saved: [] as { id: string; label: string }[],
  savedBoards: null as WatchBoardStop[] | null,
  recent: [] as Place[],
});

// ---------------------------------------------------------------------
// documents — the network (imported once), the edit document (the
// sync server's), the live snapshots (decoded per poll), the watchlist
// ---------------------------------------------------------------------

let network: Network | null = null;
let boundary: Boundary | null = null;
let edits: EditRecord[] = [];
let editsText = "[]";
let watchlist: Watchlist | null = null;
const snapshots = { trips: null as string | null, vehicles: null as string | null, alerts: null as string | null };
/// The trip asked: its ends and its time, fixed when asked (a "now"
/// plan re-asks on every poll).
let trip: { from: string; to: string; date: string; when: When } | null = null;

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
  ride.status = `Something went wrong: ${(e as Error)?.message ?? e}`;
  console.error(e);
}

/// The edit document registered once per module per version (null
/// when empty — schedule-only, the day views' contract), and the live
/// document registered once per module (its identity asserted equal
/// to rt's).
const editRegs = new WeakMap<CapModule, WeakMap<EditRecord[], string>>();
function editsIn(module: CapModule, load: (n: string, r: EditRecord[]) => { editsHash: string }): string | null {
  if (!network || edits.length === 0) return null;
  let docs = editRegs.get(module);
  if (!docs) editRegs.set(module, (docs = new WeakMap()));
  let hash = docs.get(edits);
  if (!hash) docs.set(edits, (hash = load(hashIn(module, network), edits).editsHash));
  return hash;
}

const liveRegs = new WeakMap<CapModule, Set<string>>();
function liveIn(module: CapModule, load: (live: Live["live"]) => string): string | null {
  const live = ride.live;
  if (!live) return null;
  let seen = liveRegs.get(module);
  if (!seen) liveRegs.set(module, (seen = new Set()));
  if (!seen.has(live.liveHash)) {
    const hash = load(live.live);
    if (hash !== live.liveHash) throw new Error(`live identity disagrees across capabilities: ${hash} != ${live.liveHash}`);
    seen.add(hash);
  }
  return live.liveHash;
}

const boundaries = new WeakMap<CapModule, string>();

/// The board's arguments for one stop, now.
function boardArgs(board: CapModule, stopId: string, rows: number) {
  const cap = boardCap(board);
  return [
    hashIn(board, network!),
    editsIn(board, cap.loadEdits),
    stopId,
    today(),
    nowSecs(),
    rows,
    liveIn(board, cap.loadLive),
  ] as const;
}

// ---------------------------------------------------------------------
// TRANSIENT — this UI's answer graph (src/uis/kit.ts, "the answer
// graph"): the first draft of its flow map. Inputs: "network",
// "edits" (the sync server's edit document), "snapshot" (the decoded
// live feeds), "clock" (the poll's tick), "query" / "near" / "stop" /
// "departure" / "trip" (the selection), "saved" (the watchlist
// document), "view" (which panel is on screen).
// ---------------------------------------------------------------------

const request = answerGraph(
  [
    {
      // What exists: every stop (the map draws them) and every route
      // (the badges take their authored colors).
      id: "directory.stops+routes",
      frame: "RideMap",
      needs: ["network"],
      run: async () => {
        const directory = await directoryModule();
        ride.off.directory = !directory;
        if (!network || !directory) return;
        const dir = directoryCap(directory);
        const hash = hashIn(directory, network);
        ride.stops = answer(directory, "stops", [hash], () => dir.stops(hash, null));
        ride.routes = answer(directory, "routes", [hash], () => dir.routes(hash));
      },
    },
    {
      // The type-ahead: every word typed in the name or the id.
      id: "directory.stops?query",
      frame: "PlaceSearch",
      needs: ["network", "query"],
      run: async () => {
        const [q, directory] = [ride.query, await directoryModule()];
        if (!network || !directory || !q || !q.text.trim()) {
          ride.found = null;
          return;
        }
        const hash = hashIn(directory, network);
        const text = q.text.trim();
        const stops = answer(directory, "stops", [hash, text, FOUND], () => directoryCap(directory).stops(hash, FOUND, text));
        ride.found = { field: q.field, text, places: stops.map((s) => ({ id: s.stop_id, name: s.name, lat: s.lat, lon: s.lon })) };
      },
    },
    {
      id: "directory.stops_near",
      frame: "NearbyList",
      needs: ["network", "near"],
      run: async () => {
        const [near, directory] = [ride.near, await directoryModule()];
        if (!network || !directory || !near) {
          ride.nearby = null;
          return;
        }
        const hash = hashIn(directory, network);
        const stops = answer(directory, "stops_near", [hash, near.lat, near.lon, NEARBY], () =>
          directoryCap(directory).stopsNear(hash, near.lat, near.lon, NEARBY),
        );
        ride.nearby = stops.map((s) => ({ id: s.stop_id, name: s.name, lat: s.lat, lon: s.lon, distance_m: s.distance_m }));
      },
    },
    {
      // THE live document over whichever feeds were fetched; the
      // boards and the router read it by hash, the map draws its buses.
      id: "rt.live",
      frame: "LiveFeeds",
      needs: ["network", "snapshot"],
      run: async (touch) => {
        const rt = await rtModule();
        ride.off.rt = !rt;
        const { trips, vehicles, alerts } = snapshots;
        if (!network || !boundary || !rt || (!trips && !vehicles && !alerts)) {
          ride.live = null;
          return;
        }
        const cap = rtCap(rt);
        const networkHash = hashIn(rt, network);
        let boundaryHash = boundaries.get(rt);
        if (!boundaryHash) boundaries.set(rt, (boundaryHash = cap.loadBoundary(boundary)));
        const bh = boundaryHash;
        ride.live = answer(rt, "live", [networkHash, bh, trips, vehicles, alerts], () => cap.live(networkHash, bh, trips, vehicles, alerts));
        // A picked bus that moved on to its next trip is followed onto it.
        const v = ride.vehicle;
        const now = v && ride.live.live.vehicles.find((x) => x.key === v.key);
        if (v && now && (now.trip !== v.trip || now.route !== v.route)) {
          ride.vehicle = { key: v.key, trip: now.trip, route: now.route };
          touch("vehicle");
        }
      },
    },
    {
      id: "board.stop_board",
      frame: "RiderBoard",
      needs: ["network", "edits", "rt.live", "stop", "clock", "view"],
      visible: () => ride.view === "stop",
      hide: () => {
        ride.board = null;
      },
      run: async (touch) => {
        const [stop, board] = [ride.stop, await boardModule()];
        ride.off.board = !board;
        if (!network || !board || !stop) {
          ride.board = null;
          return;
        }
        try {
          const args = boardArgs(board, stop.id, BOARD_ROWS);
          ride.board = answer(board, "stop_board", [...args], () => boardCap(board).stopBoard(...args));
          ride.boardError = "";
        } catch (e) {
          console.error(e);
          ride.board = null;
          ride.boardError = (e as Error).message ?? String(e);
          return;
        }
        // A picked departure that has left the board is unpicked.
        const d = ride.departure;
        if (d && !ride.board.rows.some((r) => r.trip === d.trip && r.start_secs === d.start_secs)) {
          ride.departure = null;
          touch("departure");
        }
      },
    },
    {
      // Each nearby place's next departures — the same board, one
      // call per card.
      id: "board.stop_board[nearby]",
      frame: "NearbyList",
      needs: ["network", "edits", "rt.live", "directory.stops_near", "clock", "view"],
      visible: () => ride.view === "home",
      run: async () => {
        const board = await boardModule();
        ride.off.board = !board;
        if (!network || !board || !ride.nearby) {
          ride.nearbyBoards = {};
          return;
        }
        const boards: Record<string, StopBoard | null> = {};
        for (const place of ride.nearby) {
          try {
            const args = boardArgs(board, place.id, CARD_ROWS);
            boards[place.id] = answer(board, "stop_board", [...args], () => boardCap(board).stopBoard(...args));
          } catch (e) {
            console.error(e);
            boards[place.id] = null;
          }
        }
        ride.nearbyBoards = boards;
      },
    },
    {
      id: "board.watchlist_board",
      frame: "SavedList",
      needs: ["network", "edits", "saved", "clock", "view"],
      visible: () => ride.view === "home",
      run: async () => {
        const board = await boardModule();
        ride.off.board = !board;
        if (!network || !board || !watchlist || ride.saved.length === 0) {
          ride.savedBoards = null;
          return;
        }
        const cap = boardCap(board);
        const { watchlistHash } = cap.loadWatchlist(watchlist);
        const args = [watchlistHash, hashIn(board, network), editsIn(board, cap.loadEdits), today(), nowSecs(), CARD_ROWS] as const;
        ride.savedBoards = answer(board, "watchlist_board", [...args], () => cap.watchlistBoard(...args));
      },
    },
    {
      // The route in play, drawn: the open route's, the picked bus's,
      // or the picked departure's.
      id: "directory.focus",
      frame: "RideMap",
      needs: ["network", "departure", "route", "vehicle", "view"],
      run: async () => {
        const directory = await directoryModule();
        const route = focusRoute();
        ride.focusRoute = route;
        if (!network || !directory || !route) {
          ride.routeFocus = null;
          return;
        }
        const hash = hashIn(directory, network);
        ride.routeFocus = answer(directory, "focus", [hash, "route", route], () => directoryCap(directory).focus(hash, "route", route));
      },
    },
    {
      // The routes, narrowed by the filter's words.
      id: "directory.entities[route]",
      frame: "RouteList",
      needs: ["network", "routeQuery", "view"],
      visible: () => ride.view === "routes",
      run: async () => {
        const directory = await directoryModule();
        ride.off.directory = !directory;
        if (!network || !directory) {
          ride.routeRows = null;
          return;
        }
        const hash = hashIn(directory, network);
        const q = ride.routeQuery.trim();
        ride.routeRows = answer(directory, "entities", [hash, "route", q], () => directoryCap(directory).entities(hash, "route", q));
      },
    },
    {
      // One route: its directions, each with the variants a rider
      // rides (a station's bays one place).
      id: "directory.route_detail",
      frame: "RouteView",
      needs: ["network", "route", "view"],
      visible: () => ride.view === "route",
      run: async () => {
        const [route, directory] = [ride.route, await directoryModule()];
        if (!network || !directory || !route) {
          ride.routeInfo = null;
          return;
        }
        const hash = hashIn(directory, network);
        ride.routeInfo = { detail: answer(directory, "route_detail", [hash, route], () => directoryCap(directory).routeDetail(hash, route)) };
      },
    },
    {
      // One bus: its trip's progress in the trip-updates snapshot,
      // and the names of the stops it lists (`stops_by_ids` — the
      // names join for views that carry ids).
      id: "rt.trip_progress+directory.stops_by_ids",
      frame: "BusView",
      needs: ["network", "snapshot", "vehicle", "view"],
      visible: () => ride.view === "bus",
      run: async () => {
        const [v, rt, directory] = [ride.vehicle, await rtModule(), await directoryModule()];
        if (!network || !boundary || !rt || !v?.trip || !snapshots.trips) {
          ride.progress = v ? { progress: null, names: [] } : null;
          return;
        }
        const cap = rtCap(rt);
        const networkHash = hashIn(rt, network);
        let bh = boundaries.get(rt);
        if (!bh) boundaries.set(rt, (bh = cap.loadBoundary(boundary)));
        const [b, rtHash, trip] = [bh, snapshots.trips, v.trip];
        const progress = answer(rt, "trip_progress", [networkHash, b, rtHash, trip], () => cap.tripProgress(networkHash, b, rtHash, trip));
        let names: DirectoryStop[] = [];
        if (progress && directory) {
          const hash = hashIn(directory, network);
          const ids = progress.stops.map((s) => s.stop_id);
          names = answer(directory, "stops_by_ids", [hash, ...ids], () => directoryCap(directory).stopsByIds(hash, ids));
        }
        ride.progress = { progress, names };
      },
    },
    {
      // The trip: leaving now or at a time (with the rider's notice
      // when a detour changes it), arriving by a time, or the last bus
      // (arrive-by with no deadline) — at the live predictions.
      id: "router.plan",
      frame: "TripPlan",
      needs: ["network", "edits", "rt.live", "trip"],
      run: async () => {
        const router = await routerModule();
        ride.off.router = !router;
        if (!network || !router || !trip) {
          ride.plan = null;
          return;
        }
        const cap = routerCap(router);
        const t = trip;
        try {
          const net = hashIn(router, network);
          const ed = editsIn(router, cap.loadEdits);
          const lv = liveIn(router, cap.loadLive);
          if (t.when.mode === "now" || t.when.mode === "depart") {
            const args = [net, ed, t.from, t.to, t.date, t.when.secs ?? nowSecs(), MAX_LEGS, lv] as const;
            const { active, notice } = answer(router, "plan_compare", [...args], () => cap.planCompare(...args));
            ride.plan = { itinerary: active, notice };
          } else {
            const deadline = t.when.mode === "arrive" ? t.when.secs : null;
            const args = [net, ed, t.from, t.to, t.date, deadline, MAX_LEGS, lv] as const;
            const { itinerary } = answer(router, "plan_arrive_by", [...args], () => cap.planArriveBy(...args));
            ride.plan = { itinerary, notice: null };
          }
        } catch (e) {
          // The trip panel says it could not plan, never "no trips".
          console.error(e);
          ride.plan = { itinerary: null, notice: null, error: (e as Error).message ?? String(e) };
        }
      },
    },
    {
      // Where the planned trip runs, one path per leg.
      id: "router.itinerary_paths",
      frame: "RideMap",
      needs: ["router.plan"],
      run: async () => {
        const [it, router] = [ride.plan?.itinerary, await routerModule()];
        if (!network || !router || !it || !trip) {
          ride.paths = null;
          return;
        }
        const cap = routerCap(router);
        ride.paths = cap.itineraryPaths(hashIn(router, network), editsIn(router, cap.loadEdits), trip.date, it);
      },
    },
  ],
  fail,
);

// ---------------------------------------------------------------------
// boot, the feed, and the poll
// ---------------------------------------------------------------------

export async function boot(): Promise<void> {
  await restoreUrls();
  await restoreSaved();
  restoreRecent();
  void locateIfAllowed();
  await withNetworkModule("the agency's network", async (module) => {
    const served = await servedNetwork(module);
    if (served) return served;
    ride.status = "This server publishes no network — load the agency's GTFS zip to begin.";
    return null;
  });
  setInterval(() => void poll(), POLL_MS);
}

/// A GTFS zip the rider loads becomes the network document (and its
/// boundary, the stop_sequence numbering the live feeds name).
export function loadFeedBytes(bytes: Uint8Array, label: string): Promise<void> {
  return withNetworkModule(label, (module) => {
    const cap = networkCap(module);
    const imported = cap.import(cap.loadFeed(bytes).feedHash);
    rememberHash(module, imported.network, imported.networkHash);
    return imported;
  });
}

/// Adopt the network (and boundary) `obtain` answers, then answer
/// everything over it.
async function withNetworkModule(
  label: string,
  obtain: (module: CapModule) => Promise<{ network: Network; boundary: Boundary } | null> | { network: Network; boundary: Boundary },
): Promise<void> {
  ride.busy = true;
  ride.status = `Loading ${label}…`;
  await new Promise((r) => setTimeout(r, 20));
  try {
    const module = await networkModule();
    if (!module) {
      ride.status = "The network capability is unavailable — nothing can be answered without it.";
      return;
    }
    const got = await obtain(module);
    if (!got) return;
    network = got.network;
    boundary = got.boundary;
    ride.feed = network.name || label;
    ride.status = "";
    await Promise.all([syncEdits(), fetchLive()]);
    await request("network", "edits", "snapshot", "clock", "query", "near", "stop", "departure", "trip", "saved", "view");
  } catch (e) {
    fail(e);
  } finally {
    ride.busy = false;
  }
}

/// One tick: the edit document, the live feeds, and the clock; a trip
/// asked for "now" is asked again from the new now.
export async function poll(): Promise<void> {
  if (!network || ride.busy) return;
  const changed = ["clock"];
  const [e, l] = await Promise.all([syncEdits(), fetchLive()]);
  if (e) changed.push("edits");
  if (l) changed.push("snapshot");
  if (trip?.when.mode === "now") {
    trip = { ...trip, date: today() };
    changed.push("trip");
  }
  await request(...changed);
}

/// The sync server's edit document (detours), read-only — a rider
/// never writes edits. No sync server, or an unreadable answer, keeps
/// the last good document. True when it changed.
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
  const kinds = (["trips", "vehicles", "alerts"] as const).filter((k) => ride.urls[k]);
  if (kinds.length === 0) {
    const had = snapshots.trips || snapshots.vehicles || snapshots.alerts;
    snapshots.trips = snapshots.vehicles = snapshots.alerts = null;
    return !!had;
  }
  const rt = await rtModule();
  ride.off.rt = !rt;
  if (!rt) return false;
  const results = await Promise.all(
    kinds.map(async (kind) => {
      try {
        const resp = await fetch(`/rt?url=${encodeURIComponent(ride.urls[kind])}`);
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        return { kind, bytes: new Uint8Array(await resp.arrayBuffer()) };
      } catch (e) {
        return { kind, error: (e as Error).message };
      }
    }),
  );
  let changed = false;
  const errors: string[] = [];
  for (const r of results) {
    if ("error" in r) {
      errors.push(`${r.kind}: ${r.error}`);
      continue;
    }
    try {
      const hash = rtCap(rt).decode(r.bytes).rtHash;
      if (hash !== snapshots[r.kind]) changed = true;
      snapshots[r.kind] = hash;
    } catch (e) {
      errors.push(`${r.kind}: ${(e as Error).message}`);
    }
  }
  ride.liveError = errors.join("; ");
  if (errors.length < kinds.length) ride.liveAt = clockLabel();
  return changed;
}

/// The rider's own feed URLs, else the deployment's (live-feeds.json
/// beside the page).
async function restoreUrls(): Promise<void> {
  try {
    const stored = localStorage.getItem(URLS_KEY);
    if (stored) {
      ride.urls = { ...ride.urls, ...JSON.parse(stored) };
      return;
    }
  } catch {
    // no storage: the deployment's feeds
  }
  try {
    const resp = await fetch("live-feeds.json");
    if (resp.ok) ride.urls = { ...ride.urls, ...((await resp.json()) as Partial<LiveUrls>) };
  } catch {
    // no deployment config: the schedule until the rider sets feeds
  }
}

export async function setLiveUrls(urls: LiveUrls): Promise<void> {
  ride.urls = { ...urls };
  try {
    localStorage.setItem(URLS_KEY, JSON.stringify(urls));
  } catch {
    // kept for this visit only
  }
  for (const k of ["trips", "vehicles", "alerts"] as const) if (!urls[k]) snapshots[k] = null;
  await fetchLive();
  await request("snapshot");
}

// ---------------------------------------------------------------------
// where the rider is
// ---------------------------------------------------------------------

/// Ask for the location without a prompt only when the rider already
/// allowed it; otherwise the "near me" button asks.
async function locateIfAllowed(): Promise<void> {
  try {
    const state = await navigator.permissions?.query({ name: "geolocation" as PermissionName });
    if (state?.state === "granted") locate();
  } catch {
    // no permissions API: wait for the button
  }
}

export function locate(): void {
  if (!navigator.geolocation) {
    ride.status = "This browser can't share a location — search for a stop or tap the map.";
    return;
  }
  ride.locating = true;
  navigator.geolocation.getCurrentPosition(
    (pos) => {
      ride.locating = false;
      ride.here = { lat: pos.coords.latitude, lon: pos.coords.longitude };
      ride.near = { ...ride.here, mine: true };
      void request("near");
    },
    (err) => {
      ride.locating = false;
      ride.status = `No location (${err.message}) — search for a stop or tap the map.`;
    },
    { enableHighAccuracy: true, timeout: 10_000 },
  );
}

/// A spot tapped on the map: what is near it.
export function nearPoint(lat: number, lon: number): Promise<void> {
  ride.near = { lat, lon, mine: false };
  ride.view = "home";
  return request("near", "view");
}

// ---------------------------------------------------------------------
// the selection — each handler sets its input and requests it
// ---------------------------------------------------------------------

export function typed(field: Field, text: string): Promise<void> {
  ride.query = { field, text };
  return request("query");
}

export function clearFound(): void {
  ride.query = null;
  ride.found = null;
}

export function show(view: View): Promise<void> {
  ride.view = view;
  return request("view");
}

/// A place known only by id (a saved stop) takes its point from the
/// directory's stop list, so the map can go to it.
function located(place: Place): Place {
  if (place.lat != null) return place;
  const stop = ride.stops.find((s) => s.stop_id === place.id);
  return stop ? { ...place, lat: stop.lat, lon: stop.lon } : place;
}

export function openStop(place: Place): Promise<void> {
  clearFound();
  place = located(place);
  ride.stop = place;
  ride.board = null;
  ride.boardError = "";
  ride.departure = null;
  ride.view = "stop";
  remember(place);
  return request("stop", "departure", "view");
}

export function pickDeparture(d: { trip: string; start_secs: number; route: string } | null): Promise<void> {
  const same = d && ride.departure && ride.departure.trip === d.trip && ride.departure.start_secs === d.start_secs;
  ride.departure = same ? null : d;
  return request("departure");
}

/// The route the map draws for the panel on screen.
function focusRoute(): string | null {
  if (ride.view === "route") return ride.route;
  if (ride.view === "bus") return ride.vehicle?.route ?? null;
  if (ride.view === "stop") return ride.departure?.route ?? null;
  return null;
}

export function showRoutes(): Promise<void> {
  ride.view = "routes";
  return request("view");
}

export function filterRoutes(text: string): Promise<void> {
  ride.routeQuery = text;
  return request("routeQuery");
}

export function openRoute(routeId: string): Promise<void> {
  ride.route = routeId;
  ride.view = "route";
  return request("route", "view");
}

/// A bus tapped on the map.
export function openVehicle(v: Picked): Promise<void> {
  ride.vehicle = v;
  ride.progress = null;
  ride.follow = true;
  ride.view = "bus";
  return request("vehicle", "view");
}

export function setFollow(on: boolean): void {
  ride.follow = on;
}

/// Directions from or to a place; the other end defaults to the
/// place nearest the rider when the directory answered one.
export function directions(end: "from" | "to", place: Place): Promise<void> {
  clearFound();
  ride[end] = place;
  const other = end === "from" ? "to" : "from";
  if (!ride[other] && ride.near?.mine && ride.nearby?.[0] && ride.nearby[0].id !== place.id) ride[other] = ride.nearby[0];
  ride.view = "trip";
  return replan();
}

export function setEnd(end: "from" | "to", place: Place): Promise<void> {
  clearFound();
  ride[end] = place;
  remember(place);
  return replan();
}

export function swapEnds(): Promise<void> {
  [ride.from, ride.to] = [ride.to, ride.from];
  return replan();
}

export function setWhen(when: When): Promise<void> {
  ride.when = when;
  return replan();
}

function replan(): Promise<void> {
  trip =
    ride.from && ride.to && ride.from.id !== ride.to.id
      ? { from: ride.from.id, to: ride.to.id, date: today(), when: { ...ride.when } }
      : null;
  if (!trip) ride.plan = null;
  return request("trip", "view");
}

// ---------------------------------------------------------------------
// the rider's own: saved stops (the watchlist document — JS owns it,
// replaced wholesale; the store keeps every version) and recent
// stops (a per-device convenience)
// ---------------------------------------------------------------------

async function restoreSaved(): Promise<void> {
  try {
    await restoreDocuments();
    const record = currentDocument("watchlist");
    watchlist = record ? (JSON.parse(record.bytes) as Watchlist) : null;
  } catch (e) {
    console.warn("watchlist restore failed:", e);
    watchlist = null;
  }
  ride.saved = watchlist
    ? watchlist.version === "v2"
      ? watchlist.stops.map((s) => ({ id: s.id, label: s.label }))
      : watchlist.stops.map((s) => ({ id: s.id, label: s.note }))
    : [];
}

async function writeSaved(stops: { id: string; label: string }[]): Promise<void> {
  const doc: Watchlist = { version: "v2", stops };
  try {
    await putDocument("watchlist", JSON.stringify(doc), 2);
  } catch (e) {
    console.warn("watchlist save failed:", e);
    ride.status = "Couldn't save on this device — your saved stops last until you leave.";
  }
  watchlist = doc;
  ride.saved = stops;
  await request("saved");
}

export function toggleSaved(place: Place): Promise<void> {
  return ride.saved.some((s) => s.id === place.id)
    ? writeSaved(ride.saved.filter((s) => s.id !== place.id))
    : writeSaved([...ride.saved, { id: place.id, label: place.name }]);
}

function restoreRecent(): void {
  try {
    ride.recent = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]") as Place[];
  } catch {
    ride.recent = [];
  }
}

function remember(place: Place): void {
  ride.recent = [{ id: place.id, name: place.name, lat: place.lat, lon: place.lon }, ...ride.recent.filter((p) => p.id !== place.id)].slice(0, 6);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(ride.recent));
  } catch {
    // recent stops last this visit
  }
}
