// The street network's JS-side shape. The routing graph, snapping
// and shortest-path logic all live in the cap-streets capability
// (transit-streets, via the load_streets / snap / route ops) — JS
// holds only the hash handle, the display copy for the map layer,
// and small pure helpers.

export type Streets = {
  /// The capability's streets registry hash (cap-streets is the
  /// hash authority — the same bytes are the same graph).
  streetsHash: string;
  /// The raw collection, for map display.
  geojson: { type: "FeatureCollection"; features: unknown[] };
  segmentCount: number;
};
