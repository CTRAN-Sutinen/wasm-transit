/// The ride-now UI's host layer — the ONLY file of this UI that calls the
/// capability bus. It owns I/O (its inputs, declared in its ui.json and
/// read by the kit: the deployment's manifest, the network and streets
/// it pins, the edit document, the live feeds; and the watchlist
/// document, the device's location, the rider's recent stops), the
/// clock, and the selection; every row,
/// time, status, line, and notice it writes into `ride` is an op's
/// answer, adopted as-is.
///
/// The questions it answers (its ui.json): which stop is
/// mine (typed — or an address typed, through the server's geocoder —, near me — by walk along the streets when the server
/// publishes them — or tapped on the map), what leaves next from it
/// live, is my bus on time, how do I get there (now, at a time, by a
/// time; from my location or a dropped pin, walking the streets),
/// when is the last bus, is a detour or alert in my way, and
/// what leaves the stops I saved — and, around them, the network:
/// where every bus is now and where one is going, and what each route
/// is (its directions and their stops).
///
/// A capability the bus cannot assemble turns its frames OFF (`off`),
/// and they say so — nothing here answers in its place.
///
/// Addresses are not the bus's: the geocoder is an ORACLE
/// (docs/architecture/web.md, "External services") — the
/// server's manifest lists it, and it turns text into a point and
/// names a point. This host sends only what the rider did (the text,
/// the tap, where the map looks; the deployment's query policy is
/// the server's) and shows the answer as given; a point it answers
/// is a trip end like a dropped pin, carrying the name it was shown
/// with; nothing durable keeps the query.

import { ask, assemble, available, keep, load, produced, readManifest, servedNetwork, servedNetworkPin, servedStreets, servedWalks, Doc, type DeploymentManifest } from "../../engine";
import { currentDocument, putDocument, restoreDocuments, type DocRecord } from "../../documents.svelte";
import { answerGraph, applyBuild, askOracle, buildWaiting, clockEvery, clockLabelAt, inputGraph, inputLabel, installApp, nowSecs, previousDay, readFeed, readHead, readLiveFeeds, setAgencyZone, today, type FrameState, type InputDeclaration, type InputState, type LiveUrls, type Reader } from "../kit";
import descriptor from "./ui.json";

// This page's capabilities answer inside its linked assembly — one
// module, one registry: each document registers once (D-77).
assemble("ride-now");
import type { DirectWalk } from "../../types/DirectWalk";
import type { End } from "../../types/End";
import type { PointWalk } from "../../types/PointWalk";
import type { Boundary } from "../../types/Boundary";
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { DirectoryStop } from "../../types/cap_directory/DirectoryStop";
import type { EntityRow } from "../../types/EntityRow";
import type { RouteDetail } from "../../types/RouteDetail";
import type { TripProgress } from "../../types/TripProgress";
import type { EditDocument } from "../../types/EditDocument";
import type { EntityFocus } from "../../types/EntityFocus";
import type { Itinerary } from "../../types/Itinerary";
import type { LegPath } from "../../types/LegPath";
import type { LiveDay } from "../../types/LiveDay";
import type { LiveSummary } from "../../types/LiveSummary";
import type { Network } from "../../types/Network";
import type { StopBoard } from "../../types/StopBoard";
import type { WatchBoardStop } from "../../types/cap_board/WatchBoardStop";
import type { Watchlist } from "../../types/cap_board/Watchlist";
import type { RecentPlaces } from "../../types/cap_board/RecentPlaces";
import type { LiveFeeds } from "../../types/cap_board/LiveFeeds";

/// A stop as the directory answers it, wherever it was picked from
/// (typed, near a point, tapped on the map): enough to name it, put it
/// on the map, and ask its board — or, as a trip's end, a POINT (my
/// location, a dropped pin), which the router plans from as a place
/// by its walks along the streets.
export type Place = {
  id: string;
  name: string;
  lat?: number;
  lon?: number;
  distance_m?: number;
  walk_secs?: number | null;
  point?: boolean;
  /// An address's town, shown beside its name.
  detail?: string;
};
export type Field = "search" | "from" | "to";
export type View = "home" | "stop" | "trip" | "alerts" | "routes" | "route" | "bus";
/// A bus picked on the map, by the live document's key.
export type Picked = { key: string; trip: string | null; route: string | null };
export type When = { mode: "now" | "depart" | "arrive" | "last"; secs: number | null };

/// How often the answers on the clock are asked again (a board's
/// waits, a trip asked for "now"). The inputs' intervals are in its
/// ui.json.
const POLL_MS = 30_000;
const BOARD_ROWS = 10;
const CARD_ROWS = 3;
const NEARBY = 5;
const FOUND = 25;
const MAX_LEGS = 4;
// Where an older build kept the rider's feed URLs and recent places:
// read once, moved into the document store, and removed.
const URLS_KEY = "ride-now:live-urls";
const RECENT_KEY = "ride-now:recent";
// Presentation preferences, kept in localStorage.
const FIX_KEY = "ride-now:last-fix";
const FRAME_KEY = "ride-now:network-view";

/// The view the network was last framed on: where the map opens for
/// a rider who shares no location, so it makes no move on load. Not
/// the rider's — the network's; a network that moved is framed again.
function readFramed(): { center: [number, number]; zoom: number } | null {
  try {
    const v = JSON.parse(localStorage.getItem(FRAME_KEY) ?? "null");
    const [lon, lat] = v?.center ?? [];
    if (Number.isFinite(lon) && Number.isFinite(lat) && Number.isFinite(v?.zoom) && Math.abs(lat) <= 90 && Math.abs(lon) <= 180) {
      return { center: [lon, lat], zoom: v.zoom };
    }
  } catch {
    // no storage, or not ours: no frame
  }
  return null;
}

export function rememberFramed(view: { center: [number, number]; zoom: number }): void {
  ride.framed = view;
  try {
    localStorage.setItem(FRAME_KEY, JSON.stringify(view));
  } catch {
    // not remembered: the map frames the network again next time
  }
}

/// The last location this device shared, kept on the device only: the
/// map opens on it (RideMap `start`), so its camera is already where
/// the rider will be when the first fix lands. Anything else in
/// storage — or none — is no start.
function readLastFix(): { lat: number; lon: number } | null {
  try {
    const v = JSON.parse(localStorage.getItem(FIX_KEY) ?? "null");
    if (Number.isFinite(v?.lat) && Number.isFinite(v?.lon) && Math.abs(v.lat) <= 90 && Math.abs(v.lon) <= 180) {
      return { lat: v.lat, lon: v.lon };
    }
  } catch {
    // no storage, or not ours: no start
  }
  return null;
}

export const ride = $state({
  /// A message the rider's last action left (a location refused, a
  /// failed step). What the page holds of the schedule is `status()`.
  status: "",
  busy: true,
  feed: "",
  /// Frames whose capability the bus could not assemble.
  /// Each frame that shows no answer, and why (the answer graph's;
  /// src/uis/answers.ts).
  frames: {} as Record<string, FrameState>,
  /// What the page holds of each input (the kit's).
  inputs: {} as Record<string, InputState>,
  view: "home" as View,
  routes: [] as DirectoryRoute[],
  /// Every stop, for the map (directory `stops`, no query).
  stops: [] as DirectoryStop[],
  /// The type-ahead: which field is asking, and the directory's answer.
  query: null as { field: Field; text: string } | null,
  found: null as { field: Field; text: string; places: Place[] } | null,
  /// The geocoder's answer for the same field and text: addresses and
  /// places, each a point. Null until it answers, or where the server
  /// runs no geocoder.
  addresses: null as { field: Field; text: string; places: Place[] } | null,
  /// The manifest lists a geocoder: the search boxes take addresses.
  geocoding: false,
  /// The device's location, once the rider shares it.
  here: null as { lat: number; lon: number } | null,
  /// The location the device shared last time: where the map opens.
  start: readLastFix(),
  /// The view the network was last framed on: where the map opens
  /// when there is no fix.
  framed: readFramed(),
  /// The published streets are registered: walks follow the streets,
  /// and a trip can start or end at a point.
  walking: false,
  /// The trip end the next tap on the map sets ("choose on map").
  picking: null as "from" | "to" | null,
  locating: false,
  /// The point the nearby list answers: the rider, or a spot tapped
  /// on the map.
  near: null as { lat: number; lon: number; mine: boolean; label?: string } | null,
  nearby: null as Place[] | null,
  /// Each nearby place's board; null where asking it failed.
  nearbyBoards: {} as Record<string, StopBoard | null>,
  stop: null as Place | null,
  board: null as StopBoard | null,
  /// A departure picked on the board: its route on the map, its bus
  /// marked.
  departure: null as { trip: string; start_secs: number; route: string } | null,
  routeFocus: null as EntityFocus | null,
  /// Which route `routeFocus` draws.
  focusRoute: null as string | null,
  urls: { trips: "", vehicles: "", alerts: "" } as LiveUrls,
  live: null as Live | null,
  from: null as Place | null,
  to: null as Place | null,
  when: { mode: "now", secs: null } as When,
  plan: null as { itinerary: Itinerary | null; notice: string | null } | null,
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
  /// A new build has downloaded and waits (kit's buildWaiting): the
  /// rider may apply it now; it applies itself at the next navigation.
  updateReady: false,
  saved: [] as { id: string; label: string }[],
  savedBoards: null as WatchBoardStop[] | null,
  recent: [] as Place[],
});

// ---------------------------------------------------------------------
// documents — the network (imported once), the edit document (the
// sync server's), the live snapshots (decoded per poll), the watchlist
// ---------------------------------------------------------------------

let network: Doc<Network> | null = null;
let boundary: Doc<Boundary> | null = null;
/// The edit document over the network — null when empty (schedule
/// only, the day views' contract).
let edits: Doc<EditDocument> | null = null;
/// The rider's saved stops, as the document the board reads.
let watchlist: Doc<Watchlist> | null = null;
/// THE live document (rt's answer over the snapshots), by which the
/// boards and the router read the live feeds.
let live: Doc<LiveDay> | null = null;
/// The decoded live feeds, one document per read's bytes.
const snapshots = { trips: null as Doc | null, vehicles: null as Doc | null, alerts: null as Doc | null };
/// The published streets (registered in the streets module) and walk
/// table (registered in the router) — null when the server publishes
/// none.
let streets: Doc | null = null;
let walks: Doc | null = null;
/// The near point's walks to the stops (streets `access`).
let nearWalks: PointWalk[] | null = null;
/// The trip asked: its ends and its time, fixed when asked (a "now"
/// plan re-asks on every poll).
let trip: { from: Place; to: Place; date: string; when: When } | null = null;
/// The planned trip's ends as the router took them — what
/// itinerary_paths draws a place's walks from.
let planEnds: { from: End; to: End; direct: DirectWalk | null } | null = null;

/// What the live frames draw: the live document and its summary.
type Live = { live: LiveDay; summary: LiveSummary };

const HERE_ID = "@here";

/// A spot as a trip's end (tapped on the map, or an address) — one
/// pin per end, so the two never share an id.
function pinPlace(end: "from" | "to", lat: number, lon: number, name = "Dropped pin"): Place {
  return { id: `@pin-${end}`, name, lat, lon, point: true };
}

/// The rider's location as a trip's end.
function herePlace(): Place | null {
  return ride.here ? { id: HERE_ID, name: "My location", lat: ride.here.lat, lon: ride.here.lon, point: true } : null;
}

/// The walks from a point to the stops (and straight to `to`), along
/// the published streets.
async function accessFrom(p: Place, to: Place | null): Promise<{ walks: PointWalk[]; direct: DirectWalk | null } | null> {
  if (!network || !streets || p.lat == null || p.lon == null || !(await available("streets"))) return null;
  const point = { lat: p.lat, lon: p.lon };
  const target = to?.lat != null && to.lon != null ? { lat: to.lat, lon: to.lon } : null;
  const { walks, direct } = await ask("streets", "access", {
    streets_hash: streets,
    network_hash: network,
    point,
    to: target,
    speed_mps: null,
    max_walk_secs: null,
  });
  return { walks, direct };
}

/// A trip's ends as the router takes them: a stop by id, a point as a
/// place with its walks — and the walk straight between two points.
async function routerEnds(from: Place, to: Place): Promise<{ from: End; to: End; direct: DirectWalk | null }> {
  const end = async (p: Place, other: Place | null) => {
    if (!p.point) return { end: p.id as End, direct: null };
    const access = await accessFrom(p, other?.point ? other : null);
    if (!access) throw new Error("planning from a point needs the published streets");
    return { end: { id: p.id, name: p.name, walks: access.walks } as End, direct: access.direct };
  };
  const [f, t] = [await end(from, to), await end(to, null)];
  return { from: f.end, to: t.end, direct: f.direct };
}


/// Whether a frame's capability did not assemble: its component says
/// so (the answer graph's state, src/uis/answers.ts).
export function off(frame: string): boolean {
  return ride.frames[frame]?.kind === "unassembled";
}

/// A frame's failure, as text ("" when it answered).
export function failure(frame: string): string {
  const state = ride.frames[frame];
  return state?.kind === "failed" ? state.message : "";
}

function fail(e: unknown): void {
  ride.status = `Something went wrong: ${(e as Error)?.message ?? e}`;
  console.error(e);
}

/// One stop's board, now.
async function stopBoard(stopId: string, rows: number): Promise<StopBoard> {
  const answer = await ask("board", "stop_board", {
    network_hash: network!,
    edits_hash: edits ?? undefined,
    stop_id: stopId,
    date: today(),
    from_secs: nowSecs(),
    // The previous date's after-midnight trips still leaving.
    previous: previousDay(),
    limit: rows,
    live_hash: live ?? undefined,
  });
  return answer.board;
}

// ---------------------------------------------------------------------
// This UI's answer graph (src/uis/kit.ts, "the answer graph").
// Inputs: "network",
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
      caps: ["directory"],
      run: async () => {
        if (!network) return;
        ride.stops = (await ask("directory", "stops", { network_hash: network, limit: null })).stops;
        ride.routes = (await ask("directory", "routes", { network_hash: network })).routes;
      },
    },
    {
      // The type-ahead: every word typed in the name or the id.
      id: "directory.stops?query",
      frame: "PlaceSearch",
      needs: ["network", "query"],
      caps: ["directory"],
      clear: () => (ride.found = null),
      run: async () => {
        const q = ride.query;
        if (!network || !q || !q.text.trim()) {
          ride.found = null;
          return;
        }
        const text = q.text.trim();
        const { stops } = await ask("directory", "stops", { network_hash: network, limit: FOUND, query: text });
        ride.found = { field: q.field, text, places: stops.map((s) => ({ id: s.stop_id, name: s.name, lat: s.lat, lon: s.lon })) };
      },
    },
    {
      // The near point's walks to the stops, along the streets.
      id: "streets.access[near]",
      // An input to the list, not a frame: without the streets the
      // list orders by distance.
      frame: "",
      needs: ["network", "streets", "near"],
      run: async () => {
        const near = ride.near;
        nearWalks = near ? ((await accessFrom({ id: "", name: "", lat: near.lat, lon: near.lon }, null))?.walks ?? null) : null;
      },
    },
    {
      // The places near the point: by walk when the streets answered
      // its walks, else by distance.
      id: "directory.stops_near",
      frame: "NearbyList",
      needs: ["network", "near", "streets.access[near]"],
      caps: ["directory"],
      clear: () => (ride.nearby = null),
      run: async () => {
        const near = ride.near;
        if (!network || !near) {
          ride.nearby = null;
          return;
        }
        const { stops } = await ask("directory", "stops_near", {
          network_hash: network,
          lat: near.lat,
          lon: near.lon,
          limit: NEARBY,
          walks: nearWalks ?? undefined,
        });
        ride.nearby = stops.map((s) => ({
          id: s.stop_id,
          name: s.name,
          lat: s.lat,
          lon: s.lon,
          distance_m: s.distance_m,
          walk_secs: s.walk_secs,
        }));
      },
    },
    {
      // THE live document over whichever feeds were fetched; the
      // boards and the router read it by hash, the map draws its buses.
      id: "rt.live",
      frame: "LiveFeeds",
      needs: ["network", "snapshot"],
      caps: ["rt"],
      clear: () => {
        live = null;
        ride.live = null;
      },
      run: async (touch) => {
        const { trips, vehicles, alerts } = snapshots;
        if (!network || !boundary || (!trips && !vehicles && !alerts)) {
          live = null;
          ride.live = null;
          return;
        }
        const answer = await ask("rt", "live", {
          network_hash: network,
          boundary_hash: boundary,
          rt_hash: trips ?? undefined,
          vp_hash: vehicles ?? undefined,
          alerts_hash: alerts ?? undefined,
        });
        // The page reads the document (its vehicles and alerts); linked
        // beside rt, the board and the router read it by hash where rt
        // minted it — it never crosses back.
        const day = answer.live;
        if (!day) throw new Error("the live answer carries no document");
        live = produced("live", day, "rt", answer.live_hash);
        ride.live = { live: day, summary: answer.summary };
        // A picked bus that moved on to its next trip is followed onto it.
        const v = ride.vehicle;
        const now = v && day.vehicles.find((x) => x.key === v.key);
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
      caps: ["board"],
      clear: () => (ride.board = null),
      run: async (touch) => {
        const stop = ride.stop;
        if (!network || !stop) {
          ride.board = null;
          return;
        }
        ride.board = await stopBoard(stop.id, BOARD_ROWS);
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
      frame: "NearbyBoards",
      needs: ["network", "edits", "rt.live", "directory.stops_near", "clock", "view"],
      visible: () => ride.view === "home",
      caps: ["board"],
      clear: () => (ride.nearbyBoards = {}),
      run: async () => {
        if (!network || !ride.nearby) {
          ride.nearbyBoards = {};
          return;
        }
        const boards: Record<string, StopBoard | null> = {};
        for (const place of ride.nearby) {
          try {
            boards[place.id] = await stopBoard(place.id, CARD_ROWS);
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
      needs: ["network", "edits", "rt.live", "saved", "clock", "view"],
      visible: () => ride.view === "home",
      caps: ["board"],
      clear: () => (ride.savedBoards = null),
      run: async () => {
        if (!network || !watchlist || ride.saved.length === 0) {
          ride.savedBoards = null;
          return;
        }
        const answer = await ask("board", "watchlist_board", {
          watchlist_hash: watchlist,
          network_hash: network,
          edits_hash: edits,
          date: today(),
          from_secs: nowSecs(),
          previous: previousDay(),
          limit: CARD_ROWS,
          live_hash: live ?? undefined,
        });
        ride.savedBoards = answer.stops;
      },
    },
    {
      // The route in play, drawn: the open route's, the picked bus's,
      // or the picked departure's.
      id: "directory.focus",
      frame: "RideMap",
      needs: ["network", "departure", "route", "vehicle", "view"],
      caps: ["directory"],
      clear: () => (ride.routeFocus = null),
      run: async () => {
        const route = focusRoute();
        ride.focusRoute = route;
        if (!network || !route) {
          ride.routeFocus = null;
          return;
        }
        ride.routeFocus = (await ask("directory", "focus", { network_hash: network, kind: "route", id: route })).focus;
      },
    },
    {
      // The routes, narrowed by the filter's words.
      id: "directory.entities[route]",
      frame: "RouteList",
      needs: ["network", "routeQuery", "view"],
      visible: () => ride.view === "routes",
      caps: ["directory"],
      clear: () => (ride.routeRows = null),
      run: async () => {
        if (!network) {
          ride.routeRows = null;
          return;
        }
        const query = ride.routeQuery.trim();
        ride.routeRows = (await ask("directory", "entities", { network_hash: network, kind: "route", query })).rows;
      },
    },
    {
      // One route: its directions, each with the variants a rider
      // rides (a station's bays one place).
      id: "directory.route_detail",
      frame: "RouteView",
      needs: ["network", "route", "view"],
      visible: () => ride.view === "route",
      caps: ["directory"],
      clear: () => (ride.routeInfo = null),
      run: async () => {
        const route = ride.route;
        if (!network || !route) {
          ride.routeInfo = null;
          return;
        }
        ride.routeInfo = { detail: (await ask("directory", "route_detail", { network_hash: network, route_id: route })).detail };
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
      caps: ["rt"],
      run: async () => {
        const v = ride.vehicle;
        if (!network || !boundary || !v?.trip || !snapshots.trips) {
          ride.progress = v ? { progress: null, names: [] } : null;
          return;
        }
        const { progress } = await ask("rt", "trip_progress", {
          network_hash: network,
          boundary_hash: boundary,
          rt_hash: snapshots.trips,
          trip_id: v.trip,
        });
        let names: DirectoryStop[] = [];
        if (progress && (await available("directory"))) {
          const ids = progress.stops.map((s) => s.stop_id);
          names = (await ask("directory", "stops_by_ids", { network_hash: network, ids })).stops;
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
      needs: ["network", "edits", "rt.live", "trip", "streets", "walks"],
      caps: ["router"],
      // The trip panel says it could not plan, never "no trips".
      clear: () => (ride.plan = null),
      run: async () => {
        if (!network || !trip) {
          ride.plan = null;
          return;
        }
        const t = trip;
        const ends = await routerEnds(t.from, t.to);
        planEnds = ends;
        const common = {
          network_hash: network,
          edits_hash: edits,
          from: ends.from,
          to: ends.to,
          date: t.date,
          max_legs: MAX_LEGS,
          live_hash: live,
          walks_hash: walks,
          direct: ends.direct,
        };
        if (t.when.mode === "now" || t.when.mode === "depart") {
          const { active, notice } = await ask("router", "plan_compare", { ...common, depart_secs: t.when.secs ?? nowSecs() });
          ride.plan = { itinerary: active, notice: notice?.text ?? null };
        } else {
          const deadline = t.when.mode === "arrive" ? t.when.secs : null;
          const { itinerary } = await ask("router", "plan_arrive_by", { ...common, arrive_by_secs: deadline });
          ride.plan = { itinerary, notice: null };
        }
      },
    },
    {
      // Where the planned trip runs, one path per leg.
      id: "router.itinerary_paths",
      frame: "RideMap",
      needs: ["router.plan"],
      caps: ["router"],
      clear: () => (ride.paths = null),
      run: async () => {
        const it = ride.plan?.itinerary;
        if (!network || !it || !trip) {
          ride.paths = null;
          return;
        }
        const answer = await ask("router", "itinerary_paths", {
          network_hash: network,
          edits_hash: edits,
          date: trip.date,
          itinerary: it,
          from: planEnds?.from ?? null,
          to: planEnds?.to ?? null,
          direct: planEnds?.direct ?? null,
        });
        ride.paths = answer.paths;
      },
    },
  ],
  ride.frames,
  fail,
  ride.inputs,
);

// ---------------------------------------------------------------------
// The inputs (its ui.json's `inputs`, read by the kit): each reader
// answers its document, null for none, or throws when the read fails.
// ---------------------------------------------------------------------

type Served = { network: Doc<Network>; boundary: Doc<Boundary>; pin: string };

/// The modules that read the network, in order: its pin is checked in
/// the first that assembles.
const NETWORK_READERS = ["directory", "board", "rt"] as const;

let manifest: (DeploymentManifest & { services?: Record<string, { path: string }> }) | null = null;
/// A network the rider loaded from a zip: the page stops following the
/// deployment.
let byHand: Served | null = null;
/// The deployment's feed URLs (live-feeds.json), and the rider's own
/// (the `live_feeds` document): the rider's win.
let deployedUrls: Partial<LiveUrls> = {};
let riderUrls: LiveUrls | null = null;

function adoptUrls(): void {
  ride.urls = riderUrls ? { ...riderUrls } : { trips: deployedUrls.trips ?? "", vehicles: deployedUrls.vehicles ?? "", alerts: deployedUrls.alerts ?? "" };
}

/// Name what the modules keep: the documents this page holds now —
/// everything else (a replaced network, a superseded edit document)
/// is dropped from every module (the engine's `keep`). The live
/// documents and rt snapshots bound themselves between calls.
function release(): Promise<void> {
  return keep([network, boundary, edits, live, snapshots.trips, snapshots.vehicles, snapshots.alerts, streets, walks, watchlist]);
}

/// One live feed: its bytes decoded into a snapshot (cap-rt registers
/// it), keyed by its hash. None when no URL names it or rt did not
/// assemble. A feed that does not answer drops its snapshot (it keeps
/// none): answers read the schedule and say so — never a stale
/// prediction as live.
function liveFeed(kind: keyof typeof snapshots): Reader<Doc> {
  return {
    read: async ({ signal }) => {
      const url = ride.urls[kind];
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
    adopt: (m) => {
      manifest = m;
      setAgencyZone(m?.timezone);
      // The services the deployment offers: the geocoder (an oracle).
      geocoderPath = m?.services?.geocoder?.path ?? null;
      ride.geocoding = geocoderPath !== null;
    },
  },
  "live-feeds": {
    read: ({ signal }) => readLiveFeeds(signal),
    adopt: (u: Partial<LiveUrls> | null) => {
      deployedUrls = u ?? {};
      adoptUrls();
    },
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
      const now = (await servedNetworkPin(today(), { manifest, signal })) ?? pin;
      return { value: { network: served.network, boundary: served.boundary, pin: now }, key: now };
    },
    adopt: async (got: Served | null) => {
      network = got?.network ?? null;
      boundary = got?.boundary ?? null;
      // The heading: the network's name, which the directory answers
      // (a published network is bytes this page never parsed).
      const about = network && (await available("directory")) ? await ask("directory", "about", { network_hash: network }).catch(() => null) : null;
      ride.feed = network ? about?.name || network.value?.name || "the agency's network" : "";
      await release();
    },
  },
  // The published streets and walk table: the streets document's bytes
  // go straight to the streets module, the walk table (which walks the
  // network) to the router. Until they land — or where the server
  // publishes none — walks are straight lines and trips run stop to
  // stop.
  streets: {
    read: async ({ signal }) => {
      if (!manifest || !(await available("streets"))) return null;
      const doc = await servedStreets({ manifest, signal });
      return doc ? { value: doc, key: doc.pin ?? "" } : null;
    },
    adopt: async (doc: Doc | null) => {
      streets = doc;
      ride.walking = !!doc;
      await release();
    },
  },
  walks: {
    read: async ({ signal }) => {
      if (!manifest || !(await available("router"))) return null;
      const doc = await servedWalks({ manifest, signal });
      return doc ? { value: doc, key: doc.pin ?? "" } : null;
    },
    adopt: async (doc: Doc | null) => {
      walks = doc;
      await release();
    },
  },
  edits: {
    // The edit document (detours), read-only — a rider never writes
    // edits — over the network the page holds: a new network reads it
    // again. The modules refuse any version they do not read. No sync
    // server keeps none (404).
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

const inputs = inputGraph(descriptor.inputs as InputDeclaration[], readers, ride.inputs, request);

/// What the page says of an input ("" when it is current).
export function label(input: string): string {
  return inputLabel(ride.inputs[input]);
}

/// The page's status line: a message the rider's last action left,
/// else what it holds of the schedule. "" when the schedule is current.
export function status(): string {
  if (ride.status) return ride.status;
  const state = ride.inputs.network;
  if (!state || state.kind === "reading") return network ? "" : "Loading the agency's network…";
  if (state.kind === "none" && !network) return "This server publishes no network — load the agency's GTFS zip to begin.";
  if (state.kind === "failed" && !state.held && !network) return `${inputLabel(state)} This page will try again.`;
  return "";
}

const FEEDS = ["trips", "vehicles", "alerts"] as const;

/// When the live feeds last answered, on the agency's clock ("" when
/// none has).
export function liveAt(): string {
  const at = Math.max(0, ...FEEDS.map((k) => ride.inputs[k]).map((s) => (s?.kind === "answered" ? s.atMs : 0)));
  return at ? clockLabelAt(at) : "";
}

/// The feeds whose last read failed, and why ("" when none).
export function liveError(): string {
  return FEEDS.map((k) => ride.inputs[k])
    .filter((s): s is Extract<InputState, { kind: "failed" }> => s?.kind === "failed")
    .map((s) => `${s.input}: ${s.message}`)
    .join("; ");
}

// ---------------------------------------------------------------------
// boot, and the clock
// ---------------------------------------------------------------------

export async function boot(): Promise<void> {
  // The app's build: a build that waited for this navigation applies
  // now (nothing on the page is unsaved yet).
  void installApp();
  await restoreDocuments().catch((e) => console.warn("the device's documents did not restore:", e));
  await restoreRiderUrls();
  await restoreSaved();
  await restoreRecent();
  void locateIfAllowed();
  try {
    await inputs.start();
    await request("clock", "query", "near", "stop", "departure", "trip", "saved", "view");
  } catch (e) {
    fail(e);
  } finally {
    ride.busy = false;
  }
  // The answers on the clock: a trip asked for "now" is asked again
  // from the new now.
  clockEvery(POLL_MS, () => {
    ride.updateReady = buildWaiting();
    const changed = ["clock"];
    if (trip?.when.mode === "now") {
      trip = { ...trip, date: today() };
      changed.push("trip");
    }
    void request(...changed);
  });
}

/// A GTFS zip the rider loads becomes the network document (and its
/// boundary, the stop_sequence numbering the live feeds name). The
/// page stops following the deployment.
export async function loadFeedBytes(bytes: Uint8Array, label: string): Promise<void> {
  ride.busy = true;
  ride.status = `Loading ${label}…`;
  try {
    // The import is the network module's: the only time this page
    // assembles it.
    if (!(await available("network"))) {
      ride.status = "The network capability is unavailable — a GTFS zip cannot be imported without it.";
      return;
    }
    const imported = await ask("network", "import", { feed_hash: new Doc("feed", { bytes }) });
    setAgencyZone(imported.timezone);
    byHand = {
      network: produced("network", imported.network, "network", imported.network_hash),
      boundary: new Doc<Boundary>("boundary", { value: imported.boundary }),
      pin: `zip:${imported.network_hash}`,
    };
    ride.status = "";
    await inputs.read("network", "edits");
  } catch (e) {
    fail(e);
  } finally {
    ride.busy = false;
  }
}

/// The rider accepts "a new version is ready": the page reloads into
/// it. Their choice — whatever they had typed is theirs to lose.
export function applyUpdate(): void {
  applyBuild();
}

/// The rider's own feed URLs: the `live_feeds` document, else the URLs
/// an older build kept in `localStorage`, moved into the store. None:
/// the deployment's (the `live-feeds` input).
async function restoreRiderUrls(): Promise<void> {
  try {
    const record = currentDocument("live_feeds") ?? (await moveFromLocalStorage(URLS_KEY, "live_feeds", (urls: Partial<LiveUrls>) => urls));
    if (record) {
      const feeds = JSON.parse(record.bytes) as Partial<LiveFeeds>;
      riderUrls = { trips: feeds.trips ?? "", vehicles: feeds.vehicles ?? "", alerts: feeds.alerts ?? "" };
    }
  } catch {
    // no storage: the deployment's feeds
  }
  adoptUrls();
}

export async function setLiveUrls(urls: LiveUrls): Promise<void> {
  riderUrls = { ...urls };
  adoptUrls();
  const doc: LiveFeeds = { kind: "live_feeds", schema_version: 1, trips: urls.trips, vehicles: urls.vehicles, alerts: urls.alerts };
  try {
    await putDocument("live_feeds", JSON.stringify(doc), doc.schema_version, { replace: true });
  } catch {
    // kept for this visit only
  }
  await inputs.read(...FEEDS);
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
      try {
        localStorage.setItem(FIX_KEY, JSON.stringify(ride.here));
      } catch {
        // not remembered: the map opens on the archive next time
      }
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

/// A spot tapped on the map: what is near it — or, while choosing a
/// trip's end on the map, that end (a point walks only where the
/// streets are published).
export function nearPoint(lat: number, lon: number): Promise<void> {
  const end = ride.picking;
  if (end) {
    if (!ride.walking) {
      ride.status = "Tap a stop — this server publishes no streets to walk from a spot.";
      return Promise.resolve();
    }
    ride.picking = null;
    ride.status = "";
    return spotName(lat, lon).then((name) => setEnd(end, pinPlace(end, lat, lon, name ?? undefined)));
  }
  return nearSpot(lat, lon, null);
}

/// What is near a spot — tapped, or an address picked — named by the
/// geocoder when the spot came without a name.
async function nearSpot(lat: number, lon: number, label: string | null): Promise<void> {
  ride.near = { lat, lon, mine: false, label: label ?? undefined };
  ride.view = "home";
  const named = label ? null : spotName(lat, lon);
  await request("near", "view");
  const name = await named;
  if (name && ride.near?.lat === lat && ride.near.lon === lon) ride.near = { ...ride.near, label: name };
}

// ---------------------------------------------------------------------
// the geocoder — an oracle the server's manifest lists: asked with
// the rider's input only, its answers adopted as it gives them
// ---------------------------------------------------------------------

/// Where the deployment's geocoder answers (the manifest's
/// `services.geocoder.path`); null when it lists none.
let geocoderPath: string | null = null;
let geocodeAsked = 0;

type GeoFeature = {
  geometry: { coordinates: [number, number] };
  properties: { gid: string; name: string; label: string; locality?: string; layer?: string };
};

async function geocoderAsk(op: "autocomplete" | "reverse", params: Record<string, string | number>): Promise<GeoFeature[] | null> {
  if (!geocoderPath) return null;
  const query = new URLSearchParams(Object.entries(params).map(([k, v]) => [k, String(v)]));
  const answer = await askOracle<{ features?: GeoFeature[] }>(`${geocoderPath}/${op}?${query}`);
  return answer ? (answer.features ?? []) : null;
}

/// The geocoder's places for what was typed, focused where the rider
/// is looking. A trip end from an address walks the streets, so the
/// from/to fields ask only where the streets are published.
async function geocode(field: Field, text: string): Promise<void> {
  const asked = ++geocodeAsked;
  const t = text.trim();
  if (!t || (field !== "search" && !ride.walking)) {
    ride.addresses = null;
    return;
  }
  const focus = ride.near ?? ride.here;
  const features = await geocoderAsk("autocomplete", {
    text: t,
    ...(focus ? { "focus.point.lat": focus.lat, "focus.point.lon": focus.lon } : {}),
  });
  if (asked !== geocodeAsked) return;
  ride.addresses = features && {
    field,
    text: t,
    places: features.map((f) => ({
      id: f.properties.gid,
      name: f.properties.name,
      detail: f.properties.locality,
      lat: f.geometry.coordinates[1],
      lon: f.geometry.coordinates[0],
      point: true,
    })),
  };
}

/// The geocoder's name for a spot (what the deployment's policy finds
/// nearest — D-37), or null when it has none or is not running.
async function spotName(lat: number, lon: number): Promise<string | null> {
  const features = await geocoderAsk("reverse", { "point.lat": lat, "point.lon": lon });
  return features?.[0]?.properties.name ?? null;
}

// ---------------------------------------------------------------------
// the selection — each handler sets its input and requests it
// ---------------------------------------------------------------------

export function typed(field: Field, text: string): Promise<void> {
  ride.query = { field, text };
  void geocode(field, text);
  return request("query");
}

export function clearFound(): void {
  ride.query = null;
  ride.found = null;
  ride.addresses = null;
}

export function show(view: View): Promise<void> {
  ride.picking = null;
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
  const end = ride.picking;
  if (end) {
    ride.picking = null;
    ride.status = "";
    return setEnd(end, located(place));
  }
  clearFound();
  // An address picked from the search: what is near it.
  if (place.point && place.lat != null && place.lon != null) return nearSpot(place.lat, place.lon, place.name);
  place = located(place);
  ride.stop = place;
  ride.board = null;
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
/// rider's location when the streets can walk it, else the place
/// nearest the rider when the directory answered one.
export function directions(end: "from" | "to", place: Place): Promise<void> {
  clearFound();
  ride[end] = place;
  const other = end === "from" ? "to" : "from";
  const here = ride.walking ? herePlace() : null;
  if (!ride[other] && here && place.id !== HERE_ID) ride[other] = here;
  else if (!ride[other] && ride.near?.mine && ride.nearby?.[0] && ride.nearby[0].id !== place.id) ride[other] = ride.nearby[0];
  ride.view = "trip";
  return replan();
}

/// Directions to the spot tapped on the map (a dropped pin).
export function directionsToSpot(): Promise<void> {
  const near = ride.near;
  if (!near || near.mine) return Promise.resolve();
  return directions("to", pinPlace("to", near.lat, near.lon, near.label));
}

/// Choose a trip's end on the map: the next tap — a stop, or (with
/// the streets published) any spot — sets it. Asked again, cancelled.
export function chooseOnMap(end: "from" | "to" | null): void {
  ride.picking = ride.picking === end ? null : end;
  ride.status = "";
}

/// The rider's location as one end of the trip.
export function fromHere(end: "from" | "to"): Promise<void> {
  const here = herePlace();
  if (!here) return Promise.resolve();
  ride[end] = here;
  return replan();
}

export function setEnd(end: "from" | "to", place: Place): Promise<void> {
  clearFound();
  // A point (an address) becomes this end's pin.
  if (place.point && place.id !== HERE_ID && place.lat != null && place.lon != null)
    place = pinPlace(end, place.lat, place.lon, place.name);
  ride[end] = place;
  remember(place);
  return replan();
}

export function swapEnds(): Promise<void> {
  // A pin's id names its end.
  const repin = (p: Place | null, end: "from" | "to") =>
    p?.id.startsWith("@pin-") && p.lat != null && p.lon != null ? pinPlace(end, p.lat, p.lon, p.name) : p;
  [ride.from, ride.to] = [repin(ride.to, "from"), repin(ride.from, "to")];
  return replan();
}

export function setWhen(when: When): Promise<void> {
  ride.when = when;
  return replan();
}

function replan(): Promise<void> {
  trip =
    ride.from && ride.to && ride.from.id !== ride.to.id
      ? { from: { ...ride.from }, to: { ...ride.to }, date: today(), when: { ...ride.when } }
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
  let doc: Doc<Watchlist> | null = null;
  try {
    const record = currentDocument("watchlist");
    doc = record ? await readStoredWatchlist(record) : null;
  } catch (e) {
    // An older version this page cannot bring forward, or one that does
    // not read: the stored record stays as it is.
    console.warn("the saved stops did not restore:", e);
  }
  watchlist = doc;
  ride.saved = doc?.value ? doc.value.stops.map((s) => ({ id: s.id, label: s.label })) : [];
}

/// A stored watchlist in the version the board reads. The board's load
/// decides: one it refuses comes forward through the migration
/// capability when the deployment serves it (fetched only then), and the
/// migrated document is a new record whose `produced_by` names the old
/// one. A version nothing here brings forward stays as stored.
async function readStoredWatchlist(record: DocRecord): Promise<Doc<Watchlist>> {
  const stored: unknown = JSON.parse(record.bytes);
  const doc = new Doc<Watchlist>("watchlist", { value: stored as Watchlist });
  try {
    await load("board", doc);
    return doc;
  } catch (refused) {
    if (!(await available("migrate"))) throw refused;
    const { version, reads } = await ask("migrate", "version", { kind: "watchlist", document: stored });
    if (version >= reads) throw refused;
    const migrated = await ask("migrate", "migrate", { kind: "watchlist", document: stored });
    await putDocument("watchlist", migrated.canonical, migrated.to, { produced_by: { op: "migrate", from: record.hash, by: "migrate.migrate" } });
    const next = new Doc<Watchlist>("watchlist", { value: migrated.document as Watchlist });
    await load("board", next);
    return next;
  }
}

async function writeSaved(stops: { id: string; label: string }[]): Promise<void> {
  const doc: Watchlist = { kind: "watchlist", schema_version: 2, stops };
  try {
    await putDocument("watchlist", JSON.stringify(doc), doc.schema_version);
  } catch (e) {
    console.warn("watchlist save failed:", e);
    ride.status = "Couldn't save on this device — your saved stops last until you leave.";
  }
  watchlist = new Doc("watchlist", { value: doc });
  ride.saved = stops;
  await request("saved");
}

export function toggleSaved(place: Place): Promise<void> {
  return ride.saved.some((s) => s.id === place.id)
    ? writeSaved(ride.saved.filter((s) => s.id !== place.id))
    : writeSaved([...ride.saved, { id: place.id, label: place.name }]);
}

/// The rider's recent places: the `recent_places` document, else the
/// list an older build kept in `localStorage`, moved into the store.
async function restoreRecent(): Promise<void> {
  ride.recent = [];
  try {
    const record = currentDocument("recent_places") ?? (await moveFromLocalStorage(RECENT_KEY, "recent_places", (places) => ({ places })));
    if (record) ride.recent = (JSON.parse(record.bytes) as RecentPlaces).places;
  } catch (e) {
    console.warn("the recent places did not restore:", e);
  }
}

function remember(place: Place): void {
  if (place.point) return;
  ride.recent = [{ id: place.id, name: place.name, lat: place.lat, lon: place.lon }, ...ride.recent.filter((p) => p.id !== place.id)].slice(0, 6);
  const doc: RecentPlaces = { kind: "recent_places", schema_version: 1, places: ride.recent.map(({ id, name, lat, lon }) => ({ id, name, lat, lon })) };
  putDocument("recent_places", JSON.stringify(doc), doc.schema_version, { replace: true }).catch(() => {
    // recent stops last this visit
  });
}

/// A device record an older build kept under a `localStorage` key, moved
/// into the store as the tagged document `kind` (the record names the key
/// it came from), and the key removed. Null when the key holds nothing.
async function moveFromLocalStorage(key: string, kind: "recent_places" | "live_feeds", tag: (stored: never) => object): Promise<DocRecord | null> {
  const stored = localStorage.getItem(key);
  if (stored === null) return null;
  const doc = { kind, schema_version: 1, ...tag(JSON.parse(stored) as never) };
  const record = await putDocument(kind, JSON.stringify(doc), 1, { produced_by: { op: "moved", from: `localStorage:${key}` } });
  localStorage.removeItem(key);
  return record;
}
