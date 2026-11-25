<script lang="ts">
  import { onMount } from "svelte";
  import * as maplibregl from "maplibre-gl";
  import { pickStyle } from "../basemap";
  import { dateText, timeText } from "./disruptionsFormat";
  import type { DirectoryStop } from "../types/cap_directory/DirectoryStop";
  import type { EntityReach } from "../types/cap_board/EntityReach";
  import type { EntityNames } from "../types/EntityNames";
  import type { InformedView } from "../types/InformedView";
  import type { Reach } from "../types/Reach";

  let {
    reach,
    stops,
    off,
    onstop,
  }: {
    reach: { entities: EntityReach[]; reach: Reach } | null;
    stops: DirectoryStop[];
    off: string;
    onstop: (stop: string) => void;
  } = $props();

  let hovered = $state<number | null>(null);

  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

  function phrase(n: EntityNames): string {
    if (n.agency) return "Every rider: all routes, all stops";
    const parts: string[] = [];
    if (n.trip) {
      const start = n.trip.start_secs != null ? `${timeText(n.trip.start_secs)} trip` : `trip ${n.trip.id}`;
      parts.push(`${n.trip.route ? `Route ${n.trip.route}, ` : ""}the ${start}${n.trip.headsign ? ` to ${n.trip.headsign}` : ""}${n.trip.date ? `, ${dateText(n.trip.date)}` : ""}`);
    }
    if (n.route && !(n.trip && n.trip.route === n.route.name)) parts.push(`Route ${n.route.name}${n.direction ? `, ${n.direction} direction` : ""}`);
    if (n.route_type != null) parts.push(`every route of type ${n.route_type}`);
    if (n.stop) parts.push(`at ${n.stop.name}`);
    return parts.join(" ").replace(/^at /, "At ");
  }

  function only(n: EntityNames): string {
    const named = [n.route || n.route_type != null, !!n.stop, !!n.trip].filter(Boolean).length;
    if (n.agency) return "";
    if (named < 2) {
      if (n.stop && !n.route) return "Every route that stops here.";
      if (n.route && !n.direction) return "Every trip, at every stop of the route.";
      if (n.route && n.direction) return "Every stop, this direction only.";
      return "";
    }
    const not: string[] = [];
    if (n.route && n.stop) not.push(`Not other stops on route ${n.route.name}.`, `Not other routes at ${n.stop.name}.`);
    if (n.trip && n.stop) not.push("Not other stops of the trip.", "Not other trips here.");
    if (n.trip && n.route && !n.stop) not.push("Not other trips of the route.");
    return not.join(" ");
  }

  function selector(v: InformedView): string {
    const f: string[] = [];
    if (v.agency_id != null) f.push(`agency_id: "${v.agency_id}"`);
    if (v.route_id != null) f.push(`route_id: "${v.route_id}"`);
    if (v.route_type != null) f.push(`route_type: ${v.route_type}`);
    if (v.direction_id != null) f.push(`direction_id: ${v.direction_id}`);
    if (v.trip_id != null) {
      const t = [`trip_id: "${v.trip_id}"`];
      if (v.trip_start_date) t.push(`start_date: "${v.trip_start_date}"`);
      if (v.trip_start_time) t.push(`start_time: "${v.trip_start_time}"`);
      f.push(`trip { ${t.join(", ")} }`);
    }
    if (v.stop_id != null) f.push(`stop_id: "${v.stop_id}"`);
    return `{ ${f.join(", ")} }`;
  }

  function reachText(r: Reach): string {
    if (!r.departures) return "Reaches no departures today";
    return `${plural(r.departures, "departure", "departures")} at ${plural(r.stops.length, "stop", "stops")} today`;
  }

  function warnings(e: EntityReach): string[] {
    const w: string[] = [];
    const n = e.names;
    if (e.not_published) w.push(`Not published: ${e.not_published}`);
    if (n.route && !n.route.known) w.push(`Route ${n.route.id} is not in the schedule`);
    if (n.stop && !n.stop.known) w.push(`Stop ${n.stop.id} is not in the schedule`);
    if (n.trip && !n.trip.known) w.push(`Trip ${n.trip.id} is not in the schedule`);
    if (n.stop && n.stop.bays > 0)
      w.push(`${n.stop.name} is a station with ${plural(n.stop.bays, "bay", "bays")}. Buses leave from the bays, so riders waiting at a bay don't see it. Add the bays.`);
    if (n.trip && n.trip.start_secs == null) w.push("Names no single trip instance. GTFS-rt asks for one.");
    if (!e.not_published && e.published && !e.reach.departures && !w.length) w.push("Reaches no departures today");
    return w;
  }

  let container: HTMLDivElement;
  let map: maplibregl.Map | null = null;
  let loaded = $state(false);
  let framedFor = "";
  const ACCENT = "#c2410c";
  const point = (lat: number, lon: number, p: Record<string, unknown>) => ({ type: "Feature", geometry: { type: "Point", coordinates: [lon, lat] }, properties: p });
  const fc = (features: unknown[]) => ({ type: "FeatureCollection", features }) as any;

  onMount(() => {
    let gone = false;
    let sized: ResizeObserver | null = null;
    (async () => {
      const { style, initialView } = await pickStyle();
      if (gone) return;
      map = new maplibregl.Map({ container, style, center: initialView.center, zoom: initialView.zoom });
      map.addControl(new maplibregl.NavigationControl({ showCompass: false }));
      sized = new ResizeObserver(() => container.clientWidth > 50 && map?.resize());
      sized.observe(container);
      map.on("load", () => {
        const m = map!;
        m.addSource("all", { type: "geojson", data: fc([]) });
        m.addSource("reached", { type: "geojson", data: fc([]) });
        m.addLayer({ id: "all", type: "circle", source: "all", minzoom: 12, paint: { "circle-radius": 3.5, "circle-color": "#fff", "circle-stroke-color": "#64748b", "circle-stroke-width": 1 } });
        m.addLayer({
          id: "reached",
          type: "circle",
          source: "reached",
          paint: {
            "circle-radius": ["interpolate", ["linear"], ["get", "departures"], 1, 6, 40, 13],
            "circle-color": ACCENT,
            "circle-opacity": ["case", ["get", "dim"], 0.2, 0.9],
            "circle-stroke-color": "#fff",
            "circle-stroke-width": 1.5,
          },
        });
        for (const layer of ["all", "reached"]) {
          m.on("click", layer, (e) => {
            const id = e.features?.[0]?.properties?.stop;
            if (id) onstop(String(id));
          });
          m.on("mouseenter", layer, () => (m.getCanvas().style.cursor = "pointer"));
          m.on("mouseleave", layer, () => (m.getCanvas().style.cursor = ""));
        }
        loaded = true;
      });
    })();
    return () => {
      gone = true;
      sized?.disconnect();
      map?.remove();
    };
  });

  $effect(() => {
    if (!loaded || !map) return;
    (map.getSource("all") as maplibregl.GeoJSONSource).setData(fc(stops.map((s) => point(s.lat, s.lon, { stop: s.stop_id, name: s.name }))));
  });

  $effect(() => {
    if (!loaded || !map) return;
    const lit = hovered == null ? null : new Set(reach?.entities[hovered]?.reach.stops.map((s) => s.stop) ?? []);
    const all = reach?.reach.stops ?? [];
    (map.getSource("reached") as maplibregl.GeoJSONSource).setData(
      fc(all.map((s) => point(s.lat, s.lon, { stop: s.stop, departures: s.departures, dim: lit ? !lit.has(s.stop) : false }))),
    );
    // Frame the reach when it changes, not on each hover.
    const key = all.map((s) => s.stop).join(",");
    if (!all.length || key === framedFor) return;
    framedFor = key;
    const first: [number, number] = [all[0].lon, all[0].lat];
    const bounds = all.reduce((b, s) => b.extend([s.lon, s.lat]), new maplibregl.LngLatBounds(first, first));
    map.fitBounds(bounds, { padding: 50, maxZoom: 15, duration: 0 });
  });
</script>

<section class="reach" data-testid="alert-reach">
  <h2>What riders see</h2>
  <div class="map" bind:this={container} data-testid="reach-map"></div>
  {#if off}
    <p class="muted">{off}</p>
  {:else if reach}
    <p class="total" data-testid="reach-total">The alert: {reachText(reach.reach)}</p>
    {#if !reach.entities.length}<p class="bad">Add where it applies</p>{/if}
    <ol class="cards">
      {#each reach.entities as e, i (i)}
        <li class:on={hovered === i} onmouseenter={() => (hovered = i)} onmouseleave={() => (hovered = null)} data-testid="reach-card">
          <strong>{phrase(e.names)}</strong>
          {#if only(e.names)}<span class="only">{only(e.names)}</span>{/if}
          <span class="count">{reachText(e.reach)}</span>
          {#if e.published}<code title="As published in the GTFS-rt feed">informed_entity {selector(e.published)}</code>{/if}
          {#each warnings(e) as w (w)}<span class="warn" data-testid="reach-warning">{w}</span>{/each}
        </li>
      {/each}
    </ol>
    <p class="muted rule">Each entity applies on its own. Within one, every field must match.</p>
  {/if}
</section>

<style>
  .reach { display: grid; grid-template-rows: auto minmax(16rem, 45vh) auto; gap: 0.6rem; }
  h2 { font-size: 0.95rem; margin: 0; }
  .map { position: relative; border-radius: 8px; overflow: hidden; border: 1px solid var(--line); }
  .total { margin: 0; font-weight: 600; }
  .cards { list-style: none; margin: 0; padding: 0; display: grid; gap: 0.5rem; }
  .cards li { display: grid; gap: 0.2rem; padding: 0.6rem 0.75rem; border: 1px solid var(--line); border-radius: 8px; }
  .cards li.on { border-color: var(--accent); }
  .only { font-size: 0.88rem; }
  .count { color: var(--muted); font-size: 0.85rem; }
  code { font-size: 0.78rem; color: var(--muted); word-break: break-word; }
  .warn { color: var(--warn); font-size: 0.85rem; }
  .bad { color: var(--bad); margin: 0; }
  .rule { font-size: 0.8rem; margin: 0; }
</style>
