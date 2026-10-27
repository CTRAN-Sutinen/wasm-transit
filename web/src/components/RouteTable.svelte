<script lang="ts">
  /// The routes running under one service type — the `routes` rows of
  /// cap-analysis `service_type_detail`, in the op's order. RENDER
  /// only: spans and headways are formatted, never derived.
  import type { RouteStats } from "../types/RouteStats";

  let {
    routes,
    typeName,
    date,
    selected,
    onselect,
  }: {
    routes: RouteStats[];
    typeName: string;
    date: string;
    selected: string;
    onselect: (routeId: string) => void;
  } = $props();

  function hm(secs: number | null): string {
    if (secs == null) return "—";
    return `${String(Math.floor(secs / 3600)).padStart(2, "0")}:${String(Math.floor((secs % 3600) / 60)).padStart(2, "0")}`;
  }
</script>

<section class="route-table" data-testid="route-table">
  <h3>Routes — {typeName} <span class="muted">({date})</span></h3>
  <div class="wb-scroll">
  <table>
    <thead>
      <tr><th>Route</th><th class="n">Trips</th><th class="n">Rev h</th><th>Span</th><th class="n">Every</th><th class="n">Stops</th></tr>
    </thead>
    <tbody>
      {#each routes as r (r.route_id)}
        <tr class:sel={r.route_id === selected} onclick={() => onselect(r.route_id)}>
          <td>{r.name}{#if r.name !== r.route_id}<span class="muted"> {r.route_id}</span>{/if}</td>
          <td class="n">{r.trips}</td>
          <td class="n">{r.revenue_hours.toFixed(1)}</td>
          <td class="span">{hm(r.first_start_secs)}–{hm(r.last_arrival_secs)}</td>
          <td class="n">{r.typical_headway_secs == null ? "—" : `${Math.round(r.typical_headway_secs / 60)}′`}</td>
          <td class="n">{r.stops}</td>
        </tr>
      {:else}
        <tr><td colspan="6" class="muted">No routes run under this type.</td></tr>
      {/each}
    </tbody>
  </table>
  </div>
</section>

<style>
  table { width: 100%; }
  .route-table { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .wb-scroll { flex: 1; }
  th { text-align: left; font-weight: 600; border-bottom: 1px solid var(--line); }
  td, th { padding: 2px 6px; font-size: 0.85rem; white-space: nowrap; }
  td { border-bottom: 1px solid var(--line); }
  .n { text-align: right; font-variant-numeric: tabular-nums; }
  .span { font-variant-numeric: tabular-nums; white-space: nowrap; }
  tbody tr { cursor: pointer; }
  tbody tr:hover { background: var(--hover); }
  tr.sel { background: var(--accent-soft); }
  .muted { color: var(--muted); }
  h3 { margin: 0 0 0.25rem; font-size: 0.95rem; }
</style>
