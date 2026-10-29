// The street network's JS-side shape. The streets document, its
// routing graph, snapping and shortest paths all live in the
// cap-streets capability (transit-streets, via the import / snap /
// route ops) — JS holds the document the import answered, its hash
// handle, and the map layer drawn from it.

import type { Streets as StreetsDoc } from "./types/Streets";

export type Streets = {
  /// The document's hash (cap-streets is the hash authority — the
  /// same streets are the same hash, whatever file they came from).
  streetsHash: string;
  /// The streets document the import answered.
  document: StreetsDoc;
  segmentCount: number;
};

type StreetsLayer = { type: "FeatureCollection"; features: unknown[] };
const layers = new WeakMap<StreetsDoc, StreetsLayer>();

/// The document as a map layer, once per document: each way a line
/// ([lon, lat] degrees from its [lat, lon] microdegrees) — the
/// streets the graph routes on, nothing the import left out.
export function streetsLayer(doc: StreetsDoc): StreetsLayer {
  let layer = layers.get(doc);
  if (!layer) layers.set(doc, (layer = drawn(doc)));
  return layer;
}

function drawn(doc: StreetsDoc): StreetsLayer {
  return {
    type: "FeatureCollection",
    features: doc.ways.map((way) => ({
      type: "Feature",
      properties: { name: way.name ?? null, class: way.class },
      geometry: { type: "LineString", coordinates: way.points.map(([lat, lon]) => [lon / 1e6, lat / 1e6]) },
    })),
  };
}
