<script lang="ts">
  /// What a change does — cap-analysis `service_type_compare`: the
  /// proposal against its baseline under one service type, totals and
  /// the routes whose numbers changed, in the op's order. RENDER only:
  /// the deltas shown are the two answered numbers side by side.
  import type { ServiceTypeComparison } from "../types/ServiceTypeComparison";
  import type { RouteStats } from "../types/RouteStats";

  let {
    comparison,
    baselineName,
    selected,
    onselect,
  }: {
    comparison: ServiceTypeComparison;
    baselineName: string;
    selected: string;
    onselect: (routeId: string) => void;
  } = $props();

  function hm(secs: number | null | undefined): string {
    if (secs == null) return "—";
    return `${String(Math.floor(secs / 3600)).padStart(2, "0")}:${String(Math.floor((secs % 3600) / 60)).padStart(2, "0")}`;
  }
  const span = (r: RouteStats | null) => (r ? `${hm(r.first_start_secs)}–${hm(r.last_arrival_secs)}` : "—");
  const every = (r: RouteStats | null) => (r?.typical_headway_secs != null ? `${Math.round(r.typical_headway_secs / 60)}′` : "—");
  const b = $derived(comparison.base);
  const p = $derived(comparison.proposal);
</script>

<section class="compare" data-testid="compare">
  <h3>Against {baselineName} <span class="muted">{comparison.name}</span></h3>
  <p class="totals">
    Trips {b.trips} → <b>{p.trips}</b> · Rev h {b.revenue_hours.toFixed(1)} → <b>{p.revenue_hours.toFixed(1)}</b>
    · Routes {b.routes} → <b>{p.routes}</b>
  </p>
  {#if comparison.routes.length === 0}
    <p class="muted">No route changed ({comparison.unchanged_routes} unchanged).</p>
  {:else}
    <table>
      <thead><tr><th>Route</th><th class="n">Trips</th><th class="n">Rev h</th><th>Span</th><th class="n">Every</th></tr></thead>
      <tbody>
        {#each comparison.routes as r (r.route_id)}
          <tr class:sel={r.route_id === selected} onclick={() => onselect(r.route_id)}>
            <td>{r.name}</td>
            <td class="n">{r.base?.trips ?? "—"} → {r.proposal?.trips ?? "—"}</td>
            <td class="n">{r.base?.revenue_hours.toFixed(1) ?? "—"} → {r.proposal?.revenue_hours.toFixed(1) ?? "—"}</td>
            <td>{span(r.base)} → {span(r.proposal)}</td>
            <td class="n">{every(r.base)} → {every(r.proposal)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="muted">{comparison.unchanged_routes} routes unchanged.</p>
  {/if}
</section>

<style>
  h3 { margin: 0 0 0.25rem; font-size: 0.95rem; }
  .muted { color: var(--muted); font-size: 0.8rem; font-weight: normal; }
  .totals { margin: 0 0 0.25rem; font-size: 0.85rem; }
  table { border-collapse: collapse; width: 100%; font-size: 0.78rem; font-variant-numeric: tabular-nums; }
  th, td { padding: 1px 4px; border-bottom: 1px solid var(--line); text-align: left; white-space: nowrap; }
  .n { text-align: right; }
  tbody tr { cursor: pointer; }
  tbody tr:hover { background: var(--hover); }
  tr.sel { background: var(--accent-soft); }
</style>
