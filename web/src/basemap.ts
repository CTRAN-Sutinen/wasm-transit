// The basemap — host glue every map view shares: the style choice
// and the worker wiring. No transit data passes through here.

import * as maplibregl from "maplibre-gl";
// MapLibre computes its worker URL from import.meta.url — inside a
// bundled app that path doesn't exist (404 + text/html => the "MIME
// type" worker error). Point it at the worker file Vite emits instead.
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import { PMTiles, Protocol } from "pmtiles";
import { layers as basemapLayers, namedFlavor } from "@protomaps/basemaps";

maplibregl.setWorkerUrl(workerUrl);

let protocolAdded = false;

/// Where the style's labels and icons come from: `basemap-assets/`
/// beside the tiles when `just basemap-assets` installed it, else the
/// Protomaps host. The URLs are absolute — MapLibre fetches glyphs in
/// its worker, where a relative URL would resolve against the
/// worker's own path.
const LOCAL_ASSETS = "basemap-assets";
const REMOTE_ASSETS = "https://protomaps.github.io/basemaps-assets";

/// Whether a file the build serves is there. A dev server answers a
/// missing file with its index page (200, text/html), so the type
/// is checked as well as the status.
async function served(url: string): Promise<boolean> {
  return fetch(url, { method: "HEAD" })
    .then((r) => r.ok && !/text\/html/.test(r.headers.get("content-type") ?? ""))
    .catch(() => false);
}

/// Basemap choice, in order of preference:
/// 1. basemap.pmtiles served next to the app (built with planetiler or
///    pmtiles extract) — the fully local option; the map opens on the
///    archive's bounds, since tiles only exist inside them. Its
///    glyphs and sprites are `basemap-assets/` when installed (it is
///    probed, never configured), else protomaps.github.io.
/// 2. OpenFreeMap — free vector tiles, no API key.
/// The pmtiles viewer and basemap style are bundled (real npm deps,
/// no runtime CDN import on every map mount).
/// Returns { style, initialView }.
export async function pickStyle(): Promise<{ style: any; initialView: { center: [number, number]; zoom: number } }> {
  const [hasLocal, hasAssets] = await Promise.all([
    served("basemap.pmtiles"),
    served(`${LOCAL_ASSETS}/sprites/v4/light.json`),
  ]);
  const assets = hasAssets ? new URL(LOCAL_ASSETS, document.baseURI).href : REMOTE_ASSETS;
  if (hasLocal) {
    try {
      // Once per page — two map views share the protocol.
      if (!protocolAdded) {
        maplibregl.addProtocol("pmtiles", new Protocol().tile);
        protocolAdded = true;
      }

      // A regional extract has no tiles outside its bbox — start the
      // map inside it. Read the bounds from the archive header.
      const archive = new PMTiles("basemap.pmtiles");
      const header = await archive.getHeader();
      const initialView = {
        center: [(header.minLon + header.maxLon) / 2, (header.minLat + header.maxLat) / 2] as [number, number],
        zoom: Math.max(2, Math.round(Math.log2(360 / Math.max(0.05, header.maxLon - header.minLon))) + 1),
      };

      return {
        style: {
          version: 8,
          glyphs: `${assets}/fonts/{fontstack}/{range}.pbf`,
          sprite: `${assets}/sprites/v4/light`,
          sources: {
            protomaps: {
              type: "vector",
              url: "pmtiles://basemap.pmtiles",
              attribution: "Protomaps © OpenStreetMap contributors",
            },
          },
          layers: basemapLayers("protomaps", namedFlavor("light"), { lang: "en" }),
        } as maplibregl.StyleSpecification,
        initialView,
      };
    } catch (e) {
      console.warn("local pmtiles basemap unavailable, falling back to OpenFreeMap:", e);
    }
  }
  return {
    style: "https://tiles.openfreemap.org/styles/liberty",
    initialView: { center: [0, 0], zoom: 1 },
  };
}
