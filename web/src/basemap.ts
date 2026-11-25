import * as maplibregl from "maplibre-gl";
import { startView, type Bounds, type Point, type View } from "./viewport";
// MapLibre derives its worker URL from import.meta.url, which does not
// exist in a bundled app. Point it at the worker file that Vite emits.
import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
import { PMTiles, Protocol } from "pmtiles";
import { layers as basemapLayers, namedFlavor } from "@protomaps/basemaps";

maplibregl.setWorkerUrl(workerUrl);

// The asset URLs are absolute: MapLibre fetches glyphs in its worker,
// where a relative URL would resolve against the worker's path.
const LOCAL_ASSETS = "basemap-assets";
const REMOTE_ASSETS = "https://protomaps.github.io/basemaps-assets";

// A dev server answers a missing file with its index page (200,
// text/html), so the check reads the type as well as the status.
async function served(url: string): Promise<boolean> {
  return fetch(url, { method: "HEAD" })
    .then((r) => r.ok && !/text\/html/.test(r.headers.get("content-type") ?? ""))
    .catch(() => false);
}

// The header read is the probe: a missing file, a dev server's index
// page, and a server that ignores Range all fail it. The same PMTiles
// instance serves every tile after, so the header and the directories
// are fetched once.
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

// Asked once. Every map view on the page shares the answer and the
// protocol.
let probed: Promise<{ archive: Bounds | null; assets: string }> | null = null;

function probe() {
  probed ??= Promise.all([openArchive(), served(`${LOCAL_ASSETS}/sprites/v4/light.json`)]).then(([archive, hasAssets]) => ({
    archive,
    assets: hasAssets ? new URL(LOCAL_ASSETS, document.baseURI).href : REMOTE_ASSETS,
  }));
  return probed;
}

// Prefers basemap.pmtiles served next to the app (tiles exist only inside
// its bounds), else OpenFreeMap. Glyphs and sprites come from
// `basemap-assets/` when it is installed (probed, never configured),
// else from the Protomaps host.
// `near` and `framed` feed `startView`, so the map does not fetch tiles
// for a view it will leave.
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
