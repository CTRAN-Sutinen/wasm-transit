/// The disruptions UI's host layer: the ONLY file of this UI that asks
/// the capability bus. It owns I/O (the deployment's network, the edit
/// document from GET /edits, the service alerts and notes by the
/// manifest's hashes, and the agency's live feeds), the clock, and the
/// selection. Every list, count, and line it writes into `dz` is an
/// op's answer, adopted as-is.
///
/// One question underlies the page: what is disrupted today, and does
/// an alert explain it (cap-disruptions, D-213, D-218)? The views read
/// it for different people:
///
/// - Status: each affected route as a card, for anyone who answers a
///   rider. A search narrows it to one route or one stop.
/// - Dispatch: the buses late now, the detours, the cancelled trips,
///   and today's notes.
/// - A disruption opened: what it is, the stops it takes away, and its
///   notes (D-214). A cancelled trip draws its path on the map.
///
/// The map is always on screen: the live buses by status, the cancelled
/// trips at their first stops, the closed stops, and the stops a detour
/// skips. The page polls the edit document, the alerts and notes, the
/// live feeds, and the clock while it is visible. The view, the search,
/// and the open disruption live in the URL hash.

import { ask, assemble, available, keep, load, produced, servedNetwork, servedNotes, servedServiceAlerts, servedStops, Doc } from "../../engine";
import { answerGraph, clockLabel, deploymentFeeds, deploymentManifest, fetchFeed, readAgencyZone, reason, staleLabel, today, whileVisible, type FrameState, type LiveUrls } from "../kit";
import { resumeSignIn } from "../../signin.svelte";
import type { AlertSource } from "../../types/AlertSource";
import type { Boundary } from "../../types/Boundary";
import type { Coord } from "../../types/Coord";
import type { Disruption } from "../../types/Disruption";
import type { DirectoryStop } from "../../types/cap_directory/DirectoryStop";
import type { EditDocument } from "../../types/EditDocument";
import type { EntityRow } from "../../types/EntityRow";
import type { LateTrip } from "../../types/LateTrip";
import type { LiveDay } from "../../types/LiveDay";
import type { Network } from "../../types/Network";
import type { NoteRow } from "../../types/NoteRow";
import type { Today } from "../../types/Today";
import type { TripStopView } from "../../types/TripStopView";

// This page's capabilities answer inside its linked assembly (D-77).
assemble("disruptions");

export type View = "status" | "dispatch";
const VIEWS: View[] = ["status", "dispatch"];

/// How often the page reads the documents, the live feeds, and the
/// clock again (one timer, kit's whileVisible).
const POLL_MS = 30_000;
/// How many search results of each kind the box shows.
const FOUND = 8;

/// The search's pick: one route or one stop, with the name it was
/// picked by.
export type Pick = { kind: "route" | "stop"; id: string; label: string };

export const dz = $state({
  status: "Reading the network…",
  alert: false,
  /// The agency's date and wall clock, as of the last poll.
  date: "",
  clock: "",
  /// The agency's zone (the manifest's `timezone`): the components
  /// format Unix times in it.
  zone: undefined as string | undefined,
  view: "status" as View,
  /// The search: the text typed, its results, and the pick.
  query: "",
  found: null as { routes: EntityRow[]; stops: DirectoryStop[] } | null,
  pick: null as Pick | null,
  /// The day by route (disruptions `today`): narrowed by the pick, and
  /// whole for the map and Dispatch.
  status_: null as Today | null,
  day: null as Today | null,
  /// The live document (its buses and its on-time window), and the
  /// trips late now (rt `late_trips`).
  live: null as LiveDay | null,
  late: [] as LateTrip[],
  liveAt: "",
  liveError: "",
  editsStale: "",
  /// Today's notes, newest first (notes `between`).
  notes: [] as NoteRow[],
  /// The disruption open, by its source; its notes, oldest first
  /// (notes `on`), and a cancelled trip's stops and path.
  open: null as AlertSource | null,
  /// The open disruption as the day's answer gives it; null while the
  /// day does not hold it (a cancel the feed dropped).
  openItem: null as Disruption | null,
  /// The open disruption's route, by its public name ("" for none).
  openRoute: "",
  openNotes: [] as NoteRow[],
  openStops: [] as TripStopView[],
  openPath: [] as Coord[],
  /// Where a late bus picked in Dispatch is: the map goes to it.
  bus: null as { lat: number; lon: number } | null,
  /// Each frame that shows no answer, and why.
  frames: {} as Record<string, FrameState>,
});

let network: Doc<Network> | null = null;
let boundary: Doc<Boundary> | null = null;
let edits: Doc<EditDocument> | null = null;
let editsText = "";
let stops: Doc | null = null;
let alerts: Doc | null = null;
let notes: Doc | null = null;
let live: Doc<LiveDay> | null = null;
const snapshots = { trips: null as Doc | null, vehicles: null as Doc | null, alerts: null as Doc | null };
let urls: Partial<LiveUrls> = {};

function say(text: string, alert = false): void {
  dz.status = text;
  dz.alert = alert;
}

function message(e: unknown): string {
  return (e as Error)?.message ?? String(e);
}

/// The host's clock, Unix seconds: what an alert explains now.
function nowUnix(): number {
  return Math.floor(Date.now() / 1000);
}

/// The documents the day reads, as the disruptions ops take them.
function documents() {
  return {
    network_hash: network!,
    edits_hash: edits ?? undefined,
    live_hash: live ?? undefined,
    stops_hash: stops ?? undefined,
    alerts_hash: alerts ?? undefined,
    notes_hash: notes ?? undefined,
    date: dz.date,
    now: nowUnix(),
  };
}

// ---------------------------------------------------------------------
// This UI's answer graph (src/uis/kit.ts). Inputs: "network", "docs"
// (the edit document, the stops, the alerts, the notes), "snapshot"
// (the decoded live feeds), "clock", "view", "query", "pick", "open".
// ---------------------------------------------------------------------

const request = answerGraph(
  [
    {
      // THE live document over the feeds fetched: the map draws its
      // buses, and the day reads its cancels and lateness by hash.
      id: "rt.live",
      frame: "Map",
      needs: ["network", "snapshot"],
      caps: ["rt"],
      clear: () => ((live = null), (dz.live = null)),
      run: async () => {
        const { trips, vehicles, alerts: rtAlerts } = snapshots;
        if (!network || !boundary || (!trips && !vehicles)) return void ((live = null), (dz.live = null));
        const answer = await ask("rt", "live", {
          network_hash: network,
          boundary_hash: boundary,
          rt_hash: trips ?? undefined,
          vp_hash: vehicles ?? undefined,
          alerts_hash: rtAlerts ?? undefined,
        });
        if (!answer.live) throw new Error("the live answer carries no document");
        live = produced("live", answer.live, "rt", answer.live_hash);
        dz.live = answer.live;
      },
    },
    {
      // The day, whole: the map and Dispatch.
      id: "disruptions.today",
      frame: "Day",
      needs: ["network", "docs", "rt.live", "clock"],
      caps: ["disruptions"],
      clear: () => (dz.day = null),
      run: async () => {
        if (!network) return void (dz.day = null);
        const { op: _, ...today } = await ask("disruptions", "today", documents());
        dz.day = today;
        hold();
      },
    },
    {
      // The open disruption, held again from the day (it may have
      // left it).
      id: "selection.open",
      frame: "",
      needs: ["open"],
      run: async () => hold(),
    },
    {
      // The day narrowed to the search's pick: the Status view.
      id: "disruptions.today?pick",
      frame: "Status",
      needs: ["network", "docs", "rt.live", "clock", "pick", "view"],
      visible: () => dz.view === "status",
      caps: ["disruptions"],
      clear: () => (dz.status_ = null),
      run: async () => {
        if (!network) return void (dz.status_ = null);
        const pick = dz.pick;
        const { op: _, ...today } = await ask("disruptions", "today", {
          ...documents(),
          ...(pick?.kind === "route" ? { route: pick.id } : {}),
          ...(pick?.kind === "stop" ? { stop: pick.id } : {}),
        });
        dz.status_ = today;
      },
    },
    {
      // The trips late now, worst first.
      id: "rt.late_trips",
      frame: "Late",
      needs: ["network", "snapshot", "rt.live", "view"],
      visible: () => dz.view === "dispatch",
      caps: ["rt"],
      clear: () => (dz.late = []),
      run: async () => {
        if (!network || !snapshots.trips || !dz.live) return void (dz.late = []);
        dz.late = (
          await ask("rt", "late_trips", {
            network_hash: network,
            rt_hash: snapshots.trips,
            vp_hash: snapshots.vehicles,
            // The live document's own window: late is what it calls late.
            min_delay_secs: dz.live.window.late_secs,
          })
        ).trips;
      },
    },
    {
      // Today's notes, newest first.
      id: "notes.between",
      frame: "Notes",
      needs: ["docs", "clock", "view"],
      visible: () => dz.view === "dispatch",
      caps: ["notes"],
      clear: () => (dz.notes = []),
      run: async () => {
        if (!notes) return void (dz.notes = []);
        const [from, to] = dayBounds(dz.date);
        dz.notes = (await ask("notes", "between", { notes_hash: notes, from, to })).notes;
      },
    },
    {
      // The search box: routes and stops by name or id.
      id: "directory.entities+stops?query",
      frame: "Search",
      needs: ["network", "query"],
      caps: ["directory"],
      clear: () => (dz.found = null),
      run: async () => {
        const text = dz.query.trim();
        if (!network || !text) return void (dz.found = null);
        const routes = (await ask("directory", "entities", { network_hash: network, kind: "route", query: text })).rows.slice(0, FOUND);
        const found = (await ask("directory", "stops", { network_hash: network, query: text, limit: FOUND })).stops;
        dz.found = { routes, stops: found };
      },
    },
    {
      // The open disruption's notes, oldest first. A disruption's
      // source names the same thing as a note's target, in the same
      // shape (D-217).
      id: "notes.on",
      frame: "OpenNotes",
      needs: ["docs", "open"],
      caps: ["notes"],
      clear: () => (dz.openNotes = []),
      run: async () => {
        if (!notes || !dz.open) return void (dz.openNotes = []);
        dz.openNotes = (await ask("notes", "on", { notes_hash: notes, target: dz.open })).notes;
      },
    },
    {
      // A cancelled trip opened: its stops and its path as scheduled.
      // The edit document's cancel takes the trip out of the patched
      // day, so the schedule is what the cancel takes away.
      id: "runcut.trip_stops+trip_path",
      frame: "OpenTrip",
      needs: ["network", "disruptions.today", "open"],
      caps: ["runcut"],
      clear: () => ((dz.openStops = []), (dz.openPath = [])),
      run: async () => {
        const d = dz.openItem;
        const trip = d?.trip && d.start_secs != null ? { trip: d.trip, date: dz.date, start_secs: d.start_secs } : null;
        if (!network || !trip) return void ((dz.openStops = []), (dz.openPath = []));
        const args = { network_hash: network, edits_hash: null, ...trip };
        dz.openStops = (await ask("runcut", "trip_stops", args)).stops;
        dz.openPath = (await ask("runcut", "trip_path", args)).path;
      },
    },
  ],
  dz.frames,
  (e) => say(`Error: ${message(e)}`, true),
);

/// The open disruption held from the day: the first with its source
/// (a closed stop is listed on each route that serves it), with its
/// route's name. Null when the day does not hold it.
function hold(): void {
  dz.openItem = null;
  dz.openRoute = "";
  if (!dz.open || !dz.day) return;
  const key = JSON.stringify(dz.open);
  for (const r of dz.day.routes)
    for (const d of r.disruptions)
      if (JSON.stringify(d.source) === key) {
        dz.openItem = d;
        dz.openRoute = d.from === "stops" ? "" : r.name;
        return;
      }
}

/// The service date's span in Unix seconds, from its midnight to the
/// next, in the agency's zone: the notes written that day.
function dayBounds(date: string): [number, number] {
  const at = (d: string) => {
    // The zone's offset at that midnight, read back from the clock.
    const guess = Date.parse(`${d}T00:00:00Z`);
    const parts = new Intl.DateTimeFormat("en-US", {
      timeZone: dz.zone,
      hourCycle: "h23",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }).formatToParts(new Date(guess));
    const get = (t: string) => Number(parts.find((p) => p.type === t)?.value);
    const seen = Date.UTC(get("year"), get("month") - 1, get("day"), get("hour"), get("minute"));
    return Math.floor((guess - (seen - guess)) / 1000);
  };
  const next = new Date(Date.parse(`${date}T12:00:00Z`) + 86_400_000).toISOString().slice(0, 10);
  return [at(date), at(next)];
}

// ---------------------------------------------------------------------
// the URL hash: the view, the pick, and the open disruption
// ---------------------------------------------------------------------

let writing = false;

function writeHash(replace = false): void {
  const q = new URLSearchParams();
  if (dz.view !== "status") q.set("view", dz.view);
  if (dz.pick) q.set(dz.pick.kind, dz.pick.id);
  if (dz.pick) q.set("label", dz.pick.label);
  if (dz.open) q.set("open", JSON.stringify(dz.open));
  const text = q.toString();
  const next = text ? `#${text}` : location.pathname + location.search;
  if (location.hash === (text ? next : "")) return;
  writing = true;
  if (replace) history.replaceState(null, "", next);
  else history.pushState(null, "", next);
  writing = false;
}

function readHash(): string[] {
  const q = new URLSearchParams(location.hash.slice(1));
  const changed: string[] = [];
  const view = (q.get("view") ?? "status") as View;
  if ((VIEWS.includes(view) ? view : "status") !== dz.view) ((dz.view = VIEWS.includes(view) ? view : "status"), changed.push("view"));
  const route = q.get("route");
  const stop = q.get("stop");
  const pick: Pick | null = route ? { kind: "route", id: route, label: q.get("label") ?? route } : stop ? { kind: "stop", id: stop, label: q.get("label") ?? stop } : null;
  if (JSON.stringify(pick) !== JSON.stringify(dz.pick)) ((dz.pick = pick), changed.push("pick"));
  let open: AlertSource | null = null;
  try {
    open = q.get("open") ? (JSON.parse(q.get("open")!) as AlertSource) : null;
  } catch {
    open = null;
  }
  if (JSON.stringify(open) !== JSON.stringify(dz.open)) ((dz.open = open), changed.push("open"));
  return changed;
}

function followHash(): void {
  if (writing) return;
  const changed = readHash();
  if (changed.length) void request(...changed);
}
addEventListener("hashchange", followHash);
addEventListener("popstate", followHash);

// ---------------------------------------------------------------------
// boot and the poll
// ---------------------------------------------------------------------

export async function boot(): Promise<void> {
  await Promise.all([readAgencyZone(), resumeSignIn()]);
  dz.zone = (await deploymentManifest<{ timezone?: string }>())?.timezone;
  dz.date = today();
  dz.clock = clockLabel();
  readHash();
  try {
    const served = await servedNetwork(dz.date, "disruptions", "rt", "directory", "runcut");
    if (!served) return say("The deployment publishes no network.", true);
    network = served.network;
    boundary = served.boundary;
  } catch (e) {
    return say(`The network could not be read: ${message(e)}`, true);
  }
  urls = await deploymentFeeds();
  await Promise.all([readEdits(), readDocuments(), fetchLive()]);
  await release();
  say("");
  writeHash(true);
  await request("network", "docs", "snapshot", "clock", "view", "query", "pick", "open");
  whileVisible(POLL_MS, () => void poll());
}

let polling = false;

/// One tick: the documents, the live feeds, and the clock.
export async function poll(): Promise<void> {
  if (!network || polling) return;
  polling = true;
  try {
    const date = today();
    const changed = ["clock"];
    if (date !== dz.date) dz.date = date;
    dz.clock = clockLabel();
    const [e, d, l] = await Promise.all([readEdits(), readDocuments(), fetchLive()]);
    if (e || d) changed.push("docs");
    if (l) changed.push("snapshot");
    if (e || d) await release();
    await request(...changed);
  } finally {
    polling = false;
  }
}

/// Name what the modules keep: the documents this page holds now.
function release(): Promise<void> {
  return keep([network, boundary, edits, stops, alerts, notes, live, snapshots.trips, snapshots.vehicles, snapshots.alerts]);
}

/// The sync server's edit document (detours and cancels). An
/// unreadable answer keeps the last good document. True when it
/// changed.
async function readEdits(): Promise<boolean> {
  if (!network) return false;
  try {
    const resp = await fetch("edits");
    if (!resp.ok) return false;
    dz.editsStale = staleLabel(resp, "Detours and cancels");
    const text = await resp.text();
    if (text === editsText) return false;
    editsText = text;
    const document = JSON.parse(text) as EditDocument;
    edits = document?.records?.length > 0 ? new Doc("edits", { value: document }, { over: network }) : null;
    return true;
  } catch (e) {
    console.warn("edit sync failed:", e);
    return false;
  }
}

/// The stops, the service alerts, and the notes, by the manifest's
/// hashes: each read again only when its hash moved. True when any
/// changed.
async function readDocuments(): Promise<boolean> {
  const manifest = (await fetch("manifest", { cache: "no-cache" })
    .then((r) => (r.ok ? r.json() : null))
    .catch(() => null)) as { stops_hash?: string; service_alerts_hash?: string; notes_hash?: string } | null;
  if (!manifest) return false;
  let changed = false;
  if ((manifest.stops_hash ?? null) !== (stops?.pin ?? null)) ((stops = await servedStops()), (changed = true));
  if ((manifest.service_alerts_hash ?? null) !== (alerts?.pin ?? null)) ((alerts = await servedServiceAlerts()), (changed = true));
  if ((manifest.notes_hash ?? null) !== (notes?.pin ?? null)) ((notes = await servedNotes()), (changed = true));
  return changed;
}

/// Fetch each live feed the deployment names, and decode it (cap-rt
/// registers the snapshot). A feed that does not answer drops its
/// snapshot: the page shows the schedule's day, never a stale bus as
/// live. True when any snapshot changed.
async function fetchLive(): Promise<boolean> {
  const kinds = (["trips", "vehicles", "alerts"] as const).filter((k) => urls[k]);
  if (!kinds.length || !(await available("rt"))) return false;
  const errors: string[] = [];
  let changed = false;
  await Promise.all(
    kinds.map(async (kind) => {
      try {
        const resp = await fetchFeed(urls[kind]!);
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        const snapshot = new Doc("gtfs_rt", { bytes: new Uint8Array(await resp.arrayBuffer()) });
        const { hash } = await load("rt", snapshot);
        if (hash !== snapshots[kind]?.hash) ((snapshots[kind] = snapshot), (changed = true));
      } catch (e) {
        errors.push(`${kind}: ${message(e)}`);
        if (snapshots[kind]) ((snapshots[kind] = null), (changed = true));
      }
    }),
  );
  dz.liveError = errors.join("; ");
  if (errors.length < kinds.length) dz.liveAt = clockLabel();
  return changed;
}

// ---------------------------------------------------------------------
// the selection
// ---------------------------------------------------------------------

export function setView(view: View): Promise<void> {
  dz.view = view;
  writeHash();
  return request("view");
}

export function setQuery(text: string): Promise<void> {
  dz.query = text;
  return request("query");
}

/// A route or a stop picked in the search, or the pick cleared.
export function setPick(pick: Pick | null): Promise<void> {
  dz.pick = pick;
  dz.query = "";
  dz.found = null;
  writeHash();
  return request("pick");
}

/// A disruption opened (or closed, with null).
export function openDisruption(source: AlertSource | null): Promise<void> {
  dz.open = source;
  dz.bus = null;
  writeHash();
  return request("open");
}

/// A late bus picked: the map goes to it.
export function showBus(at: { lat: number; lon: number }): void {
  dz.bus = at;
}

/// Why a frame has no answer, as text ("" when it answered).
export function why(frame: string): string {
  return reason(dz.frames[frame]);
}

/// Each failed frame asks again: errors are never cached.
export function retry(): Promise<void> {
  return request.retry();
}
