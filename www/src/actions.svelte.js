import { app } from "./state.svelte.js";
import { set as idbSet, del as idbDel } from "idb-keyval";

// ---------------------------------------------------------------
// persistence: feed bytes in IndexedDB, settings in localStorage
// ---------------------------------------------------------------

const FEED_KEY = "transit:gtfs-feed";
const SETTINGS_KEY = "transit:settings";

export function saveSettings() {
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

export async function clearCachedFeed() {
  await idbDel(FEED_KEY);
  app.cachedFeed = null;
  app.status = "Cached feed cleared.";
}

/** Cache the raw zip so the next page load can restore instantly.
 * (We cache bytes and re-parse on restore — parsing is cheap, and the
 * wasm-side parsed structures can't be serialized.) The write is
 * awaited by loadGtfsBytes so a following reload cannot race it. */
async function cacheFeed(label, bytes) {
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
let nowOverride = null;
export function setNowOverride(value) {
  nowOverride = value; // { date: "YYYY-MM-DD", secs: number } | null
}

export function serviceDate() {
  if (nowOverride) return nowOverride.date;
  const now = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;
}

export function secondsSinceMidnight() {
  if (nowOverride) return nowOverride.secs;
  const now = new Date();
  return now.getHours() * 3600 + now.getMinutes() * 60 + now.getSeconds();
}

// ---------------------------------------------------------------
// fetching (through the transit-server proxy for remote URLs)
// ---------------------------------------------------------------

async function fetchBytes(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText} for ${url}`);
  return new Uint8Array(await res.arrayBuffer());
}

function proxied(url) {
  return `/rt?url=${encodeURIComponent(url)}`;
}

function error(e) {
  app.status = `Error: ${e?.message ?? e}`;
  console.error(e);
}

// ---------------------------------------------------------------
// loaders — each writes only the state it owns
// ---------------------------------------------------------------

export async function loadGtfsBytes(bytes, label) {
  app.gtfs = app.wasm.GtfsHandle.load(bytes); // all parsing in wasm
  app.selectedStop = null;
  app.selectedTripId = null;
  app.tripFeature = null;
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

export async function loadGtfsFile(file) {
  app.status = `Parsing ${file.name}...`;
  try {
    loadGtfsBytes(new Uint8Array(await file.arrayBuffer()), file.name);
  } catch (e) { error(e); }
}

export async function loadGtfsFromUrl() {
  if (!app.gtfsUrl) { app.status = "Enter a GTFS zip URL first."; return; }
  try {
    app.status = `Downloading ${app.gtfsUrl}...`;
    loadGtfsBytes(await fetchBytes(proxied(app.gtfsUrl)), app.gtfsUrl);
  } catch (e) { error(e); }
}

export async function loadRtBytes(bytes) {
  app.rtOverview = app.wasm.decode_rt(bytes);
  // Only a combined feed carries vehicles; a pure TripUpdates feed must
  // not touch the vehicle dots (they belong to the VP feed / last good
  // positions).
  if (app.rtOverview.vehicle_positions?.length) {
    app.vehicles = app.rtOverview.vehicle_positions;
  }
}

export async function loadRtFromUrl() {
  if (!app.rtUrl) { app.status = "Enter a GTFS-rt URL first."; return; }
  try {
    await loadRtBytes(await fetchBytes(proxied(app.rtUrl)));
  } catch (e) { error(e); }
}

export async function loadVpFromUrl() {
  if (!app.vpUrl) { app.status = "Enter a vehicle positions URL first."; return; }
  try {
    const rt = app.wasm.decode_rt(await fetchBytes(proxied(app.vpUrl)));
    app.vehicles = rt.vehicle_positions ?? [];
    app.status = `Vehicle positions loaded: ${app.vehicles.length} vehicles on the map.`;
  } catch (e) { error(e); } // keep the last good positions on the map
}

export async function dropFile(file) {
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

export function selectStop(stopId) {
  app.selectedStop = stopId;
  app.clock = Date.now(); // re-derive the departure board
}

export function showTrip(tripId) {
  app.selectedTripId = tripId;
  const feature = app.gtfs?.trip_path_geojson(tripId);
  // No geometry -> clear the path, but never wipe other state.
  app.tripFeature = feature ?? null;
}

export function refreshNow() {
  app.clock = Date.now();
}

// ---------------------------------------------------------------
// trip planning (RAPTOR, in wasm)
// ---------------------------------------------------------------

function hhmmssToSecs(hhmm) {
  const [h = 0, m = 0, s = 0] = (hhmm ?? "").split(":").map(Number);
  return h * 3600 + m * 60 + s;
}

export function planTrip(fromStopId, toStopId, departTime) {
  if (!app.gtfs) return;
  if (!fromStopId || !toStopId) {
    app.planNotice = "Pick a from and a to stop first.";
    return;
  }
  try {
    const depart = departTime
      ? hhmmssToSecs(departTime)
      : secondsSinceMidnight();
    const plan = app.gtfs.plan_trip(fromStopId, toStopId, serviceDate(), depart, 6);
    app.plan = plan;
    app.planNotice = plan
      ? null
      : "No trip found within 6 legs — try a different time or closer stops.";
    app.itineraryFeatures = buildItineraryFeatures(plan);
  } catch (e) {
    app.planNotice = `Planning failed: ${e.message}`;
    error(e);
  }
}

function buildItineraryFeatures(plan) {
  if (!plan) return [];
  const features = [];
  for (const leg of plan.legs) {
    if (leg.type === "transit") {
      // The ridden segment only — the full route would be confusing.
      const path = app.gtfs.trip_segment_geojson(leg.trip_id, leg.board_stop_id, leg.alight_stop_id);
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
