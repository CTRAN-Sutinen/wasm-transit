/// Shared reactive state — the Svelte 5 version of the hand-rolled
/// mapState. Every component reads and writes through this one object,
/// so there is exactly one owner of "what is on screen": the state.
/// The map replays from it when it finishes loading; feeds update only
/// the slice they own (TripUpdates can never wipe vehicle positions).

/// The sync document (transit-sync `EditRecord[]`) — **owned here**.
/// Plain data under `$state.raw`: replaced wholesale, never mutated
/// in place. That is what makes it safe to persist (IndexedDB can
/// structured-clone it directly — no proxies) and, later, to sync to
/// an ops server. wasm never owns it; `set_diversions` gives wasm a
/// materialized view and returns validation reports.
let editDocument = $state.raw([]);
export function getEditDocument() {
  return editDocument;
}
export function setEditDocument(next) {
  editDocument = next;
}

export const app = $state({
  // wasm exports, injected at startup (main.js)
  wasm: null,

  status: "loading wasm...",

  // GTFS static
  gtfs: null, // GtfsHandle
  selectedStop: null, // stop_id
  clock: 0, // bumped to re-derive time-dependent views
  // Bumped whenever the *effective feed* changes (edits pushed to
  // wasm). wasm internals are not observable from JS, so derived
  // views (departure boards, block chains) read this to know when to
  // re-query. `clock` means "time moved"; `feedEpoch` means "data
  // moved".
  feedEpoch: 0,
  selectedTripId: null,
  tripFeature: null, // GeoJSON LineString feature for the selected trip

  // Validation reports for the edit document, from wasm, aligned by
  // id. Replaced wholesale on every push.
  editReports: [],

  // GTFS-rt
  rtOverview: null, // decode_rt() result (delays panel)
  vehicles: [], // last good vehicle positions (map dots)

  // Trip planning (RAPTOR)
  plan: null, // plan_trip() result: legs with times and stops
  planNotice: null, // inline feedback when planning fails
  planBaseline: null, // same plan against the base feed when diversions are active
  itineraryFeatures: [], // GeoJSON features for drawing the plan on the map

  // Diversion editor form (UI state only — the document above is the
  // persisted truth)
  diversionDraft: null, // editor form state, null when closed
  pickingAnchor: null, // "first" | "last" | null — stop clicks set anchors
  drawingDetour: false, // map clicks append detour vertices

  // URL fields and poll toggles
  gtfsUrl: "",
  rtUrl: "",
  vpUrl: "",
  rtPolling: false,
  vpPolling: false,

  // Persistence: the raw feed zip is cached in IndexedDB so refreshes
  // restore instantly without re-downloading.
  cachedFeed: null, // label of the cached feed, if any
});
