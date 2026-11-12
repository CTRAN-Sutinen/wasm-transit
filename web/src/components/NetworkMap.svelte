<script lang="ts">
  /// The whole network on a map (cap-directory `route_lines`): every
  /// route the day type runs, the routes this plan changed drawn in
  /// the accent, the open route on top. A click on a line opens its
  /// route. A RENDER component: the lines come in as props; the map
  /// computes no geometry (`mapview.ts` only reorders coordinates).
  import { onMount } from "svelte";
  import * as maplibregl from "maplibre-gl";
  import { pickStyle } from "../basemap";
  import { lineFeature, type LineFeature } from "../mapview";
  import type { RouteLines } from "../types/RouteLines";

  let {
    routes,
    changed,
    selected,
    onroute,
  }: { routes: RouteLines[]; changed: string[]; selected: string; onroute: (route: string) => void } = $props();

  let container: HTMLDivElement;
  let map: maplibregl.Map | null = null;
  let loaded = $state(false);
  let framed = false;
  let hover = $state("");

  type Collection = { type: "FeatureCollection"; features: LineFeature[] };

  function features(rs: RouteLines[], changedIds: string[], open: string): Collection {
    const out: LineFeature[] = [];
    for (const r of rs) {
      const state = r.route_id === open ? 2 : changedIds.includes(r.route_id) ? 1 : 0;
      for (const l of r.lines) {
        const f = lineFeature(l, { route_id: r.route_id, name: r.name, state, color: r.color ? `#${r.color}` : null });
        if (f) out.push(f);
      }
    }
    return { type: "FeatureCollection", features: out };
  }

  onMount(() => {
    let gone = false;
    (async () => {
      const { style, initialView } = await pickStyle();
      if (gone) return;
      map = new maplibregl.Map({ container, style, center: initialView.center, zoom: initialView.zoom });
      map.addControl(new maplibregl.NavigationControl({ showCompass: false }));
      map.on("load", () => {
        map!.addSource("routes", { type: "geojson", data: { type: "FeatureCollection", features: [] } as any });
        map!.addLayer({
          id: "routes-casing",
          type: "line",
          source: "routes",
          layout: { "line-join": "round", "line-cap": "round", "line-sort-key": ["get", "state"] },
          paint: { "line-width": ["match", ["get", "state"], 2, 9, 1, 7, 5], "line-color": "#fff", "line-opacity": 0.9 },
        });
        map!.addLayer({
          id: "routes",
          type: "line",
          source: "routes",
          layout: { "line-join": "round", "line-cap": "round", "line-sort-key": ["get", "state"] },
          paint: {
            "line-width": ["match", ["get", "state"], 2, 6, 1, 4.5, 2.5],
            "line-color": ["match", ["get", "state"], 2, "#7c2d12", 1, "#ea580c", "#64748b"],
            "line-opacity": ["match", ["get", "state"], 0, 0.75, 1],
          },
        });
        map!.on("click", "routes", (e) => {
          const id = e.features?.[0]?.properties?.route_id;
          if (id) onroute(String(id));
        });
        map!.on("mousemove", "routes", (e) => {
          map!.getCanvas().style.cursor = "pointer";
          hover = String(e.features?.[0]?.properties?.name ?? "");
        });
        map!.on("mouseleave", "routes", () => {
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
    (map.getSource("routes") as maplibregl.GeoJSONSource).setData(features(routes, changed, selected) as any);
    // Frame the network once; after that the planner's view stays.
    if (framed) return;
    const coords = routes.flatMap((r) => r.lines.flat());
    if (coords.length === 0) return;
    const first: [number, number] = [coords[0].lon, coords[0].lat];
    const bounds = coords.reduce((b, c) => b.extend([c.lon, c.lat]), new maplibregl.LngLatBounds(first, first));
    map.fitBounds(bounds, { padding: 40, duration: 0 });
    framed = true;
  });
</script>

<div class="network-map" data-testid="network-map">
  <div class="canvas" bind:this={container}></div>
  <div class="legend">
    <span><i class="sw base"></i>route</span>
    <span><i class="sw changed"></i>changed in this plan</span>
    {#if hover}<strong>{hover}</strong>{/if}
  </div>
</div>

<style>
  .network-map { position: relative; height: 100%; min-height: 300px; }
  .canvas { position: absolute; inset: 0; }
  .legend {
    position: absolute; left: 10px; bottom: 10px; display: flex; gap: 0.8rem; align-items: center;
    padding: 4px 10px; border-radius: 6px; background: var(--surface); font-size: 0.75rem;
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
  }
  .sw { display: inline-block; width: 18px; height: 4px; border-radius: 2px; margin-right: 4px; vertical-align: middle; }
  .sw.base { background: #64748b; }
  .sw.changed { background: #ea580c; }
</style>
