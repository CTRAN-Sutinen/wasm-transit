// Every line the map draws is an answer from the bus. This file only
// reorders [{lat, lon}] into GeoJSON's [lon, lat] and computes no
// geometry.

import type { DirectoryStop } from "./types/cap_directory/DirectoryStop";
import type { Coord } from "./types/Coord";

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

export function lineFeature(
  path: Coord[],
  props: Record<string, string | number | null>,
): LineFeature | null {
  return path.length >= 2
    ? {
        type: "Feature",
        geometry: { type: "LineString", coordinates: lonLat(path) },
        properties: props,
      }
    : null;
}

export function lonLat(path: Coord[]): MapPoint[] {
  return path.map((c) => [c.lon, c.lat]);
}
