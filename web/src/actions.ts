import { app, getEditDocument, setEditDocument, setStreets, getStreets, type DiversionDraft } from "./state.svelte";
import { get as idbGet, set as idbSet, del as idbDel } from "idb-keyval";
import { ensureAnalysisLayer, ensureEditReports, ensureRuncutBoarding, ensureRuncutEdits, ensureRouterBoarding, ensureRouterEdits, ensureStreets, runcutCap, runcutModule, routerCap, routerModule, rtCap, rtModule, boardCap, boardModule, ensureBoardBoarding, ensureBoardEdits, calendarCap, calendarModule, ensureCalendarNetwork, contentHash, networkCap, networkModule, divertModule, divertCap, divertOverFeed, streetsSnap, streetsRoute, ensureDirectory, ensureFeedSummary, ensureGtfsFeed, gtfsModule, gtfsCap } from "./capabilities.svelte";
import { lineFeature, lonLat } from "./mapview";
import type { EditRecord } from "./types/EditRecord";
import type { DiversionDraft as DraftDoc } from "./types/DiversionDraft";
import type { Itinerary } from "./types/Itinerary";
import type { StreetPosition } from "./types/StreetPosition";
import type { StreetPoint } from "./types/StreetPoint";
import type { Streets } from "./streets";

/// One drawn plan feature (transit leg or walk segment). MapView
/// consumes these; the geometry is GeoJSON built by the host's view
/// code (mapview.ts).
export type PlanFeature = {
  type: "Feature";
  geometry: { type: "LineString"; coordinates: [number, number][] };
  properties: { kind: "transit" | "walk" };
};

// ---------------------------------------------------------------
// persistence: feed bytes in IndexedDB, settings in localStorage
// ---------------------------------------------------------------

const FEED_KEY = "transit:gtfs-feed";
export const EDITS_KEY = "transit:edits";
const SETTINGS_KEY = "transit:settings";

export function saveSettings(): void {
  localStorage.setItem(
    SETTINGS_KEY,
    JSON.stringify({
      gtfsUrl: app.gtfsUrl,
      rtUrl: app.rtUrl,
      vpUrl: app.vpUrl,
      streetsUrl: app.streetsUrl,
      rtPolling: app.rtPolling,
      vpPolling: app.vpPolling,
    })
  );
}

export async function clearCachedFeed(): Promise<void> {
  await idbDel(FEED_KEY);
  app.cachedFeed = null;
  app.status = "Cached feed cleared.";
}

/** Cache the raw zip so the next page load can restore instantly.
 * (We cache bytes and re-parse on restore — parsing is cheap, and the
 * wasm-side parsed structures can't be serialized.) The write is
 * awaited by loadGtfsBytes so a following reload cannot race it. */
async function cacheFeed(label: string, bytes: Uint8Array): Promise<void> {
  app.cachedFeed = label;
  try {
    await idbSet(FEED_KEY, { label, bytes });
  } catch (e) {
    console.warn("IndexedDB cache write failed:", e);
  }
}

export { FEED_KEY };

/// The raw bytes of the ACTIVE feed, held for the bus: capability
/// modules (the station board) address the same document the
/// monolith holds. Wholesale-replaced with the feed, never mutated.
let activeFeedBytes: Uint8Array | null = null;
export function setFeedBytes(bytes: Uint8Array): void {
  activeFeedBytes = bytes;
}
let feedLineSeq = 0;
/// The feed line — the feed's OWN reads, identity (publisher,
/// version) and the at-a-glance summary, answered through cap-gtfs —
/// and the directory's prime (memoized: every picker, name lookup,
/// and the diversion editor share the load). Decoration-grade: null
/// when the capability cannot assemble.
export async function refreshFeedLine(): Promise<void> {
  const seq = ++feedLineSeq;
  const bytes = feedBytes();
  if (!app.feedLoaded || !bytes) {
    app.feedInfo = null;
    app.feedSummary = null;
    return;
  }
  try {
    await ensureDirectory(bytes);
    if (seq !== feedLineSeq) return;
    app.feedSummary = await ensureFeedSummary(bytes);
    if (seq !== feedLineSeq) return;
    const feedHash = await ensureGtfsFeed(bytes);
    if (seq !== feedLineSeq || !feedHash) return;
    const module = await gtfsModule();
    if (!module || seq !== feedLineSeq) return;
    app.feedInfo = gtfsCap(module).feedInfo(feedHash);
  } catch {
    if (seq === feedLineSeq) {
      app.feedInfo = null;
      app.feedSummary = null;
    }
  }
}

export function feedBytes(): Uint8Array | null {
  return activeFeedBytes;
}

// ---------------------------------------------------------------
// clock helpers: JS owns time, wasm receives explicit values
// ---------------------------------------------------------------

// Deterministic clock for tests: when set, serviceDate()/secondsSince-
// Midnight() return the override instead of the wall clock. JS owns
// time for the whole app (wasm receives explicit values), so this is
// the single lever.
let nowOverride: { date: string; secs: number } | null = null;
export function setNowOverride(value: { date: string; secs: number } | null): void {
  nowOverride = value; // { date: "YYYY-MM-DD", secs: number } | null
  // The override changing IS time moving: re-derive the views that
  // track the clock (departure board, blocks-of-the-day).
  app.clock = Date.now();
}

export function serviceDate(): string {
  if (nowOverride) return nowOverride.date;
  const now = new Date();
  const p = (n: number): string => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;
}

export function secondsSinceMidnight(): number {
  if (nowOverride) return nowOverride.secs;
  const now = new Date();
  return now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds();
}

// ---------------------------------------------------------------
// fetching (through the transit-server proxy for remote URLs)
// ---------------------------------------------------------------

async function fetchBytes(url: string): Promise<Uint8Array> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText} for ${url}`);
  return new Uint8Array(await res.arrayBuffer());
}

function proxied(url: string): string {
  return `/rt?url=${encodeURIComponent(url)}`;
}

function error(e: any): void {
  app.status = `Error: ${e?.message ?? e}`;
  console.error(e);
}

// ---------------------------------------------------------------
// loaders — each writes only the state it owns
// ---------------------------------------------------------------

export async function loadGtfsBytes(bytes: Uint8Array, label: string): Promise<void> {
  setFeedBytes(bytes); // the bus reads this document everywhere
  app.feedLoaded = true;
  app.selectedStop = null;
  app.selectedTrip = null;
  app.tripFeature = null;
  app.plan = null;
  app.planBaseline = null;
  app.planNotice = null;
  // The edit document may reference this feed (or an older one) —
  // revalidate every record against the new document and derived
  // views re-derive.
  pushEdits();
  // A new feed is a new network: the live document re-resolves.
  void refreshLive();
  await cacheFeed(label, bytes.slice());
  app.status = `Loaded ${label}.` + " Everything below is running client-side.";
}

export async function loadGtfsFile(file: File): Promise<void> {
  app.status = `Parsing ${file.name}...`;
  try {
    loadGtfsBytes(new Uint8Array(await file.arrayBuffer()), file.name);
  } catch (e) { error(e); }
}

export async function loadGtfsFromUrl(): Promise<void> {
  if (!app.gtfsUrl) { app.status = "Enter a GTFS zip URL first."; return; }
  try {
    app.status = `Downloading ${app.gtfsUrl}...`;
    loadGtfsBytes(await fetchBytes(proxied(app.gtfsUrl)), app.gtfsUrl);
  } catch (e) { error(e); }
}

export async function loadRtBytes(bytes: Uint8Array): Promise<void> {
  // The decoder lives in the REALTIME CAPABILITY; the bytes are
  // awaited before crossing into wasm — a Promise in a &[u8] slot
  // silently decodes as an empty buffer. The module registers the
  // snapshot; the live document is its answer over it.
  const module = await rtModule();
  if (!module) {
    app.status =
      "Realtime capability unavailable — live times are off until it loads (the capability bus did not assemble).";
    return;
  }
  const { overview, rtHash } = rtCap(module).decode(bytes);
  app.rtOverview = overview;
  app.rtHash = rtHash;
  await refreshLive();
}

export async function loadRtFromUrl(): Promise<void> {
  if (!app.rtUrl) { app.status = "Enter a GTFS-rt URL first."; return; }
  try {
    await loadRtBytes(await fetchBytes(proxied(app.rtUrl)));
  } catch (e) { error(e); }
}

export async function loadVpFromUrl(): Promise<void> {
  if (!app.vpUrl) { app.status = "Enter a vehicle positions URL first."; return; }
  try {
    const module = await rtModule();
    if (!module) {
      app.status =
        "Realtime capability unavailable — vehicle positions are off until it loads (the capability bus did not assemble).";
      return;
    }
    app.vpHash = rtCap(module).decode(await fetchBytes(proxied(app.vpUrl))).rtHash;
    await refreshLive();
    app.status = `Vehicle positions loaded: ${app.live?.summary.vehicles ?? 0} vehicles on the map.`;
  } catch (e) { error(e); } // keep the last good positions on the map
}

// ---------------------------------------------------------------
// the live document — cap-rt's answer, never a host join
// ---------------------------------------------------------------

/// Registrations per rt module instance (a module dropped and
/// re-fetched starts empty): the network and its boundary, by their
/// own identities.
const rtRegistrations = new WeakMap<object, Map<string, string>>();
let liveSeq = 0;

/// Ask cap-rt for THE live document over the current snapshots: the
/// network and its boundary (from the board's boarding — the import
/// already ran) resolve rt trip ids into trip instances inside the
/// module.
/// Keeps the last good document when the bus cannot answer.
export async function refreshLive(): Promise<void> {
  const seq = ++liveSeq;
  const bytes = feedBytes();
  if (!bytes || (!app.rtHash && !app.vpHash)) {
    app.live = null;
    return;
  }
  try {
    const [module, boarding] = await Promise.all([rtModule(), ensureBoardBoarding(bytes)]);
    if (seq !== liveSeq || !module || !boarding) return;
    const cap = rtCap(module);
    let seen = rtRegistrations.get(module);
    if (!seen) rtRegistrations.set(module, (seen = new Map()));
    const netKey = `net:${boarding.networkHash}`;
    const networkHash = seen.get(netKey) ?? cap.loadNetwork(boarding.network);
    seen.set(netKey, networkHash);
    const boundaryKey = `boundary:${boarding.networkHash}`;
    const boundaryHash = seen.get(boundaryKey) ?? cap.loadBoundary(boarding.boundary);
    seen.set(boundaryKey, boundaryHash);
    if (seq !== liveSeq) return;
    app.live = cap.live(networkHash, boundaryHash, app.rtHash, app.vpHash);
  } catch (e) {
    console.warn("live document failed:", e);
  }
}

// ---------------------------------------------------------------
// the board — cap-board's answer at the selected stop
// ---------------------------------------------------------------

const boardLives = new WeakMap<object, Set<string>>();
let boardSeq = 0;

/// Ask cap-board for the selected stop's board: the edit document
/// registered, the live document (when there is one) registered by
/// hash, and the module answers whole — pooling, realtime-first
/// order, each row's status. Off (and saying so) when the bus cannot
/// assemble it.
export async function refreshBoard(): Promise<void> {
  const seq = ++boardSeq;
  const stop = app.selectedStop;
  const bytes = feedBytes();
  const live = app.live;
  const records = getEditDocument();
  const date = serviceDate();
  const fromSecs = secondsSinceMidnight();
  if (!stop || !app.feedLoaded || !bytes) {
    app.board = null;
    app.boardOff = false;
    return;
  }
  try {
    const boarding = await ensureBoardBoarding(bytes);
    const editsHash = boarding ? await ensureBoardEdits(boarding.networkHash, records) : null;
    const module = boarding ? await boardModule() : null;
    if (seq !== boardSeq) return;
    if (!boarding || !module) {
      app.board = null;
      app.boardOff = true;
      return;
    }
    const cap = boardCap(module);
    let liveHash: string | null = null;
    if (live) {
      let loaded = boardLives.get(module);
      if (!loaded) boardLives.set(module, (loaded = new Set()));
      if (!loaded.has(live.liveHash)) {
        const hash = cap.loadLive(live.live);
        if (hash !== live.liveHash) {
          throw new Error(`live identity disagrees across capabilities: ${hash} != ${live.liveHash}`);
        }
        loaded.add(hash);
      }
      liveHash = live.liveHash;
    }
    app.board = cap.stopBoard(boarding.networkHash, editsHash, stop, date, fromSecs, 15, liveHash);
    app.boardOff = false;
  } catch (e) {
    console.warn("board failed:", e);
    if (seq === boardSeq) {
      app.board = null;
      app.boardOff = true;
    }
  }
}

let blocksSeq = 0;

/// The day's blocks (cap-runcut's index facet): only blocks with
/// authored trips running that day. Keeps a valid selection,
/// defaulting to the first block.
export async function refreshBlocks(): Promise<void> {
  const seq = ++blocksSeq;
  const bytes = feedBytes();
  const records = getEditDocument();
  const date = serviceDate();
  if (!app.feedLoaded || !bytes) {
    app.blocks = [];
    return;
  }
  try {
    const boarding = await ensureRuncutBoarding(bytes);
    const editsHash = boarding ? await ensureRuncutEdits(boarding.networkHash, records) : null;
    const module = boarding ? await runcutModule() : null;
    if (seq !== blocksSeq) return;
    if (!boarding || !module) {
      app.blocks = [];
      app.blocksOff = true;
      app.status =
        "runcut capability unavailable — the blocks view is off until it loads (the capability bus did not assemble).";
      return;
    }
    app.blocks = runcutCap(module).blocks(boarding.networkHash, editsHash, date);
    app.blocksOff = false;
    if (!app.blocks.some((b) => b.block_id === app.selectedBlock)) {
      app.selectedBlock = app.blocks[0]?.block_id ?? "";
    }
  } catch {
    if (seq === blocksSeq) app.blocks = [];
  }
}

let chainSeq = 0;

/// The selected block's chain (cap-runcut — the layover ripple and
/// its health are the module's), each row naming its trip by id.
export async function refreshBlockChain(): Promise<void> {
  const seq = ++chainSeq;
  const blockId = app.selectedBlock;
  const bytes = feedBytes();
  const records = getEditDocument();
  const date = serviceDate();
  if (!blockId || !app.feedLoaded || !bytes) {
    app.blockChain = null;
    return;
  }
  try {
    const boarding = await ensureRuncutBoarding(bytes);
    const editsHash = boarding ? await ensureRuncutEdits(boarding.networkHash, records) : null;
    const module = boarding ? await runcutModule() : null;
    if (seq !== chainSeq) return;
    if (!boarding || !module) {
      app.blockChain = null;
      app.chainOff = true;
      return;
    }
    app.blockChain = { trips: runcutCap(module).blockTrips(boarding.networkHash, editsHash, blockId, date) };
    app.chainOff = false;
  } catch {
    if (seq === chainSeq) {
      app.blockChain = null;
      app.chainOff = true;
    }
  }
}

let servicesSeq = 0;

/// The service keys running today (cap-calendar) — decoration: empty
/// when the capability cannot assemble.
export async function refreshServices(): Promise<void> {
  const seq = ++servicesSeq;
  const bytes = feedBytes();
  const date = serviceDate();
  if (!app.feedLoaded || !bytes) {
    app.activeServices = [];
    return;
  }
  try {
    const networkHash = await ensureCalendarNetwork(bytes);
    const module = networkHash ? await calendarModule() : null;
    if (seq !== servicesSeq) return;
    app.activeServices = networkHash && module ? calendarCap(module).activeServices(networkHash, date) : [];
  } catch {
    if (seq === servicesSeq) app.activeServices = [];
  }
}

export async function dropFile(file: File): Promise<void> {
  if (/\.zip$/i.test(file.name)) return loadGtfsFile(file);
  // NB: the bytes must be awaited before crossing into wasm — a
  // Promise in a &[u8] slot silently decodes as an empty buffer.
  if (/\.(pb|protobuf)$/i.test(file.name)) {
    return loadRtBytes(new Uint8Array(await file.arrayBuffer()));
  }
  app.status = `Unrecognized file: ${file.name}`;
}

// ---------------------------------------------------------------
// selections
// ---------------------------------------------------------------

export function selectStop(stopId: string): void {
  app.selectedStop = stopId;
  app.clock = Date.now(); // re-derive the departure board
}

/// A trip instance's selection key — the model's identity.
export function tripKey(trip: { trip: string; start_secs: number }): string {
  return `${trip.trip}@${trip.start_secs}`;
}

export async function showTrip(trip: { trip: string; start_secs: number }): Promise<void> {
  const key = tripKey(trip);
  // Clicking the selected row again deselects: the route line always
  // has a way off the map.
  if (app.selectedTrip === key) {
    clearTrip();
    return;
  }
  app.selectedTrip = key;
  const bytes = feedBytes();
  if (!bytes) return;
  try {
    // Click-through: cap-runcut answers the trip's line over the
    // EFFECTIVE day (a diverted trip draws its diversion).
    const boarding = await ensureRuncutBoarding(bytes);
    const module = boarding ? await runcutModule() : null;
    if (!boarding || !module) {
      // No geometry -> clear the path, but never wipe other state.
      app.tripFeature = null;
      return;
    }
    const editsHash = await ensureRuncutEdits(boarding.networkHash, getEditDocument());
    const path = runcutCap(module).tripPath(
      boarding.networkHash, editsHash, trip.trip, trip.start_secs, serviceDate());
    app.tripFeature = lineFeature(path, { trip: key });
  } catch (e) {
    app.tripFeature = null;
    error(e);
  }
}

/** Deselect the board's trip: the highlighted row and its map line. */
export function clearTrip(): void {
  app.selectedTrip = null;
  app.tripFeature = null;
}

export function refreshNow(): void {
  app.clock = Date.now();
}

// ---------------------------------------------------------------
// trip planning (RAPTOR, in wasm)
// ---------------------------------------------------------------

function hhmmssToSecs(hhmm: string | null): number {
  const [h = 0, m = 0, s = 0] = (hhmm ?? "").split(":").map(Number);
  return h * 3600 + m * 60 + s;
}

export async function planTrip(fromStopId: string, toStopId: string, departTime: string): Promise<void> {
  if (!fromStopId || !toStopId) {
    app.planNotice = "Pick a from and a to stop first.";
    return;
  }
  const bytes = feedBytes();
  if (!bytes) return;
  try {
    const depart = departTime
      ? hhmmssToSecs(departTime)
      : secondsSinceMidnight();
    // The router capability plans over the NETWORK document derived
    // from the loaded feed; the legs name their trip instance
    // (trip, start).
    const boarding = await ensureRouterBoarding(bytes);
    const module = boarding ? await routerModule() : null;
    if (!module || !boarding) {
      app.planNotice = "Planning needs the router capability — the assembly is unavailable.";
      return;
    }
    // One op whether or not edits exist: with edits in force it
    // plans both worlds (the delta), and its notice says what the
    // rider is told whenever the diversion changes the trip.
    const editsHash = await ensureRouterEdits(boarding.networkHash, getEditDocument());
    const cmp = routerCap(module).planCompare(
      boarding.networkHash, editsHash, fromStopId, toStopId, serviceDate(), depart, 6);
    app.plan = cmp.active;
    app.planBaseline = cmp.baseline;
    app.planNotice = cmp.notice;
    // A new plan is the focus now — the previously selected trip's
    // line would fight the itinerary for the same pixels.
    clearTrip();
    app.itineraryFeatures = await buildItineraryFeatures(app.plan);
  } catch (e) {
    app.planNotice = `Planning failed: ${e.message}`;
    error(e);
  }
}

/// The plan's drawn features: cap-router answers each leg's path over
/// the day it planned on (a transit leg's ridden segment, a walk
/// through its via stops); the host only shapes GeoJSON.
async function buildItineraryFeatures(plan: Itinerary | null): Promise<PlanFeature[]> {
  if (!plan) return [];
  const bytes = feedBytes();
  if (!bytes) return [];
  const boarding = await ensureRouterBoarding(bytes);
  const module = boarding ? await routerModule() : null;
  if (!boarding || !module) return [];
  const editsHash = await ensureRouterEdits(boarding.networkHash, getEditDocument());
  return routerCap(module)
    .itineraryPaths(boarding.networkHash, editsHash, serviceDate(), plan)
    .filter((leg) => leg.path.length >= 2)
    .map((leg) => ({
      type: "Feature",
      geometry: { type: "LineString", coordinates: lonLat(leg.path) },
      properties: { kind: leg.kind },
    }));
}

// ---------------------------------------------------------------
// operator edits (the sync document)
//
// Every next document is an answer: cap-divert proposes, retires and
// restores (the envelope rules — rev, stamp, tombstone), cap-sync
// converges. adoptEdits(next) is the ONLY path that installs one: it
// (1) adopts it wholesale, (2) schedules one debounced persist, and
// (3) pushEdits: revalidates it through the day views (which drop
// their derived caches), bumps feedEpoch so derived views re-query,
// and re-reads the list rows. The host passes the clock and the record ids.
// ---------------------------------------------------------------

let persistTimer: ReturnType<typeof setTimeout> | null = null;
let pendingWrite: Promise<void> | null = null;
function persistNow(): Promise<void> {
  if (persistTimer !== null) clearTimeout(persistTimer);
  // The document is plain data ($state.raw) — IndexedDB can clone
  // it directly, no plain-ifying needed.
  pendingWrite = idbSet(EDITS_KEY, getEditDocument()).catch((e) =>
    console.warn("edit document cache write failed:", e));
  return pendingWrite;
}

function schedulePersist(): void {
  if (persistTimer !== null) clearTimeout(persistTimer);
  persistTimer = setTimeout(persistNow, 400);
}

/** Write the document immediately (cancels any scheduled write).
 * Test hook for deterministic reloads; normal flow only schedules. */
export function flushPersist(): Promise<void> {
  return persistNow() ?? Promise.resolve();
}

/** Revalidate the edit document: the day views validate per record
 * against the network at load_edits (reported, never rejected);
 * the reports land here, and derived views re-query via feedEpoch.
 * Also called after a feed load — every record is revalidated
 * against the new document. No-op when no feed is loaded (records
 * wait in the doc). */
export async function pushEdits(): Promise<void> {
  const bytes = feedBytes();
  if (!bytes) return;
  try {
    const reports = await ensureEditReports(bytes, getEditDocument());
    app.editReports = reports ?? [];
    await refreshDiversionRows();
  } catch (e) {
    error(e);
    return;
  }
  app.feedEpoch++;
}

export function adoptEdits(next: EditRecord[]): Promise<void> {
  setEditDocument(next);
  schedulePersist();
  return pushEdits();
}

/** Read a stored edit document (IndexedDB, a server response) through
 * cap-divert's `sanitize`: the records valid under the current schema,
 * defaults filled; what does not parse is dropped, and said so. */
export async function readEdits(stored: unknown): Promise<EditRecord[]> {
  const module = await divertModule();
  if (!module) {
    app.status = "The stored edit document needs the divert capability to read — it did not assemble.";
    return [];
  }
  const { records, dropped } = divertCap(module).sanitize(stored);
  if (dropped > 0) {
    app.status = `Dropped ${dropped} malformed edit record(s) from the saved document — they were not valid under the current schema.`;
    console.warn(`dropped ${dropped} malformed edit record(s)`);
  }
  return records;
}

/** The dispatcher's list: the edit document's rows, names resolved by
 * cap-divert over the imported network. */
export async function refreshDiversionRows(): Promise<void> {
  const bytes = feedBytes();
  const records = getEditDocument();
  const over = bytes && records.length ? await divertOverFeed(bytes) : null;
  app.diversionRows = over ? over.divert.list(over.networkHash, records) : [];
}

export function startDiversion(): void {
  const today = serviceDate();
  app.diversionDraft = {
    routeId: "",
    patternId: "", // the pattern anchored against (the model's direction)
    firstKept: "",
    lastKept: "",
    served: [],
    addedMins: 5,
    start: today,
    end: today,
    slices: [],
  };
  app.pickingAnchor = "first";
}

export function cancelDiversion(): void {
  app.diversionDraft = null;
  app.diversionView = null;
  app.routeContext = null;
  app.pickingAnchor = null;
  app.drawingDetour = false;
}

/** Stop click (list or map) while the editor is open: first click sets
 * the first anchor, second sets the last. */
export function setDiversionAnchor(stopId: string): void {
  const draft: DiversionDraft | null = app.diversionDraft;
  if (!draft || !stopId) return;
  if (app.pickingAnchor === "first") {
    draft.firstKept = stopId;
    draft.lastKept = "";
    app.pickingAnchor = "last";
  } else if (app.pickingAnchor === "last") {
    if (stopId === draft.firstKept) return;
    draft.lastKept = stopId;
    app.pickingAnchor = null;
  }
}

/** Skip ↔ serve one interior stop (cap-divert skips every interior
 * stop the draft does not list as served). */
export function toggleServed(stopId: string): void {
  const draft = app.diversionDraft;
  if (!draft) return;
  draft.served = draft.served.includes(stopId)
    ? draft.served.filter((s) => s !== stopId)
    : [...draft.served, stopId];
}

/// Snap to the imported street graph — sync, for the map's click
/// handlers (an event handler cannot await). Null with no graph
/// imported (or the module gone): "no street nearby".
export function snapToStreets(lat: number, lon: number): StreetPosition | null {
  const streets = getStreets();
  return streets ? streetsSnap(streets.streetsHash, lat, lon) : null;
}

/// Route between two graph positions along the imported streets —
/// empty (the straight-line fallback) when there is no path or graph.
export function routeOnStreets(from: StreetPosition, to: StreetPosition): StreetPoint[] {
  const streets = getStreets();
  return streets ? streetsRoute(streets.streetsHash, from, to) : [];
}

/// Commit one waypoint step of the trace: the routed geometry plus
/// the graph position it ends at (so the next step routes from
/// there). cap-divert flattens the steps.
export function addDetourSlice(points: [number, number][], endPos: StreetPosition | null): void {
  if (!points.length) return;
  app.diversionDraft?.slices.push({ points, endPos });
}

/// Revert the last waypoint step (its whole routed geometry).
export function undoDetourSlice(): void {
  const draft = app.diversionDraft;
  if (!draft?.slices.length) return;
  draft.slices.pop();
}

export function clearDetour(): void {
  if (app.diversionDraft) app.diversionDraft.slices = [];
}

export function toggleDrawing(): void {
  app.drawingDetour = !app.drawingDetour;
}

/// Import a street network (GeoJSON: LineString/MultiLineString
/// features, e.g. an OSM extract from an open data portal). Detour
/// clicks snap to it. Not persisted — reference data, re-imported
/// per session.
export async function importStreetsFile(file: File): Promise<void> {
  try {
    await installStreets(JSON.parse(await file.text()), file.name);
  } catch (e) {
    app.status = `Street import failed: ${(e as Error).message}`;
    console.warn("street import failed:", e);
  }
}

/// Same, from a URL (through the /rt proxy — open data portals
/// rarely send CORS headers). Paginates ArcGIS-style queries: when a
/// page comes back flagged `exceededTransferLimit`, the next page is
/// requested with `resultOffset`/`resultCount` until the service is
/// drained. Static .geojson files ignore the params and never set
/// the flag, so they load in exactly one request as before.
export async function loadStreetsFromUrl(): Promise<void> {
  if (!app.streetsUrl) { app.status = "Enter a street network URL first."; return; }
  try {
    const features: unknown[] = [];
    for (let offset = 0; ; offset += features.length) {
      if (offset > 2_000_000) {
        throw new Error("over 2M features — the service may be ignoring resultOffset");
      }
      const page = new URL(app.streetsUrl, location.href);
      page.searchParams.set("resultOffset", String(offset));
      page.searchParams.set("resultCount", "2000");
      app.status = `Downloading street network — ${features.length} features so far...`;
      const bytes = await fetchBytes(proxied(page.toString()));
      const json = JSON.parse(new TextDecoder().decode(bytes));
      const pageFeatures = (json as { features?: unknown[] })?.features;
      if (!Array.isArray(pageFeatures)) {
        throw new Error("not a GeoJSON FeatureCollection (is the URL a direct geojson endpoint?)");
      }
      features.push(...pageFeatures);
      const more = (json as { properties?: { exceededTransferLimit?: boolean } })
        ?.properties?.exceededTransferLimit === true;
      if (!more || pageFeatures.length === 0) break;
    }
    await installStreets({ type: "FeatureCollection", features }, app.streetsUrl);
  } catch (e) {
    app.status = `Street import failed: ${(e as Error).message}`;
    console.warn("street import failed:", e);
  }
}

export function clearStreets(): void {
  setStreets(null);
  app.status = "Street network cleared — detour clicks snap to stops only.";
}

// ------------------------------------------------------------------
// Population layer (planner coverage analysis). Census block
// centroids as GeoJSON points with a population property — the
// same standing as the street network: app config, reference data,
// re-imported per session. The analysis capability owns the points
// under their content hash; the registration answer is the import
// handle, and coverage runs against the planner's document.
// ------------------------------------------------------------------

export async function importPopulationFile(file: File): Promise<void> {
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    await installPopulation(bytes, file.name);
  } catch (e) {
    app.status = `Population layer import failed: ${(e as Error).message}`;
    console.warn("population import failed:", e);
  }
}

export async function loadPopulationFromUrl(): Promise<void> {
  if (!app.popUrl) {
    app.status = "Enter a population layer URL first.";
    return;
  }
  try {
    const features: unknown[] = [];
    for (let offset = 0; ; offset += features.length) {
      if (offset > 2_000_000) {
        throw new Error("over 2M features — the service may be ignoring resultOffset");
      }
      const page = new URL(app.popUrl, location.href);
      page.searchParams.set("resultOffset", String(offset));
      page.searchParams.set("resultCount", "2000");
      app.status = `Downloading population layer — ${features.length} features so far...`;
      const bytes = await fetchBytes(proxied(page.toString()));
      const json = JSON.parse(new TextDecoder().decode(bytes));
      const pageFeatures = (json as { features?: unknown[] })?.features;
      if (!Array.isArray(pageFeatures)) {
        throw new Error("not a GeoJSON FeatureCollection (is the URL a direct geojson endpoint?)");
      }
      features.push(...pageFeatures);
      const more = (json as { properties?: { exceededTransferLimit?: boolean } })
        ?.properties?.exceededTransferLimit === true;
      if (!more || pageFeatures.length === 0) break;
    }
    const bytes = new TextEncoder().encode(JSON.stringify({ type: "FeatureCollection", features }));
    await installPopulation(bytes, app.popUrl);
  } catch (e) {
    app.status = `Population layer import failed: ${(e as Error).message}`;
    console.warn("population import failed:", e);
  }
}

export function clearPopulation(): void {
  app.popLayer = null;
  app.popLayerBytes = null;
  app.status = "Population layer cleared — coverage analysis is off until one is imported.";
}

async function installPopulation(bytes: Uint8Array, label: string): Promise<void> {
  // The layer lives in the analysis capability (the monolith's
  // parse-only op is retired — U4): registering it returns the
  // import handle, and the same registration serves coverage.
  const handle = await ensureAnalysisLayer(bytes);
  if (!handle) {
    app.status = "Population import needs the analysis capability — the assembly is unavailable.";
    return;
  }
  app.popLayer = {
    points: handle.points,
    pop_field: handle.pop_field,
    total_pop: handle.total_pop,
    label,
  };
  app.popLayerBytes = bytes;
  app.status =
    `Population layer loaded (${label}) — ${handle.points} points, field "${handle.pop_field}", ` +
    `${Math.round(handle.total_pop).toLocaleString()} people. Coverage analysis is on.`;
}

async function installStreets(geojson: unknown, label: string): Promise<void> {
  try {
    // The cap-streets capability owns the routing graph; the bytes
    // cross the seam once and register under their content hash.
    const bytes = new TextEncoder().encode(JSON.stringify(geojson));
    const graph = await ensureStreets(bytes);
    if (!graph) {
      app.status = "Street import needs the streets capability — the assembly is unavailable.";
      return;
    }
    const streets: Streets = {
      streetsHash: graph.streetsHash,
      geojson: geojson as { type: "FeatureCollection"; features: unknown[] },
      segmentCount: graph.segments,
    };
    setStreets(streets);
    app.status = `Street network loaded (${label}) — ${graph.segments} segments; detour clicks snap to streets and stops.`;
  } catch (e) {
    // cap-streets validates the GeoJSON: no lines, malformed JSON —
    // surface the module's message.
    app.status = `Street import failed: ${(e as Error).message}`;
    console.warn("street import failed:", e);
  }
}

/// The draft as cap-divert reads it.
function draftDoc(d: DiversionDraft): DraftDoc {
  return {
    route_id: d.routeId,
    pattern: d.patternId || undefined,
    first_kept_stop: d.firstKept || undefined,
    last_kept_stop: d.lastKept || undefined,
    served_stops: d.served,
    slices: d.slices.map((s) => s.points.map(([lat, lon]) => ({ lat, lon }))),
    added_mins: Number(d.addedMins),
    start_date: d.start,
    end_date: d.end,
  };
}

let draftSeq = 0;
/** What the draft amounts to — cap-divert's view (the default pattern,
 * each stop's role, the skips, the flattened trace, the problems) and
 * the alignment ghost the map draws. The latest call wins. */
export async function refreshDiversionView(): Promise<void> {
  const draft = app.diversionDraft;
  const seq = ++draftSeq;
  const bytes = feedBytes();
  if (!draft || !bytes) {
    app.diversionView = null;
    app.routeContext = null;
    return;
  }
  const doc = draftDoc(draft);
  try {
    const over = await divertOverFeed(bytes);
    if (seq !== draftSeq) return;
    if (!over) {
      app.status = "The diversion editor needs the divert and network capabilities — the assembly is unavailable.";
      return;
    }
    const view = over.divert.draft(over.networkHash, doc);
    app.diversionView = view;
    app.routeContext = lineFeature(view.alignment, { route: draft.routeId });
  } catch (e) {
    error(e);
  }
}

export async function applyDiversion(): Promise<void> {
  const draft: DiversionDraft | null = app.diversionDraft;
  const bytes = feedBytes();
  if (!draft || !bytes) return;
  const over = await divertOverFeed(bytes);
  if (!over) {
    app.status = "Applying a diversion needs the divert capability — it did not assemble.";
    return;
  }
  const recordId = crypto.randomUUID ? crypto.randomUUID() : `edit-${Date.now()}`;
  let next: EditRecord[];
  try {
    next = over.divert.propose(over.networkHash, draftDoc(draft), getEditDocument(), recordId, Date.now());
  } catch (e) {
    app.status = `Diversion not applied: ${(e as Error).message}.`;
    return;
  }
  adoptEdits(next);
  // The validation is the day views' load_edits (reported, never
  // rejected) — awaited here so the status speaks the record's own
  // report, not the previous document's.
  const reports = (await ensureEditReports(bytes, getEditDocument())) ?? [];
  const report = reports.find((r) => r.id === recordId);
  app.status = !report || !report.valid
    ? `Diversion saved but invalid: ${report?.error ?? "unknown error"}.`
    : report.affected_trips === 0
      ? "Diversion saved, but no trips match its anchor stops — check the route and anchors."
      : `Diversion applied: ${report.affected_trips} trips updated. Boards, blocks and plans now use it.`;
  cancelDiversion();
}

/** Retire (the sync tombstone: the record stays so replicas see the
 * deletion) or restore one record — cap-divert's envelope rules. */
export async function setDiversionRetired(id: string, retired: boolean): Promise<void> {
  try {
    const module = await divertModule();
    if (!module) {
      app.status = "Retiring a diversion needs the divert capability — it did not assemble.";
      return;
    }
    const divert = divertCap(module);
    const records = getEditDocument();
    adoptEdits(retired ? divert.retire(records, id, Date.now()) : divert.restore(records, id, Date.now()));
  } catch (e) {
    error(e);
  }
}

// ---------------------------------------------------------------
// sync: push the local document, adopt the merged response
// ---------------------------------------------------------------

/** Sync with the server's edit document: POST our document, get back
 * the LWW-merged result (drafts submitted, tombstones propagated),
 * adopt it wholesale. One round trip is the whole protocol — the
 * server merges with its copy in the same artifact we run locally.
 * `quiet` (bootstrap) leaves app.status untouched. */
export async function syncEdits({ quiet = false }: { quiet?: boolean } = {}): Promise<void> {
  if (!app.feedLoaded && !quiet) return;
  try {
    const res = await fetch("/edits", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(getEditDocument()),
    });
    if (res.status === 404) {
      if (!quiet) app.status = "Sync unavailable — this server has no /edits endpoint.";
      return;
    }
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const merged = await readEdits(await res.json());
    // Adopt: wholesale replace, revalidate through wasm, persist.
    await adoptEdits(merged);
    if (!quiet) {
      const inForce = app.diversionRows.filter((r) => r.in_force).length;
      app.status = `Synced with server — ${merged.length} edit record(s), ${inForce} in force.`;
    }
  } catch (e) {
    if (!quiet) app.status = `Sync failed: ${e.message}`;
    console.warn("edit sync failed:", e);
  }
}
