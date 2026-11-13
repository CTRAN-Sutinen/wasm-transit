<script lang="ts">
  /// Every stop of the plan on a map (cap-directory `stops`), over the
  /// network's lines (cap-directory `route_lines`) drawn quietly for
  /// orientation; the open stop marked. A click on a stop opens it. A
  /// RENDER component: the stops and lines come in as props; the map
  /// computes no geometry (`mapview.ts` only reorders coordinates).
  import { onMount } from "svelte";
  import * as maplibregl from "maplibre-gl";
  import { pickStyle } from "../basemap";
  import { lineFeature, stopsFeatures, type LineFeature } from "../mapview";
  import type { DirectoryStop } from "../types/cap_directory/DirectoryStop";
  import type { RouteLines } from "../types/RouteLines";

  let {
    stops,
    lines,
    selected,
    onstop,
  }: { stops: DirectoryStop[]; lines: RouteLines[]; selected: string; onstop: (stop: string) => void } = $props();

  let container: HTMLDivElement;
  let map: maplibregl.Map | null = null;
  let loaded = $state(false);
  let framed = false;
  let hover = $state("");

  function lineCollection(rs: RouteLines[]) {
    const features: LineFeature[] = [];
    for (const r of rs) for (const l of r.lines) {
      const f = lineFeature(l, { route_id: r.route_id });
      if (f) features.push(f);
    }
    return { type: "FeatureCollection", features };
  }

  onMount(() => {
    let gone = false;
    (async () => {
      const { style, initialView } = await pickStyle();
      if (gone) return;
      map = new maplibregl.Map({ container, style, center: initialView.center, zoom: initialView.zoom });
      map.addControl(new maplibregl.NavigationControl({ showCompass: false }));
      map.on("load", () => {
        const empty = { type: "FeatureCollection", features: [] } as any;
        map!.addSource("lines", { type: "geojson", data: empty });
        map!.addSource("stops", { type: "geojson", data: empty });
        map!.addLayer({
          id: "lines",
          type: "line",
          source: "lines",
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 2, "line-color": "#64748b", "line-opacity": 0.45 },
        });
        map!.addLayer({
          id: "stops",
          type: "circle",
          source: "stops",
          paint: {
            "circle-radius": ["interpolate", ["linear"], ["zoom"], 10, 2, 14, 4.5, 17, 7],
            "circle-color": "#fff",
            "circle-stroke-color": "#334155",
            "circle-stroke-width": 1.5,
          },
        });
        map!.addLayer({
          id: "stop-open",
          type: "circle",
          source: "stops",
          filter: ["==", ["get", "stop_id"], ""],
          paint: { "circle-radius": 9, "circle-color": "#ea580c", "circle-stroke-color": "#fff", "circle-stroke-width": 2.5 },
        });
        map!.on("click", "stops", (e) => {
          const id = e.features?.[0]?.properties?.stop_id;
          if (id) onstop(String(id));
        });
        map!.on("mousemove", "stops", (e) => {
          map!.getCanvas().style.cursor = "pointer";
          hover = String(e.features?.[0]?.properties?.name ?? "");
        });
        map!.on("mouseleave", "stops", () => {
          map!.getCanvas().style.cursor = "";
          hover = "";
        });
        loaded = true;
      });
    })();
    return () => {
      gone = true;
      map?.remove();
    };
  });

  $effect(() => {
    if (!loaded || !map) return;
    (map.getSource("stops") as maplibregl.GeoJSONSource).setData(stopsFeatures(stops) as any);
    (map.getSource("lines") as maplibregl.GeoJSONSource).setData(lineCollection(lines) as any);
    // Frame the stops once; after that the planner's view stays.
    if (framed || stops.length === 0) return;
    const first: [number, number] = [stops[0].lon, stops[0].lat];
    const bounds = stops.reduce((b, s) => b.extend([s.lon, s.lat]), new maplibregl.LngLatBounds(first, first));
    map.fitBounds(bounds, { padding: 40, duration: 0 });
    framed = true;
  });

  // The open stop marked, and brought into view when it is off screen
  // (opened from the search box).
  $effect(() => {
    if (!loaded || !map) return;
    map.setFilter("stop-open", ["==", ["get", "stop_id"], selected]);
    const s = stops.find((x) => x.stop_id === selected);
    if (s && !map.getBounds().contains([s.lon, s.lat])) map.easeTo({ center: [s.lon, s.lat], zoom: Math.max(map.getZoom(), 15) });
  });
</script>

<div class="stop-map" data-testid="stop-map">
  <div class="canvas" bind:this={container}></div>
  <div class="legend">
    <span><i class="dot"></i>stop</span>
    <span><i class="dot open"></i>this stop</span>
    {#if hover}<strong>{hover}</strong>{/if}
  </div>
</div>

<style>
  .stop-map { position: relative; height: 100%; min-height: 300px; }
  .canvas { position: absolute; inset: 0; }
  .legend {
    position: absolute; left: 10px; bottom: 10px; display: flex; gap: 0.8rem; align-items: center;
    padding: 4px 10px; border-radius: 6px; background: var(--surface); font-size: 0.75rem;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
  }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; margin-right: 4px; vertical-align: middle;
    background: #fff; border: 1.5px solid #334155; }
  .dot.open { background: #ea580c; border-color: #fff; box-shadow: 0 0 0 1px #ea580c; }
</style>
