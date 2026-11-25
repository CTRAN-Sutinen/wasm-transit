// The only file of this UI that asks the bus. The rules and the failure table are in
// docs/architecture/generated-uis/disruptions.md.

import { ask, assemble, available, keep, load, produced, readManifest, served, servedNetwork, servedNetworkPin, Doc, type DeploymentManifest } from "../../engine";
import { answerGraph, applyBuild, buildWaiting, onBuildReady, clockEvery, clockLabel, clockLabelAt, inputGraph, inputLabel, installApp, nowSecs, previousDay, readFeed, readHeadKept, readLiveFeeds, reason, setAgencyZone, today, type FrameState, type InputDeclaration, type InputState, type LiveUrls, type Reader } from "../kit";
import descriptor from "./ui.json";
import { resumeSignIn } from "../../signin.svelte";
import { discardWrite, heldReason, openOutbox, outbox, queueWrite, retryWrite, sendWaiting, type Outcome } from "../../outbox.svelte";
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

assemble("disruptions");

export type View = "status" | "alerts" | "dispatch";
const VIEWS: View[] = ["status", "alerts", "dispatch"];

// `startsNow` stamps the start when the alert is saved, so riders are told
// from that moment.
export type AlertForm = { id: string; fresh: boolean; alert: ServiceAlert; template: string; unfilled: Unfilled[]; startsNow: boolean };

// The inputs' intervals are in ui.json.
const POLL_MS = 30_000;
const FOUND = 8;

export type Pick = { kind: "route" | "stop"; id: string; label: string };

export const dz = $state({
  status: "",
  alert: false,
  date: "",
  clock: "",
  zone: undefined as string | undefined,
  view: "status" as View,
  query: "",
  found: null as { routes: EntityRow[]; stops: DirectoryStop[] } | null,
  pick: null as Pick | null,
  // `status_` is the day narrowed by the pick, and `day` is whole for the
  // map and Dispatch.
  status_: null as Today | null,
  day: null as Today | null,
  live: null as LiveDay | null,
  late: [] as LateTrip[],
  notes: [] as NoteRow[],
  open: null as AlertSource | null,
  // Null while the day does not hold it, such as a cancel that the feed
  // dropped.
  openItem: null as Disruption | null,
  openRoute: "",
  openNotes: [] as NoteRow[],
  openStops: [] as TripStopView[],
  openPath: [] as Coord[],
  bus: null as { lat: number; lon: number } | null,
  // The host's clock in Unix seconds, as of the last poll.
  now: 0,
  alerts: null as AlertList | null,
  gaps: [] as AlertGap[],
  templates: [] as TemplateRow[],
  routes: [] as DirectoryRoute[],
  form: null as AlertForm | null,
  formStops: [] as { stop_id: string; name: string }[],
  reach: null as { entities: EntityReach[]; reach: Reach } | null,
  allStops: [] as DirectoryStop[],
  previewOpen: false,
  refusals: [] as AlertRefusal[],
  flags: [] as AlertFlag[],
  previewStop: null as { id: string; name: string } | null,
  previewQuery: "",
  previewFound: null as { routes: EntityRow[]; stops: DirectoryStop[] } | null,
  preview: null as StopBoard | null,
  // True while what Publish needs is missing from the form.
  previewWaits: false,
  previewFeed: "",
  busy: false,
  writeError: "",
  noteRefusals: [] as NoteRefusal[],
  updateReady: false,
  frames: {} as Record<string, FrameState>,
  inputs: {} as Record<string, InputState>,
});

// A write names these seqs, so the server checks it against the records it
// changes and each reference it adds against what the page read
// (D-233).
let logSeqs: Record<string, number> = {};

let network: Doc<Network> | null = null;
let boundary: Doc<Boundary> | null = null;
let edits: Doc<EditDocument> | null = null;
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

function nowUnix(): number {
  return Math.floor(Date.now() / 1000);
}

function documents() {
  return {
    network_hash: network!,
    edits_hash: edits ?? undefined,
    live_hash: live ?? undefined,
    stops_hash: stops ?? undefined,
    alerts_hash: alerts ?? undefined,
    notes_hash: notes ?? undefined,
    date: dz.date,
    now: dz.now,
  };
}

// The graph's inputs are "network", "docs" (the edit document, the stops,
// the alerts, the notes), "snapshot" (the decoded live feeds), "clock",
// "view", "query", "pick", and "open".
const request = answerGraph(
  [
    {
      // The map draws its buses, and the day reads its cancels and lateness
      // by hash.
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
      // The day may no longer hold the open disruption.
      id: "selection.open",
      frame: "",
      needs: ["open"],
      run: async () => hold(),
    },
    {
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
            min_delay_secs: dz.live.window.late_secs,
          })
        ).trips;
      },
    },
    {
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
      // A disruption's source has the same shape as a note's target (D-217).
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
      // The edit document's cancel takes the trip out of the patched day, so
      // `edits_hash: null` asks for the schedule that the cancel takes away.
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
      id: "directory.stops_by_ids",
      frame: "",
      needs: ["network", "form"],
      run: async () => {
        const ids = [...new Set((dz.form?.alert.informed ?? []).map((e) => e.stop).filter((x): x is string => !!x))];
        dz.formStops = network && ids.length ? (await ask("directory", "stops_by_ids", { network_hash: network, ids })).stops : [];
      },
    },
    {
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
      // The board over a live document whose alerts are the agency's feed
      // encoded with this alert published.
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
          alerts_hash: produced("rt", undefined, "rt", encoded.rt_hash),
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
  dz.inputs,
);

// A closed stop is listed on each route that serves it, so this takes the
// first disruption with the open source.
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

function dayBounds(date: string): [number, number] {
  const at = (d: string) => {
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

type Served = { network: Doc<Network>; boundary: Doc<Boundary>; pin: string };
type Manifest = DeploymentManifest & { log_seqs?: Record<string, number> };

// The network's pin is checked in the first of these that assembles.
const NETWORK_READERS = ["disruptions", "rt", "directory", "runcut"] as const;

let manifest: Manifest | null = null;

function release(): Promise<void> {
  return keep([network, boundary, edits, stops, alerts, notes, templates, live, snapshots.trips, snapshots.vehicles, snapshots.alerts]);
}

// Read again only when the pin moved.
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
    adopt: async (doc) => {
      adopt(doc);
      await release();
    },
  };
}

// A feed that does not answer drops its snapshot, so the page shows the
// schedule's day and never a stale bus as live.
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
    adopt: (m: Manifest | null) => {
      manifest = m;
      logSeqs = m?.log_seqs ?? {};
      setAgencyZone(m?.timezone);
      // The first tick ran before the first read, so a new zone reaches the
      // clock now and not at the next tick.
      const moved = m?.timezone != null && m.timezone !== dz.zone;
      dz.zone = m?.timezone;
      if (moved) tickClock();
    },
  },
  "live-feeds": {
    read: ({ signal }) => readLiveFeeds(signal),
    adopt: (u: Partial<LiveUrls> | null) => void (urls = u ?? {}),
  },
  network: {
    // Only a moved pin reads the network again.
    read: async ({ signal, held }: { signal: AbortSignal; held: Served | null }) => {
      if (!manifest) throw new Error("the deployment's manifest is not read");
      const pin = await servedNetworkPin(today(), { manifest, signal });
      if (!pin) return null;
      if (held?.pin === pin) return { value: held, key: pin };
      const got = await servedNetwork(today(), [...NETWORK_READERS], { manifest, signal });
      if (!got) return null;
      return { value: { network: got.network, boundary: got.boundary, pin }, key: pin };
    },
    adopt: async (got: Served | null) => {
      network = got?.network ?? null;
      boundary = got?.boundary ?? null;
      await release();
    },
  },
  edits: {
    // Kept on the device, so the page boots offline on it.
    read: async ({ signal }) => {
      if (!network) return null;
      const got = await readHeadKept("disruptions", "edits", signal);
      if (!got) return null;
      let document: EditDocument;
      try {
        document = JSON.parse(got.value) as EditDocument;
      } catch {
        throw new Error("the edit document is not JSON");
      }
      return { ...got, value: { document, over: network }, key: `${network.pin}:${got.key}` };
    },
    adopt: async (got: { document: EditDocument; over: Doc<Network> } | null) => {
      edits = got && got.document?.records?.length > 0 ? new Doc("edits", { value: got.document }, { over: got.over }) : null;
      await release();
    },
  },
  stops: logged("stops", (doc) => (stops = doc)),
  service_alerts: logged("service_alerts", (doc) => (alerts = doc)),
  notes: logged("notes", (doc) => (notes = doc)),
  alert_templates: logged("alert_templates", (doc) => (templates = doc)),
  trips: liveFeed("trips"),
  vehicles: liveFeed("vehicles"),
  alerts: liveFeed("alerts"),
};

const inputs = inputGraph(descriptor.inputs as InputDeclaration[], readers, dz.inputs, request);

const WRITTEN = ["manifest", "edits", "stops", "service_alerts", "notes", "alert_templates"];

export function label(input: string): string {
  return inputLabel(dz.inputs[input]);
}

export function status(): string {
  if (dz.status) return dz.status;
  const state = dz.inputs.network;
  if (!state || state.kind === "reading") return network ? "" : "Reading the network…";
  if (state.kind === "none" && !network) return "The deployment publishes no network.";
  if (state.kind === "failed" && !state.held && !network) return inputLabel(state);
  return "";
}

export function offline(): boolean {
  const m = dz.inputs.manifest;
  return m?.kind === "failed" || (m?.kind === "answered" && m.why === "the server is not answering");
}

export function asOf(): string {
  const m = dz.inputs.manifest;
  if (!offline() || !m || (m.kind !== "answered" && m.kind !== "failed") || m.asOfMs == null) return "";
  return clockLabelAt(m.asOfMs);
}

const FEEDS = ["trips", "vehicles", "alerts"] as const;

export function liveAt(): string {
  const at = Math.max(0, ...FEEDS.map((k) => dz.inputs[k]).map((s) => (s?.kind === "answered" ? s.atMs : 0)));
  return at ? clockLabelAt(at) : "";
}

export function liveError(): string {
  return FEEDS.map((k) => dz.inputs[k])
    .filter((s): s is Extract<InputState, { kind: "failed" }> => s?.kind === "failed")
    .map((s) => `${s.input}: ${s.message}`)
    .join("; ");
}

export async function boot(): Promise<void> {
  void installApp();
  onBuildReady(() => (dz.updateReady = true));
  await resumeSignIn();
  await openOutbox("disruptions", () => void afterWrite());
  tickClock();
  readHash();
  try {
    await inputs.start();
    writeHash(true);
    await request("clock", "view", "query", "pick", "open", "form", "preview-stop");
  } catch (e) {
    say(`Error: ${message(e)}`, true);
  }
  clockEvery(POLL_MS, () => {
    tickClock();
    void sendWaiting();
    dz.updateReady = buildWaiting();
    void request("clock");
  });
}

function tickClock(): void {
  dz.date = today();
  dz.clock = clockLabel();
  dz.now = nowUnix();
}

export function setView(view: View): Promise<void> {
  dz.view = view;
  writeHash();
  return request("view");
}

export function setQuery(text: string): Promise<void> {
  dz.query = text;
  return request("query");
}

export function setPick(pick: Pick | null): Promise<void> {
  dz.pick = pick;
  dz.query = "";
  dz.found = null;
  writeHash();
  return request("pick");
}

export function openDisruption(source: AlertSource | null): Promise<void> {
  dz.open = source;
  dz.bus = null;
  writeHash();
  return request("open");
}

export function showBus(at: { lat: number; lon: number }): void {
  dz.bus = at;
}

export async function newAlert(source: AlertSource | null = null): Promise<void> {
  if (!network) return;
  const start = await ask("disruptions", "start_alert", { ...documents(), source: source ?? undefined });
  return openForm({ id: newId("alert"), fresh: true, alert: start.alert, template: "", unfilled: [], startsNow: true }, start.preview_stop ?? null);
}

export function startAlertFor(source: AlertSource): Promise<void> {
  return newAlert(source);
}

export function editAlert(id: string, alert: ServiceAlert): Promise<void> {
  return openForm({ id, fresh: false, alert: $state.snapshot(alert) as ServiceAlert, template: "", unfilled: [], startsNow: false }, null);
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

function stamped(form: AlertForm): ServiceAlert {
  if (!form.startsNow) return form.alert;
  const [first, ...rest] = form.alert.communication ?? [{ start: null, end: null }];
  return { ...form.alert, communication: [{ ...first, start: dz.now }, ...rest] };
}

export function setStartsNow(on: boolean): Promise<void> {
  if (!dz.form) return Promise.resolve();
  dz.form = on ? { ...dz.form, startsNow: true } : { ...dz.form, startsNow: false, alert: stamped({ ...dz.form, startsNow: true }) };
  return request("form");
}

export function openPreview(open: boolean): Promise<void> {
  dz.previewOpen = open;
  return request("preview-open");
}

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

export async function changeAlert(alert: ServiceAlert): Promise<void> {
  if (!dz.form) return;
  dz.form = { ...dz.form, alert };
  const stop = dz.previewStop ? null : alert.informed.find((e) => e.stop)?.stop;
  if (stop && network) {
    const [s] = (await ask("directory", "stops_by_ids", { network_hash: network, ids: [stop] })).stops;
    if (s) dz.previewStop = { id: s.stop_id, name: s.name };
    return request("form", "preview-stop");
  }
  return request("form");
}

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

export async function fillTemplate(template: string): Promise<void> {
  const form = dz.form;
  if (!form || !network || !templates) return;
  const period = form.alert.communication?.[0];
  const dateOf = (unix: number | null | undefined) => (unix == null ? undefined : zonedDate(unix));
  const { filled } = await ask("alerts", "fill", {
    templates_hash: templates,
    template,
    network_hash: network,
    input: {
      informed: form.alert.informed,
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

// Alerts `put` checks the alert here first, then the write goes as its
// intent (POST /logs/alerts). Returns true when the write was kept, sent or
// waiting on the device.
export function saveAlert(publish: boolean): Promise<boolean> {
  const form = dz.form;
  if (!form) return Promise.resolve(false);
  const alert: ServiceAlert = { ...stamped(form), status: publish ? "published" : "draft" };
  const label = `${publish ? "Publish" : "Draft"} alert: ${alert.header?.[0]?.text ?? form.id}`;
  return writeAlerts(publish ? "Published." : "Saved as a draft.", label, { cap: "alerts", op: "put", args: { id: form.id, alert } }, () =>
    ask("alerts", "put", { alerts_hash: alerts ?? undefined, id: form.id, alert }),
  );
}

// The time is the person's act, so it travels in the write's arguments.
export function endAlert(id: string): Promise<boolean> {
  if (!alerts) return Promise.resolve(false);
  const doc = alerts;
  const at = nowUnix();
  // A period holds through its end second, so the list is read once that
  // second has passed.
  return writeAlerts("Ended.", `End alert ${id}`, { cap: "alerts", op: "end", args: { id, at } }, () => ask("alerts", "end", { alerts_hash: doc, id, at }), at + 1);
}

async function writeAlerts(
  done: string,
  label: string,
  op: { cap: string; op: string; args: unknown },
  check: () => Promise<{ refusals: AlertRefusal[]; alerts: unknown | null }>,
  readAt = 0,
): Promise<boolean> {
  dz.busy = true;
  dz.writeError = "";
  try {
    const answer = await check();
    if (answer.refusals.length || !answer.alerts) {
      dz.refusals = answer.refusals;
      dz.writeError = "Not saved";
      return false;
    }
    const outcome = await queueWrite({ log: "alerts", ops: [op], readSeq: logSeqs.alerts, readSeqs: logSeqs, label });
    if (!(await settled(outcome, "alerts"))) return false;
    dz.form = null;
    dz.now = Math.max(nowUnix(), readAt);
    say(outcome.state === "sent" ? done : waitingText());
    if (outcome.state === "sent") await afterWrite();
    await request("form", "clock");
    return true;
  } catch (e) {
    dz.writeError = message(e);
    return false;
  } finally {
    dz.busy = false;
  }
}

function waitingText(): string {
  return outbox.needsSignIn ? "Saved on this device. Sign in to send it." : "Saved on this device. It is sent when the server answers.";
}

async function afterWrite(): Promise<void> {
  await inputs.read(...WRITTEN);
}

// A write that the server refused at once stays in its form with the
// server's reasons, so the outbox does not keep a second copy. Returns true
// when the write was kept.
async function settled(outcome: Outcome, what: "alerts" | "notes"): Promise<boolean> {
  if (outcome.state !== "held") return true;
  const w = outbox.held.find((h) => h.key === outcome.key);
  await discardWrite(outcome.key);
  const body = (outcome.reasons ?? {}) as { refusals?: unknown[] };
  if (outcome.status === 409) {
    await afterWrite();
    dz.writeError = "Changed by someone else. Check it, and save again.";
  } else if (outcome.status === 422) {
    if (what === "alerts") dz.refusals = (body.refusals ?? []) as AlertRefusal[];
    else dz.noteRefusals = (body.refusals ?? []) as NoteRefusal[];
    dz.writeError = "Not saved";
  } else dz.writeError = w ? heldReason(w) : `Not saved (${outcome.status})`;
  return false;
}

// Notes are written as their intent (POST /logs/notes), and the server
// names the writer and the time.

// The note keeps the id it was made with through every resend.
export async function writeNote(target: NoteTarget, text: string, id: string | null = null): Promise<boolean> {
  dz.busy = true;
  dz.writeError = "";
  dz.noteRefusals = [];
  try {
    const op = { cap: "notes", op: "write", args: { id: id ?? newId("note"), target, text } };
    const outcome = await queueWrite({ log: "notes", ops: [op], readSeq: logSeqs.notes, readSeqs: logSeqs, label: `${id ? "Edit note" : "Note"}: ${text.trim().slice(0, 60)}` });
    if (!(await settled(outcome, "notes"))) return false;
    if (outcome.state === "sent") await afterWrite();
    else say(waitingText());
    return true;
  } catch (e) {
    dz.writeError = message(e);
    return false;
  } finally {
    dz.busy = false;
  }
}

export function heldWrites(): { key: string; label: string; reason: string }[] {
  return outbox.held.map((w) => ({ key: w.key, label: w.label, reason: heldReason(w) }));
}

export function sendNow(): Promise<void> {
  return sendWaiting();
}

export async function retryHeld(key: string): Promise<void> {
  await inputs.read(...WRITTEN);
  const w = outbox.held.find((h) => h.key === key);
  await retryWrite(key, { readSeq: w?.log ? logSeqs[w.log] : undefined, readSeqs: logSeqs });
}

export function discardHeld(key: string): Promise<void> {
  return discardWrite(key);
}

// Never over a form being written.
export function applyUpdate(): void {
  if (dz.form || dz.busy) return;
  applyBuild();
}

function newId(kind: string): string {
  return `${kind}-${dz.date}-${crypto.randomUUID().slice(0, 8)}`;
}

function zonedDate(unix: number): string {
  const parts = new Intl.DateTimeFormat("en-CA", { timeZone: dz.zone, year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date(unix * 1000));
  return parts;
}

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

export function why(frame: string): string {
  return reason(dz.frames[frame]);
}

// Errors are never cached.
export function retry(): Promise<void> {
  return request.retry();
}
