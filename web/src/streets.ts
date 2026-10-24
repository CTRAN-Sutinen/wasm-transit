// Street-network snapping for detour drawing.
//
// The dispatcher imports a street network as GeoJSON (an OSM extract
// exported to LineStrings). Snapping is an *authoring aid*: the detour
// points remain the sync unit — this module only decides where a click
// lands. Deliberately JS-side for now (stateless geometry, no ABI
// churn); if it graduates into a wasm artifact, the contract is
// `load(bytes) -> id` + `snap(id, lat, lon)`, never shared state.

export type StreetNetwork = {
  /// Flat segment endpoints, 4 floats per segment:
  /// [aLat, aLon, bLat, bLon, aLat, aLon, ...].
  coords: Float64Array;
  /// Grid cells -> segment ids (a segment registers in every cell its
  /// bbox covers, so long segments are found anywhere along them).
  cells: Map<number, number[]>;
  segmentCount: number;
};

export type Streets = {
  /// The raw collection, kept for map display.
  geojson: { type: "FeatureCollection"; features: unknown[] };
  network: StreetNetwork;
};

/// ~110 m cells: snapping thresholds are smaller than this, so a
/// 3x3 neighborhood around the click's cell is always enough.
const CELL = 0.001;
const cellKey = (ix: number, iy: number): number => (ix + 200_000) * 400_000 + (iy + 100_000);

/// Extract the line segments of a GeoJSON FeatureCollection. Only
/// LineString and MultiLineString geometries are streets; anything
/// else (points, polygons, bad rows) is skipped. Returns null when
/// the file carries no usable line geometry.
export function buildStreetNetwork(geojson: unknown): Streets | null {
  if (typeof geojson !== "object" || geojson === null) return null;
  const fc = geojson as { type?: string; features?: unknown[] };
  const features =
    fc.type === "FeatureCollection" && Array.isArray(fc.features)
      ? fc.features
      : fc.type === "Feature"
        ? [fc]
        : null;
  if (!features) return null;

  const coords: number[] = [];
  const cells = new Map<number, number[]>();
  const addSegment = (aLat: number, aLon: number, bLat: number, bLon: number): void => {
    const id = coords.length / 4;
    coords.push(aLat, aLon, bLat, bLon);
    // Register in every cell the segment's bbox touches.
    const ix0 = Math.floor(Math.min(aLon, bLon) / CELL);
    const ix1 = Math.floor(Math.max(aLon, bLon) / CELL);
    const iy0 = Math.floor(Math.min(aLat, bLat) / CELL);
    const iy1 = Math.floor(Math.max(aLat, bLat) / CELL);
    for (let ix = ix0; ix <= ix1; ix++) {
      for (let iy = iy0; iy <= iy1; iy++) {
        const key = cellKey(ix, iy);
        const bucket = cells.get(key);
        if (bucket) bucket.push(id);
        else cells.set(key, [id]);
      }
    }
  };
  const addLine = (line: unknown): boolean => {
    if (!Array.isArray(line) || line.length < 2) return false;
    let added = false;
    let prev: [number, number] | null = null;
    for (const pt of line) {
      if (!Array.isArray(pt) || pt.length < 2) { prev = null; continue; }
      const [lon, lat] = pt as number[];
      if (!Number.isFinite(lat) || !Number.isFinite(lon)) { prev = null; continue; }
      if (prev) { addSegment(prev[0], prev[1], lat, lon); added = true; }
      prev = [lat, lon];
    }
    return added;
  };

  for (const feature of features) {
    const geometry = (feature as { geometry?: { type?: string; coordinates?: unknown } })
      ?.geometry;
    if (!geometry) continue;
    if (geometry.type === "LineString") addLine(geometry.coordinates);
    else if (geometry.type === "MultiLineString" && Array.isArray(geometry.coordinates))
      for (const line of geometry.coordinates) addLine(line);
  }

  const segmentCount = coords.length / 4;
  if (!segmentCount) return null;
  return {
    geojson: { type: "FeatureCollection", features: features as unknown[] },
    network: { coords: Float64Array.from(coords), cells, segmentCount },
  };
}

export type StreetSnap = { lat: number; lon: number; distM: number };

/// The nearest point on any street segment to (lat, lon), within the
/// 3x3 cell neighborhood of the click. Distance is equirectangular
/// meters around the click — accurate at the ~100 m scale snapping
/// operates at. Returns null when no street is nearby.
export function snapToStreet(net: StreetNetwork, lat: number, lon: number): StreetSnap | null {
  const ix = Math.floor(lon / CELL);
  const iy = Math.floor(lat / CELL);
  const cosLat = Math.cos((lat * Math.PI) / 180);
  const M_LAT = 110_540; // meters per degree latitude
  const M_LON = 111_320 * cosLat; // meters per degree longitude, here

  let best: StreetSnap | null = null;
  for (let dx = -1; dx <= 1; dx++) {
    for (let dy = -1; dy <= 1; dy++) {
      const bucket = net.cells.get(cellKey(ix + dx, iy + dy));
      if (!bucket) continue;
      for (const id of bucket) {
        const o = id * 4;
        const aLat = net.coords[o];
        const aLon = net.coords[o + 1];
        const bLat = net.coords[o + 2];
        const bLon = net.coords[o + 3];
        // Project the click onto the segment in local meters.
        const ax = (aLon - lon) * M_LON;
        const ay = (aLat - lat) * M_LAT;
        const bx = (bLon - lon) * M_LON;
        const by = (bLat - lat) * M_LAT;
        const len2 = (bx - ax) * (bx - ax) + (by - ay) * (by - ay);
        const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, -(ax * (bx - ax) + ay * (by - ay)) / len2));
        const px = ax + t * (bx - ax);
        const py = ay + t * (by - ay);
        const distM = Math.hypot(px, py);
        if (!best || distM < best.distM) {
          best = { lat: lat + py / M_LAT, lon: lon + px / M_LON, distM };
        }
      }
    }
  }
  return best;
}
