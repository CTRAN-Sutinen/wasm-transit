// The basemap — host glue every map view shares: the style choice
// and the worker wiring. No transit data passes through here.

import * as maplibregl from "maplibre-gl";
import { startView, type Bounds, type Point, type View } from "./viewport";
// MapLibre computes its worker URL from import.meta.url — inside a
// bundled app that path doesn't exist (404 + text/html => the "MIME
// type" worker error). Point it at the worker file Vite emits instead.
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import { PMTiles, Protocol } from "pmtiles";
import { layers as basemapLayers, namedFlavor } from "@protomaps/basemaps";

maplibregl.setWorkerUrl(workerUrl);

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

/// The local archive's bounds, or null when there is no archive. The
/// header read IS the probe — a missing file, a dev server's index
/// page, and a server that ignores Range all fail it — and the one
/// PMTiles instance that read it serves every tile after (the
/// protocol is given it, so the header and the directories are
/// fetched once, never by a second instance of the same file).
async function openArchive(): Promise<Bounds | null> {
  try {
    const archive = new PMTiles("basemap.pmtiles");
    const h = await archive.getHeader();
    const protocol = new Protocol();
    protocol.add(archive);
    maplibregl.addProtocol("pmtiles", protocol.tile);
    return { minLon: h.minLon, minLat: h.minLat, maxLon: h.maxLon, maxLat: h.maxLat };
  } catch (e) {
    console.info("no local pmtiles basemap, using OpenFreeMap:", e);
    return null;
  }
}

/// What this page's deployment has, asked once — every map view on
/// the page shares the answer (and the one protocol).
let probed: Promise<{ archive: Bounds | null; assets: string }> | null = null;

function probe() {
  probed ??= Promise.all([openArchive(), served(`${LOCAL_ASSETS}/sprites/v4/light.json`)]).then(([archive, hasAssets]) => ({
    archive,
    assets: hasAssets ? new URL(LOCAL_ASSETS, document.baseURI).href : REMOTE_ASSETS,
  }));
  return probed;
}

/// Basemap choice, in order of preference:
/// 1. basemap.pmtiles served next to the app (built with planetiler or
///    pmtiles extract) — the fully local option. Tiles exist only
///    inside its bounds. Its glyphs and sprites are `basemap-assets/`
///    when installed (probed, never configured), else
///    protomaps.github.io.
/// 2. OpenFreeMap — free vector tiles, no API key.
/// The pmtiles viewer and basemap style are bundled (real npm deps,
/// no runtime CDN import on every map mount).
///
/// `near` is where the rider is likely to be (their last fix) and
/// `framed` the view the network was last framed on: the map opens on
/// the first that tiles cover (`startView`), else on the archive's
/// own view, so it does not fetch tiles for a view it will leave.
/// Returns { style, initialView }.
export async function pickStyle(
  near: Point | null = null,
  framed: View | null = null,
): Promise<{ style: any; initialView: View }> {
  const { archive, assets } = await probe();
  const initialView = startView(archive, near, framed);
  if (archive) {
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
  }
  return { style: "https://tiles.openfreemap.org/styles/liberty", initialView };
}
