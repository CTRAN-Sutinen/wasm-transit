<script lang="ts">
  /// The unserved WHERE (analysis `unserved_points`): every layer
  /// feature with weight beyond the radius of every stop the selected
  /// service group (a combination of service types, D-55) serves — a
  /// point as a circle sized by its unserved weight, a polygon as its
  /// shape, shaded by the share of its area unserved; the selected one
  /// outlined and framed. A RENDER component — places in, a tapped
  /// place out.
  import { onMount } from "svelte";
  import * as maplibregl from "maplibre-gl";
  import { pickStyle } from "../basemap";
  import type { UnservedPoint } from "../types/UnservedPoint";

  let {
    points,
    selected,
    onselect,
  }: { points: UnservedPoint[]; selected: number | null; onselect: (i: number) => void } = $props();

  let container: HTMLDivElement;
  let map: maplibregl.Map | null = null;
  let loaded = $state(false);

  const collection = (ps: UnservedPoint[]) => ({
    type: "FeatureCollection",
    features: ps.map((p, i) => ({
      type: "Feature",
      properties: { i, weight: p.weight, share: p.share },
      geometry: p.polygon ? { type: "MultiPolygon", coordinates: p.polygon } : { type: "Point", coordinates: [p.lon, p.lat] },
    })),
  });

  onMount(() => {
    let gone = false;
    (async () => {
      const { style, initialView } = await pickStyle();
      if (gone) return;
      map = new maplibregl.Map({ container, style, center: initialView.center, zoom: initialView.zoom });
      map.addControl(new maplibregl.NavigationControl({ showCompass: false }));
      map.on("load", () => {
        map!.addSource("unserved", { type: "geojson", data: collection([]) as any });
        map!.addLayer({
          id: "unserved-area",
          type: "fill",
          source: "unserved",
          filter: ["==", ["geometry-type"], "Polygon"],
          paint: {
            "fill-color": "#f97316",
            "fill-opacity": ["interpolate", ["linear"], ["get", "share"], 0, 0.1, 1, 0.5],
            "fill-outline-color": "#c2410c",
          },
        });
        map!.addLayer({
          id: "unserved-area-selected",
          type: "line",
          source: "unserved",
          filter: ["==", ["get", "i"], -1],
          paint: { "line-color": "#1c1917", "line-width": 3 },
        });
        map!.addLayer({
          id: "unserved",
          type: "circle",
          source: "unserved",
          filter: ["==", ["geometry-type"], "Point"],
          paint: {
            "circle-radius": ["interpolate", ["linear"], ["sqrt", ["get", "weight"]], 0, 3, 100, 18],
            "circle-color": "#f97316",
            "circle-opacity": 0.4,
            "circle-stroke-color": "#c2410c",
            "circle-stroke-width": 1,
          },
        });
        map!.addLayer({
          id: "unserved-selected",
          type: "circle",
          source: "unserved",
          filter: ["==", ["get", "i"], -1],
          paint: {
            "circle-radius": ["interpolate", ["linear"], ["sqrt", ["get", "weight"]], 0, 8, 100, 24],
            "circle-color": "transparent",
            "circle-stroke-color": "#1c1917",
            "circle-stroke-width": 3,
          },
        });
        for (const layer of ["unserved", "unserved-area"]) {
          map!.on("click", layer, (e) => {
            const i = e.features?.[0]?.properties?.i;
            if (typeof i === "number") onselect(i);
          });
        }
        map!.resize();
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
    (map.getSource("unserved") as maplibregl.GeoJSONSource).setData(collection(points) as any);
    if (!points.length) return;
    const first: [number, number] = [points[0].lon, points[0].lat];
    const bounds = points.reduce((b, p) => b.extend([p.lon, p.lat]), new maplibregl.LngLatBounds(first, first));
    map.fitBounds(bounds, { padding: 40, maxZoom: 13, duration: 0 });
  });

  $effect(() => {
    if (!loaded || !map) return;
    map.setFilter("unserved-selected", ["all", ["==", ["geometry-type"], "Point"], ["==", ["get", "i"], selected ?? -1]]);
    map.setFilter("unserved-area-selected", ["==", ["get", "i"], selected ?? -1]);
    const p = selected != null ? points[selected] : null;
    if (p) map.easeTo({ center: [p.lon, p.lat], zoom: Math.max(map.getZoom(), 12), duration: 400 });
  });
</script>

<div class="coverage-map" data-testid="coverage-map"><div class="canvas" bind:this={container}></div></div>

<style>
  .coverage-map { position: relative; height: 100%; min-height: 360px; }
  .canvas { position: absolute; inset: 0; }
</style>
