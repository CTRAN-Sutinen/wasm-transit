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

/// Length of a [lat, lon] polyline in meters (equirectangular —
/// exact enough at street scales). Used to show the dispatcher what
/// they drew.
export function pathLengthMeters(points: [number, number][]): number {
  let total = 0;
  for (let i = 1; i < points.length; i++) {
    const [lat1, lon1] = points[i - 1];
    const [lat2, lon2] = points[i];
    const cosLat = Math.cos((((lat1 + lat2) / 2) * Math.PI) / 180);
    total += Math.hypot((lon2 - lon1) * 111_320 * cosLat, (lat2 - lat1) * 110_540);
  }
  return total;
}
