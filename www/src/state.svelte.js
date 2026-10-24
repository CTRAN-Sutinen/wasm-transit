/// Shared reactive state — the Svelte 5 version of the hand-rolled
/// mapState. Every component reads and writes through this one object,
/// so there is exactly one owner of "what is on screen": the state.
/// The map replays from it when it finishes loading; feeds update only
/// the slice they own (TripUpdates can never wipe vehicle positions).
export const app = $state({
  // wasm exports, injected at startup (main.js)
  wasm: null,

  status: "loading wasm...",

  // GTFS static
  gtfs: null, // GtfsHandle
  selectedStop: null, // stop_id
  clock: 0, // bumped to re-derive time-dependent views
  selectedTripId: null,
  tripFeature: null, // GeoJSON LineString feature for the selected trip

  // GTFS-rt
  rtOverview: null, // decode_rt() result (delays panel)
  vehicles: [], // last good vehicle positions (map dots)

  // Trip planning (RAPTOR)
  plan: null, // plan_trip() result: legs with times and stops
  planNotice: null, // inline feedback when planning fails
  itineraryFeatures: [], // GeoJSON features for drawing the plan on the map

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
