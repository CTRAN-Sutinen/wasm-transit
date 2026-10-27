// The map view — HOST code over MODEL DOCUMENTS. The monolith's
// five feed-side geojson ops are retired (U6): nothing in wasm
// draws the map. The layers render from what the host already
// owns — the network document (stops, patterns with their authored
// alignments), the effective day (cap-runcut's trip detail), the
// edit document (detour polylines, drawn in MapView), and the
// trip-id map (agency ids ↔ derived trips) for click-throughs.
// The map is a view, not a capability.

import type { DirectoryStop } from "./types/cap_directory/DirectoryStop";
import type { Network } from "./types/Network";
import type { TripStopView } from "./types/TripStopView";

/// GeoJSON order: [lon, lat].
export type MapPoint = [number, number];

export type LineFeature = {
  type: "Feature";
  geometry: { type: "LineString"; coordinates: MapPoint[] };
  properties: Record<string, string | number | null>;
};

export type StopFeature = {
  type: "Feature";
  geometry: { type: "Point"; coordinates: MapPoint };
  properties: { stop_id: string; name: string };
};

export type StopFeatureCollection = {
  type: "FeatureCollection";
  features: StopFeature[];
};

/// The stops layer: what exists in the network, with coordinates —
/// the directory's subject, drawn.
export function stopsFeatures(stops: DirectoryStop[]): StopFeatureCollection {
  return {
    type: "FeatureCollection",
    features: stops.map((s) => ({
      type: "Feature",
      geometry: { type: "Point", coordinates: [s.lon, s.lat] },
      properties: { stop_id: s.stop_id, name: s.name },
    })),
  };
}

/// One stop's coordinates from the network document.
export function stopPointOf(net: Network, stopId: string): MapPoint | null {
  const stop = net.stops[stopId];
  return stop ? [stop.lon, stop.lat] : null;
}

/// Does the effective trip still follow its authored pattern's
/// stop sequence? A diverted trip (stops skipped) does not — it
/// rides its own shape, exactly like the router's patterns.
function followsPattern(net: Network, patternId: string, stops: TripStopView[]): boolean {
  const pattern = net.patterns[patternId];
  if (!pattern) return false;
  const authored = pattern.stops.map((ps) => ps.stop);
  return authored.length === stops.length && authored.every((id, i) => id === stops[i].stop_id);
}

/// The pattern's authored alignment as [lon, lat] points, when it
/// has one ("None means draw stop-to-stop lines" — the model's own
/// contract).
function patternPath(net: Network, patternId: string): MapPoint[] | null {
  const path = net.patterns[patternId]?.path;
  return path && path.length >= 2 ? path.map((c) => [c.lon, c.lat] as MapPoint) : null;
}

/// The vertex of `path` nearest `at` — planar distance in degrees,
/// exact at city scale. The same rule the retired feed-side op
/// applied when slicing segments.
function nearestVertex(path: MapPoint[], at: MapPoint): number {
  let best = 0;
  let bestDist = Infinity;
  path.forEach((pt, i) => {
    const d = (pt[0] - at[0]) ** 2 + (pt[1] - at[1]) ** 2;
    if (d < bestDist) {
      bestDist = d;
      best = i;
    }
  });
  return best;
}

function dedupe(points: MapPoint[]): MapPoint[] {
  const out: MapPoint[] = [];
  for (const p of points) {
    const last = out[out.length - 1];
    if (!last || last[0] !== p[0] || last[1] !== p[1]) out.push(p);
  }
  return out;
}

function lineOf(points: MapPoint[], props: Record<string, string | number | null>): LineFeature {
  return {
    type: "Feature",
    geometry: { type: "LineString", coordinates: points },
    properties: props,
  };
}

/// One trip's line as it runs in the effective day: the authored
/// alignment when the trip still follows its pattern (and the
/// pattern has one), otherwise straight stop-to-stop lines over the
/// effective sequence — a diverted trip draws its patched path.
export function tripLine(
  net: Network,
  patternId: string,
  stops: TripStopView[],
  props: Record<string, string | number | null>,
): LineFeature | null {
  if (followsPattern(net, patternId, stops)) {
    const path = patternPath(net, patternId);
    if (path) return lineOf(path, props);
  }
  const pts = dedupe(
    stops
      .map((s) => stopPointOf(net, s.stop_id))
      .filter((p): p is MapPoint => p !== null),
  );
  return pts.length >= 2 ? lineOf(pts, props) : null;
}

/// One segment of a trip's line — the itinerary leg's ridden
/// portion, from one of the trip's stops to a later one. Prefers
/// the authored alignment sliced between the stops' nearest
/// vertices; falls back to the effective stops between the two
/// positions; finally to a straight line. Null when the endpoints
/// are unknown to the network.
export function tripSegment(
  net: Network,
  patternId: string,
  stops: TripStopView[],
  fromStopId: string,
  toStopId: string,
): MapPoint[] | null {
  const from = stopPointOf(net, fromStopId);
  const to = stopPointOf(net, toStopId);
  if (!from || !to) return null;
  if (followsPattern(net, patternId, stops)) {
    const path = patternPath(net, patternId);
    if (path) {
      const i0 = nearestVertex(path, from);
      const i1 = nearestVertex(path, to);
      const [lo, hi] = i0 <= i1 ? [i0, i1] : [i1, i0];
      if (hi > lo) return path.slice(lo, hi + 1);
    }
  }
  const i = stops.findIndex((s) => s.stop_id === fromStopId);
  const j = stops.findIndex((s) => s.stop_id === toStopId);
  if (i >= 0 && j >= 0 && i !== j) {
    const [lo, hi] = i < j ? [i, j] : [j, i];
    const pts = dedupe(
      stops
        .slice(lo, hi + 1)
        .map((s) => stopPointOf(net, s.stop_id))
        .filter((p): p is MapPoint => p !== null),
    );
    if (pts.length >= 2) return pts;
  }
  return [from, to];
}
