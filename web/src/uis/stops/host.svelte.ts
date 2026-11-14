/// The stops UI's host layer — the ONLY file of this UI that calls the
/// capability bus. It owns I/O (the stops and stop requests documents
/// read through the manifest, their writes through POST /stops and
/// POST /stop-requests, the work order adapter, attachments, the
/// device's location, the URL hash) and the selection. Every list,
/// count, card, and choice it writes into `st` is an op's answer,
/// adopted as-is.
///
/// A stop has a life of its own — planned, permitted, built, in
/// service, suspended for a while, retired — managed here, apart from
/// the service plans, which only read it (D-157). Each change is one
/// event: cap-stops `add_event` (or `correct_event`, for an event that
/// was wrong) places and checks it, and POST /stops — the deployment's
/// receive path — checks it again and stores it. With no stops document
/// yet, the page reads the stops of the network that runs today
/// (cap-stops `seed`), and the first change saves that document.
///
/// The page's screens: the stops (on a map beside a list, or a table),
/// the work across them (cap-requests `list`), what the feed did, one
/// stop's page, one request's page, and a survey's page. A piece of work
/// on a stop (a change, a correction, a survey, a request, a field
/// report, a new stop) is a TASK, shown until it is saved or left. The
/// view, the stops' mode, the date, the stop, the request, the task,
/// and the filters live in the URL hash, so the browser's Back leaves a
/// task as a phone user expects.

import { ask, assemble, available, produced, servedFeed, servedNetwork, servedStopRequests, servedStops, Doc } from "../../engine";
import { answerGraph, deploymentManifest, readAgencyZone, today } from "../kit";
import { readFetch, resumeSignIn, writeFetch } from "../../signin.svelte";

// This page's capabilities answer inside its linked assembly (D-77).
assemble("stops");
import type { Event } from "../../types/Event";
import type { Refusal } from "../../types/Refusal";
import type { Status } from "../../types/Status";
import type { StatusCount } from "../../types/StatusCount";
import type { StopOn } from "../../types/StopOn";
import type { NearStop } from "../../types/NearStop";
import type { StopDetail } from "../../types/StopDetail";
import type { SiteQuestion } from "../../types/cap_stops/SiteQuestion";
import type { SurveyDraft } from "../../components/SurveyPage.svelte";
import type { IdRequest } from "../../types/cap_requests/IdRequest";
import type { RequestCount } from "../../types/RequestCount";
import type { StopRequest } from "../../types/StopRequest";
import type { RequestRefusal } from "../../types/RequestRefusal";
import type { WorkOrder } from "../../types/WorkOrder";

export type Next = "planned" | "permitted" | "built" | "in_service" | "suspended" | "retired";
export type View = "stops" | "work" | "feed";
/// How the Stops view shows its stops: on a map beside a list, or as a table.
export type Mode = "map" | "table";

export const STATUSES: Status[] = ["planned", "permitted", "built", "in_service", "suspended", "retired"];
const VIEWS: View[] = ["stops", "work", "feed"];
/// The work queue opens on what is still in flight.
const OPEN_WORK = ["drafted", "submitted", "ordered"];

export const st = $state({
  status: "Reading the stops…",
  alert: false,
  busy: false,
  /// The date read ("": today, the agency's).
  date: "",
  view: "stops" as View,
  mode: "map" as Mode,
  /// The statuses shown on the map and the list (retired hidden until
  /// asked).
  show: ["planned", "permitted", "built", "in_service", "suspended"] as Status[],
  /// The deployment keeps a stops document; until it does, the page
  /// shows the stops of the network running today, and the first change
  /// saves them.
  held: false,
  /// Every stop on the date (the search box's options), and those the
  /// filters show, with every stop counted by status.
  all: [] as StopOn[],
  shown: [] as StopOn[],
  counts: [] as StatusCount[],
  /// The device's location, when asked ("Near me"), and the stops
  /// nearest it on the date at the statuses shown (cap-stops
  /// `stops_near`).
  near: null as { lat: number; lon: number; accuracy: number } | null,
  nearby: null as NearStop[] | null,
  locating: false,
  /// The stop open, and the task under way:
  /// "" (none), "change:<kind>" (a lifecycle event, "move", "details"),
  /// "fix:<index>", "survey", "report", "request" (a new one), "edit:<section>"
  /// (of the open request), or "new" (a new stop).
  open: "",
  task: "",
  /// The request open (its page).
  request: "",
  /// The open stop as it stands on the date (null before it exists),
  /// its events, and what it may become that day.
  card: null as { stop: StopOn | null; events: Event[]; next: Next[] } | null,
  /// The routes visiting the open stop in the schedule running on the
  /// date (cap-directory `stop_detail` over the published network).
  served: null as StopDetail | null,
  /// A point chosen for a new stop or a move: clicked on the map, typed,
  /// or the device's fix.
  point: null as { lat: number; lon: number } | null,
  /// What a site survey asks (cap-stops `questions`, D-158).
  questions: { site: [] as SiteQuestion[], amenities: [] as string[] },
  /// The open stop's requests (cap-requests `requests_for`, D-159), the
  /// lists their answers come from (`vocabulary`), and why the last
  /// request was refused.
  requests: [] as IdRequest[],
  requestVocab: { jobs: [] as string[], statuses: [] as string[], sides: [] as string[], directions: [] as string[], markings: [] as string[] },
  requestRefusals: [] as RequestRefusal[],
  /// The work across every stop (cap-requests `list`): the requests at
  /// the statuses and jobs asked, every request counted by status, and
  /// those requests' stops on the date (cap-stops `stops_on`).
  work: [] as IdRequest[],
  workCounts: [] as RequestCount[],
  workStops: [] as StopOn[],
  workStatuses: [...OPEN_WORK] as string[],
  workJobs: [] as string[],
  /// Where the work order system's adapter answers (D-165); null when
  /// none is connected — a work order is then entered by hand.
  workOrders: null as string | null,
  /// Where photos and drawings are added and read (D-162); null when
  /// the deployment keeps none.
  attachments: null as string | null,
  /// What the agency's feed did to the stops record (D-150); null when
  /// there is nothing to say.
  feed: null as null | { on: string; recorded: { stop: string; event: Event }[]; unlisted: string[]; unapplied: string[] },
  /// Bumped whenever this device's survey drafts change.
  drafts: 0,
  /// Why the last change was refused, each naming its event.
  refusals: [] as Refusal[],
  /// A refusal said in words beside the task (a new stop's taken id).
  taskError: "",
  failed: {} as Record<string, string>,
});

/// The date read.
export function readDate(): string {
  return st.date || today();
}

let stopsDoc: Doc | null = null;
/// The deployment's stop requests document; null until the first
/// request begins it.
let requestsDoc: Doc | null = null;

function say(text: string, alert = false): void {
  st.status = text;
  st.alert = alert;
}

function message(e: unknown): string {
  return (e as Error)?.message ?? String(e);
}

async function framed(frame: string, clear: () => void, run: () => Promise<void>): Promise<void> {
  try {
    await run();
    delete st.failed[frame];
  } catch (e) {
    clear();
    st.failed[frame] = message(e);
    console.error(frame, e);
  }
}

// ---------------------------------------------------------------------
// TRANSIENT — this UI's answer graph (src/uis/kit.ts). Inputs:
// "stops-doc", "requests-doc", "date", "show", "open", "near", "work".
// ---------------------------------------------------------------------

const request = answerGraph(
  [
    {
      id: "document.stops",
      frame: "",
      needs: ["stops-doc"],
      run: async () => {
        if (!(await available("stops"))) return void (stopsDoc = null);
        const served = await servedStops().catch(() => null);
        st.held = !!served;
        if (served) return void (stopsDoc = served);
        // No stops document yet: the stops of the network running today,
        // not saved until the first change.
        const network = await servedNetwork(today(), "stops").catch(() => null);
        if (!network) return void (stopsDoc = null);
        const seeded = await ask("stops", "seed", { network_hash: network.network });
        stopsDoc = produced("stops", seeded.stops, "stops", seeded.stops_hash);
      },
    },
    {
      id: "document.requests",
      frame: "",
      needs: ["requests-doc"],
      run: async () => {
        requestsDoc = (await available("requests")) ? await servedStopRequests().catch(() => null) : null;
      },
    },
    {
      id: "requests.requests_for",
      frame: "Requests",
      needs: ["document.requests", "open"],
      run: () =>
        framed("Requests", () => (st.requests = []), async () => {
          if (!requestsDoc || !st.open) return void (st.requests = []);
          st.requests = (await ask("requests", "requests_for", { requests_hash: requestsDoc, stop: st.open })).requests;
        }),
    },
    {
      // The work across every stop, and its stops on the date (the
      // queue names each request's stop; the map shows them).
      id: "requests.list",
      frame: "Work",
      needs: ["document.requests", "document.stops", "date", "work"],
      run: () =>
        framed("Work", () => ((st.work = []), (st.workCounts = []), (st.workStops = [])), async () => {
          if (!requestsDoc) return void ((st.work = []), (st.workCounts = []), (st.workStops = []));
          const listed = await ask("requests", "list", {
            requests_hash: requestsDoc,
            ...(st.workStatuses.length ? { statuses: [...st.workStatuses] } : {}),
            ...(st.workJobs.length ? { jobs: [...st.workJobs] } : {}),
          });
          st.work = listed.requests;
          st.workCounts = listed.counts;
          st.workStops = stopsDoc
            ? (await ask("stops", "stops_on", { stops_hash: stopsDoc, date: readDate(), stop_ids: listed.requests.map((r) => r.request.stop) })).stops
            : [];
        }),
    },
    {
      // What the agency's feed did to the stops record (D-150).
      id: "stops.feed_events",
      frame: "Feed",
      needs: ["document.stops"],
      run: () =>
        framed("Feed", () => (st.feed = null), async () => {
          st.feed = null;
          if (!stopsDoc || !st.held) return;
          const feed = await servedFeed("stops");
          if (!feed) return;
          const recorded = (await ask("stops", "feed_events", { stops_hash: stopsDoc, feed: feed.id })).events;
          const now = await ask("stops", "import_events", { stops_hash: stopsDoc, network_hash: feed.network });
          if (!recorded.length && !now.unlisted.length && !now.unapplied.length) return;
          st.feed = { on: now.on, recorded, unlisted: now.unlisted, unapplied: now.unapplied };
        }),
    },
    {
      id: "stops.stops_on",
      frame: "Stops",
      needs: ["document.stops", "date", "show"],
      run: () =>
        framed("Stops", () => ((st.all = []), (st.shown = []), (st.counts = [])), async () => {
          if (!stopsDoc) return void ((st.all = []), (st.shown = []), (st.counts = []));
          const all = await ask("stops", "stops_on", { stops_hash: stopsDoc, date: readDate() });
          st.all = all.stops;
          st.counts = all.counts;
          st.shown = (await ask("stops", "stops_on", { stops_hash: stopsDoc, date: readDate(), statuses: [...st.show] })).stops;
        }),
    },
    {
      // The field's "which stop am I at?": the stops nearest the device.
      id: "stops.stops_near",
      frame: "Near",
      needs: ["document.stops", "date", "show", "near"],
      run: () =>
        framed("Near", () => (st.nearby = null), async () => {
          if (!stopsDoc || !st.near) return void (st.nearby = null);
          st.nearby = (
            await ask("stops", "stops_near", { stops_hash: stopsDoc, date: readDate(), lat: st.near.lat, lon: st.near.lon, limit: 12, statuses: [...st.show] })
          ).stops;
        }),
    },
    {
      id: "stops.stop",
      frame: "Card",
      needs: ["document.stops", "date", "open"],
      run: () =>
        framed("Card", () => (st.card = null), async () => {
          if (!stopsDoc || !st.open) return void (st.card = null);
          st.card = await ask("stops", "stop", { stops_hash: stopsDoc, stop_id: st.open, date: readDate() });
        }),
    },
    {
      // The routes visiting the open stop in the schedule running on the
      // date: read only, the planners' (D-157).
      id: "directory.stop_detail",
      frame: "Served",
      needs: ["date", "open"],
      run: () =>
        framed("Served", () => (st.served = null), async () => {
          if (!st.open || !(await available("directory"))) return void (st.served = null);
          const network = await servedNetwork(readDate(), "directory");
          if (!network) return void (st.served = null);
          st.served = (await ask("directory", "stop_detail", { network_hash: network.network, stop_id: st.open })).detail;
        }),
    },
  ],
  (e) => say(`Error: ${message(e)}`, true),
);

// ---------------------------------------------------------------------
// the URL hash — the selection, followed both ways
// ---------------------------------------------------------------------

let writing = false;

function writeHash(replace = false): void {
  const q = new URLSearchParams();
  if (st.view !== "stops") q.set("view", st.view);
  if (st.date) q.set("on", st.date);
  if (st.open) q.set("open", st.open);
  if (st.mode !== "map") q.set("mode", st.mode);
  if (st.request) q.set("request", st.request);
  if (st.task) q.set("task", st.task);
  q.set("show", st.show.join(","));
  if (st.workStatuses.join(",") !== OPEN_WORK.join(",")) q.set("work", st.workStatuses.join(","));
  if (st.workJobs.length) q.set("jobs", st.workJobs.join(","));
  const next = `#${q}`;
  if (location.hash === next) return;
  writing = true;
  if (replace) history.replaceState(null, "", next);
  else history.pushState(null, "", next);
  writing = false;
}

function readHash(): string[] {
  const q = new URLSearchParams(location.hash.slice(1));
  const changed: string[] = [];
  const view = (q.get("view") ?? "stops") as View;
  st.view = VIEWS.includes(view) ? view : "stops";
  const on = q.get("on") ?? "";
  const date = /^\d{4}-\d{2}-\d{2}$/.test(on) ? on : "";
  if (date !== st.date) ((st.date = date), changed.push("date"));
  const open = q.get("open") ?? "";
  if (open !== st.open) ((st.open = open), changed.push("open"));
  st.mode = q.get("mode") === "table" ? "table" : "map";
  st.request = q.get("request") ?? "";
  const task = q.get("task") ?? "";
  if (task !== st.task) {
    st.task = task;
    st.point = null;
    st.refusals = [];
    st.requestRefusals = [];
    st.taskError = "";
  }
  const show = q.get("show");
  if (show !== null) {
    const wanted = STATUSES.filter((s) => show.split(",").includes(s));
    if (wanted.join(",") !== st.show.join(",")) ((st.show = wanted), changed.push("show"));
  }
  const work = q.get("work");
  const statuses = work === null ? [...OPEN_WORK] : work.split(",").filter(Boolean);
  const jobs = (q.get("jobs") ?? "").split(",").filter(Boolean);
  if (statuses.join(",") !== st.workStatuses.join(",") || jobs.join(",") !== st.workJobs.join(","))
    ((st.workStatuses = statuses), (st.workJobs = jobs), changed.push("work"));
  return changed;
}

async function select(...inputs: string[]): Promise<void> {
  writeHash();
  if (inputs.length) await request(...inputs);
}

function followHash(): void {
  if (writing) return;
  const changed = readHash();
  if (changed.length) void request(...changed);
}
addEventListener("hashchange", followHash);
addEventListener("popstate", followHash);

export async function boot(): Promise<void> {
  await Promise.all([readAgencyZone(), resumeSignIn()]);
  readHash();
  if (await available("stops")) {
    const q = await ask("stops", "questions", {});
    st.questions = { site: q.site, amenities: q.amenities };
  }
  const manifest = await deploymentManifest<{ services?: Record<string, { path: string }> }>().catch(() => null);
  st.workOrders = manifest?.services?.work_orders?.path ?? null;
  st.attachments = manifest?.services?.attachments?.path ?? null;
  if (await available("requests")) {
    const v = await ask("requests", "vocabulary", {});
    st.requestVocab = { jobs: v.jobs, statuses: v.statuses, sides: v.sides, directions: v.directions, markings: v.markings };
  }
  writeHash(true);
  await request("stops-doc", "requests-doc", "date", "show", "open", "work");
  say(
    !stopsDoc
      ? "The deployment publishes no stops yet."
      : st.held
        ? `${st.all.length} stops on ${readDate()}.`
        : `${st.all.length} stops from the schedule running today. The first change you save keeps them as the stops record.`,
  );
}

// ---------------------------------------------------------------------
// the selection
// ---------------------------------------------------------------------

export function setView(view: View): Promise<void> {
  st.view = view;
  st.task = st.task === "new" ? "" : st.task;
  return select();
}

export function setDate(date: string): Promise<void> {
  st.date = /^\d{4}-\d{2}-\d{2}$/.test(date) && date !== today() ? date : "";
  return select("date");
}

export function toggleStatus(status: Status): Promise<void> {
  st.show = st.show.includes(status) ? st.show.filter((s) => s !== status) : STATUSES.filter((s) => s === status || st.show.includes(s));
  return select("show");
}

/// The work queue's filters: the statuses shown, and the jobs ([] every
/// job).
export function setWork(statuses: string[], jobs: string[]): Promise<void> {
  st.workStatuses = statuses;
  st.workJobs = jobs;
  return select("work");
}

/// A stop's page opened — from the map, a list, the search, the queue
/// (with one of its requests) — with no task under way.
export function openStop(stop: string, requestId = ""): Promise<void> {
  st.open = stop;
  st.request = requestId;
  st.task = "";
  st.point = null;
  st.refusals = [];
  st.taskError = "";
  return select("open");
}

export function closeStop(): Promise<void> {
  return openStop("");
}

export function setMode(mode: Mode): void {
  st.mode = mode;
  writeHash();
}

/// One request's page opened, or closed ("").
export function openRequest(id: string): void {
  st.request = id;
  st.requestRefusals = [];
  writeHash();
}

/// A task begun on the open stop (or a new stop): the panel shows only
/// it until it is saved or left.
let taskPushed = false;

export function startTask(task: string): void {
  taskPushed = true;
  st.task = task;
  st.point = null;
  st.refusals = [];
  st.requestRefusals = [];
  st.taskError = "";
  writeHash();
}

/// The task left unsaved (a survey's draft stays on the device).
export function endTask(): void {
  if (!st.task) return;
  // A task this page opened is left the way it came, so Back and the
  // page's own Close agree; one the page was opened on just closes.
  if (taskPushed) return void ((taskPushed = false), history.back());
  st.task = "";
  st.point = null;
  writeHash(true);
}

/// The point a map click (or the fields, or the device) chose, while a
/// task places one.
export function picked(lat: number, lon: number): void {
  if (placing()) st.point = { lat, lon };
}

/// Whether a click on the map places a point (a new stop, a move).
export function placing(): boolean {
  return st.task === "new" || st.task === "change:move";
}

// ---------------------------------------------------------------------
// the device: its location ("Near me", "Use my location")
// ---------------------------------------------------------------------

/// The device's own fix, with how far off it may be (metres). A start,
/// not a measurement: the point typed from a survey-grade tool is the
/// record.
export function locate(): Promise<{ lat: number; lon: number; accuracy: number }> {
  return new Promise((resolve, reject) => {
    if (!("geolocation" in navigator)) return reject(new Error("this device gives no location"));
    navigator.geolocation.getCurrentPosition(
      (p) => resolve({ lat: p.coords.latitude, lon: p.coords.longitude, accuracy: p.coords.accuracy }),
      (e) => reject(new Error(e.code === e.PERMISSION_DENIED ? "the location was not allowed" : "no location could be found")),
      { enableHighAccuracy: true, timeout: 20_000, maximumAge: 0 },
    );
  });
}

/// "Near me": the stops nearest the device, listed nearest first.
export async function nearMe(): Promise<void> {
  st.locating = true;
  say("Finding this device's location…");
  try {
    st.near = await locate();
    await request("near");
    say(`Nearest stops to you (location good to about ${Math.round(st.near.accuracy)} m).`);
  } catch (e) {
    say(`No location: ${message(e)}.`, true);
  } finally {
    st.locating = false;
  }
}

export function clearNear(): Promise<void> {
  st.near = null;
  return request("near");
}

// ---------------------------------------------------------------------
// the changes: each one event, written through POST /stops
// ---------------------------------------------------------------------

/// One event added to the open stop (or to a new one). True when saved.
export function addEvent(stopId: string, event: Event, done: string): Promise<boolean> {
  return write(done, (doc) => ask("stops", "add_event", { stops_hash: doc, stop_id: stopId, event }), stopId);
}

/// An event that was wrong, corrected — or, with no event, taken out.
export function correctEvent(stopId: string, index: number, event: Event | null, done: string): Promise<boolean> {
  return write(done, (doc) => ask("stops", "correct_event", { stops_hash: doc, stop_id: stopId, index, event: event ?? undefined }), stopId);
}

/// A new stop: its first event, at the point chosen. An id the document
/// already holds is another stop's — said beside the form.
/// Saved, it may go straight on to request its installation.
export async function newStop(stopId: string, event: Event, thenRequest: boolean): Promise<boolean> {
  st.taskError = "";
  if (st.all.some((s) => s.stop_id === stopId) || (stopsDoc && (await ask("stops", "stop", { stops_hash: stopsDoc, stop_id: stopId, date: readDate() }).catch(() => null)))) {
    st.taskError = `There is already a stop ${stopId}. Choose another id.`;
    return false;
  }
  const saved = await addEvent(stopId, event, `Added stop ${stopId}.`);
  if (saved && thenRequest) startTask("request:new");
  return saved;
}

/// A survey recorded: one event, its draft dropped once it is saved.
export async function survey(stopId: string, event: Event, done: string): Promise<boolean> {
  const saved = await addEvent(stopId, event, done);
  if (saved) keepDraft(stopId, null);
  return saved;
}

async function write(
  done: string,
  edit: (doc: Doc) => Promise<{ refusals: Refusal[]; stops: unknown | null }>,
  stopId: string,
): Promise<boolean> {
  if (!stopsDoc) return false;
  const doc = stopsDoc;
  st.refusals = [];
  st.taskError = "";
  st.busy = true;
  say("Saving…");
  try {
    const answer = await edit(doc);
    if (answer.refusals.length || !answer.stops) {
      st.refusals = answer.refusals;
      say("That change does not fit the stop's history.", true);
      return false;
    }
    const from = st.held ? (doc.pin ?? "") : "";
    const resp = await writeFetch("/stops", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ from, stops: answer.stops }) });
    if (resp.status === 401 || resp.status === 403) {
      st.taskError = resp.status === 401 ? "Sign in to save. Your change is not saved yet." : "Your sign-in may not change the stops here.";
      say(st.taskError, true);
      return false;
    }
    if (resp.status === 422) {
      st.refusals = ((await resp.json()) as { refusals?: Refusal[] }).refusals ?? [];
      say("The server refuses that change.", true);
      return false;
    }
    if (resp.status === 409) {
      await request("stops-doc");
      st.taskError = "Someone else saved a change first. The stops are read again: save yours again.";
      say(st.taskError, true);
      return false;
    }
    if (!resp.ok) throw new Error(`the server did not save it: ${(await resp.text()).trim() || resp.status}`);
    // Saved: the task ends where its answer is read, replacing the
    // task's place in the history (Back does not reopen a saved form).
    st.point = null;
    st.open = stopId;
    st.task = "";
    st.request = "";
    taskPushed = false;
    writeHash(true);
    await request("stops-doc", "open");
    say(done);
    return true;
  } catch (e) {
    st.taskError = `Error: ${message(e)}`;
    say(st.taskError, true);
    console.error(e);
    return false;
  } finally {
    st.busy = false;
  }
}

// ---------------------------------------------------------------------
// the field: a survey's draft kept on this device
// ---------------------------------------------------------------------

/// A survey half filled is kept on this device until it is saved or
/// thrown away, so a dropped connection or a closed tab loses nothing
/// (D-158). Per stop; never shared, never sent.
const draftKey = (stopId: string) => `stops.survey-draft.${stopId}`;

export function readDraft(stopId: string): SurveyDraft | null {
  void st.drafts;
  try {
    const text = localStorage.getItem(draftKey(stopId));
    return text ? (JSON.parse(text) as SurveyDraft) : null;
  } catch {
    return null;
  }
}

export function keepDraft(stopId: string, draft: SurveyDraft | null): void {
  const had = readDraft(stopId) !== null;
  try {
    if (draft) localStorage.setItem(draftKey(stopId), JSON.stringify(draft));
    else localStorage.removeItem(draftKey(stopId));
  } catch {
    // no storage here (a private window): the draft lives as long as the page
  }
  // only a draft begun or dropped changes what the card says
  if (had !== (draft !== null)) st.drafts++;
}

// ---------------------------------------------------------------------
// requests for work at a stop (D-159): each put whole, through
// POST /stop-requests
// ---------------------------------------------------------------------

/// One request placed by id — new, or replacing the one held — checked
/// by cap-requests `put`, then stored by the deployment's receive path.
/// Saved, the request's page shows it. True when saved.
/// A new request (`fresh`) never replaces one held under its id.
export async function putRequest(id: string, req: StopRequest, done: string, fresh = false): Promise<boolean> {
  if (fresh && requestsDoc && (await ask("requests", "list", { requests_hash: requestsDoc })).requests.some((r) => r.id === id)) {
    st.taskError = `There is already a request ${id}. Choose another id.`;
    return false;
  }
  return writeRequests(req.stop, id, done, (doc) => ask("requests", "put", { ...(doc ? { requests_hash: doc } : {}), id, request: req }));
}

/// A work order opened by hand (no work order system connected, D-165):
/// recorded on the request as the adapter's answer would be (cap-requests
/// `record_work_order` — a drafted or submitted request moves to
/// ordered), then stored by the receive path.
export function recordWorkOrder(id: string, stop: string, wo: WorkOrder): Promise<boolean> {
  if (!requestsDoc) return Promise.resolve(false);
  const doc = requestsDoc;
  return writeRequests(stop, id, `Work order ${wo.id} recorded on ${id}.`, () => ask("requests", "record_work_order", { requests_hash: doc, id, work_order: wo }));
}

async function writeRequests(
  stop: string,
  id: string,
  done: string,
  edit: (doc: Doc | null) => Promise<{ refusals: RequestRefusal[]; requests: unknown | null }>,
): Promise<boolean> {
  const doc = requestsDoc;
  st.requestRefusals = [];
  st.taskError = "";
  st.busy = true;
  say("Saving…");
  try {
    const answer = await edit(doc);
    if (answer.refusals.length || !answer.requests) {
      st.requestRefusals = answer.refusals;
      say("That request is not complete.", true);
      return false;
    }
    const from = doc?.pin ?? "";
    const resp = await writeFetch("/stop-requests", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ from, requests: answer.requests }) });
    if (resp.status === 401 || resp.status === 403) {
      st.taskError = resp.status === 401 ? "Sign in to save the request. It is not saved yet." : "Your sign-in may not change requests here.";
      say(st.taskError, true);
      return false;
    }
    if (resp.status === 422) {
      st.requestRefusals = ((await resp.json()) as { refusals?: RequestRefusal[] }).refusals ?? [];
      say("The server refuses that request.", true);
      return false;
    }
    if (resp.status === 409) {
      await request("requests-doc");
      st.taskError = "Someone else saved a request first. The requests are read again: save yours again.";
      say(st.taskError, true);
      return false;
    }
    if (!resp.ok) throw new Error(`the server did not save it: ${(await resp.text()).trim() || resp.status}`);
    st.open = stop;
    st.request = id;
    st.task = "";
    taskPushed = false;
    writeHash(true);
    await request("requests-doc", "open");
    say(done);
    return true;
  } catch (e) {
    st.taskError = `Error: ${message(e)}`;
    say(st.taskError, true);
    console.error(e);
    return false;
  } finally {
    st.busy = false;
  }
}

/// The work order system, through the server's adapter (D-165): open a
/// work order for a request, or read its status — the answer written on
/// the request by the server, the requests read again.
export async function workOrder(id: string, action: "open" | "refresh"): Promise<void> {
  if (!st.workOrders) return;
  st.busy = true;
  say(action === "open" ? "Opening a work order…" : "Reading the work order's status…");
  try {
    const resp = await writeFetch(st.workOrders, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ request: id, action }) });
    if (resp.status === 501) return say("No work order system is connected. Enter the work order by hand.", true);
    if (resp.status === 401 || resp.status === 403) return say(resp.status === 401 ? "Sign in to reach the work order system." : "Your sign-in may not change requests here.", true);
    if (resp.status === 409) return void (await request("requests-doc"), say("The requests changed meanwhile. They are read again: try again.", true));
    if (!resp.ok) throw new Error(`the work order system did not answer: ${(await resp.text()).trim() || resp.status}`);
    const out = (await resp.json()) as { work_order: { id: string; status?: string } };
    await request("requests-doc");
    say(action === "open" ? `Work order ${out.work_order.id} opened for ${id}.` : `Work order ${out.work_order.id}: ${out.work_order.status ?? "no status"}.`);
  } catch (e) {
    say(`Error: ${message(e)}`, true);
    console.error(e);
  } finally {
    st.busy = false;
  }
}

// ---------------------------------------------------------------------
// photos and drawings (D-162): added to the deployment's bucket, named
// by their hash, read only by the signed-in
// ---------------------------------------------------------------------

export type Attachment = { hash: string; type: string; name?: string; caption?: string; taken_on?: string };

/// One file added: its hash and type, or null with the reason said.
export async function addAttachment(file: File): Promise<Attachment | null> {
  if (!st.attachments) return null;
  const resp = await writeFetch(st.attachments, { method: "POST", headers: { "Content-Type": file.type || "application/octet-stream" }, body: file });
  if (resp.status === 401) return (say("Sign in to add photos.", true), null);
  if (resp.status === 403) return (say("Your sign-in may not add photos here.", true), null);
  if (resp.status === 413) return (say(`${file.name} is larger than 25 MB.`, true), null);
  if (resp.status === 415) return (say(`${file.name} is not a photo or a PDF.`, true), null);
  if (!resp.ok) return (say(`Error: the photo was not kept: ${(await resp.text()).trim() || resp.status}`, true), null);
  const out = (await resp.json()) as { hash: string; type: string };
  return { hash: out.hash, type: out.type, name: file.name, taken_on: readDate() };
}

const shown = new Map<string, Promise<string | null>>();

/// An attachment as this page can show it (an object URL of its bytes,
/// read with the signed-in person's token), or null — not signed in, or
/// gone. Kept for the page's life: a hash's bytes never change.
export function attachmentSrc(hash: string): Promise<string | null> {
  if (!st.attachments) return Promise.resolve(null);
  let got = shown.get(hash);
  if (!got) {
    got = readFetch(`${st.attachments}/${hash}`)
      .then(async (r) => (r.ok ? URL.createObjectURL(await r.blob()) : null))
      .catch(() => null);
    // a refusal (not signed in yet) is asked again after signing in
    got.then((u) => u === null && shown.delete(hash));
    shown.set(hash, got);
  }
  return got;
}

export function retry(): Promise<void> {
  return request("stops-doc", "requests-doc");
}
