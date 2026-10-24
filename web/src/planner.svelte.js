/// Planner-persona actions: authoring an edited network document
/// (transit-network `Network`) in the browser, against the same wasm
/// artifact the server runs. The document is JS-owned state
/// (state.svelte.js `networkDoc`, plain data, wholesale replacement —
/// the same discipline as the edit document); these functions are
/// the only writers:
///
///   source → deriveFromFeed (GTFS is the bootstrap), loadNetworkText/
///            loadNetworkFile (round-tripping documents)
///   check  → validate (issues from the artifact, never throws)
///   edit   → one mutation path: mutateNetwork clones, edits,
///            replaces, revalidates — every entity CRUD below it
///   apply  → materializeNetwork (registers a derived feed; the
///            Rider/Dispatcher views then run on the authored network)
///
/// The planner works off an existing network: there is no blank
/// canvas. GTFS-the-format never appears here — inference and
/// materialization live in the artifact (transit-ops), this file only
/// moves documents across the seam.

import { app, getNetworkDoc, setNetworkDoc } from "./state.svelte.js";
import { pushEdits } from "./actions.svelte.js";

// ---------------------------------------------------------------
// time helpers — service-day times, hours may pass 24
// ---------------------------------------------------------------

export function fmtHHMMSS(secs) {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  return [h, m, s].map((n) => String(n).padStart(2, "0")).join(":");
}

export function parseHHMMSS(text) {
  const parts = (text ?? "").trim().split(":").map(Number);
  if (parts.length < 2 || parts.length > 3 || parts.some((n) => Number.isNaN(n))) return null;
  const [h, m, s = 0] = parts;
  if (m < 0 || m > 59 || s < 0 || s > 59 || h < 0 || h > 47) return null;
  return h * 3600 + m * 60 + s;
}

// ---------------------------------------------------------------
// source / validate / apply
// ---------------------------------------------------------------

/** Re-run `network_validate` on the current document. The op always
 * returns the whole issue list (snake_case variant + fields) — it
 * never throws, and the UI never blocks on it. */
export function validateNetwork() {
  const doc = getNetworkDoc();
  if (!doc || !app.wasm) {
    app.networkIssues = [];
    return;
  }
  app.networkIssues = app.wasm.network_validate(doc);
}

// Validation is whole-document work in the artifact; on a real
// agency feed that is real work per edit. Debounce it: the chip
// catches up a beat after the last change instead of on every one.
let validateTimer = null;
function validateNetworkSoon() {
  clearTimeout(validateTimer);
  validateTimer = setTimeout(validateNetwork, 250);
}

function setDoc(next, label, { validateNow = false } = {}) {
  setNetworkDoc(next);
  app.networkLabel = label;
  if (validateNow) {
    clearTimeout(validateTimer);
    validateNetwork();
  } else {
    validateNetworkSoon();
  }
}

/** The planner's bootstrap: infer a network document from the feed
 * the app already has loaded (Rider/Dispatcher views load feeds). */
export async function deriveFromFeed() {
  if (!app.gtfs) {
    app.status = "Load a GTFS feed first (Rider or Dispatcher view).";
    return;
  }
  // The artifact runs on this thread: yield a frame so the status
  // paints before the (potentially seconds-long) inference + JSON
  // transfer on a real feed. A worker host is the eventual fix for
  // the freeze itself.
  app.status = "Deriving a network from the loaded feed…";
  await new Promise((r) => setTimeout(r, 30));
  try {
    const net = app.wasm.network_import(app.gtfs.feedId());
    setDoc(net, "derived from loaded feed", { validateNow: true });
    app.status = `Derived a network from the loaded feed (${Object.keys(net.patterns).length} patterns, ${Object.keys(net.services).length} services) — edit it, then load it into the app.`;
  } catch (e) {
    app.status = `Error: ${e?.message ?? e}`;
  }
}

export function loadNetworkText(text, label) {
  let doc;
  try {
    doc = JSON.parse(text);
  } catch (e) {
    app.status = `Not valid JSON: ${e.message}`;
    return false;
  }
  setDoc(doc, label, { validateNow: true });
  const n = app.networkIssues.length;
  app.status =
    n === 0
      ? `Network "${doc.name ?? label}" parsed and valid — load it into the app to use it.`
      : `Network "${doc.name ?? label}" parsed with ${n} issue(s).`;
  return true;
}

export async function loadNetworkFile(file) {
  loadNetworkText(await file.text(), file.name);
}

export function exportNetworkText() {
  const doc = getNetworkDoc();
  return doc ? JSON.stringify(doc, null, 2) : "";
}

export function downloadNetwork() {
  const doc = getNetworkDoc();
  if (!doc) return;
  const name = (doc.name ?? "network").replace(/[^\w.-]+/g, "-").toLowerCase();
  const blob = new Blob([JSON.stringify(doc, null, 2)], { type: "application/json" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `${name}.network.json`;
  a.click();
  URL.revokeObjectURL(a.href);
}

/** Materialize the document into the app's active feed (same registry
 * a GTFS zip lands in). Mirrors loadGtfsBytes's side effects minus
 * caching: the document, not a zip, is the source of truth. Invalid
 * documents are refused — the artifact's own gate, not a JS guess. */
export function materializeNetwork() {
  const doc = getNetworkDoc();
  if (!doc) {
    app.status = "No network document loaded.";
    return;
  }
  validateNetwork(); // flush any debounced check first — this is the gate
  if (app.networkIssues.length > 0) {
    app.status = `Cannot load: ${app.networkIssues.length} validation issue(s) to fix first.`;
    return;
  }
  try {
    app.gtfs = app.wasm.GtfsHandle.load_network(doc);
    app.selectedStop = null;
    app.selectedTripId = null;
    app.tripFeature = null;
    app.plan = null;
    app.planBaseline = null;
    app.planNotice = null;
    pushEdits();
    app.status = `Materialized "${doc.name}" — the Rider and Dispatcher views now run on the authored network.`;
  } catch (e) {
    app.status = `Error: ${e?.message ?? e}`;
  }
}

// ---------------------------------------------------------------
// editing — one mutation path: clone → edit → replace → revalidate
// ---------------------------------------------------------------

export function mutateNetwork(fn) {
  const doc = structuredClone(getNetworkDoc());
  if (!doc) return;
  fn(doc);
  setDoc(doc, app.networkLabel);
}

function nextId(map, prefix) {
  if (!map) return `${prefix}1`;
  let n = 1;
  while (Object.prototype.hasOwnProperty.call(map, `${prefix}${n}`)) n++;
  return `${prefix}${n}`;
}

// --- stops ------------------------------------------------------

export function addStop() {
  const doc = getNetworkDoc();
  const i = Object.keys(doc?.stops ?? {}).length + 1;
  const id = nextId(doc?.stops, "s");
  mutateNetwork((d) => {
    d.stops[id] = { name: `Stop ${i}`, lat: 45.52, lon: -122.68 };
  });
}

export function updateStop(id, { name, lat, lon }) {
  mutateNetwork((d) => {
    const s = d.stops[id];
    if (!s) return;
    if (name !== undefined) s.name = name;
    if (lat !== undefined && !Number.isNaN(lat)) s.lat = lat;
    if (lon !== undefined && !Number.isNaN(lon)) s.lon = lon;
  });
}

export function removeStop(id) {
  mutateNetwork((d) => {
    delete d.stops[id];
  });
}

// --- routes -----------------------------------------------------

const MODES = ["bus", "tram", "subway", "rail", "ferry", "cable_car", "gondola", "funicular", "trolleybus", "monorail", "other"];

export function modes() {
  return MODES;
}

export function addRoute() {
  const doc = getNetworkDoc();
  const i = Object.keys(doc?.routes ?? {}).length + 1;
  const id = nextId(doc?.routes, "r");
  mutateNetwork((d) => {
    d.routes[id] = { name: `Route ${i}`, mode: "bus" };
  });
}

export function updateRoute(id, { name, mode }) {
  mutateNetwork((d) => {
    const r = d.routes[id];
    if (!r) return;
    if (name !== undefined) r.name = name;
    if (mode !== undefined) r.mode = mode;
  });
}

export function removeRoute(id) {
  mutateNetwork((d) => {
    delete d.routes[id];
  });
}

// --- patterns ---------------------------------------------------

export function addPattern() {
  const doc = getNetworkDoc();
  if (!doc) return;
  const routeId = Object.keys(doc.routes ?? {}).sort()[0];
  if (!routeId) {
    app.status = "Add a route first.";
    return;
  }
  const stopIds = Object.keys(doc.stops ?? {}).sort();
  if (stopIds.length < 1) {
    app.status = "Add stops first.";
    return;
  }
  const id = nextId(doc.patterns, "pat");
  mutateNetwork((d) => {
    d.patterns[id] = {
      route: routeId,
      name: `Pattern ${Object.keys(d.patterns).length + 1}`,
      stops: [
        { stop: stopIds[0], timepoint: true, distance_m: 0 },
        { stop: stopIds[1 % stopIds.length], timepoint: true, distance_m: 500 },
      ],
      timing: [{ start_secs: 0, end_secs: 30 * 3600, at_timepoints: [0, 600] }],
    };
  });
}

export function updatePattern(id, { name, route }) {
  mutateNetwork((d) => {
    const p = d.patterns[id];
    if (!p) return;
    if (name !== undefined) p.name = name;
    if (route !== undefined) p.route = route;
  });
}

export function removePattern(id) {
  mutateNetwork((d) => {
    delete d.patterns[id];
  });
}

/** Insert a stop visit before the pattern's last stop (endpoints stay
 * endpoints; non-timepoint, so timing vectors are untouched). */
export function addPatternStop(patternId, stopId) {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p || !stopId || p.stops.length < 2) return;
    const last = p.stops[p.stops.length - 1];
    const before = p.stops[p.stops.length - 2];
    const distance = (before.distance_m + last.distance_m) / 2;
    p.stops.splice(p.stops.length - 1, 0, { stop: stopId, timepoint: false, distance_m: distance });
  });
}

/** Middle stops only — the first and last are timepoints by model
 * invariant and the UI keeps them fixed. */
export function removePatternStop(patternId, index) {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p) return;
    if (index <= 0 || index >= p.stops.length - 1) return;
    const wasTimepoint = p.stops[index].timepoint;
    const before = p.stops.slice(0, index).filter((s) => s.timepoint).length;
    p.stops.splice(index, 1);
    if (wasTimepoint) {
      for (const prof of p.timing) prof.at_timepoints.splice(before, 1);
    }
  });
}

export function movePatternStop(patternId, index, dir) {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p) return;
    const j = index + dir;
    if (index <= 0 || j <= 0 || index >= p.stops.length - 1 || j >= p.stops.length - 1) return;
    const [row] = p.stops.splice(index, 1);
    p.stops.splice(j, 0, row);
  });
}

export function setPatternStop(patternId, index, { stop, distance_m }) {
  mutateNetwork((d) => {
    const row = d.patterns[patternId]?.stops[index];
    if (!row) return;
    if (stop !== undefined) row.stop = stop;
    if (distance_m !== undefined && !Number.isNaN(distance_m)) row.distance_m = distance_m;
  });
}

/** Toggle a middle stop's timepoint flag, keeping every timing
 * profile's vector aligned (insert a midpoint / drop the entry). */
export function togglePatternTimepoint(patternId, index) {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p) return;
    if (index <= 0 || index >= p.stops.length - 1) return; // endpoints stay
    const row = p.stops[index];
    const pos = p.stops.slice(0, index).filter((s) => s.timepoint).length;
    const turningOff = row.timepoint;
    row.timepoint = !row.timepoint;
    for (const prof of p.timing) {
      if (turningOff) {
        prof.at_timepoints.splice(pos, 1);
      } else {
        const prev = pos > 0 ? prof.at_timepoints[pos - 1] : 0;
        const next = pos < prof.at_timepoints.length ? prof.at_timepoints[pos] : prev;
        prof.at_timepoints.splice(pos, 0, Math.round((prev + next) / 2));
      }
    }
  });
}

// --- timing profiles --------------------------------------------

export function addTimingProfile(patternId) {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p) return;
    const last = p.timing[p.timing.length - 1];
    const start = last ? last.end_secs : 0;
    p.timing.push({
      start_secs: start,
      end_secs: start + 6 * 3600,
      at_timepoints: last ? [...last.at_timepoints] : [0],
    });
  });
}

export function removeTimingProfile(patternId, index) {
  mutateNetwork((d) => {
    const p = d.patterns[patternId];
    if (!p || p.timing.length <= 1) return;
    p.timing.splice(index, 1);
  });
}

export function setTimingProfileField(patternId, index, { start_secs, end_secs }) {
  mutateNetwork((d) => {
    const prof = d.patterns[patternId]?.timing[index];
    if (!prof) return;
    if (start_secs !== undefined && start_secs !== null) prof.start_secs = start_secs;
    if (end_secs !== undefined && end_secs !== null) prof.end_secs = end_secs;
  });
}

export function setTimepointTime(patternId, profileIndex, timepointIndex, secs) {
  mutateNetwork((d) => {
    const prof = d.patterns[patternId]?.timing[profileIndex];
    if (!prof || timepointIndex >= prof.at_timepoints.length || secs === null) return;
    prof.at_timepoints[timepointIndex] = secs;
  });
}

// --- calendars --------------------------------------------------

const DAYS = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

export function dayNames() {
  return DAYS;
}

export function addCalendar() {
  const doc = getNetworkDoc();
  const id = nextId(doc?.calendars, "cal");
  mutateNetwork((d) => {
    d.calendars[id] = {
      days: { mon: true, tue: true, wed: true, thu: true, fri: true, sat: false, sun: false },
      ranges: [{ start: "2026-01-01", end: "2026-12-31" }],
      exceptions: [],
    };
  });
}

export function toggleCalendarDay(id, day) {
  mutateNetwork((d) => {
    const c = d.calendars[id];
    if (c) c.days[day] = !c.days[day];
  });
}

export function setCalendarRange(id, index, { start, end }) {
  mutateNetwork((d) => {
    const r = d.calendars[id]?.ranges[index];
    if (!r) return;
    if (start) r.start = start;
    if (end) r.end = end;
  });
}

export function addCalendarRange(id) {
  mutateNetwork((d) => {
    const c = d.calendars[id];
    if (!c) return;
    const last = c.ranges[c.ranges.length - 1];
    if (last) {
      c.ranges.push({ start: last.end, end: last.end });
    } else {
      c.ranges.push({ start: "2026-01-01", end: "2026-12-31" });
    }
  });
}

export function removeCalendarRange(id, index) {
  mutateNetwork((d) => {
    d.calendars[id]?.ranges.splice(index, 1);
  });
}

export function addCalendarException(id, date, state) {
  if (!date) return;
  mutateNetwork((d) => {
    const c = d.calendars[id];
    if (!c) return;
    const existing = c.exceptions.find((e) => e.date === date);
    if (existing) existing.state = state;
    else c.exceptions.push({ date, state });
    c.exceptions.sort((a, b) => (a.date < b.date ? -1 : 1));
  });
}

export function removeCalendarException(id, date) {
  mutateNetwork((d) => {
    const c = d.calendars[id];
    if (!c) return;
    c.exceptions = c.exceptions.filter((e) => e.date !== date);
  });
}

export function removeCalendar(id) {
  mutateNetwork((d) => {
    delete d.calendars[id];
  });
}

// --- services ---------------------------------------------------

export function addService() {
  const doc = getNetworkDoc();
  if (!doc) return;
  const patternId = Object.keys(doc.patterns ?? {}).sort()[0];
  const calendarId = Object.keys(doc.calendars ?? {}).sort()[0];
  if (!patternId || !calendarId) {
    app.status = "Add a pattern and a calendar first.";
    return;
  }
  const id = nextId(doc.services, "svc");
  mutateNetwork((d) => {
    d.services[id] = {
      pattern: patternId,
      calendar: calendarId,
      schedule: { kind: "exact_times", starts: [8 * 3600] },
    };
  });
}

export function setServiceRefs(id, { pattern, calendar }) {
  mutateNetwork((d) => {
    const s = d.services[id];
    if (!s) return;
    if (pattern) s.pattern = pattern;
    if (calendar) s.calendar = calendar;
  });
}

export function removeService(id) {
  mutateNetwork((d) => {
    delete d.services[id];
  });
}

/** Replace an exact-times service's trip starts (sorted, unique). */
export function setServiceStarts(serviceId, starts) {
  const clean = [...new Set(starts)].sort((a, b) => a - b);
  mutateNetwork((d) => {
    const svc = d.services[serviceId];
    if (svc) svc.schedule = { kind: "exact_times", starts: clean };
  });
}

export function addServiceStart(serviceId) {
  const doc = getNetworkDoc();
  const starts = doc?.services?.[serviceId]?.schedule?.starts;
  if (!starts) return;
  const last = starts.length ? starts[starts.length - 1] : 6 * 3600;
  setServiceStarts(serviceId, [...starts, Math.min(last + 600, 47 * 3600)]);
}

export function removeServiceStart(serviceId, index) {
  const doc = getNetworkDoc();
  const starts = doc?.services?.[serviceId]?.schedule?.starts;
  if (!starts || starts.length <= 1) return;
  setServiceStarts(serviceId, starts.filter((_, i) => i !== index));
}

/** Set one field of one headway band. */
export function setBandField(serviceId, index, field, value) {
  mutateNetwork((d) => {
    const svc = d.services[serviceId];
    if (svc?.schedule?.kind !== "headway_bands" || !svc.schedule.bands[index]) return;
    svc.schedule.bands[index][field] = value;
  });
}

export function addBand(serviceId) {
  mutateNetwork((d) => {
    const svc = d.services[serviceId];
    if (svc?.schedule?.kind !== "headway_bands") return;
    const bands = svc.schedule.bands;
    const last = bands[bands.length - 1];
    const start = last ? last.end_secs : 6 * 3600;
    bands.push({ start_secs: start, end_secs: start + 3 * 3600, headway_secs: 900 });
  });
}

export function removeBand(serviceId, index) {
  mutateNetwork((d) => {
    const svc = d.services[serviceId];
    if (svc?.schedule?.kind !== "headway_bands" || svc.schedule.bands.length <= 1) return;
    svc.schedule.bands.splice(index, 1);
  });
}

// --- blocks (the runcut) ----------------------------------------

export function addBlock() {
  const doc = getNetworkDoc();
  const id = nextId(doc?.blocks, "b");
  mutateNetwork((d) => {
    d.blocks[id] = { trips: [], layovers: [] };
  });
}

export function removeBlock(id) {
  mutateNetwork((d) => {
    delete d.blocks[id];
  });
}

export function addBlockTrip(blockId, serviceId, startSecs) {
  if (!serviceId || startSecs === null) {
    app.status = "Pick a service and a valid start time first.";
    return;
  }
  mutateNetwork((d) => {
    const b = d.blocks[blockId];
    if (!b) return;
    b.trips.push({ service: serviceId, start_secs: startSecs });
    b.trips.sort((a, c) => a.start_secs - c.start_secs || (a.service < c.service ? -1 : 1));
    // Layovers after the (possibly moved) last trip are meaningless.
    b.layovers = b.layovers.filter((l) => l.after_trip + 1 < b.trips.length);
  });
}

export function removeBlockTrip(blockId, index) {
  mutateNetwork((d) => {
    const b = d.blocks[blockId];
    if (!b) return;
    b.trips.splice(index, 1);
    b.layovers = b.layovers.filter((l) => l.after_trip + 1 < b.trips.length);
  });
}

export function setBlockLayover(blockId, afterTrip, secs) {
  mutateNetwork((d) => {
    const b = d.blocks[blockId];
    if (!b || afterTrip + 1 >= b.trips.length) return;
    const existing = b.layovers.find((l) => l.after_trip === afterTrip);
    if (secs === null) {
      b.layovers = b.layovers.filter((l) => l.after_trip !== afterTrip);
    } else if (existing) {
      existing.secs = secs;
    } else {
      b.layovers.push({ after_trip: afterTrip, secs, at: null });
    }
  });
}

// ---------------------------------------------------------------
// drill-down: select an entity, focus the map on it
// ---------------------------------------------------------------

/// Ordered [lon, lat] of a pattern's stops (skips stops without
/// coordinates).
function patternCoords(doc, patternId) {
  const p = doc?.patterns?.[patternId];
  if (!p) return [];
  const out = [];
  for (const s of p.stops) {
    const stop = doc.stops?.[s.stop];
    if (stop) out.push([stop.lon, stop.lat]);
  }
  return out;
}

function focusFor(doc, kind, id) {
  if (!doc) return null;
  const line = (coords) => (coords.length >= 2 ? [coords] : []);
  const point = (coords) => (coords.length ? [coords[coords.length - 1]] : []);
  if (kind === "pattern") {
    const coords = patternCoords(doc, id);
    return { lines: line(coords), points: coords };
  }
  if (kind === "service") {
    const coords = patternCoords(doc, doc.services?.[id]?.pattern);
    return { lines: line(coords), points: coords };
  }
  if (kind === "route") {
    const lines = [];
    const points = [];
    for (const [pid, p] of Object.entries(doc.patterns ?? {})) {
      if (p.route !== id) continue;
      const coords = patternCoords(doc, pid);
      if (coords.length >= 2) lines.push(coords);
      points.push(...coords);
    }
    return { lines, points };
  }
  if (kind === "stop") {
    const s = doc.stops?.[id];
    return s ? { lines: [], points: [[s.lon, s.lat]] } : null;
  }
  return null; // calendars and blocks have no geometry
}

/** Drill into one entity: opens its editor and frames it on the map. */
export function selectEntity(kind, id) {
  app.plannerSelection = id ? { kind, id } : null;
  app.plannerFocus = focusFor(getNetworkDoc(), kind, id);
}

/** Clear focus (persona switch away, or a removed entity). */
export function clearSelection() {
  app.plannerSelection = null;
  app.plannerFocus = null;
}
