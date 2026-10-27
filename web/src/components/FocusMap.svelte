<script lang="ts">
  /// A map that draws ONE answer: an entity's focus (cap-directory
  /// `focus` — the lines it runs and the stops it serves) and frames
  /// it. A RENDER component: the focus comes in as a prop; the map
  /// computes no geometry and reads no app state. `mapview.ts` only
  /// reorders [{lat, lon}] into GeoJSON's [lon, lat].
  import { onMount } from "svelte";
  import * as maplibregl from "maplibre-gl";
  import { pickStyle } from "../basemap";
  import { lineFeature, lonLat, type LineFeature, type MapPoint } from "../mapview";
  import type { EntityFocus } from "../types/EntityFocus";

  let { focus, label = "" }: { focus: EntityFocus | null; label?: string } = $props();

  let container: HTMLDivElement;
  let map: maplibregl.Map | null = null;
  let loaded = $state(false);

  type PointFeature = { type: "Feature"; properties: {}; geometry: { type: "Point"; coordinates: MapPoint } };
  type Collection = { type: "FeatureCollection"; features: (LineFeature | PointFeature)[] };
  const EMPTY: Collection = { type: "FeatureCollection", features: [] };

  function linesOf(f: EntityFocus | null): Collection {
    if (!f) return EMPTY;
    return { type: "FeatureCollection", features: f.lines.map((l) => lineFeature(l, {})).filter((x) => x !== null) };
  }

  function pointsOf(f: EntityFocus | null): Collection {
    if (!f) return EMPTY;
    return {
      type: "FeatureCollection",
      features: lonLat(f.points).map((c) => ({ type: "Feature", properties: {}, geometry: { type: "Point", coordinates: c } })),
    };
  }

  onMount(() => {
    let gone = false;
    (async () => {
      const { style, initialView } = await pickStyle();
      if (gone) return;
      map = new maplibregl.Map({ container, style, center: initialView.center, zoom: initialView.zoom });
      map.addControl(new maplibregl.NavigationControl({ showCompass: false }));
      map.on("load", () => {
        map!.addSource("focus-lines", { type: "geojson", data: EMPTY as any });
        map!.addSource("focus-points", { type: "geojson", data: EMPTY as any });
        map!.addLayer({
          id: "focus-lines",
          type: "line",
          source: "focus-lines",
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 4, "line-color": "#c2410c", "line-opacity": 0.85 },
        });
        map!.addLayer({
          id: "focus-points",
          type: "circle",
          source: "focus-points",
          paint: { "circle-radius": 3.5, "circle-color": "#fff", "circle-stroke-width": 1.5, "circle-stroke-color": "#7c2d12" },
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
    (map.getSource("focus-lines") as maplibregl.GeoJSONSource).setData(linesOf(focus) as any);
    (map.getSource("focus-points") as maplibregl.GeoJSONSource).setData(pointsOf(focus) as any);
    const coords = focus ? [...focus.lines.flat(), ...focus.points] : [];
    if (coords.length === 0) return;
    // Extend one point at a time: a bare array-of-arrays reads as
    // the [[w, s], [e, n]] form and frames a degenerate box.
    const first: [number, number] = [coords[0].lon, coords[0].lat];
    const bounds = coords.reduce((b, c) => b.extend([c.lon, c.lat]), new maplibregl.LngLatBounds(first, first));
    map.fitBounds(bounds, { padding: 32, maxZoom: 15, duration: 300 });
  });
</script>

<div class="focus-map" data-testid="focus-map">
  <div class="canvas" bind:this={container}></div>
  {#if label}<div class="label">{label}</div>{/if}
</div>

<style>
  .focus-map { position: relative; height: 100%; min-height: 220px; }
  .canvas { position: absolute; inset: 0; }
  .label {
    position: absolute; left: 8px; top: 8px; padding: 2px 8px; border-radius: 4px;
    background: var(--surface); font-size: 0.8rem; box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
  }
</style>
