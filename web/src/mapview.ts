// The map's GeoJSON shaping — and nothing else. Every line the map
// draws is an ANSWER from the bus (a trip's line: cap-runcut
// trip_path; an itinerary's legs: cap-router itinerary_paths; a
// planner entity: cap-directory focus; the diversion ghost:
// cap-divert's draft view); this module only turns [{lat, lon}]
// into GeoJSON's [lon, lat]. No geometry is computed here.

import type { DirectoryStop } from "./types/cap_directory/DirectoryStop";
import type { Coord } from "./types/Coord";

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

/// A path the bus answered ([{lat, lon}]) as a GeoJSON line — null
/// below two points.
export function lineFeature(path: Coord[], props: Record<string, string | number | null>): LineFeature | null {
  return path.length >= 2
    ? { type: "Feature", geometry: { type: "LineString", coordinates: lonLat(path) }, properties: props }
    : null;
}

/// GeoJSON order for a path the bus answered.
export function lonLat(path: Coord[]): MapPoint[] {
  return path.map((c) => [c.lon, c.lat]);
}
