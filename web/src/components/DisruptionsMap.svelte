<script lang="ts">
  import { onMount } from "svelte";
  import * as maplibregl from "maplibre-gl";
  import { pickStyle } from "../basemap";
  import { lonLat } from "../mapview";
  import { minutesText, timeText } from "./disruptionsFormat";
  import type { AlertSource } from "../types/AlertSource";
  import type { Coord } from "../types/Coord";
  import type { Disruption } from "../types/Disruption";
  import type { LiveDay } from "../types/LiveDay";
  import type { Today } from "../types/Today";

  let {
    live,
    day,
    focus,
    path,
    open,
    bus,
    ondisruption,
  }: {
    live: LiveDay | null;
    day: Today | null;
    focus: string;
    path: Coord[];
    open: Disruption | null;
    bus: { lat: number; lon: number } | null;
    ondisruption: (source: AlertSource) => void;
  } = $props();

  const ORANGE = "#a84e0a";
  const SLATE = "#5a6573";
  const BLUE = "#1f5fbf";
  const RED = "#b91c1c";
  const PURPLE = "#7c3aed";

  let container: HTMLDivElement;
  let map: maplibregl.Map | null = null;
  let loaded = $state(false);
  let framed = false;
  /// The map frames this view first and keeps it until the reader moves it. A
  /// change of the map's size frames it again.
  let fitted: maplibregl.LngLatBounds | null = null;
  let hover = $state("");

  type Point = { type: "Feature"; geometry: { type: "Point"; coordinates: [number, number] }; properties: Record<string, unknown> };
  const point = (lat: number, lon: number, properties: Record<string, unknown>): Point => ({
    type: "Feature",
    geometry: { type: "Point", coordinates: [lon, lat] },
    properties,
  });
  const collection = (features: unknown[]) => ({ type: "FeatureCollection", features }) as any;

  function vehicles(l: LiveDay | null): Point[] {
    return (l?.vehicles ?? []).map((v) =>
      point(v.lat, v.lon, {
        key: v.key,
        route: v.route ?? "",
        status: v.status,
        label:
          v.status === "late" || v.status === "very_late" || v.status === "early"
            ? `${v.route_name ?? ""} · ${minutesText(Math.abs(v.delay_secs ?? 0))}${v.status === "early" ? " early" : ""}`
            : (v.route_name ?? ""),
      }),
    );
  }

  /// Each disruption appears once, though a closed stop lists on every route
  /// that serves it.
  function marks(t: Today | null): { cancels: Point[]; closed: Point[]; skipped: Point[] } {
    const out = { cancels: [] as Point[], closed: [] as Point[], skipped: [] as Point[] };
    const seen = new Set<string>();
    for (const r of t?.routes ?? [])
      for (const d of r.disruptions) {
        const source = JSON.stringify(d.source);
        if (seen.has(source)) continue;
        seen.add(source);
        if (d.from === "stops") {
          for (const s of d.stops ?? []) if (s.lat != null && s.lon != null) out.closed.push(point(s.lat, s.lon, { source, route: "", name: `${s.name ?? s.id} closed` }));
        } else if (d.trip) {
          const s = d.first_stop;
          if (s?.lat != null && s.lon != null)
            out.cancels.push(point(s.lat, s.lon, { source, route: r.route, name: `${r.name} · ${d.start_secs != null ? timeText(d.start_secs) : "every trip"} cancelled` }));
        } else {
          for (const s of d.stops ?? []) if (s.lat != null && s.lon != null) out.skipped.push(point(s.lat, s.lon, { source, route: r.route, name: `${r.name} detour: ${s.name ?? s.id} not served` }));
        }
      }
    return out;
  }

  const routeOpacity = (on: number, off: number) => ["case", ["any", ["==", focus, ""], ["==", ["get", "route"], focus], ["==", ["get", "route"], ""]], on, off] as any;

  onMount(() => {
    let gone = false;
    let cleanup = () => {};
    (async () => {
      const { style, initialView } = await pickStyle();
      if (gone) return;
      map = new maplibregl.Map({ container, style, center: initialView.center, zoom: initialView.zoom });
      map.addControl(new maplibregl.NavigationControl({ showCompass: false }));
      map.on("movestart", (e) => {
        if ((e as { originalEvent?: unknown }).originalEvent) fitted = null;
      });
      const sized = new ResizeObserver(() => {
        // A box with no size yet frames nothing (a layout in change).
        if (container.clientWidth < 50 || container.clientHeight < 50) return;
        map?.resize();
        if (fitted) map?.fitBounds(fitted, { padding: 40, duration: 0, maxZoom: 14 });
      });
      sized.observe(container);
      cleanup = () => sized.disconnect();
      map.on("load", () => {
        const m = map!;
        for (const id of ["path", "vehicles", "cancels", "closed", "skipped"]) m.addSource(id, { type: "geojson", data: collection([]) });
        m.addLayer({ id: "path", type: "line", source: "path", layout: { "line-join": "round", "line-cap": "round" }, paint: { "line-width": 5, "line-color": SLATE, "line-dasharray": [1.5, 1.5] } });
        m.addLayer({
          id: "skipped",
          type: "circle",
          source: "skipped",
          paint: { "circle-radius": 6, "circle-color": "#fff", "circle-stroke-color": BLUE, "circle-stroke-width": 2.5 },
        });
        m.addLayer({
          id: "closed",
          type: "circle",
          source: "closed",
          paint: { "circle-radius": 8, "circle-color": RED, "circle-stroke-color": "#fff", "circle-stroke-width": 2 },
        });
        m.addLayer({
          id: "closed-x",
          type: "symbol",
          source: "closed",
          layout: { "text-field": "×", "text-size": 14, "text-font": ["Noto Sans Regular"], "text-allow-overlap": true },
          paint: { "text-color": "#fff" },
        });
        m.addLayer({
          id: "cancels",
          type: "circle",
          source: "cancels",
          paint: { "circle-radius": 7, "circle-color": "#fff", "circle-stroke-color": RED, "circle-stroke-width": 3 },
        });
        m.addLayer({
          id: "vehicles",
          type: "circle",
          source: "vehicles",
          paint: {
            "circle-radius": ["match", ["get", "status"], ["late", "very_late", "early"], 7, 4],
            "circle-color": ["match", ["get", "status"], "very_late", ORANGE, "early", PURPLE, "late", SLATE, "#94a3b8"],
            "circle-stroke-color": "#fff",
            "circle-stroke-width": 1.5,
          },
        });
        m.addLayer({
          id: "vehicles-label",
          type: "symbol",
          source: "vehicles",
          filter: ["match", ["get", "status"], ["late", "very_late", "early"], true, false],
          layout: {
            "text-field": ["get", "label"],
            "text-size": 12,
            "text-offset": [0, -1.3],
            "text-anchor": "bottom",
            "text-font": ["Noto Sans Regular"],
            "text-allow-overlap": false,
          },
          paint: { "text-color": "#1c1917", "text-halo-color": "#fff", "text-halo-width": 1.5 },
        });
        for (const layer of ["cancels", "closed", "skipped"]) {
          m.on("click", layer, (e) => {
            const source = e.features?.[0]?.properties?.source;
            if (source) ondisruption(JSON.parse(String(source)) as AlertSource);
          });
          m.on("mousemove", layer, (e) => {
            m.getCanvas().style.cursor = "pointer";
            hover = String(e.features?.[0]?.properties?.name ?? "");
          });
          m.on("mouseleave", layer, () => {
            m.getCanvas().style.cursor = "";
            hover = "";
          });
        }
        loaded = true;
      });
    })();
    return () => {
      gone = true;
      cleanup();
      map?.remove();
    };
  });

  $effect(() => {
    if (!loaded || !map) return;
    const v = vehicles(live);
    const k = marks(day);
    (map.getSource("vehicles") as maplibregl.GeoJSONSource).setData(collection(v));
    (map.getSource("cancels") as maplibregl.GeoJSONSource).setData(collection(k.cancels));
    (map.getSource("closed") as maplibregl.GeoJSONSource).setData(collection(k.closed));
    (map.getSource("skipped") as maplibregl.GeoJSONSource).setData(collection(k.skipped));
    const all = [...v, ...k.cancels, ...k.closed, ...k.skipped];
    if (framed || all.length === 0) return;
    const first = all[0].geometry.coordinates;
    const bounds = all.reduce((b, p) => b.extend(p.geometry.coordinates), new maplibregl.LngLatBounds(first, first));
    fitted = bounds;
    map.fitBounds(bounds, { padding: 40, duration: 0, maxZoom: 14 });
    framed = true;
  });

  $effect(() => {
    if (!loaded || !map) return;
    for (const layer of ["vehicles", "cancels", "skipped"]) map.setPaintProperty(layer, "circle-opacity", routeOpacity(1, 0.2));
    map.setPaintProperty("vehicles", "circle-stroke-opacity", routeOpacity(1, 0.2));
    map.setPaintProperty("vehicles-label", "text-opacity", routeOpacity(1, 0));
  });

  $effect(() => {
    if (!loaded || !map) return;
    const coords = lonLat(path);
    (map.getSource("path") as maplibregl.GeoJSONSource).setData(
      collection(coords.length > 1 ? [{ type: "Feature", geometry: { type: "LineString", coordinates: coords }, properties: {} }] : []),
    );
    if (coords.length > 1) {
      fitted = null;
      const bounds = coords.reduce((b, c) => b.extend(c), new maplibregl.LngLatBounds(coords[0], coords[0]));
      map.fitBounds(bounds, { padding: 60, maxZoom: 15 });
    }
  });

  $effect(() => {
    if (!loaded || !map || !open || open.trip) return;
    const at = (open.stops ?? []).filter((s) => s.lat != null && s.lon != null).map((s) => [s.lon!, s.lat!] as [number, number]);
    if (!at.length) return;
    fitted = null;
    const bounds = at.reduce((b, c) => b.extend(c), new maplibregl.LngLatBounds(at[0], at[0]));
    map.fitBounds(bounds, { padding: 80, maxZoom: 16 });
  });

  $effect(() => {
    if (!loaded || !map || !bus) return;
    fitted = null;
    map.easeTo({ center: [bus.lon, bus.lat], zoom: Math.max(map.getZoom(), 15) });
  });

  const veryLate = $derived(live ? Math.round(live.window.very_late_secs / 60) : 5);
</script>

<div class="dz-map" data-testid="disruptions-map">
  <div class="canvas" bind:this={container}></div>
  <div class="legend">
    <span><i class="dot" style:background={ORANGE}></i>Late {veryLate}+ min</span>
    <span><i class="dot" style:background={SLATE}></i>Late under {veryLate} min</span>
    <span><i class="dot" style:background={PURPLE}></i>Early</span>
    <span><i class="ring" style:border-color={RED}></i>Cancelled trip</span>
    <span><i class="ring" style:border-color={BLUE}></i>Detour skips</span>
    <span><i class="dot" style:background={RED}></i>Stop closed</span>
    {#if hover}<strong>{hover}</strong>{/if}
  </div>
</div>

<style>
  .dz-map { position: relative; height: 100%; min-height: 300px; }
  .canvas { position: absolute; inset: 0; }
  .legend {
    position: absolute; left: 10px; bottom: 10px; display: flex; flex-wrap: wrap; gap: 0.4rem 0.9rem; align-items: center;
    max-width: calc(100% - 20px); padding: 6px 12px; border-radius: 6px; background: var(--surface); color: var(--text);
    font-size: 0.78rem; box-shadow: 0 1px 3px rgb(0 0 0 / 0.2);
  }
  .dot, .ring { display: inline-block; width: 10px; height: 10px; border-radius: 50%; margin-right: 5px; vertical-align: middle; }
  .ring { background: #fff; border: 2.5px solid; width: 11px; height: 11px; }
</style>
