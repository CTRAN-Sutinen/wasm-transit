/// The stops UI's host layer — the ONLY file of this UI that calls the
/// capability bus. It owns I/O (its inputs, declared in its ui.json and
/// read by the kit: the manifest, the stops and stop requests
/// documents it pins, and the networks a stop is read beside; their
/// writes as intents through POST
/// /logs/stops, the work order adapter, attachments, the device's
/// location, the URL hash), the selection, and the outbox (D-189): a
/// write, a work order, or a photo made while the server does not
/// answer waits on the device, in order, and is sent when it answers. Every list,
/// count, card, and choice it writes into `st` is an op's answer,
/// adopted as-is.
///
/// A stop has a life of its own — planned, permitted, built, in
/// service, suspended for a while, retired — managed here, apart from
/// the service plans, which only read it (D-157). Each change is one
/// event: cap-stops `add_event` (or `correct_event`, for an event that
/// was wrong) places and checks it, and the change is sent as its intent
/// (POST /logs/stops): the deployment's receive path runs it again over
/// the stops it holds. With no stops document yet, the page reads the
/// stops of the network that runs today (cap-stops `seed`), and the
/// first change saves that document (POST /stops).
///
/// The page's screens: the stops (on a map beside a list, or a table),
/// the work across them (cap-requests `list`), what the feed did, one
/// stop's page, one request's page, and a survey's page. A piece of work
/// on a stop (a change, a correction, a survey, a request, a field
/// report, a new stop) is a TASK, shown until it is saved or left. The
/// view, the stops' mode, the date, the stop, the request, the task,
/// and the filters live in the URL hash, so the browser's Back leaves a
/// task as a phone user expects.

import { ask, assemble, available, produced, readManifest, served, servedFeed, servedNetwork, servedNetworkPin, Doc, type DeploymentManifest } from "../../engine";
import { answerGraph, applyBuild, buildWaiting, onBuildReady, clockEvery, clockLabelAt, inputGraph, inputLabel, installApp, reason, setAgencyZone, today, type FrameState, type InputDeclaration, type InputState, type Reader } from "../kit";
import descriptor from "./ui.json";
import { readFetch, resumeSignIn } from "../../signin.svelte";
import { discardWrite, heldReason, openOutbox, outbox, queueWrite, retryWrite, sendWaiting, type Outcome } from "../../outbox.svelte";

// This page's capabilities answer inside its linked assembly (D-77).
assemble("stops");
import type { Event } from "../../types/Event";
import type { Network } from "../../types/Network";
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

const VIEWS: View[] = ["stops", "work", "feed"];

export const st = $state({
  /// A message the last step left (the stops counted, a write's
  /// outcome). A stops read that failed is `status()`.
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
  /// A new build waits (kit's buildWaiting): the person applies it.
  updateReady: false,
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
  /// The open stop's requests that name it while it is retired today
  /// (cap-stops `request_warnings`, D-238): why, by request id.
  requestWarnings: {} as Record<string, string>,
  /// The stop statuses, in a stop's order (cap-stops `vocabulary`).
  statusList: [] as Status[],
  requestVocab: { jobs: [] as string[], statuses: [] as string[], openStatuses: [] as string[], sides: [] as string[], directions: [] as string[], markings: [] as string[] },
  requestRefusals: [] as RequestRefusal[],
  /// The work across every stop (cap-requests `list`): the requests at
  /// the statuses and jobs asked, every request counted by status, and
  /// those requests' stops on the date (cap-stops `stops_on`).
  work: [] as IdRequest[],
  workCounts: [] as RequestCount[],
  workStops: [] as StopOn[],
  workStatuses: [] as string[],
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
  /// Each frame that shows no answer, and why (the answer graph's,
  /// and an action's failure: export, publish).
  frames: {} as Record<string, FrameState>,
  /// What the page holds of each input (the kit's).
  inputs: {} as Record<string, InputState>,
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

/// An action's failure (an export, a publish) belongs to its frame, as
/// a node's does: the frame says why, and its next answer clears it.
async function act(frame: string, clear: () => void, run: () => Promise<void>): Promise<void> {
  try {
    await run();
    delete st.frames[frame];
  } catch (e) {
    clear();
    st.frames[frame] = { kind: "failed", message: message(e) };
    console.error(frame, e);
  }
}

// ---------------------------------------------------------------------
// This UI's answer graph (src/uis/kit.ts). Inputs:
// "stops-doc", "requests-doc", "date", "show", "open", "near", "work".
// ---------------------------------------------------------------------

const request = answerGraph(
  [
    {
      id: "document.stops",
      frame: "",
      needs: ["stops-doc", "seed-network"],
      run: async () => {
        if (!(await available("stops"))) return void (stopsDoc = null);
        st.held = !!heldStops;
        if (heldStops) return void (stopsDoc = heldStops);
        // No stops document yet (the deployment keeps none, not a read
        // that failed): the stops of the network running today, not
        // saved until the first change.
        if (st.inputs.stops?.kind !== "none" || !seedNetwork) return void (stopsDoc = null);
        const seeded = await ask("stops", "seed", { network_hash: seedNetwork });
        stopsDoc = produced("stops", seeded.stops, "stops", seeded.stops_hash);
      },
    },
    {
      id: "document.requests",
      frame: "",
      needs: ["requests-doc"],
      run: async () => {
        requestsDoc = (await available("requests")) ? heldRequests : null;
      },
    },
    {
      id: "requests.requests_for",
      frame: "Requests",
      needs: ["document.requests", "open"],
      caps: ["requests"],
      clear: () => (st.requests = []),
      run: async () => {
        if (!requestsDoc || !st.open) return void (st.requests = []);
        st.requests = (await ask("requests", "requests_for", { requests_hash: requestsDoc, stop: st.open })).requests;
      },
    },
    {
      // The work across every stop, and its stops on the date (the
      // queue names each request's stop; the map shows them).
      id: "requests.list",
      frame: "Work",
      needs: ["document.requests", "document.stops", "date", "work"],
      caps: ["requests", "stops"],
      clear: () => ((st.work = []), (st.workCounts = []), (st.workStops = [])),
      run: async () => {
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
      },
    },
    {
      // What the agency's feed did to the stops record (D-150).
      id: "stops.feed_events",
      frame: "Feed",
      needs: ["document.stops", "feed"],
      caps: ["stops"],
      clear: () => (st.feed = null),
      run: async () => {
        st.feed = null;
        if (!stopsDoc || !st.held || !feed) return;
        const recorded = (await ask("stops", "feed_events", { stops_hash: stopsDoc, feed: feed.id })).events;
        const now = await ask("stops", "import_events", { stops_hash: stopsDoc, network_hash: feed.network });
        if (!recorded.length && !now.unlisted.length && !now.unapplied.length) return;
        st.feed = { on: now.on, recorded, unlisted: now.unlisted, unapplied: now.unapplied };
      },
    },
    {
      id: "stops.stops_on",
      frame: "Stops",
      needs: ["document.stops", "date", "show"],
      caps: ["stops"],
      clear: () => ((st.all = []), (st.shown = []), (st.counts = [])),
      run: async () => {
        if (!stopsDoc) return void ((st.all = []), (st.shown = []), (st.counts = []));
        const all = await ask("stops", "stops_on", { stops_hash: stopsDoc, date: readDate() });
        st.all = all.stops;
        st.counts = all.counts;
        st.shown = (await ask("stops", "stops_on", { stops_hash: stopsDoc, date: readDate(), statuses: [...st.show] })).stops;
      },
    },
    {
      // The field's "which stop am I at?": the stops nearest the device.
      id: "stops.stops_near",
      frame: "Near",
      needs: ["document.stops", "date", "show", "near"],
      caps: ["stops"],
      clear: () => (st.nearby = null),
      run: async () => {
        if (!stopsDoc || !st.near) return void (st.nearby = null);
        st.nearby = (
          await ask("stops", "stops_near", { stops_hash: stopsDoc, date: readDate(), lat: st.near.lat, lon: st.near.lon, limit: 12, statuses: [...st.show] })
        ).stops;
      },
    },
    {
      // The rule of each request's reference to the open stop, on the
      // agency's today (D-238): the day a write would be judged on, not
      // the date read.
      id: "stops.request_warnings",
      frame: "Requests",
      needs: ["document.stops", "document.requests", "open"],
      caps: ["stops"],
      clear: () => (st.requestWarnings = {}),
      run: async () => {
        if (!stopsDoc || !requestsDoc || !st.open) return void (st.requestWarnings = {});
        const got = await ask("stops", "request_warnings", { stops_hash: stopsDoc, requests_hash: requestsDoc, date: today(), stop: st.open });
        st.requestWarnings = Object.fromEntries(got.warnings.map((w) => [w.request, `This request names stop ${w.stop}, which ${w.reason}.`]));
      },
    },
    {
      id: "stops.stop",
      frame: "Card",
      needs: ["document.stops", "date", "open"],
      caps: ["stops"],
      clear: () => (st.card = null),
      run: async () => {
        if (!stopsDoc || !st.open) return void (st.card = null);
        st.card = await ask("stops", "stop", { stops_hash: stopsDoc, stop_id: st.open, date: readDate() });
      },
    },
    {
      // The routes visiting the open stop in the schedule running on the
      // date: read only, the planners' (D-157).
      id: "directory.stop_detail",
      frame: "Served",
      needs: ["network", "open"],
      caps: ["directory"],
      // A network that could not be read is said, never "no route
      // stops here".
      inputs: ["network"],
      clear: () => (st.served = null),
      run: async () => {
        if (!st.open || !datedNetwork) return void (st.served = null);
        st.served = (await ask("directory", "stop_detail", { network_hash: datedNetwork.network, stop_id: st.open })).detail;
      },
    },
  ],
  st.frames,
  (e) => say(`Error: ${message(e)}`, true),
  st.inputs,
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
  if (st.workStatuses.join(",") !== st.requestVocab.openStatuses.join(",")) q.set("work", st.workStatuses.join(","));
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
    const wanted = st.statusList.filter((s) => show.split(",").includes(s));
    if (wanted.join(",") !== st.show.join(",")) ((st.show = wanted), changed.push("show"));
  }
  const work = q.get("work");
  const statuses = work === null ? [...st.requestVocab.openStatuses] : work.split(",").filter(Boolean);
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

// ---------------------------------------------------------------------
// The inputs (its ui.json's `inputs`, read by the kit): each reader
// answers its document, null for none, or throws when the read fails.
// ---------------------------------------------------------------------

type Manifest = DeploymentManifest & { log_seqs?: Record<string, number>; services?: Record<string, { path: string }> };
type Dated = { network: Doc<Network>; pin: string };

let manifest: Manifest | null = null;
/// The documents the inputs hold: the stops and stop requests the
/// deployment keeps, the network running today (read only while it
/// keeps no stops document, to seed them), the network in force on the
/// page's date, and the agency's feed as the server imported it.
let heldStops: Doc | null = null;
let heldRequests: Doc | null = null;
let seedNetwork: Doc<Network> | null = null;
let datedNetwork: Dated | null = null;
let feed: { network: Doc<Network>; id: string } | null = null;

/// One of the deployment's logged documents, by the hash the manifest
/// pins: read again only when the pin moved.
function logged(kind: string, adopt: (doc: Doc | null) => void): Reader<Doc> {
  return {
    read: async ({ signal, held }) => {
      if (!manifest) throw new Error("the deployment's manifest is not read");
      const pin = manifest[`${kind}_hash`];
      if (typeof pin !== "string") return null;
      if (held?.pin === pin) return { value: held, key: pin };
      const doc = await served(kind, { manifest, signal });
      return doc ? { value: doc, key: pin } : null;
    },
    adopt,
  };
}

/// The network in force on `date`, by its pin: read again only when
/// the pin moved.
async function networkOn(date: string, signal: AbortSignal, held: Dated | null, readers: ("stops" | "directory")[]) {
  if (!manifest) throw new Error("the deployment's manifest is not read");
  const pin = await servedNetworkPin(date, { manifest, signal });
  if (!pin) return null;
  if (held?.pin === pin) return { value: held, key: pin };
  const got = await servedNetwork(date, readers, { manifest, signal });
  return got ? { value: { network: got.network, pin }, key: pin } : null;
}

const readers: Record<string, Reader<any>> = {
  manifest: {
    read: async ({ signal }) => {
      const read = await readManifest(signal);
      return { value: read.manifest, key: JSON.stringify(read.manifest), asOfMs: read.asOfMs ?? undefined, why: read.why ?? undefined };
    },
    adopt: (m: Manifest | null) => {
      manifest = m;
      setAgencyZone(m?.timezone);
      // The seq of the stops log's fold the page's documents came from:
      // a write names it (D-189).
      stopsSeq = m?.log_seqs?.stops;
      st.workOrders = m?.services?.work_orders?.path ?? null;
      st.attachments = m?.services?.attachments?.path ?? null;
    },
  },
  stops: logged("stops", (doc) => (heldStops = doc)),
  stop_requests: logged("stop_requests", (doc) => (heldRequests = doc)),
  "seed-network": {
    read: async ({ signal, held }: { signal: AbortSignal; held: Dated | null }) => {
      if (!manifest || manifest.stops_hash || !(await available("stops"))) return null;
      return networkOn(today(), signal, held, ["stops"]);
    },
    adopt: (got: Dated | null) => void (seedNetwork = got?.network ?? null),
  },
  network: {
    read: async ({ signal, held }: { signal: AbortSignal; held: Dated | null }) => {
      if (!(await available("directory"))) return null;
      return networkOn(readDate(), signal, held, ["directory"]);
    },
    adopt: (got: Dated | null) => void (datedNetwork = got),
  },
  feed: {
    read: async ({ signal, held }) => {
      if (!manifest?.network_hash || !manifest.feed_hash || !(await available("stops"))) return null;
      if (held?.id === manifest.feed_hash && held.network.pin === manifest.network_hash) return { value: held, key: `${manifest.feed_hash}:${manifest.network_hash}` };
      const got = await servedFeed(["stops"], { manifest, signal });
      return got ? { value: got, key: `${got.id}:${got.network.pin}` } : null;
    },
    adopt: (got) => void (feed = got),
  },
};

const inputs = inputGraph(descriptor.inputs as InputDeclaration[], readers, st.inputs, request);

/// The inputs a write moves: the manifest, and what it pins.
const WRITTEN = ["manifest", "stops", "stop_requests", "seed-network", "feed"];

/// The page's status line. A stops read that failed with nothing held
/// is said first: an outage is never "no stops yet".
export function status(): string {
  const s = st.inputs.stops;
  if (s?.kind === "failed" && !s.held) return inputLabel(s);
  return st.status;
}

/// Whether the server did not answer the last read of the manifest:
/// the page works from its last documents.
export function offline(): boolean {
  const m = st.inputs.manifest;
  return m?.kind === "failed" || (m?.kind === "answered" && m.why === "the server is not answering");
}

/// When the server last confirmed the documents the page works from
/// ("4:05 PM"), while it is offline; else "".
export function asOf(): string {
  const m = st.inputs.manifest;
  if (!offline() || !m || (m.kind !== "answered" && m.kind !== "failed") || m.asOfMs == null) return "";
  return clockLabelAt(m.asOfMs);
}

export async function boot(): Promise<void> {
  void installApp();
  onBuildReady(() => (st.updateReady = true));
  await resumeSignIn();
  await openOutbox("stops", () => void inputs.read(...WRITTEN).then(() => request("open")));
  // The vocabularies come first: the address names statuses from them.
  if (await available("requests")) {
    const v = await ask("requests", "vocabulary", {});
    st.requestVocab = { jobs: v.jobs, statuses: v.statuses, openStatuses: v.open_statuses, sides: v.sides, directions: v.directions, markings: v.markings };
    st.workStatuses = [...v.open_statuses];
  }
  if (await available("stops")) {
    st.statusList = (await ask("stops", "vocabulary", {})).statuses as Status[];
  }
  readHash();
  if (await available("stops")) {
    const q = await ask("stops", "questions", {});
    st.questions = { site: q.site, amenities: q.amenities };
  }
  writeHash(true);
  await inputs.start();
  await request("date", "show", "open", "work");
  const s = st.inputs.stops;
  say(
    s?.kind === "failed" && !s.held
      ? ""
      : !stopsDoc
        ? "The deployment publishes no stops yet."
        : st.held
          ? `${st.all.length} stops on ${readDate()}.`
          : `${st.all.length} stops from the schedule running today. The first change you save keeps them as the stops record.`,
  );
  // The writes that wait on the device, and a new build.
  clockEvery(POLL_MS, () => {
    void sendWaiting();
    st.updateReady = buildWaiting();
  });
}

/// How often the page sends what waits and looks for a new build. The
/// inputs' intervals are in its ui.json.
const POLL_MS = 30_000;

/// The seq of the stops log's fold the page's documents came from (the
/// manifest's log_seqs): a write names it, so the server checks it
/// against the records it changes (D-189).
let stopsSeq: number | undefined;

// ---------------------------------------------------------------------
// the selection
// ---------------------------------------------------------------------

export function setView(view: View): Promise<void> {
  st.view = view;
  st.task = st.task === "new" ? "" : st.task;
  return select();
}

export async function setDate(date: string): Promise<void> {
  st.date = /^\d{4}-\d{2}-\d{2}$/.test(date) && date !== today() ? date : "";
  // The network in force on the new date.
  await inputs.read("network");
  return select("date");
}

export function toggleStatus(status: Status): Promise<void> {
  st.show = st.show.includes(status) ? st.show.filter((s) => s !== status) : st.statusList.filter((s) => s === status || st.show.includes(s));
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

/// How the outbox names a change: its event, in words.
function eventLabel(event: Event): string {
  const kind = (event as { event?: string }).event ?? "change";
  return kind.charAt(0).toUpperCase() + kind.slice(1).replaceAll("_", " ");
}

/// One event added to the open stop (or to a new one). True when saved.
export function addEvent(stopId: string, event: Event, done: string, isNew = false): Promise<boolean> {
  const fresh = isNew ? { new: true } : {};
  return write(done, (doc) => ask("stops", "add_event", { stops_hash: doc, stop_id: stopId, event, ...fresh }), stopId, { cap: "stops", op: "add_event", args: { stop_id: stopId, event, ...fresh } }, `${eventLabel(event)}: stop ${stopId}`);
}

/// An event that was wrong, corrected — or, with no event, taken out.
/// The event is named by its id (D-166), so another writer's event
/// saved above it does not change which event is corrected.
export function correctEvent(stopId: string, eventId: string, event: Event | null, done: string): Promise<boolean> {
  return write(
    done,
    (doc) => ask("stops", "correct_event", { stops_hash: doc, stop_id: stopId, event_id: eventId, event: event ?? undefined }),
    stopId,
    { cap: "stops", op: "correct_event", args: { stop_id: stopId, event_id: eventId, ...(event ? { event } : {}) } },
    `${event ? "Correction" : "Event taken out"}: stop ${stopId}`,
  );
}

/// A new stop: its first event, at the point chosen. The write says the
/// stop is new, so cap-stops refuses an id the document already holds
/// (`stop_exists`), said beside the form.
/// Saved, it may go straight on to request its installation.
export async function newStop(stopId: string, event: Event, thenRequest: boolean): Promise<boolean> {
  st.taskError = "";
  const saved = await addEvent(stopId, event, `Added stop ${stopId}.`, true);
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
  op: { cap: string; op: string; args: unknown },
  label: string,
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
    // The first change keeps the stops the page read from the schedule
    // as the stops record: the whole document (POST /stops). Every
    // change after it is its intent, run over the stops the server holds.
    const outcome = st.held
      ? await queueWrite({ log: "stops", ops: [op], readSeq: stopsSeq, label })
      : await queueWrite({ url: "stops", body: { from: "", stops: answer.stops }, label });
    if (!(await settled(outcome, "stops"))) return false;
    // Kept: the task ends where its answer is read, replacing the
    // task's place in the history (Back does not reopen a saved form).
    st.point = null;
    st.open = stopId;
    st.task = "";
    st.request = "";
    taskPushed = false;
    writeHash(true);
    if (outcome.state === "sent") await request("stops-doc", "open");
    say(outcome.state === "sent" ? done : waitingText());
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

/// What the page says of a write that waits on the device.
function waitingText(): string {
  return outbox.needsSignIn ? "Saved on this device. Sign in to send it." : "Saved on this device. It is sent when the server answers.";
}

/// The outcome of a write the person just saved. A write the server
/// refused at once stays in its task with the server's reasons, so the
/// outbox does not keep a second copy. True when it was kept (sent, or
/// waiting on the device).
async function settled(outcome: Outcome, what: "stops" | "requests"): Promise<boolean> {
  if (outcome.state !== "held") return true;
  const w = outbox.held.find((h) => h.key === outcome.key);
  await discardWrite(outcome.key);
  const body = (outcome.reasons ?? {}) as { refusals?: unknown[] };
  if (outcome.status === 422) {
    if (what === "stops") st.refusals = (body.refusals ?? []) as Refusal[];
    else st.requestRefusals = (body.refusals ?? []) as RequestRefusal[];
    st.taskError = "The server refuses that change.";
  } else if (outcome.status === 409) {
    await request(what === "stops" ? "stops-doc" : "requests-doc", "open");
    st.taskError = "Someone else changed this first. It is read again: check it, and save yours again.";
  } else if (outcome.status === 403) {
    st.taskError = what === "stops" ? "Your sign-in may not change the stops here." : "Your sign-in may not change requests here.";
  } else st.taskError = w ? heldReason(w) : `Not saved (${outcome.status}).`;
  say(st.taskError, true);
  return false;
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
/// A new request (`fresh`) says so in the write: cap-requests refuses an
/// id the document holds (`id_taken`) instead of replacing that request.
export async function putRequest(id: string, req: StopRequest, done: string, fresh = false): Promise<boolean> {
  const isNew = fresh ? { new: true } : {};
  return writeRequests(req.stop, id, done, (doc) => ask("requests", "put", { ...(doc ? { requests_hash: doc } : {}), id, request: req, ...isNew }), {
    cap: "requests",
    op: "put",
    args: { id, request: req, ...isNew },
  });
}

/// A work order opened by hand (no work order system connected, D-165):
/// recorded on the request as the adapter's answer would be (cap-requests
/// `record_work_order` — a drafted or submitted request moves to
/// ordered), then stored by the receive path.
export function recordWorkOrder(id: string, stop: string, wo: WorkOrder): Promise<boolean> {
  if (!requestsDoc) return Promise.resolve(false);
  const doc = requestsDoc;
  return writeRequests(stop, id, `Work order ${wo.id} recorded on ${id}.`, () => ask("requests", "record_work_order", { requests_hash: doc, id, work_order: wo }), {
    cap: "requests",
    op: "record_work_order",
    args: { id, work_order: wo },
  });
}

async function writeRequests(
  stop: string,
  id: string,
  done: string,
  edit: (doc: Doc | null) => Promise<{ refusals: RequestRefusal[]; requests: unknown | null }>,
  op: { cap: string; op: string; args: unknown },
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
    const outcome = await queueWrite({ log: "stops", ops: [op], readSeq: stopsSeq, label: `Request ${id}: stop ${stop}` });
    if (!(await settled(outcome, "requests"))) return false;
    st.open = stop;
    st.request = id;
    st.task = "";
    taskPushed = false;
    writeHash(true);
    if (outcome.state === "sent") await request("requests-doc", "open");
    say(outcome.state === "sent" ? done : waitingText());
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
    // The key lets a resend whose answer was lost land once: the
    // server answers the work order the request holds (D-189).
    const outcome = await queueWrite({ url: st.workOrders.replace(/^\//, ""), body: { request: id, action }, keyed: true, label: `${action === "open" ? "Open a work order" : "Read the work order"}: ${id}` });
    if (outcome.state === "waiting") return say(outbox.needsSignIn ? "Sign in to reach the work order system. The request waits on this device." : "The work order system is asked when the server answers.");
    if (outcome.state === "held") {
      await discardWrite(outcome.key);
      if (outcome.status === 501) return say("No work order system is connected. Enter the work order by hand.", true);
      if (outcome.status === 403) return say("Your sign-in may not change requests here.", true);
      if (outcome.status === 409) return void (await request("requests-doc"), say("The requests changed meanwhile. They are read again: try again.", true));
      return say(`Error: the work order system did not answer: ${typeof outcome.reasons === "string" ? outcome.reasons.trim() : outcome.status}`, true);
    }
    const out = outcome.body as { work_order: { id: string; status?: string } | null };
    await request("requests-doc");
    const wo = out.work_order;
    say(!wo ? `No work order on ${id}.` : action === "open" ? `Work order ${wo.id} opened for ${id}.` : `Work order ${wo.id}: ${wo.status ?? "no status"}.`);
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

/// The media types the deployment takes (server/attachments.go).
const ATTACHMENT_TYPES = ["image/jpeg", "image/png", "image/webp", "image/heic", "image/heif", "application/pdf"];

/// One file added: its hash and type, or null with the reason said. The
/// file is named by the hash of its bytes, as the deployment names it,
/// and queued in the outbox before the write that names it (D-162,
/// D-189): offline, it waits on the device with the change.
export async function addAttachment(file: File): Promise<Attachment | null> {
  if (!st.attachments) return null;
  const type = (file.type || "").toLowerCase();
  if (file.size > 25 << 20) return (say(`${file.name} is larger than 25 MB.`, true), null);
  if (!ATTACHMENT_TYPES.includes(type)) return (say(`${file.name} is not a photo or a PDF.`, true), null);
  const digest = await crypto.subtle.digest("SHA-256", await file.arrayBuffer());
  const hash = "sha256:" + Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
  const outcome = await queueWrite({ url: st.attachments.replace(/^\//, ""), body: file, contentType: type, label: `Photo ${file.name}` });
  if (outcome.state === "held") {
    await discardWrite(outcome.key);
    if (outcome.status === 403) return (say("Your sign-in may not add photos here.", true), null);
    if (outcome.status === 413) return (say(`${file.name} is larger than 25 MB.`, true), null);
    if (outcome.status === 415) return (say(`${file.name} is not a photo or a PDF.`, true), null);
    return (say(`Error: the photo was not kept (${outcome.status}).`, true), null);
  }
  // Shown from the device's copy until the deployment holds it.
  shown.set(hash, Promise.resolve(URL.createObjectURL(file)));
  return { hash, type, name: file.name, taken_on: readDate() };
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

// ---------------------------------------------------------------------
// the outbox and the build, as the page shows them
// ---------------------------------------------------------------------

/// The writes the server refused, each with its reasons in words.
export function heldWrites(): { key: string; label: string; reason: string }[] {
  return outbox.held.map((w) => ({ key: w.key, label: w.label, reason: heldReason(w) }));
}

/// Send what waits now.
export function sendNow(): Promise<void> {
  return sendWaiting();
}

/// A refused write sent again, over the stops the page reads now.
export async function retryHeld(key: string): Promise<void> {
  await inputs.read(...WRITTEN);
  await retryWrite(key, { readSeq: stopsSeq });
}

/// The person drops a refused write.
export function discardHeld(key: string): Promise<void> {
  return discardWrite(key);
}

/// The person accepts the new version: never over a task being written.
export function applyUpdate(): void {
  if (st.task || st.busy) return;
  applyBuild();
}

/// Why a frame has no answer, as text ("" when it answered).
export function why(frame: string): string {
  return reason(st.frames[frame]);
}

/// Each failed frame asks again: errors are never cached.
export function retry(): Promise<void> {
  return request.retry();
}
