/// The stops UI's host layer — the ONLY file of this UI that calls the
/// capability bus. It owns I/O (the stops document read through the
/// manifest, its writes through POST /stops, the URL hash), and the
/// selection. Every list, count, card, and choice it writes into `st`
/// is an op's answer, adopted as-is.
///
/// A stop has a life of its own — planned, permitted, built, in
/// service, suspended for a while, retired — managed here, apart from
/// the service plans, which only read it (D-157). Each change is one
/// event: cap-stops `add_event` (or `correct_event`, for an event that
/// was wrong) places and checks it, and POST /stops — the deployment's
/// receive path — checks it again and stores it. With no stops document
/// yet, the page reads the stops of the network that runs today
/// (cap-stops `seed`), and the first change saves that document.

import { ask, assemble, available, produced, servedNetwork, servedStops, Doc } from "../../engine";
import { answerGraph, readAgencyZone, today } from "../kit";

// This page's capabilities answer inside its linked assembly (D-77).
assemble("stops");
import type { Event } from "../../types/Event";
import type { Refusal } from "../../types/Refusal";
import type { Status } from "../../types/Status";
import type { StatusCount } from "../../types/StatusCount";
import type { StopOn } from "../../types/StopOn";
import type { StopDetail } from "../../types/StopDetail";
import type { SiteQuestion } from "../../types/cap_stops/SiteQuestion";
import type { SurveyDraft } from "../../components/SiteSurvey.svelte";

export type Next = "planned" | "permitted" | "built" | "in_service" | "suspended" | "retired";
/// What a click on the map does: open a stop, or place a new stop or
/// a stop's new point (a point the forms' fields may also type).
export type Picking = "" | "new" | "move";

export const STATUSES: Status[] = ["planned", "permitted", "built", "in_service", "suspended", "retired"];

export const st = $state({
  status: "Reading the stops…",
  alert: false,
  busy: false,
  /// The date read ("": today, the agency's).
  date: "",
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
  /// The stop open on its card: as it stands on the date (null before
  /// it exists), its events, and what it may become that day.
  open: "",
  card: null as { stop: StopOn | null; events: Event[]; next: Next[] } | null,
  /// The routes visiting the open stop in the schedule running on the
  /// date (cap-directory `stop_detail` over the published network).
  served: null as StopDetail | null,
  /// What the next click on the map does, and the point it chose.
  picking: "" as Picking,
  point: null as { lat: number; lon: number } | null,
  /// What a site survey asks (cap-stops `questions`, D-158): the core
  /// site questions and amenity names.
  questions: { site: [] as SiteQuestion[], amenities: [] as string[] },
  /// Bumped whenever this device's survey drafts change (they live in
  /// the device's storage, which nothing watches).
  drafts: 0,
  /// Why the last change was refused, each naming its event.
  refusals: [] as Refusal[],
  failed: {} as Record<string, string>,
});

/// The date read.
export function readDate(): string {
  return st.date || today();
}

let stopsDoc: Doc | null = null;

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
// "stops-doc" (the document), "date", "show", "open".
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

function writeHash(): void {
  const q = new URLSearchParams();
  if (st.date) q.set("on", st.date);
  if (st.open) q.set("open", st.open);
  q.set("show", st.show.join(","));
  const next = `#${q}`;
  if (location.hash === next) return;
  writing = true;
  history.pushState(null, "", next);
  writing = false;
}

function readHash(): string[] {
  const q = new URLSearchParams(location.hash.slice(1));
  const changed: string[] = [];
  const on = q.get("on") ?? "";
  const date = /^\d{4}-\d{2}-\d{2}$/.test(on) ? on : "";
  if (date !== st.date) ((st.date = date), changed.push("date"));
  const open = q.get("open") ?? "";
  if (open !== st.open) ((st.open = open), changed.push("open"));
  const show = q.get("show");
  if (show !== null) {
    const wanted = STATUSES.filter((s) => show.split(",").includes(s));
    if (wanted.join(",") !== st.show.join(",")) ((st.show = wanted), changed.push("show"));
  }
  return changed;
}

async function select(...inputs: string[]): Promise<void> {
  writeHash();
  await request(...inputs);
}

addEventListener("hashchange", () => {
  if (writing) return;
  const changed = readHash();
  if (changed.length) void request(...changed);
});
addEventListener("popstate", () => {
  const changed = readHash();
  if (changed.length) void request(...changed);
});

export async function boot(): Promise<void> {
  await readAgencyZone();
  readHash();
  if (await available("stops")) {
    const q = await ask("stops", "questions", {});
    st.questions = { site: q.site, amenities: q.amenities };
  }
  await select("stops-doc", "date", "show", "open");
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

export function setDate(date: string): Promise<void> {
  st.date = /^\d{4}-\d{2}-\d{2}$/.test(date) && date !== today() ? date : "";
  return select("date");
}

export function toggleStatus(status: Status): Promise<void> {
  st.show = st.show.includes(status) ? st.show.filter((s) => s !== status) : STATUSES.filter((s) => s === status || st.show.includes(s));
  return select("show");
}

export function openStop(stop: string): Promise<void> {
  st.open = stop;
  st.picking = "";
  st.point = null;
  st.refusals = [];
  return select("open");
}

export function closeStop(): Promise<void> {
  return openStop("");
}

/// The next click on the map (or the point typed in the form) places a
/// new stop, or the open stop's new point.
export function startPicking(what: Picking): void {
  st.picking = what;
  st.point = null;
  st.refusals = [];
}

export function picked(lat: number, lon: number): void {
  if (st.picking) st.point = { lat, lon };
}

// ---------------------------------------------------------------------
// the changes: each one event, written through POST /stops
// ---------------------------------------------------------------------

/// One event added to the open stop (or to a new one).
export async function addEvent(stopId: string, event: Event, done: string): Promise<void> {
  await write(done, (doc) => ask("stops", "add_event", { stops_hash: doc, stop_id: stopId, event }), stopId);
}

// ---------------------------------------------------------------------
// the field: a survey's draft kept on this device, and the device's fix
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

/// A survey recorded: one event, its draft dropped once it is saved.
export async function survey(stopId: string, event: Event, done: string): Promise<void> {
  if (await write(done, (doc) => ask("stops", "add_event", { stops_hash: doc, stop_id: stopId, event }), stopId)) keepDraft(stopId, null);
}

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

/// An event that was wrong, corrected — or, with no event, taken out.
export async function correctEvent(stopId: string, index: number, event: Event | null, done: string): Promise<void> {
  await write(
    done,
    (doc) => ask("stops", "correct_event", { stops_hash: doc, stop_id: stopId, index, event: event ?? undefined }),
    stopId,
  );
}

/// A new stop: its first event, at the point clicked or typed. An id
/// the document already holds is another stop's.
export async function newStop(stopId: string, event: Event): Promise<void> {
  if (st.all.some((s) => s.stop_id === stopId) || st.card?.stop?.stop_id === stopId) {
    st.refusals = [];
    say(`There is already a stop ${stopId}. Choose another id.`, true);
    return;
  }
  await addEvent(stopId, event, `Added stop ${stopId}.`);
}

async function write(
  done: string,
  edit: (doc: Doc) => Promise<{ refusals: Refusal[]; stops: unknown | null }>,
  stopId: string,
): Promise<boolean> {
  if (!stopsDoc) return false;
  const doc = stopsDoc;
  st.refusals = [];
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
    const resp = await fetch("/stops", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ from, stops: answer.stops }) });
    if (resp.status === 422) {
      st.refusals = ((await resp.json()) as { refusals?: Refusal[] }).refusals ?? [];
      say("The server refuses that change.", true);
      return false;
    }
    if (resp.status === 409) {
      await select("stops-doc");
      say("Someone else saved a change first. The stops are read again: make your change again.", true);
      return false;
    }
    if (!resp.ok) throw new Error(`the server did not save it: ${(await resp.text()).trim() || resp.status}`);
    st.picking = "";
    st.point = null;
    st.open = stopId;
    await select("stops-doc", "open");
    say(done);
    return true;
  } catch (e) {
    say(`Error: ${message(e)}`, true);
    console.error(e);
    return false;
  } finally {
    st.busy = false;
  }
}

export function retry(): Promise<void> {
  return select("stops-doc");
}
