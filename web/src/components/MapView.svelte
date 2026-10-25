<script lang="ts">
  import { onMount } from "svelte";
  import * as maplibregl from "maplibre-gl";
  // MapLibre computes its worker URL from import.meta.url — inside a
  // bundled app that path doesn't exist (404 + text/html => the "MIME
  // type" worker error). Point it at the worker file Vite emits instead.
  import workerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?url";
  import { PMTiles, Protocol } from "pmtiles";
  import { layers as basemapLayers, namedFlavor } from "@protomaps/basemaps";
  import { app, getEditDocument, getStreets } from "../state.svelte";
  import { selectStop, setDiversionAnchor, addDetourSlice } from "../actions";
  import type { StreetPosition } from "../types/StreetPosition";

  maplibregl.setWorkerUrl(workerUrl);

  const EMPTY_FC = { type: "FeatureCollection", features: [] };

  /// Every source this view adds is a geojson source; this accessor
  /// keeps the setData calls typed.
  const src = (name: string): maplibregl.GeoJSONSource => map.getSource(name) as maplibregl.GeoJSONSource;
  let map!: maplibregl.Map;
  let mapLoaded = $state(false);
  /// Stop positions as [lat, lon] — snap candidates for detour
  /// drawing, cached when the stops layer updates.
  let stopPts: [number, number][] = [];

  /// Basemap choice, in order of preference:
  /// 1. basemap.pmtiles served next to the app (built with planetiler or
  ///    pmtiles extract) — the fully local option; the map opens on the
  ///    archive's bounds, since tiles only exist inside them.
  /// 2. OpenFreeMap — free vector tiles, no API key.
  /// The pmtiles viewer and basemap style are bundled (real npm deps,
  /// no runtime CDN import on every map mount); only the glyph/sprite
  /// assets still come from protomaps.github.io.
  /// Returns { style, initialView }.
  async function pickStyle(): Promise<{ style: any; initialView: { center: [number, number]; zoom: number } }> {
    const hasLocal = await fetch("basemap.pmtiles", { method: "HEAD" })
      .then((r) => r.ok)
      .catch(() => false);
    if (hasLocal) {
      try {
        maplibregl.addProtocol("pmtiles", new Protocol().tile);

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
            glyphs: "https://protomaps.github.io/basemaps-assets/fonts/{fontstack}/{range}.pbf",
            sprite: "https://protomaps.github.io/basemaps-assets/sprites/v4/light",
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

  /// Bounds over many coordinates, extended ONE point at a time.
  /// MapLibre's `extend` parses a bare array-of-arrays as the
  /// `[[west, south], [east, north]]` bounds form — passing all
  /// coordinates at once silently ignores everything past the first
  /// two and produces a degenerate box (this bit the planner framing).
  const boundsOf = (coords: number[][]): maplibregl.LngLatBounds =>
    coords.reduce(
      (b, c) => b.extend(c as [number, number]),
      new maplibregl.LngLatBounds(coords[0] as [number, number], coords[0] as [number, number]),
    );

  /// Snap radius in pixels: forgiving, but not fighting the drawer.
  const SNAP_PX = 14;

  /// One waypoint step of the detour trace. The waypoint lands on the
  /// nearest street or stop within a few pixels (raw otherwise); the
  /// path from the previous waypoint is ROUTED along the street
  /// graph inside the artifact, so the committed trace is street
  /// geometry — not freehand chords. Falls back to a straight line
  /// when no street network is imported or no route exists.
  function commitWaypoint(e: maplibregl.MapMouseEvent): void {
    const streets = getStreets();
    // The artifact snaps; JS owns pixels (thresholds are visual).
    const streetPos = streets && app.wasm
      ? app.wasm.street_snap(streets.streetsId, e.lngLat.lat, e.lngLat.lng)
      : null;
    const pxOf = (lat: number, lon: number): number => {
      const p = map.project([lon, lat]);
      return Math.hypot(p.x - e.point.x, p.y - e.point.y);
    };
    const streetPx = streetPos ? pxOf(streetPos.lat, streetPos.lon) : Infinity;

    // Nearest stop within the threshold.
    let stop: { lat: number; lon: number } | null = null;
    let stopPx = SNAP_PX;
    for (const [lat, lon] of stopPts) {
      const px = pxOf(lat, lon);
      if (px <= stopPx) {
        stopPx = px;
        stop = { lat, lon };
      }
    }

    if (stop && stopPx <= streetPx) {
      // A stop waypoint: commit the stop exactly; route onward from
      // the street beneath it when there is one.
      const anchor = streets && app.wasm
        ? app.wasm.street_snap(streets.streetsId, stop.lat, stop.lon)
        : null;
      addDetourSlice([[stop.lat, stop.lon]], anchor);
      return;
    }
    if (streetPos && streetPx <= SNAP_PX && streets && app.wasm) {
      const prev: StreetPosition | null =
        app.diversionDraft?.slices.at(-1)?.endPos ?? null;
      const routed = prev
        ? app.wasm.street_route(streets.streetsId, prev, streetPos)
        : [];
      if (routed.length) {
        addDetourSlice(routed.map((p) => [p.lat, p.lon] as [number, number]), streetPos);
        return;
      }
      if (prev) app.status = "No street path found — using a direct line.";
      addDetourSlice([[streetPos.lat, streetPos.lon]], streetPos);
      return;
    }
    addDetourSlice([[e.lngLat.lat, e.lngLat.lng]], null);
  }

  onMount(() => {
    (async () => {
      const { style, initialView } = await pickStyle();
      console.log("map style chosen:", typeof style === "string" ? style : "protomaps/local pmtiles");
      map = new maplibregl.Map({
        container: "map",
        style,
        center: initialView.center,
        zoom: initialView.zoom,
      });
      // Surface map errors (style fetch failure, tile errors) — they are
      // otherwise console-only and look like "the map is just blank".
      map.on("error", (e) => {
        console.error("maplibre error:", e);
        app.status = `Map error: ${e?.error?.message ?? e.error ?? "unknown"}`;
      });
      map.addControl(new maplibregl.NavigationControl());
      map.addControl(new maplibregl.FullscreenControl(), "top-right");

      map.on("load", () => {
        console.log("map finished loading; adding layers");
        // Test/inspection hook: the app is opaque from outside, the map
        // doubly so. Exposing the instance lets e2e tests (and you, via
        // the console) inspect sources directly.
        window.__map = map;
        map.addSource("stops", { type: "geojson", data: EMPTY_FC });
        // Streets sit under everything: a context layer for drawing,
        // not the subject of the view.
        map.addSource("streets", { type: "geojson", data: EMPTY_FC });
        map.addLayer({
          id: "streets-lines", type: "line", source: "streets",
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 1.5, "line-color": "#b8b8b8" },
        });
        // The diversion editor's selected route, as a muted ghost —
        // where the route runs, so the detour is drawn against it.
        map.addSource("route-context", { type: "geojson", data: EMPTY_FC });
        map.addLayer({
          id: "route-ghost", type: "line", source: "route-context",
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 5, "line-color": "#7d8ba1", "line-opacity": 0.65 },
        });
        map.addLayer({
          id: "stops-base", type: "circle", source: "stops",
          paint: {
            "circle-radius": 3.5, "circle-color": "#555",
            "circle-stroke-width": 1, "circle-stroke-color": "#fff",
          },
        });
        map.addLayer({
          id: "stops-selected", type: "circle", source: "stops",
          filter: ["==", ["get", "stop_id"], ""],
          paint: {
            "circle-radius": 7, "circle-color": "#007cbf",
            "circle-stroke-width": 2, "circle-stroke-color": "#fff",
          },
        });

        map.addSource("trip", { type: "geojson", data: EMPTY_FC });
        map.addLayer({
          id: "trip-line", type: "line", source: "trip",
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 4, "line-color": "#007cbf" },
        });

        map.addSource("vehicles", { type: "geojson", data: EMPTY_FC });
        map.addLayer({
          id: "vehicle-dots", type: "circle", source: "vehicles",
          paint: {
            "circle-radius": 6,
            // Delay from joined trip updates: green on time, amber late,
            // red very late, grey when no prediction exists.
            "circle-color": ["case",
              ["==", ["get", "delay"], null], "#888",
              ["<=", ["get", "delay"], 60], "#109648",
              ["<=", ["get", "delay"], 300], "#e6a23c",
              "#d02b2b",
            ],
            "circle-stroke-width": 2,
            "circle-stroke-color": "#fff",
          },
        });

        // Planned itineraries: transit legs solid, walking dashed.
        map.addSource("itinerary", { type: "geojson", data: EMPTY_FC });
        map.addLayer({
          id: "itinerary-walk", type: "line", source: "itinerary",
          filter: ["==", ["get", "kind"], "walk"],
          layout: { "line-join": "round", "line-cap": "round" },
          paint: {
            "line-width": 3, "line-color": "#777", "line-dasharray": [2, 2],
          },
        });
        map.addLayer({
          id: "itinerary-transit", type: "line", source: "itinerary",
          filter: ["==", ["get", "kind"], "transit"],
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 6, "line-color": "#0a7" },
        });

        // Active diversions: the drawn detour polylines (orange),
        // dashed so they read as "not the normal alignment".
        map.addSource("diversions", { type: "geojson", data: EMPTY_FC });
        map.addLayer({
          id: "diversion-detours", type: "line", source: "diversions",
          layout: { "line-join": "round", "line-cap": "round" },
          paint: {
            "line-width": 4, "line-color": "#e67e22", "line-dasharray": [2, 1.5],
          },
        });

        // The detour being drawn: vertices + draft polyline.
        map.addSource("detour-draft", { type: "geojson", data: EMPTY_FC });
        map.addLayer({
          id: "detour-draft-line", type: "line", source: "detour-draft",
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 3, "line-color": "#c2185b", "line-dasharray": [1, 1] },
        });
        map.addLayer({
          id: "detour-draft-verts", type: "circle", source: "detour-draft",
          paint: { "circle-radius": 4, "circle-color": "#c2185b" },
        });

        // Planner focus: the selected element's geometry (the drill-
        // down target), framed by its own effect below.
        map.addSource("planner", { type: "geojson", data: EMPTY_FC });
        map.addLayer({
          id: "planner-line", type: "line", source: "planner",
          filter: ["==", ["get", "kind"], "line"],
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 5, "line-color": "#7c4dff" },
        });
        map.addLayer({
          id: "planner-points", type: "circle", source: "planner",
          filter: ["==", ["get", "kind"], "point"],
          paint: {
            "circle-radius": 5, "circle-color": "#7c4dff",
            "circle-stroke-width": 2, "circle-stroke-color": "#fff",
          },
        });

        // Detour drawing: waypoints. A click places one waypoint; a
        // drag places them every ~30 px along the stroke. Each step
        // routes along the street graph (see commitWaypoint). Panning
        // is off while the trace mode is armed — Done drawing
        // re-enables it (the effect below owns the toggle).
        let drawingStroke = false;
        let lastWaypointPx: [number, number] | null = null;
        map.on("mousedown", (e) => {
          if (!app.drawingDetour) return;
          drawingStroke = true;
          commitWaypoint(e);
          lastWaypointPx = [e.point.x, e.point.y];
        });
        map.on("mousemove", (e) => {
          if (!drawingStroke || !lastWaypointPx) return;
          // 30 px between waypoints: routing makes denser sampling
          // pointless, and each step is a cheap graph search.
          if (Math.hypot(e.point.x - lastWaypointPx[0], e.point.y - lastWaypointPx[1]) < 30) return;
          commitWaypoint(e);
          lastWaypointPx = [e.point.x, e.point.y];
        });
        const endStroke = (): void => {
          drawingStroke = false;
          lastWaypointPx = null;
        };
        map.on("mouseup", endStroke);
        map.on("dragend", endStroke);

        map.on("click", "stops-base", (e) => {
          const stopId = (e.features?.[0]?.properties as Record<string, string> | undefined)?.stop_id;
          // While the diversion editor is armed, stop clicks set
          // anchors; selecting the stop stays useful either way.
          if (!stopId) return;
          if (app.pickingAnchor) setDiversionAnchor(stopId);
          selectStop(stopId);
        });
        for (const layer of ["stops-base", "stops-selected", "vehicle-dots"]) {
          map.on("mouseenter", layer, () => (map.getCanvas().style.cursor = "pointer"));
          map.on("mouseleave", layer, () => (map.getCanvas().style.cursor = ""));
        }

        mapLoaded = true; // the $effects below replay current state
      });
    })();
    return () => {
      // The inspection hook must not outlive the instance: personas
      // remount the map (the planner workspace has its own).
      if (window.__map === map) window.__map = undefined;
      map?.remove();
    };
  });

  // Reactive bindings: each map layer follows exactly one slice of
  // state. This is what makes "TripUpdates can't wipe vehicle dots"
  // structural rather than a convention.

  $effect(() => {
    if (!mapLoaded || !app.gtfs) return;
    const data = app.gtfs.stops_geojson();
    src("stops").setData(data);
    stopPts = data.features.map((f) => [
      f.geometry.coordinates[1],
      f.geometry.coordinates[0],
    ]);
    const coords = data.features.map((f) => f.geometry.coordinates);
    if (coords.length === 1) map.jumpTo({ center: coords[0] as [number, number], zoom: 13 });
    else if (coords.length > 1) {
      map.fitBounds(boundsOf(coords), { padding: 40 });
    }
  });

  $effect(() => {
    if (!mapLoaded) return;
    map.setFilter("stops-selected", ["==", ["get", "stop_id"], app.selectedStop ?? ""]);
  });

  // The imported street network: context for detour drawing.
  $effect(() => {
    if (!mapLoaded) return;
    src("streets").setData(getStreets()?.geojson ?? EMPTY_FC);
  });

  // The diversion editor's route ghost.
  $effect(() => {
    if (!mapLoaded) return;
    src("route-context").setData(app.routeContext ?? EMPTY_FC);
  });

  // Stroke mode: the map becomes a drawing surface — no panning
  // (distinguish stroke from pan), crosshair cursor.
  $effect(() => {
    if (!mapLoaded) return;
    if (app.drawingDetour) {
      map.dragPan.disable();
      map.getCanvas().style.cursor = "crosshair";
    } else {
      map.dragPan.enable();
      map.getCanvas().style.cursor = "";
    }
  });

  $effect(() => {
    if (!mapLoaded) return;
    const feature = app.tripFeature;
    src("trip").setData(feature ?? EMPTY_FC);
    if (feature) {
      const coords = feature.geometry.coordinates as [number, number][];
      map.fitBounds(boundsOf(coords), { padding: 60 });
    }
  });

  $effect(() => {
    if (!mapLoaded) return;
    // Buses signal their lateness by color: join the vehicle's trip_id
    // against the GTFS-rt trip updates (worst known delay for the trip).
    const delayByTrip = new Map();
    for (const t of app.rtOverview?.trips ?? []) {
      if (!t.trip_id) continue;
      let worst = null;
      for (const s of t.stop_updates ?? []) {
        const d = s.departure_delay ?? s.arrival_delay;
        if (d != null) worst = worst == null ? d : Math.max(worst, d);
      }
      if (worst != null) delayByTrip.set(t.trip_id, worst);
    }
    const features = app.vehicles.map((v) => ({
      type: "Feature",
      geometry: { type: "Point", coordinates: [v.lon, v.lat] },
      properties: {
        trip_id: v.trip_id ?? "",
        route_id: v.route_id ?? "",
        delay: v.trip_id ? (delayByTrip.get(v.trip_id) ?? null) : null,
      },
    }));
    src("vehicles").setData({ type: "FeatureCollection", features });
  });

  $effect(() => {
    if (!mapLoaded) return;
    const features = app.itineraryFeatures;
    src("itinerary").setData({ type: "FeatureCollection", features });
    if (features.length) {
      const coords = features.flatMap((f) => f.geometry.coordinates as [number, number][]);
      map.fitBounds(boundsOf(coords), { padding: 60 });
    }
  });

  // In-force diversions draw their detours on the map.
  $effect(() => {
    if (!mapLoaded) return;
    const features = [];
    for (const record of getEditDocument()) {
      const detour = record.diversion.detour;
      if (record.status === "retired" || !detour?.length) continue;
      features.push({
        type: "Feature",
        geometry: {
          type: "LineString",
          coordinates: detour.map((p) => [p.lon, p.lat]),
        },
        properties: { id: record.id, route: record.diversion.route_id },
      });
    }
    src("diversions").setData({ type: "FeatureCollection", features });
  });

  // The detour draft: vertices as points, the polyline once it has
  // two of them. (Circle layers only render Point features, line
  // layers only LineStrings, so one source feeds both layers.)
  $effect(() => {
    if (!mapLoaded) return;
    const pts = app.diversionDraft?.detour ?? [];
    const features: any[] = pts.map(([lat, lon]: [number, number]) => ({
      type: "Feature",
      geometry: { type: "Point", coordinates: [lon, lat] },
      properties: {},
    }));
    if (pts.length >= 2) {
      features.push({
        type: "Feature",
        geometry: { type: "LineString", coordinates: pts.map(([lat, lon]: [number, number]) => [lon, lat] as [number, number]) },
        properties: {},
      });
    }
    src("detour-draft").setData({ type: "FeatureCollection", features });
  });

  // Planner drill-down focus: the selected element, drawn on the map
  // and framed. Empty when nothing is selected (rider/dispatcher).
  $effect(() => {
    if (!mapLoaded) return;
    const focus = app.plannerFocus;
    const features = [];
    if (focus) {
      for (const coords of focus.lines ?? []) {
        features.push({
          type: "Feature",
          geometry: { type: "LineString", coordinates: coords },
          properties: { kind: "line" },
        });
      }
      for (const coord of focus.points ?? []) {
        features.push({
          type: "Feature",
          geometry: { type: "Point", coordinates: coord },
          properties: { kind: "point" },
        });
      }
    }
    src("planner").setData({ type: "FeatureCollection", features });
    const all: [number, number][] = features.flatMap((f) =>
      f.geometry.type === "Point" ? [f.geometry.coordinates as [number, number]] : (f.geometry.coordinates as [number, number][]),
    );
    if (all.length) {
      map.fitBounds(boundsOf(all), { padding: 60 });
    }
  });
</script>

<!-- The map is the stage: full-viewport, with the UI panel floating
     over it (see App.svelte). -->
<div id="map"></div>
