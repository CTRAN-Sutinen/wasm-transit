<script lang="ts">
  /// Every stop the stops document holds on a date, coloured by its
  /// status there (cap-stops `stops_on`); the open stop marked. A click
  /// on a stop opens it; while the page is choosing a point (a new stop,
  /// a move), a click anywhere chooses it, and the point is marked. A
  /// RENDER component: the stops come in as props; the map computes no
  /// geometry.
  import { onMount } from "svelte";
  import * as maplibregl from "maplibre-gl";
  import { pickStyle } from "../basemap";
  import type { StopOn } from "../types/StopOn";
  import { STATUS_COLORS, STATUS_WORDS } from "./stopWords";

  let {
    stops,
    selected,
    picking,
    point,
    onstop,
    onpoint,
  }: {
    stops: StopOn[];
    selected: string;
    /// A click chooses a point, not a stop.
    picking: boolean;
    point: { lat: number; lon: number } | null;
    onstop: (stop: string) => void;
    onpoint: (lat: number, lon: number) => void;
  } = $props();

  let container: HTMLDivElement;
  let map: maplibregl.Map | null = null;
  let loaded = $state(false);
  let framed = false;
  let hover = $state("");
  const shownStatuses = $derived([...new Set(stops.map((s) => s.status))]);

  const match = (i: 0 | 1, fallback: string): any => [
    "match",
    ["get", "status"],
    ...Object.entries(STATUS_COLORS).flatMap(([k, c]) => [k, c[i]]),
    fallback,
  ];

  onMount(() => {
    let gone = false;
    (async () => {
      const { style, initialView } = await pickStyle();
      if (gone) return;
      map = new maplibregl.Map({ container, style, center: initialView.center, zoom: initialView.zoom });
      map.addControl(new maplibregl.NavigationControl({ showCompass: false }));
      map.on("load", () => {
        const empty = { type: "FeatureCollection", features: [] } as any;
        map!.addSource("stops", { type: "geojson", data: empty });
        map!.addSource("point", { type: "geojson", data: empty });
        map!.addLayer({
          id: "stops",
          type: "circle",
          source: "stops",
          paint: {
            "circle-radius": ["interpolate", ["linear"], ["zoom"], 10, 2.5, 14, 5, 17, 8],
            "circle-color": match(0, "#fff"),
            "circle-stroke-color": match(1, "#334155"),
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
        map!.addLayer({
          id: "point",
          type: "circle",
          source: "point",
          paint: { "circle-radius": 8, "circle-color": "#16a34a", "circle-stroke-color": "#fff", "circle-stroke-width": 2.5 },
        });
        map!.on("click", (e) => {
          if (pickingNow) return onpoint(e.lngLat.lat, e.lngLat.lng);
          const hit = map!.queryRenderedFeatures(e.point, { layers: ["stops"] })[0];
          const id = hit?.properties?.stop_id;
          if (id) onstop(String(id));
        });
        map!.on("mousemove", "stops", (e) => {
          if (!pickingNow) map!.getCanvas().style.cursor = "pointer";
          hover = String(e.features?.[0]?.properties?.name ?? "");
        });
        map!.on("mouseleave", "stops", () => {
          map!.getCanvas().style.cursor = pickingNow ? "crosshair" : "";
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

  let pickingNow = false;
  $effect(() => {
    pickingNow = picking;
    if (map) map.getCanvas().style.cursor = picking ? "crosshair" : "";
  });

  $effect(() => {
    if (!loaded || !map) return;
    const features = stops.map((s) => ({
      type: "Feature",
      geometry: { type: "Point", coordinates: [s.lon, s.lat] },
      properties: { stop_id: s.stop_id, name: s.name ?? s.stop_id, status: s.status },
    }));
    (map.getSource("stops") as maplibregl.GeoJSONSource).setData({ type: "FeatureCollection", features } as any);
    // Frame the stops once; after that the view stays.
    if (framed || stops.length === 0) return;
    const first: [number, number] = [stops[0].lon, stops[0].lat];
    const bounds = stops.reduce((b, s) => b.extend([s.lon, s.lat]), new maplibregl.LngLatBounds(first, first));
    map.fitBounds(bounds, { padding: 40, duration: 0 });
    framed = true;
  });

  $effect(() => {
    if (!loaded || !map) return;
    const features = point ? [{ type: "Feature", geometry: { type: "Point", coordinates: [point.lon, point.lat] }, properties: {} }] : [];
    (map.getSource("point") as maplibregl.GeoJSONSource).setData({ type: "FeatureCollection", features } as any);
  });

  // The open stop marked, and brought into view when it is off screen.
  $effect(() => {
    if (!loaded || !map) return;
    map.setFilter("stop-open", ["==", ["get", "stop_id"], selected]);
    const s = stops.find((x) => x.stop_id === selected);
    if (s && !map.getBounds().contains([s.lon, s.lat])) map.easeTo({ center: [s.lon, s.lat], zoom: Math.max(map.getZoom(), 15) });
  });
</script>

<div class="stop-map" data-testid="stop-life-map">
  <div class="canvas" bind:this={container}></div>
  <div class="legend">
    {#each shownStatuses as s (s)}
      <span data-testid="map-key-{s}"><i class="dot" style="background:{STATUS_COLORS[s][0]};border-color:{STATUS_COLORS[s][1]}"></i>{STATUS_WORDS[s]}</span>
    {/each}
    <span><i class="dot open"></i>this stop</span>
    {#if picking}<strong>Click the map to choose the point</strong>{:else if hover}<strong>{hover}</strong>{/if}
  </div>
</div>

<style>
  .stop-map { position: relative; height: 100%; min-height: 300px; }
  .canvas { position: absolute; inset: 0; }
  .legend {
    position: absolute; left: 10px; top: 10px; display: flex; flex-wrap: wrap; gap: 0.3rem 0.8rem; align-items: center;
    padding: 4px 10px; border-radius: 6px; background: var(--surface); font-size: 0.75rem; max-width: calc(100% - 20px);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
  }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; margin-right: 4px; vertical-align: middle; border: 1.5px solid; }
  .dot.open { background: #ea580c; border-color: #fff; box-shadow: 0 0 0 1px #ea580c; }
</style>
