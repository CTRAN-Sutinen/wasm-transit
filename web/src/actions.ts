import { app, getEditDocument, setEditDocument, setStreets, type DiversionDraft } from "./state.svelte";
import { get as idbGet, set as idbSet, del as idbDel } from "idb-keyval";
import type { EditRecord } from "./types/EditRecord";
import type { Itinerary } from "./types/Itinerary";
import type { LineGeometry } from "./types/LineGeometry";
import { buildStreetNetwork } from "./streets";

/// One drawn plan feature (transit leg or walk segment). MapView
/// consumes these; the geometry is GeoJSON.
export type PlanFeature = {
  type: "Feature";
  geometry: LineGeometry | { type: "LineString"; coordinates: number[][] };
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
  if (!app.wasm) throw new Error("wasm not loaded");
  app.gtfs = app.wasm.GtfsHandle.load(bytes); // all parsing in wasm
  app.selectedStop = null;
  app.selectedTripId = null;
  app.tripFeature = null;
  app.plan = null;
  app.planBaseline = null;
  app.planNotice = null;
  // The edit document may reference this feed (or an older one) —
  // re-push so wasm revalidates every record against the new artifact
  // and derived views re-derive.
  pushEdits();
  // Diagnostics: proves which wasm queries actually answer in-browser.
  const stops = app.gtfs.stops();
  const blocks = app.gtfs.blocks();
  const features = app.gtfs.stops_geojson().features.length;
  console.log("gtfs loaded:", { stops: stops.length, blocks: blocks.length, geoFeatures: features });
  await cacheFeed(label, bytes.slice());
  app.status =
    `Loaded ${label} — stops: ${stops.length}, blocks: ${blocks.length}, map features: ${features}.` +
    " Everything below is running client-side.";
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
  if (!app.wasm) return;
  app.rtOverview = app.wasm.decode_rt(bytes);
  // Only a combined feed carries vehicles; a pure TripUpdates feed must
  // not touch the vehicle dots (they belong to the VP feed / last good
  // positions).
  if (app.rtOverview.vehicle_positions?.length) {
    app.vehicles = app.rtOverview.vehicle_positions;
  }
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
    if (!app.wasm) return;
    const rt = app.wasm.decode_rt(await fetchBytes(proxied(app.vpUrl)));
    app.vehicles = rt.vehicle_positions ?? [];
    app.status = `Vehicle positions loaded: ${app.vehicles.length} vehicles on the map.`;
  } catch (e) { error(e); } // keep the last good positions on the map
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

export function showTrip(tripId: string): void {
  app.selectedTripId = tripId;
  // The date scopes operator edits: diverted trips draw patched shapes.
  const feature = app.gtfs?.trip_path_geojson(tripId, serviceDate());
  // No geometry -> clear the path, but never wipe other state.
  app.tripFeature = feature ?? null;
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

function fmtClock(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

export function planTrip(fromStopId: string, toStopId: string, departTime: string): void {
  if (!app.gtfs) return;
  if (!fromStopId || !toStopId) {
    app.planNotice = "Pick a from and a to stop first.";
    return;
  }
  try {
    const depart = departTime
      ? hhmmssToSecs(departTime)
      : secondsSinceMidnight();
    const anyActive = getEditDocument().some((e) => e.status !== "retired");
    if (anyActive) {
      // With edits in force, plan both worlds and show the delta.
      const cmp = app.gtfs.plan_trip_compare(fromStopId, toStopId, serviceDate(), depart, 6);
      app.plan = cmp.active ?? null;
      app.planBaseline = cmp.baseline ?? null;
      if (!cmp.active && cmp.baseline) {
        app.planNotice =
          `No trip found while the diversion is active — ` +
          `without it you would arrive ${fmtClock(cmp.baseline.arrival_secs)}.`;
      } else {
        app.planNotice = cmp.active ? null : "No trip found within 6 legs — try a different time or closer stops.";
      }
    } else {
      app.plan = app.gtfs.plan_trip(fromStopId, toStopId, serviceDate(), depart, 6);
      app.planBaseline = null;
      app.planNotice = app.plan
        ? null
        : "No trip found within 6 legs — try a different time or closer stops.";
    }
    app.itineraryFeatures = buildItineraryFeatures(app.plan);
  } catch (e) {
    app.planNotice = `Planning failed: ${e.message}`;
    error(e);
  }
}

function buildItineraryFeatures(plan: Itinerary | null): PlanFeature[] {
  if (!plan || !app.gtfs) return [];
  const features: PlanFeature[] = [];
  for (const leg of plan.legs) {
    if (leg.type === "transit") {
      // The ridden segment only — the full route would be confusing.
      const path = app.gtfs.trip_segment_geojson(
        leg.trip_id, leg.board_stop_id, leg.alight_stop_id, serviceDate());
      if (path) {
        features.push({ type: "Feature", geometry: path.geometry, properties: { kind: "transit" } });
      }
    } else {
      const from = app.gtfs.stop_point(leg.from_stop_id);
      const to = app.gtfs.stop_point(leg.to_stop_id);
      if (from && to) {
        features.push({
          type: "Feature",
          geometry: {
            type: "LineString",
            coordinates: [from.geometry.coordinates, to.geometry.coordinates],
          },
          properties: { kind: "walk" },
        });
      }
    }
  }
  return features;
}

// ---------------------------------------------------------------
// operator edits (the sync document)
//
// One mutation path: mutateEdits(fn) is the ONLY code that changes
// the document or wasm's view of it. It (1) clones the current
// records, (2) lets fn transform the copy, (3) adopts it wholesale,
// (4) pushes it to wasm (which validates each record and drops its
// derived caches), (5) bumps feedEpoch so derived views re-query, and
// (6) schedules one debounced persist. Call sites cannot forget a
// step because there are no steps.
// ---------------------------------------------------------------

function cloneEdits(edits: EditRecord[]): EditRecord[] {
  return edits.map((r) => ({
    ...r,
    diversion: {
      ...r.diversion,
      skipped_stops: [...(r.diversion.skipped_stops ?? [])],
      detour: r.diversion.detour ? r.diversion.detour.map((p) => ({ ...p })) : null,
    },
  }));
}

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

/** Push the current document to wasm: validation reports come back,
 * caches drop, and derived views re-query via feedEpoch. Also called
 * after a feed load — every record is revalidated against the new
 * artifact. No-op when no feed is loaded (records wait in the doc). */
export function pushEdits(): void {
  if (!app.gtfs) return;
  try {
    app.editReports = app.gtfs.set_diversions(getEditDocument());
  } catch (e) {
    error(e);
    return;
  }
  app.feedEpoch++;
}

export function mutateEdits(fn: (edits: EditRecord[]) => void): void {
  const next = cloneEdits(getEditDocument());
  fn(next);
  setEditDocument(next);
  pushEdits();
  schedulePersist();
}

export function startDiversion(): void {
  const today = serviceDate();
  app.diversionDraft = {
    routeId: "",
    directionId: "", // "" = any, "0"/"1" filter
    firstKept: "",
    lastKept: "",
    skipped: new Set(),
    addedMins: 5,
    start: today,
    end: today,
    detour: [], // [[lat, lon], ...] as drawn on the map
  };
  app.pickingAnchor = "first";
}

export function cancelDiversion(): void {
  app.diversionDraft = null;
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

export function toggleSkipped(stopId: string): void {
  const skipped = app.diversionDraft?.skipped;
  if (!skipped) return;
  if (skipped.has(stopId)) skipped.delete(stopId);
  else skipped.add(stopId);
}

export function addDetourPoint(lat: number, lon: number): void {
  app.diversionDraft?.detour.push([lat, lon]);
}

export function clearDetour(): void {
  if (app.diversionDraft) app.diversionDraft.detour = [];
}

export function toggleDrawing(): void {
  app.drawingDetour = !app.drawingDetour;
}

/// Import a street network (GeoJSON: LineString/MultiLineString
/// features, e.g. an OSM extract). Detour clicks snap to it. Not
/// persisted — reference data, re-imported per session.
export async function importStreetsFile(file: File): Promise<void> {
  try {
    const text = await file.text();
    const streets = buildStreetNetwork(JSON.parse(text));
    if (!streets) {
      app.status = "No line features in that file — expected GeoJSON with LineString/MultiLineString geometries.";
      return;
    }
    setStreets(streets);
    app.status = `Street network loaded — ${streets.network.segmentCount} segments; detour clicks snap to streets and stops.`;
  } catch (e) {
    app.status = `Street import failed: ${(e as Error).message}`;
    console.warn("street import failed:", e);
  }
}

export function applyDiversion(): void {
  const draft: DiversionDraft | null = app.diversionDraft;
  if (!draft) return;
  if (!draft.routeId || !draft.firstKept || !draft.lastKept) {
    app.status = "Diversion needs a route and both anchor stops.";
    return;
  }
  const record: EditRecord = {
    id: crypto.randomUUID ? crypto.randomUUID() : `edit-${Date.now()}`,
    diversion: {
      id: `div-${draft.routeId}-${Date.now()}`, // domain id; sync id is above
      route_id: draft.routeId,
      direction_id: draft.directionId === "" ? null : Number(draft.directionId),
      first_kept_stop: draft.firstKept,
      last_kept_stop: draft.lastKept,
      skipped_stops: [...draft.skipped],
      detour: draft.detour.length >= 2 ? draft.detour.map(([lat, lon]) => ({ lat, lon })) : null,
      added_secs: Math.round(Number(draft.addedMins) * 60),
      start_date: draft.start,
      end_date: draft.end,
    },
    status: "draft",
    rev: 1,
    updated_at: Date.now(),
  };
  mutateEdits((edits) => {
    const i = edits.findIndex((e) => e.id === record.id);
    if (i >= 0) edits[i] = record;
    else edits.push(record);
  });
  const report = app.editReports.find((r) => r.id === record.id);
  app.status = !report || !report.valid
    ? `Diversion saved but invalid: ${report?.error ?? "unknown error"}.`
    : report.affected_trips === 0
      ? "Diversion saved, but no trips match its anchor stops — check the route and anchors."
      : `Diversion applied: ${report.affected_trips} trips updated. Boards, blocks and plans now use it.`;
  cancelDiversion();
}

/** In force <-> retired. Retiring is the sync tombstone: the record
 * stays in the document (a future server needs to see the deletion);
 * the UI greys it out and un-retiring is always possible. */
export function toggleDiversion(id: string): void {
  mutateEdits((edits) => {
    const edit = edits.find((e) => e.id === id);
    if (!edit) return;
    edit.status = edit.status === "retired" ? "draft" : "retired";
    edit.rev += 1;
    edit.updated_at = Date.now();
  });
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
  if (!app.gtfs && !quiet) return;
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
    const merged = await res.json();
    // Adopt: wholesale replace, revalidate through wasm, persist.
    setEditDocument(merged);
    pushEdits();
    schedulePersist();
    if (!quiet) {
      const inForce = merged.filter((r: EditRecord) => r.status !== "retired").length;
      app.status = `Synced with server — ${merged.length} edit record(s), ${inForce} in force.`;
    }
  } catch (e) {
    if (!quiet) app.status = `Sync failed: ${e.message}`;
    console.warn("edit sync failed:", e);
  }
}
