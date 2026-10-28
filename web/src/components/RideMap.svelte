<script lang="ts">
  /// The rider's map — a RENDER component. It draws answers: every
  /// stop (directory `stops`), the picked stop, the live buses (the
  /// live document's vehicles, colored by their status), a picked
  /// departure's route (directory `focus`), the planned trip's legs
  /// (router `itinerary_paths`), the trip's ends, and the rider's
  /// location. It computes no geometry; it frames what it draws. A
  /// tapped stop and a tapped spot are events.
  import { onMount } from "svelte";
  import * as maplibregl from "maplibre-gl";
  import { pickStyle } from "../basemap";
  import { lineFeature, lonLat, stopsFeatures } from "../mapview";
  import { routeColor as css } from "./rideFormat";
  import type { DirectoryStop } from "../types/cap_directory/DirectoryStop";
  import type { LiveVehicle } from "../types/LiveVehicle";
  import type { EntityFocus } from "../types/EntityFocus";
  import type { LegPath } from "../types/LegPath";

  type Point = { lat: number; lon: number };
  type Place = { id: string; name: string; lat?: number; lon?: number };

  let {
    stops,
    marked,
    selected,
    vehicles,
    trip,
    route,
    routeColor,
    legs,
    from,
    to,
    here,
    padding,
    onstop,
    onpoint,
  }: {
    stops: DirectoryStop[];
    /// Stops the panel lists (the places near the rider), ringed.
    marked: string[];
    /// The stop on the panel (drawn large; the map moves to it).
    selected: Place | null;
    vehicles: LiveVehicle[];
    /// The picked departure's trip: its bus is drawn large.
    trip: string | null;
    route: EntityFocus | null;
    /// The route's authored color (the directory's, bare hex).
    routeColor: string | null;
    legs: LegPath[] | null;
    from: Place | null;
    to: Place | null;
    here: Point | null;
    /// The screen the panel covers, so framing avoids it.
    padding: { top: number; right: number; bottom: number; left: number };
    onstop: (place: Place) => void;
    onpoint: (lat: number, lon: number) => void;
  } = $props();

  let container: HTMLDivElement;
  let map: maplibregl.Map | null = null;
  let loaded = $state(false);
  let framedStops = false;
  const EMPTY = { type: "FeatureCollection", features: [] } as const;

  /// The theme's tokens, read once the map exists (MapLibre paints
  /// with colors, not CSS variables).
  function token(name: string, fallback: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
  }

  function point(p: Point, props: Record<string, string | number | null> = {}) {
    return { type: "Feature" as const, properties: props, geometry: { type: "Point" as const, coordinates: [p.lon, p.lat] } };
  }

  function collection(features: unknown[]) {
    return { type: "FeatureCollection", features } as any;
  }

  function bounds(coords: [number, number][]): maplibregl.LngLatBounds | null {
    if (coords.length === 0) return null;
    return coords.reduce((b, c) => b.extend(c), new maplibregl.LngLatBounds(coords[0], coords[0]));
  }

  function set(source: string, data: unknown) {
    (map?.getSource(source) as maplibregl.GeoJSONSource | undefined)?.setData(data as any);
  }

  onMount(() => {
    let gone = false;
    (async () => {
      const { style, initialView } = await pickStyle();
      if (gone) return;
      map = new maplibregl.Map({ container, style, center: initialView.center, zoom: initialView.zoom, attributionControl: { compact: true } });
      map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "bottom-right");
      map.on("load", () => {
        const m = map!;
        const accent = token("--accent", "#c2410c");
        const ok = token("--ok", "#15803d");
        const warn = token("--warn", "#a16207");
        const bad = token("--bad", "#b91c1c");
        const muted = token("--muted", "#78716c");
        for (const id of ["route", "legs", "stops", "vehicles", "ends", "here"]) m.addSource(id, { type: "geojson", data: EMPTY as any });

        m.addLayer({
          id: "route", type: "line", source: "route",
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 5, "line-opacity": 0.55, "line-color": ["coalesce", ["get", "color"], accent] },
        });
        m.addLayer({
          id: "legs-walk", type: "line", source: "legs", filter: ["==", ["get", "kind"], "walk"],
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 3, "line-color": muted, "line-dasharray": [0.5, 2] },
        });
        m.addLayer({
          id: "legs-transit-casing", type: "line", source: "legs", filter: ["==", ["get", "kind"], "transit"],
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 9, "line-color": "#fff", "line-opacity": 0.9 },
        });
        m.addLayer({
          id: "legs-transit", type: "line", source: "legs", filter: ["==", ["get", "kind"], "transit"],
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 5, "line-color": accent },
        });
        m.addLayer({
          id: "stops", type: "circle", source: "stops", minzoom: 12,
          paint: {
            "circle-radius": ["interpolate", ["linear"], ["zoom"], 12, 1.5, 15, 4, 18, 7],
            "circle-color": "#fff",
            "circle-stroke-color": muted,
            "circle-stroke-width": ["interpolate", ["linear"], ["zoom"], 12, 0.5, 16, 1.5],
          },
        });
        m.addLayer({
          id: "stops-label", type: "symbol", source: "stops", minzoom: 16,
          layout: { "text-field": ["get", "name"], "text-size": 11, "text-offset": [0, 1.1], "text-anchor": "top", "text-optional": true, "text-font": ["Noto Sans Regular"] },
          paint: { "text-color": "#44403c", "text-halo-color": "#fff", "text-halo-width": 1.2 },
        });
        m.addLayer({
          id: "stops-marked", type: "circle", source: "stops", filter: ["in", ["get", "stop_id"], ["literal", []]],
          paint: { "circle-radius": 6, "circle-color": "#fff", "circle-stroke-color": accent, "circle-stroke-width": 2.5 },
        });
        m.addLayer({
          id: "stop-selected", type: "circle", source: "stops", filter: ["==", ["get", "stop_id"], ""],
          paint: { "circle-radius": 9, "circle-color": accent, "circle-stroke-color": "#fff", "circle-stroke-width": 3 },
        });
        m.addLayer({
          id: "ends", type: "circle", source: "ends",
          paint: {
            "circle-radius": 8,
            "circle-color": ["match", ["get", "end"], "from", "#fff", accent],
            "circle-stroke-color": accent,
            "circle-stroke-width": 3,
          },
        });
        m.addLayer({
          id: "vehicles", type: "circle", source: "vehicles",
          paint: {
            "circle-radius": ["interpolate", ["linear"], ["zoom"], 10, ["case", ["==", ["get", "picked"], 1], 8, 3.5], 15, ["case", ["==", ["get", "picked"], 1], 13, 8]],
            "circle-color": ["match", ["get", "status"], "on_time", ok, "late", warn, "very_late", bad, "early", bad, muted],
            "circle-stroke-color": "#fff",
            "circle-stroke-width": ["case", ["==", ["get", "picked"], 1], 3, 1.5],
          },
        });
        m.addLayer({
          id: "vehicles-label", type: "symbol", source: "vehicles", minzoom: 13,
          layout: { "text-field": ["coalesce", ["get", "route_name"], ""], "text-size": 10, "text-font": ["Noto Sans Regular"], "text-allow-overlap": true },
          paint: { "text-color": "#fff" },
        });
        m.addLayer({
          id: "here-halo", type: "circle", source: "here",
          paint: { "circle-radius": 16, "circle-color": "#2563eb", "circle-opacity": 0.15 },
        });
        m.addLayer({
          id: "here", type: "circle", source: "here",
          paint: { "circle-radius": 6, "circle-color": "#2563eb", "circle-stroke-color": "#fff", "circle-stroke-width": 2 },
        });

        // A tapped stop opens it; a tapped spot asks what is near it.
        m.on("click", (e) => {
          const pad = 6;
          const hits = m.queryRenderedFeatures(
            [[e.point.x - pad, e.point.y - pad], [e.point.x + pad, e.point.y + pad]],
            { layers: ["stops", "stops-marked", "stop-selected"] },
          );
          const hit = hits[0];
          if (hit) {
            const [lon, lat] = (hit.geometry as { coordinates: number[] }).coordinates;
            onstop({ id: String(hit.properties.stop_id), name: String(hit.properties.name), lat, lon });
          } else {
            onpoint(e.lngLat.lat, e.lngLat.lng);
          }
        });
        m.on("mouseenter", "stops", () => (m.getCanvas().style.cursor = "pointer"));
        m.on("mouseleave", "stops", () => (m.getCanvas().style.cursor = ""));
        loaded = true;
      });
    })();
    return () => {
      gone = true;
      map?.remove();
    };
  });

  // Every stop; the first answer frames the network.
  $effect(() => {
    if (!loaded || !map) return;
    set("stops", stopsFeatures(stops));
    if (!framedStops && stops.length) {
      framedStops = true;
      const b = bounds(stops.map((s) => [s.lon, s.lat]));
      if (b && !selected && !here) map.fitBounds(b, { padding, duration: 0, maxZoom: 15 });
    }
  });

  $effect(() => {
    if (!loaded || !map) return;
    map.setFilter("stop-selected", ["==", ["get", "stop_id"], selected?.id ?? ""]);
    if (selected?.lat != null && selected.lon != null) {
      map.easeTo({ center: [selected.lon, selected.lat], zoom: Math.max(map.getZoom(), 15.5), padding, duration: 500 });
    }
  });

  $effect(() => {
    if (!loaded) return;
    set("vehicles", collection(vehicles.map((v) => point(v, { status: v.status, route_name: v.route_name, picked: trip != null && v.trip === trip ? 1 : 0 }))));
  });

  $effect(() => {
    if (!loaded || !map) return;
    map.setFilter("stops-marked", ["in", ["get", "stop_id"], ["literal", marked]]);
  });

  // A picked departure's route, framed whole.
  $effect(() => {
    if (!loaded || !map) return;
    set("route", collection(route ? route.lines.map((l) => lineFeature(l, { color: css(routeColor) })).filter(Boolean) : []));
    const b = route ? bounds(route.lines.flatMap((l) => lonLat(l))) : null;
    if (b) map.fitBounds(b, { padding, maxZoom: 15, duration: 500 });
  });

  // The trip's legs, framed whole when a different trip arrives (a
  // trip re-asked each poll keeps the rider's view).
  let framedLegs = "";
  $effect(() => {
    if (!loaded || !map) return;
    set("legs", collection(legs ? legs.map((l) => lineFeature(l.path, { kind: l.kind })).filter(Boolean) : []));
    const coords = legs ? legs.flatMap((l) => lonLat(l.path)) : [];
    const key = coords.length ? JSON.stringify([coords[0], coords[coords.length - 1], legs!.length]) : "";
    const b = bounds(coords);
    if (b && key !== framedLegs) map.fitBounds(b, { padding, maxZoom: 16, duration: 500 });
    framedLegs = key;
  });

  $effect(() => {
    if (!loaded) return;
    const ends = [
      from?.lat != null && from.lon != null ? point({ lat: from.lat, lon: from.lon }, { end: "from" }) : null,
      to?.lat != null && to.lon != null ? point({ lat: to.lat, lon: to.lon }, { end: "to" }) : null,
    ].filter(Boolean);
    set("ends", collection(ends));
  });

  // The rider's location; the first fix brings the map to them.
  let wentHome = false;
  $effect(() => {
    if (!loaded || !map) return;
    set("here", collection(here ? [point(here)] : []));
    if (here && !wentHome && !selected) {
      wentHome = true;
      map.easeTo({ center: [here.lon, here.lat], zoom: 15.5, padding, duration: 600 });
    }
  });
</script>

<div class="ride-map" data-testid="ride-map" data-stops={stops.length} data-vehicles={vehicles.length} data-legs={legs?.length ?? 0}>
  <div class="canvas" bind:this={container}></div>
</div>

<style>
  .ride-map { position: absolute; inset: 0; }
  .canvas { position: absolute; inset: 0; }
</style>
