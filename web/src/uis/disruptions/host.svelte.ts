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

import { ask, assemble, available, keep, load, produced, servedAlertTemplates, servedNetwork, servedNotes, servedServiceAlerts, servedStops, Doc } from "../../engine";
import { answerGraph, clockLabel, nowSecs, previousDay, deploymentFeeds, deploymentManifest, fetchFeed, readAgencyZone, reason, staleLabel, today, whileVisible, type FrameState, type LiveUrls } from "../kit";
import { resumeSignIn, writeFetch } from "../../signin.svelte";
import type { AlertFlag } from "../../types/AlertFlag";
import type { AlertGap } from "../../types/AlertGap";
import type { AlertList } from "../../types/AlertList";
import type { AlertRefusal } from "../../types/AlertRefusal";
import type { EntityReach } from "../../types/cap_board/EntityReach";
import type { Reach } from "../../types/Reach";
import type { DirectoryRoute } from "../../types/cap_directory/DirectoryRoute";
import type { NoteRefusal } from "../../types/NoteRefusal";
import type { NoteTarget } from "../../types/NoteTarget";
import type { ServiceAlert } from "../../types/ServiceAlert";
import type { StopBoard } from "../../types/StopBoard";
import type { TemplateRow } from "../../types/TemplateRow";
import type { Unfilled } from "../../types/Unfilled";
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

export type View = "status" | "alerts" | "dispatch";
const VIEWS: View[] = ["status", "alerts", "dispatch"];

/// The alert open in the form: its id, whether it is new, the alert as
/// it stands, and the blanks the last fill left (alerts `fill`).
/// `startsNow`: riders are told from the moment it is saved; the start
/// is stamped then.
export type AlertForm = { id: string; fresh: boolean; alert: ServiceAlert; template: string; unfilled: Unfilled[]; startsNow: boolean };

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
  /// The host's clock, Unix seconds, as of the last poll.
  now: 0,
  /// The agency's alerts (alerts `list`), the disruptions no alert
  /// explains (disruptions `gaps`), the templates offered (alerts
  /// `templates`), and every route (directory `routes`: names and
  /// colors).
  alerts: null as AlertList | null,
  gaps: [] as AlertGap[],
  templates: [] as TemplateRow[],
  routes: [] as DirectoryRoute[],
  /// The alert open in the form, its stops by name (directory
  /// `stops_by_ids`), what a write must pass, what a person is told
  /// (alerts `flags`), and the rider preview: a stop's board with the
  /// alert published (alerts `put`, rt `encode_alerts`, rt `live`,
  /// board `stop_board`) and the feed's bytes.
  form: null as AlertForm | null,
  formStops: [] as { stop_id: string; name: string }[],
  /// What the form's alert reaches today, entity by entity (board
  /// `alert_reach`), and every stop, for the map that picks them.
  reach: null as { entities: EntityReach[]; reach: Reach } | null,
  allStops: [] as DirectoryStop[],
  /// The rider preview, open in its dialog.
  previewOpen: false,
  refusals: [] as AlertRefusal[],
  flags: [] as AlertFlag[],
  previewStop: null as { id: string; name: string } | null,
  /// The preview's own stop search.
  previewQuery: "",
  previewFound: null as { routes: EntityRow[]; stops: DirectoryStop[] } | null,
  preview: null as StopBoard | null,
  /// The preview waits on the form: what Publish needs is missing.
  previewWaits: false,
  previewFeed: "",
  /// A write under way, and why the last one was refused (in words).
  busy: false,
  writeError: "",
  noteRefusals: [] as NoteRefusal[],
  frames: {} as Record<string, FrameState>,
});

let network: Doc<Network> | null = null;
let boundary: Doc<Boundary> | null = null;
let edits: Doc<EditDocument> | null = null;
let editsText = "";
let stops: Doc | null = null;
let alerts: Doc | null = null;
let notes: Doc | null = null;
let templates: Doc | null = null;
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
    // The clock the page last read (each poll, and each write).
    now: dz.now,
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
    {
      id: "directory.stops?preview",
      frame: "",
      needs: ["network", "preview-query"],
      run: async () => {
        const text = dz.previewQuery.trim();
        dz.previewFound = network && text ? { routes: [], stops: (await ask("directory", "stops", { network_hash: network, query: text, limit: FOUND })).stops } : null;
      },
    },
    {
      // Every route, by name and color: the preview's badges and the
      // form's route names.
      id: "directory.routes",
      frame: "",
      needs: ["network"],
      run: async () => {
        dz.routes = network && (await available("directory")) ? (await ask("directory", "routes", { network_hash: network })).routes : [];
      },
    },
    {
      id: "alerts.list",
      frame: "Alerts",
      needs: ["docs", "clock", "view"],
      visible: () => dz.view === "alerts",
      caps: ["alerts"],
      clear: () => (dz.alerts = null),
      run: async () => {
        const { op: _, ...list } = await ask("alerts", "list", { alerts_hash: alerts ?? undefined, now: dz.now });
        dz.alerts = list;
      },
    },
    {
      id: "disruptions.gaps",
      frame: "Gaps",
      needs: ["network", "docs", "rt.live", "clock", "view"],
      visible: () => dz.view === "alerts",
      caps: ["disruptions"],
      clear: () => (dz.gaps = []),
      run: async () => {
        dz.gaps = network ? (await ask("disruptions", "gaps", documents())).gaps : [];
      },
    },
    {
      id: "alerts.templates",
      frame: "Templates",
      needs: ["docs"],
      caps: ["alerts"],
      clear: () => (dz.templates = []),
      run: async () => {
        dz.templates = (await ask("alerts", "templates", { templates_hash: templates ?? undefined })).templates;
      },
    },
    {
      // The form's stops by name.
      id: "directory.stops_by_ids",
      frame: "",
      needs: ["network", "form"],
      run: async () => {
        const ids = [...new Set((dz.form?.alert.informed ?? []).map((e) => e.stop).filter((x): x is string => !!x))];
        dz.formStops = network && ids.length ? (await ask("directory", "stops_by_ids", { network_hash: network, ids })).stops : [];
      },
    },
    {
      // What a write of the form's alert must pass, and what a person
      // is told about it.
      id: "alerts.put+flags",
      frame: "Form",
      needs: ["network", "docs", "form"],
      caps: ["alerts"],
      clear: () => ((dz.refusals = []), (dz.flags = [])),
      run: async () => {
        const form = dz.form;
        if (!form || !network) return void ((dz.refusals = []), (dz.flags = []));
        const put = await ask("alerts", "put", { alerts_hash: alerts ?? undefined, id: form.id, alert: stamped(form) });
        dz.refusals = put.refusals;
        if (!put.alerts || !put.alerts_hash) return void (dz.flags = []);
        const next = produced("service_alerts", put.alerts, "alerts", put.alerts_hash);
        dz.flags = (await ask("alerts", "flags", { alerts_hash: next, network_hash: network })).flags.filter((f) => f.alert === form.id);
      },
    },
    {
      // Every stop: the reach map's stops to pick.
      id: "directory.stops[all]",
      frame: "",
      needs: ["network", "form"],
      visible: () => !!dz.form,
      clear: () => (dz.allStops = []),
      run: async () => {
        if (!network || dz.allStops.length) return;
        dz.allStops = (await ask("directory", "stops", { network_hash: network, limit: null })).stops;
      },
    },
    {
      // What the alert reaches today, read as a rider's app reads the
      // feed.
      id: "board.alert_reach",
      frame: "Reach",
      needs: ["network", "docs", "form", "clock"],
      caps: ["board"],
      clear: () => (dz.reach = null),
      run: async () => {
        const form = dz.form;
        if (!form || !network) return void (dz.reach = null);
        const { op: _, ...reach } = await ask("board", "alert_reach", { network_hash: network, edits_hash: edits ?? undefined, date: dz.date, alert: stamped(form) });
        dz.reach = reach;
      },
    },
    {
      // What a rider would see at the preview stop with the alert
      // published: the board over a live document whose alerts are the
      // agency's feed encoded with it.
      id: "preview",
      frame: "Preview",
      needs: ["network", "docs", "snapshot", "form", "preview-stop", "clock", "preview-open"],
      visible: () => dz.previewOpen,
      caps: ["alerts", "rt", "board"],
      clear: () => ((dz.preview = null), (dz.previewFeed = "")),
      run: async () => {
        const form = dz.form;
        if (!form || !network || !boundary) return void ((dz.preview = null), (dz.previewFeed = ""));
        const put = await ask("alerts", "put", { alerts_hash: alerts ?? undefined, id: form.id, alert: { ...stamped(form), status: "published" } });
        dz.previewWaits = !put.alerts;
        if (!put.alerts || !put.alerts_hash) return void ((dz.preview = null), (dz.previewFeed = ""));
        const doc = produced("service_alerts", put.alerts, "alerts", put.alerts_hash);
        const encoded = await ask("rt", "encode_alerts", { network_hash: network, alerts_hash: doc, timestamp: dz.now });
        dz.previewFeed = encoded.feed_b64;
        const stop = dz.previewStop;
        if (!stop) return void (dz.preview = null);
        const previewLive = await ask("rt", "live", {
          network_hash: network,
          boundary_hash: boundary,
          rt_hash: snapshots.trips ?? undefined,
          vp_hash: snapshots.vehicles ?? undefined,
          alerts_hash: produced("gtfs_rt", undefined, "rt", encoded.rt_hash),
        });
        if (!previewLive.live) throw new Error("the live answer carries no document");
        const liveDoc = produced("live", previewLive.live, "rt", previewLive.live_hash);
        dz.preview = (
          await ask("board", "stop_board", {
            network_hash: network,
            edits_hash: edits ?? undefined,
            stop_id: stop.id,
            date: dz.date,
            from_secs: nowSecs(),
            previous: previousDay(),
            limit: 6,
            live_hash: liveDoc,
          })
        ).board;
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
  dz.now = nowUnix();
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
  await request("network", "docs", "snapshot", "clock", "view", "query", "pick", "open", "form", "preview-stop");
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
    dz.now = nowUnix();
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
  return keep([network, boundary, edits, stops, alerts, notes, templates, live, snapshots.trips, snapshots.vehicles, snapshots.alerts]);
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
    .catch(() => null)) as { stops_hash?: string; service_alerts_hash?: string; alert_templates_hash?: string; notes_hash?: string } | null;
  if (!manifest) return false;
  let changed = false;
  if ((manifest.stops_hash ?? null) !== (stops?.pin ?? null)) ((stops = await servedStops()), (changed = true));
  if ((manifest.service_alerts_hash ?? null) !== (alerts?.pin ?? null)) ((alerts = await servedServiceAlerts()), (changed = true));
  if ((manifest.notes_hash ?? null) !== (notes?.pin ?? null)) ((notes = await servedNotes()), (changed = true));
  if ((manifest.alert_templates_hash ?? null) !== (templates?.pin ?? null)) ((templates = await servedAlertTemplates()), (changed = true));
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

// ---------------------------------------------------------------------
// the alert form
// ---------------------------------------------------------------------

/// A new alert: blank, or started from a gap (its effect, the riders
/// it informs, and what it explains). It is told to riders from now
/// until someone sets an end.
export function newAlert(gap: AlertGap | null = null): Promise<void> {
  const alert: ServiceAlert = {
    status: "draft",
    cause: "unknown",
    effect: gap?.effect ?? "unknown",
    severity: "unknown",
    header: [{ text: "" }],
    informed: gap ? gap.informed : [],
    explains: gap ? [gap.source] : [],
    communication: [{ start: dz.now, end: null }],
  };
  // The preview opens at a stop the gap takes away, else at its trip's
  // first stop, as the day names it.
  const key = gap ? JSON.stringify(gap.source) : "";
  const first = gap ? dz.day?.routes.flatMap((r) => r.disruptions).find((d) => JSON.stringify(d.source) === key)?.first_stop?.id : undefined;
  return openForm({ id: newId("alert"), fresh: true, alert, template: "", unfilled: [], startsNow: true }, gap?.stops?.[0] ?? first ?? null);
}

/// An alert started from a disruption opened: the day's gap with its
/// source (disruptions `gaps`), or a blank alert that explains it when
/// an alert explains it already.
export async function startAlertFor(source: AlertSource): Promise<void> {
  if (!network) return;
  const key = JSON.stringify(source);
  const gap = (await ask("disruptions", "gaps", documents())).gaps.find((g) => JSON.stringify(g.source) === key) ?? null;
  if (gap) return newAlert(gap);
  await newAlert(null);
  if (dz.form) await changeAlert({ ...dz.form.alert, explains: [source] });
}

/// An alert of the agency's list, opened to edit.
export function editAlert(id: string, alert: ServiceAlert): Promise<void> {
  return openForm({ id, fresh: false, alert: structuredClone($state.snapshot(alert)) as ServiceAlert, template: "", unfilled: [], startsNow: false }, null);
}

async function openForm(form: AlertForm, stop: string | null): Promise<void> {
  dz.form = form;
  dz.writeError = "";
  dz.refusals = [];
  dz.open = null;
  dz.view = "alerts";
  writeHash();
  const first = stop ?? form.alert.informed.find((e) => e.stop)?.stop ?? null;
  dz.previewStop = null;
  if (first && network) {
    const [s] = (await ask("directory", "stops_by_ids", { network_hash: network, ids: [first] })).stops;
    dz.previewStop = s ? { id: s.stop_id, name: s.name } : null;
  }
  await request("form", "preview-stop", "view", "open");
}

/// The form's alert as it would be saved now: a "Starts now" alert is
/// told from the host's clock.
function stamped(form: AlertForm): ServiceAlert {
  if (!form.startsNow) return form.alert;
  const [first, ...rest] = form.alert.communication ?? [{ start: null, end: null }];
  return { ...form.alert, communication: [{ ...first, start: dz.now }, ...rest] };
}

/// "Starts now" set or cleared: cleared, the start is the clock's now,
/// to change from there.
export function setStartsNow(on: boolean): Promise<void> {
  if (!dz.form) return Promise.resolve();
  dz.form = on ? { ...dz.form, startsNow: true } : { ...dz.form, startsNow: false, alert: stamped({ ...dz.form, startsNow: true }) };
  return request("form");
}

export function openPreview(open: boolean): Promise<void> {
  dz.previewOpen = open;
  return request("preview-open");
}

/// A stop clicked on the reach map: added to where the alert applies.
export function addStop(stop: string): Promise<void> {
  if (!dz.form) return Promise.resolve();
  return changeAlert({ ...dz.form.alert, informed: [...dz.form.alert.informed, { stop }] });
}

export function closeForm(): Promise<void> {
  dz.previewOpen = false;
  dz.form = null;
  dz.writeError = "";
  return request("form");
}

/// The form's alert as edited.
export async function changeAlert(alert: ServiceAlert): Promise<void> {
  if (!dz.form) return;
  dz.form = { ...dz.form, alert };
  // The preview follows the first stop the alert names, until one is
  // picked.
  const stop = dz.previewStop ? null : alert.informed.find((e) => e.stop)?.stop;
  if (stop && network) {
    const [s] = (await ask("directory", "stops_by_ids", { network_hash: network, ids: [stop] })).stops;
    if (s) dz.previewStop = { id: s.stop_id, name: s.name };
    return request("form", "preview-stop");
  }
  return request("form");
}

/// The stop the preview shows.
export function setPreviewStop(stop: { id: string; name: string } | null): Promise<void> {
  dz.previewStop = stop;
  dz.previewQuery = "";
  dz.previewFound = null;
  return request("preview-stop", "preview-query");
}

export function setPreviewQuery(text: string): Promise<void> {
  dz.previewQuery = text;
  return request("preview-query");
}

/// A template written out for the form's routes, stops, and dates
/// (alerts `fill`): its cause, effect, severity, and texts replace the
/// form's. A blank it cannot fill is kept and named.
export async function fillTemplate(template: string): Promise<void> {
  const form = dz.form;
  if (!form || !network || !templates) return;
  const informed = form.alert.informed;
  const period = form.alert.communication?.[0];
  const dateOf = (unix: number | null | undefined) => (unix == null ? undefined : zonedDate(unix));
  const { filled } = await ask("alerts", "fill", {
    templates_hash: templates,
    template,
    network_hash: network,
    input: {
      routes: [...new Set(informed.map((e) => e.route).filter((x): x is string => !!x))],
      stops: [...new Set(informed.map((e) => e.stop).filter((x): x is string => !!x))],
      start_date: dateOf(period?.start),
      end_date: dateOf(period?.end),
    },
  });
  dz.form = {
    ...form,
    template,
    unfilled: filled.unfilled,
    alert: { ...form.alert, cause: filled.cause, effect: filled.effect, severity: filled.severity, header: filled.header, description: filled.description, url: filled.url },
  };
  await request("form");
}

/// The form's alert saved: as a draft, or published to riders. Alerts
/// `put` places it, and POST /service-alerts checks it again and keeps
/// it. True when saved.
export function saveAlert(publish: boolean): Promise<boolean> {
  const form = dz.form;
  if (!form) return Promise.resolve(false);
  const alert: ServiceAlert = { ...stamped(form), status: publish ? "published" : "draft" };
  return writeAlerts(publish ? "Published." : "Saved as a draft.", () => ask("alerts", "put", { alerts_hash: alerts ?? undefined, id: form.id, alert }));
}

/// An alert ended now (alerts `end`): its periods end at the host's
/// clock, or it is withdrawn when riders were never told.
export function endAlert(id: string): Promise<boolean> {
  if (!alerts) return Promise.resolve(false);
  const doc = alerts;
  const at = nowUnix();
  // A period holds through its end second: the list is read once that
  // second has passed.
  return writeAlerts("Ended.", () => ask("alerts", "end", { alerts_hash: doc, id, at }), at + 1);
}

async function writeAlerts(done: string, edit: () => Promise<{ refusals: AlertRefusal[]; alerts: unknown | null }>, readAt = 0): Promise<boolean> {
  dz.busy = true;
  dz.writeError = "";
  try {
    const answer = await edit();
    if (answer.refusals.length || !answer.alerts) {
      dz.refusals = answer.refusals;
      dz.writeError = "Not saved";
      return false;
    }
    const resp = await writeFetch("service-alerts", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ from: alerts?.pin ?? "", alerts: answer.alerts }),
    });
    if (!(await written(resp, "alerts"))) return false;
    dz.form = null;
    // The list reads the clock the write was made at.
    dz.now = Math.max(nowUnix(), readAt);
    say(done);
    await readDocuments();
    await release();
    await request("docs", "form", "clock");
    return true;
  } catch (e) {
    dz.writeError = message(e);
    return false;
  } finally {
    dz.busy = false;
  }
}

/// The receive path's answer, in words. True when it kept the write.
async function written(resp: Response, what: "alerts" | "notes"): Promise<boolean> {
  if (resp.ok) return true;
  if (resp.status === 401) dz.writeError = "Sign in to save";
  else if (resp.status === 403) dz.writeError = "Your sign-in can't save here";
  else if (resp.status === 409) {
    await readDocuments();
    await release();
    await request("docs");
    dz.writeError = "Changed by someone else. Save again.";
  } else if (resp.status === 422) {
    const body = (await resp.json().catch(() => ({}))) as { refusals?: unknown[] };
    if (what === "alerts") dz.refusals = (body.refusals ?? []) as AlertRefusal[];
    else dz.noteRefusals = (body.refusals ?? []) as NoteRefusal[];
    dz.writeError = "Not saved";
  } else dz.writeError = `Not saved: ${(await resp.text()).trim() || resp.status}`;
  return false;
}

// ---------------------------------------------------------------------
// notes (D-214): written through POST /notes, which names the writer
// and the time
// ---------------------------------------------------------------------

/// A note written about `target`, or a note's text edited (`id`). True
/// when saved.
export async function writeNote(target: NoteTarget, text: string, id: string | null = null): Promise<boolean> {
  dz.busy = true;
  dz.writeError = "";
  dz.noteRefusals = [];
  try {
    const resp = await writeFetch("notes", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: id ?? newId("note"), target, text }),
    });
    if (!(await written(resp, "notes"))) return false;
    await readDocuments();
    await release();
    await request("docs");
    return true;
  } catch (e) {
    dz.writeError = message(e);
    return false;
  } finally {
    dz.busy = false;
  }
}

/// A new record's id: the agency's date and a random part.
function newId(kind: string): string {
  return `${kind}-${dz.date}-${crypto.randomUUID().slice(0, 8)}`;
}

/// The agency's date of a Unix time, "YYYY-MM-DD".
function zonedDate(unix: number): string {
  const parts = new Intl.DateTimeFormat("en-CA", { timeZone: dz.zone, year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date(unix * 1000));
  return parts;
}

/// The feed the preview encoded, as a file to keep.
export function downloadFeed(): void {
  if (!dz.previewFeed) return;
  const bytes = Uint8Array.from(atob(dz.previewFeed), (c) => c.charCodeAt(0));
  const url = URL.createObjectURL(new Blob([bytes], { type: "application/octet-stream" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = `alerts-${dz.date}.pb`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

/// Why a frame has no answer, as text ("" when it answered).
export function why(frame: string): string {
  return reason(dz.frames[frame]);
}

/// Each failed frame asks again: errors are never cached.
export function retry(): Promise<void> {
  return request.retry();
}
