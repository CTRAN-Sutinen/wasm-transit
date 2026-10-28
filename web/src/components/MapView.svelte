<script lang="ts">
  import { onMount } from "svelte";
  import * as maplibregl from "maplibre-gl";
  import { pickStyle } from "../basemap";
  import { app, getEditDocument, getStreets, directory } from "../state.svelte";
  import { selectStop, setDiversionAnchor, addDetourSlice, snapToStreets, routeOnStreets } from "../actions";
  import { stopsFeatures } from "../mapview";
  import type { StreetPosition } from "../types/StreetPosition";
  import type { LiveStatus } from "../types/LiveStatus";

  const EMPTY_FC = { type: "FeatureCollection", features: [] };

  /// Every source this view adds is a geojson source; this accessor
  /// keeps the setData calls typed.
  const src = (name: string): maplibregl.GeoJSONSource => map.getSource(name) as maplibregl.GeoJSONSource;
  let map!: maplibregl.Map;
  let mapLoaded = $state(false);
  /// Stop positions as [lat, lon] — snap candidates for detour
  /// drawing, cached when the stops layer updates.
  let stopPts: [number, number][] = [];

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

  /// Where a trace click at `e` would land: the nearest stop within
  /// the snap radius, a snapped street position, or — only when NO
  /// street network is imported — the raw click. Null when a network
  /// is loaded and the click hits neither: waypoints must stay on the
  /// imported graph (an off-street detour is not drivable).
  type WaypointTarget = {
    kind: "stop" | "street" | "raw";
    /// [lat, lon] where the waypoint would be committed.
    point: [number, number];
    /// Graph address when the target is on a street — the next
    /// step routes from here.
    streetPos: StreetPosition | null;
  };

  function resolveWaypoint(e: maplibregl.MapMouseEvent): WaypointTarget | null {
    const streets = getStreets();
    // JS owns pixels (thresholds are visual); the capability snaps.
    const pxOf = (lat: number, lon: number): number => {
      const p = map.project([lon, lat]);
      return Math.hypot(p.x - e.point.x, p.y - e.point.y);
    };
    let streetPos: StreetPosition | null = null;
    let streetPx = Infinity;
    if (streets) {
      const snap = snapToStreets(e.lngLat.lat, e.lngLat.lng);
      if (snap) {
        const px = pxOf(snap.lat, snap.lon);
        if (px <= SNAP_PX) {
          streetPos = snap;
          streetPx = px;
        }
      }
    }
    // Nearest stop within the threshold.
    let stop: [number, number] | null = null;
    let stopPx = SNAP_PX;
    for (const [lat, lon] of stopPts) {
      const px = pxOf(lat, lon);
      if (px <= stopPx) {
        stopPx = px;
        stop = [lat, lon];
      }
    }
    if (stop && stopPx <= streetPx) {
      return { kind: "stop", point: stop, streetPos };
    }
    if (streetPos) {
      return { kind: "street", point: [streetPos.lat, streetPos.lon], streetPos };
    }
    if (!streets) {
      return { kind: "raw", point: [e.lngLat.lat, e.lngLat.lng], streetPos: null };
    }
    return null;
  }

  /// One waypoint step of the detour trace: the waypoint lands on the
  /// nearest street or stop within a few pixels; the path from the
  /// previous waypoint is ROUTED along the street graph inside the
  /// cap-streets capability, so the committed trace is street
  /// geometry — not freehand chords. Falls back to a straight line
  /// when no street network is imported or no route exists.
  function commitWaypoint(e: maplibregl.MapMouseEvent): void {
    const streets = getStreets();
    const target = resolveWaypoint(e);
    if (!target) {
      app.status = "Waypoint not placed — click a street or a stop; off-network points are ignored.";
      return;
    }
    if (target.kind === "stop") {
      // A stop waypoint: commit the stop exactly; route onward from
      // the street beneath it when there is one.
      const anchor = snapToStreets(target.point[0], target.point[1]);
      addDetourSlice([target.point], anchor);
      return;
    }
    if (target.kind === "street" && streets) {
      const prev: StreetPosition | null =
        app.diversionDraft?.slices.at(-1)?.endPos ?? null;
      const routed = prev
        ? routeOnStreets(prev, target.streetPos!)
        : [];
      if (routed.length) {
        addDetourSlice(routed.map((p) => [p.lat, p.lon] as [number, number]), target.streetPos);
        return;
      }
      if (prev) app.status = "No street path found — using a direct line.";
      addDetourSlice([target.point], target.streetPos);
      return;
    }
    addDetourSlice([target.point], null);
  }

  /// Live preview of the next trace segment: the routed street path
  /// from the last confirmed waypoint to where the click would land
  /// (the same resolution rule as commitWaypoint). Nothing previews
  /// where a click would place nothing. Purely visual; the click
  /// confirms.
  function previewSegment(e: maplibregl.MapMouseEvent): void {
    const draft = app.diversionDraft;
    const lastPt = draft?.slices.at(-1)?.points.at(-1);
    if (!draft || !lastPt) {
      src("detour-preview").setData(EMPTY_FC);
      return;
    }
    const target = resolveWaypoint(e);
    if (!target) {
      src("detour-preview").setData(EMPTY_FC);
      return;
    }
    const streets = getStreets();
    const prev = draft.slices.at(-1)?.endPos ?? null;
    let path: [number, number][] = [];
    if (target.streetPos && prev && streets) {
      path = routeOnStreets(prev, target.streetPos)
        .map((p) => [p.lat, p.lon] as [number, number]);
    }
    // No route (or none possible): preview the direct line — that is
    // what a click would commit.
    if (!path.length) path = [target.point];
    src("detour-preview").setData({
      type: "FeatureCollection",
      features: [
        {
          type: "Feature",
          properties: {},
          geometry: {
            type: "LineString",
            coordinates: [[lastPt[1], lastPt[0]], ...path.map(([lat, lon]) => [lon, lat])],
          },
        },
        {
          type: "Feature",
          properties: {},
          geometry: { type: "Point", coordinates: [target.point[1], target.point[0]] },
        },
      ],
    });
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
        // Buses as small rectangle ICONS, drawn on a canvas — constant
        // screen size at every zoom (a meter-based rect vanishes
        // below z13), rotated to the vehicle's bearing, colored by
        // the delay join: green on time, amber late, red very late,
        // grey when no prediction exists. Square when the feed gives
        // no bearing — an axis-aligned rectangle would imply a
        // direction the data doesn't support. The bus number rides
        // above each rectangle: route badge · vehicle label.
        const busIcons = (shape: "bus" | "busq"): void => {
          for (const fill of ["#888888", "#109648", "#e6a23c", "#d02b2b"]) {
            const c = document.createElement("canvas");
            c.width = shape === "bus" ? 28 : 16;
            c.height = 16;
            const g = c.getContext("2d")!;
            g.fillStyle = "#fff";
            g.fillRect(0, 0, c.width, c.height);
            g.fillStyle = fill;
            g.fillRect(2, 2, c.width - 4, 12);
            map.addImage(`${shape}-${fill.slice(1)}`, {
              width: c.width,
              height: c.height,
              data: g.getImageData(0, 0, c.width, c.height).data,
            });
          }
        };
        busIcons("bus");
        busIcons("busq");
        map.addLayer({
          id: "vehicle-bodies", type: "symbol", source: "vehicles",
          layout: {
            "icon-image": ["concat",
              ["case", ["==", ["get", "bearing"], null], "busq-", "bus-"],
              // Color by the status cap-rt classified (the live
              // document's window) — the map holds no thresholds.
              ["match", ["get", "status"],
                "on_time", "109648",
                ["early", "late"], "e6a23c",
                "very_late", "d02b2b",
                "888888",
              ],
            ],
            "icon-rotate": ["coalesce", ["get", "bearing"], 0],
            "icon-size": 1,
            "icon-allow-overlap": false,
            "text-field": ["get", "label_text"],
            "text-font": ["Noto Sans Regular"],
            "text-size": 11,
            "text-offset": [0, -1.1],
            "text-anchor": "bottom",
            "text-allow-overlap": false,
          },
          paint: {
            "text-color": "#222",
            "text-halo-width": 1.5,
            "text-halo-color": "#fff",
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
        // The live preview of the next trace segment: routed path to
        // the cursor, thin and dashed so it reads as tentative — it
        // vanishes the moment the click confirms and the committed
        // detour-draft layer takes over.
        map.addSource("detour-preview", { type: "geojson", data: EMPTY_FC });
        map.addLayer({
          id: "detour-preview-line", type: "line", source: "detour-preview",
          filter: ["==", ["geometry-type"], "LineString"],
          layout: { "line-join": "round", "line-cap": "round" },
          paint: { "line-width": 2, "line-color": "#c2185b", "line-opacity": 0.6, "line-dasharray": [2, 2] },
        });
        map.addLayer({
          id: "detour-preview-dot", type: "circle", source: "detour-preview",
          filter: ["==", ["geometry-type"], "Point"],
          paint: { "circle-radius": 4, "circle-color": "#fff", "circle-stroke-width": 2, "circle-stroke-color": "#c2185b" },
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

        // Detour drawing: click to confirm each segment. Between
        // clicks the routed path to the cursor is PREVIEWED (see
        // previewSegment); the mouse never commits anything by
        // itself, so the trace stays under the dispatcher's control.
        // Panning stays ON — MapLibre only fires "click" when the
        // press did not become a drag, so tracing and panning cannot
        // conflict, and clicks over the panel never reach the canvas.
        let previewPx: [number, number] | null = null;
        map.on("mousemove", (e) => {
          if (!app.drawingDetour) return;
          // Recompute only when the cursor actually moved: each step
          // is a graph search inside the capability.
          if (previewPx && Math.hypot(e.point.x - previewPx[0], e.point.y - previewPx[1]) < 3) return;
          previewPx = [e.point.x, e.point.y];
          previewSegment(e);
        });
        map.on("dragstart", () => {
          if (!app.drawingDetour) return;
          previewPx = null;
          src("detour-preview").setData(EMPTY_FC);
        });
        map.on("click", (e) => {
          if (!app.drawingDetour) return;
          commitWaypoint(e);
          previewPx = null;
          src("detour-preview").setData(EMPTY_FC);
        });

        map.on("click", "stops-base", (e) => {
          const stopId = (e.features?.[0]?.properties as Record<string, string> | undefined)?.stop_id;
          // While the diversion editor is armed, stop clicks set
          // anchors; selecting the stop stays useful either way.
          if (!stopId) return;
          if (app.pickingAnchor) setDiversionAnchor(stopId);
          selectStop(stopId);
        });
        // Click a bus for its details. DOM content (not HTML strings):
        // feed ids are untrusted text, never markup. The popup then
        // RIDES with the bus — the vehicles effect re-anchors it on
        // every refresh and drops it when the bus leaves the feed.
        map.on("click", "vehicle-bodies", (e) => {
          const p = e.features?.[0]?.properties as VehicleProps | undefined;
          if (!p) return;
          vehiclePopup?.popup.remove();
          const key = String(p.key ?? "");
          // Anchor at the vehicle's true position (the icon's anchor),
          // not wherever inside the icon the click landed.
          const v = app.live?.live.vehicles.find((x) => x.key === key);
          const popup = new maplibregl.Popup({ offset: 10, maxWidth: "260px" })
            .setLngLat(v ? [v.lon, v.lat] : e.lngLat)
            .setDOMContent(vehiclePopupContent(p))
            .addTo(map);
          vehiclePopup = { popup, key };
          popup.on("close", () => {
            if (vehiclePopup?.popup === popup) vehiclePopup = null;
          });
        });
        for (const layer of ["stops-base", "stops-selected", "vehicle-bodies"]) {
          // While tracing, the crosshair wins — every position on the
          // canvas is a potential waypoint.
          map.on("mouseenter", layer, () => {
            map.getCanvas().style.cursor = app.drawingDetour ? "crosshair" : "pointer";
          });
          map.on("mouseleave", layer, () => {
            map.getCanvas().style.cursor = app.drawingDetour ? "crosshair" : "";
          });
        }

        mapLoaded = true; // the $effects below replay current state
      });
    })();
    return () => {
      // The inspection hook must not outlive the instance: personas
      // remount the map (the planner workspace has its own).
      if (window.__map === map) window.__map = undefined;
      vehiclePopup = null; // goes down with the map instance
      map?.remove();
    };
  });

  // Reactive bindings: each map layer follows exactly one slice of
  // state. This is what makes "TripUpdates can't wipe vehicle dots"
  // structural rather than a convention.

  $effect(() => {
    if (!mapLoaded) return;
    // The stops layer is the host's view over the directory's stop
    // list (what exists in the network, coordinates included) — the
    // feed-side geojson op retired in U6.
    const data = stopsFeatures(directory.stops);
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

  // The map persists across persona switches — so each persona-owned
  // layer DERIVES from the persona: leaving a persona clears its
  // state from the canvas, returning re-draws it. Without this the
  // detour ghost would ride along into the rider view.

  // The diversion editor's route ghost (dispatcher).
  $effect(() => {
    if (!mapLoaded) return;
    const feature = app.persona === "dispatcher" ? app.routeContext : null;
    src("route-context").setData(feature ?? EMPTY_FC);
  });

  // Trace mode: crosshair over the canvas, preview cleared. Panning
  // stays on — clicks confirm segments, drags pan; MapLibre tells
  // them apart, so no mode switch is needed. Ends with the persona
  // too (the map persists; the mode is dispatcher-only).
  $effect(() => {
    if (!mapLoaded) return;
    if (app.drawingDetour && app.persona === "dispatcher") {
      map.getCanvas().style.cursor = "crosshair";
    } else {
      map.getCanvas().style.cursor = "";
      src("detour-preview").setData(EMPTY_FC);
    }
  });

  // The rider's selected trip (rider-owned).
  $effect(() => {
    if (!mapLoaded) return;
    const feature = app.persona === "rider" ? app.tripFeature : null;
    src("trip").setData(feature ?? EMPTY_FC);
    if (feature) {
      const coords = feature.geometry.coordinates as [number, number][];
      map.fitBounds(boundsOf(coords), { padding: 60 });
    }
  });

  type VehicleProps = Record<string, string | number | null | undefined>;
  /// The open bus popup rides with its bus: keyed by vehicle (trip
  /// as fallback), re-anchored and re-rendered by the vehicles effect
  /// on every feed refresh, closed when the bus leaves the feed.
  let vehiclePopup: { popup: maplibregl.Popup; key: string } | null = null;

  /// Popup rows as DOM nodes. Rows with nothing to say stay out —
  /// C-TRAN's vehicle feed, for instance, carries no next stop.
  const vehiclePopupContent = (p: VehicleProps): HTMLDivElement => {
    const content = document.createElement("div");
    content.className = "bus-popup";
    const row = (k: string, v: string): void => {
      const div = document.createElement("div");
      const b = document.createElement("b");
      b.textContent = k;
      div.append(b, document.createTextNode(` ${v}`));
      content.append(div);
    };
    if (p.label_text) {
      const bus = document.createElement("div");
      bus.className = "bus-no";
      bus.textContent = String(p.label_text);
      content.append(bus);
    }
    if (p.trip_id) row("Trip", String(p.trip_id));
    const stopName = p.stop_id
      ? (directory.stops.find((s) => s.stop_id === p.stop_id)?.name ?? String(p.stop_id))
      : undefined;
    if (stopName) row("Next stop", stopName);
    const delayRow = document.createElement("div");
    const delayLabel = document.createElement("b");
    delayLabel.textContent = "Delay";
    const delayValue = document.createElement("span");
    const delay = p.delay == null ? null : Number(p.delay);
    const min = Math.round(Math.abs(delay ?? 0) / 60);
    const [cls, text] = {
      no_prediction: ["none", "no prediction"],
      on_time: ["on", "on time"],
      early: ["late", `${min} min early`],
      late: ["late", `+${min} min late`],
      very_late: ["verylate", `+${min} min late`],
    }[String(p.status) as LiveStatus] ?? ["none", "no prediction"];
    delayValue.className = `delay ${cls}`;
    delayValue.textContent = text;
    delayRow.append(delayLabel, document.createTextNode(" "), delayValue);
    content.append(delayRow);
    if (p.vehicle_id) row("Vehicle", String(p.vehicle_id));
    return content;
  };

  $effect(() => {
    if (!mapLoaded) return;
    // Buses signal their lateness by color — cap-rt's live document
    // carries each vehicle's trip instance, route name, delay, and
    // status; the map only draws.
    const features = (app.live?.live.vehicles ?? []).map((v) => {
      // The rider-visible bus number, and the small label on the map:
      // route badge · bus number (falling back to what exists).
      const busNo = v.label ?? v.vehicle_id;
      const parts = [v.route_name, busNo].filter(Boolean);
      return {
        type: "Feature",
        geometry: { type: "Point", coordinates: [v.lon, v.lat] },
        properties: {
          key: v.key,
          trip_id: v.trip ?? "",
          route_id: v.route ?? "",
          delay: v.delay_secs,
          status: v.status,
          bearing: v.bearing ?? null,
          stop_id: v.next_stop ?? "",
          vehicle_id: v.vehicle_id ?? "",
          label: v.label ?? "",
          label_text: parts.length ? parts.join(" · ") : (v.trip ?? ""),
        },
      };
    });
    src("vehicles").setData({ type: "FeatureCollection", features });
    // The open popup follows its bus: re-anchor to the fresh
    // position and re-render the rows (delays move too); a bus that
    // left the feed takes its popup with it.
    if (vehiclePopup) {
      const open = vehiclePopup;
      const f = features.find((x) => x.properties.key === open.key);
      if (f) {
        open.popup.setLngLat(f.geometry.coordinates as [number, number]);
        open.popup.setDOMContent(vehiclePopupContent(f.properties));
      } else {
        open.popup.remove();
      }
    }
  });

  // The rider's planned itinerary (rider-owned).
  $effect(() => {
    if (!mapLoaded) return;
    const features = app.persona === "rider" ? app.itineraryFeatures : [];
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
  // two of them. Dispatcher-owned. (Circle layers only render Point
  // features, line layers only LineStrings, so one source feeds both
  // layers.)
  $effect(() => {
    if (!mapLoaded) return;
    // cap-divert's flattened trace (the draft view).
    const pts = app.persona === "dispatcher" && app.diversionDraft ? app.diversionView?.detour : undefined;
    const features: any[] = (pts ?? []).map(({ lat, lon }) => ({
      type: "Feature",
      geometry: { type: "Point", coordinates: [lon, lat] },
      properties: {},
    }));
    if (pts && pts.length >= 2) {
      features.push({
        type: "Feature",
        geometry: { type: "LineString", coordinates: pts.map(({ lat, lon }) => [lon, lat] as [number, number]) },
        properties: {},
      });
    }
    src("detour-draft").setData({ type: "FeatureCollection", features });
  });

  // Planner drill-down focus: the selected element, drawn on the map
  // and framed. Planner-owned: empty when another persona is active.
  $effect(() => {
    if (!mapLoaded) return;
    const focus = app.persona === "planner" ? app.plannerFocus : null;
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
