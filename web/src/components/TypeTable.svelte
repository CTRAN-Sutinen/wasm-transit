<script lang="ts">
  /// The service types and what each costs — cap-analysis's
  /// `service_types_overview` rows, each on its representative day.
  /// RENDER only. The baseline line shows the comparison plan's row
  /// for the SELECTED type beside this plan's; there is no diff here
  /// (a scenario diff is an op the bus does not answer yet — QAF-20).
  import type { ServiceTypeSummary } from "../types/ServiceTypeSummary";

  let {
    types,
    baseline,
    baselineName,
    selected,
    onselect,
  }: {
    types: ServiceTypeSummary[];
    baseline: ServiceTypeSummary[];
    baselineName: string;
    selected: string;
    onselect: (serviceType: string) => void;
  } = $props();

  const current = $derived(types.find((t) => t.service_type === selected) ?? null);
  const base = $derived(baseline.find((t) => t.service_type === selected) ?? null);
</script>

<section class="type-table" data-testid="type-table">
  <h3>Service types</h3>
  <table>
    <thead><tr><th>Type</th><th>Day</th><th class="n">Trips</th><th class="n">Routes</th><th class="n">Rev h</th></tr></thead>
    <tbody>
      {#each types as t (t.service_type)}
        <tr class:sel={t.service_type === selected} onclick={() => onselect(t.service_type)}>
          <td>{t.name}</td>
          <td class="muted">{t.date}</td>
          <td class="n">{t.trips}</td>
          <td class="n">{t.active_routes}</td>
          <td class="n">{t.revenue_hours.toFixed(1)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#if current && baselineName}
    <div class="vs" data-testid="type-vs">
      <span class="muted">{current.name} vs {baselineName}:</span>
      {#if base}
        trips {base.trips} → <b>{current.trips}</b>, rev h {base.revenue_hours.toFixed(2)} → <b>{current.revenue_hours.toFixed(2)}</b>
      {:else}
        <i>not in the baseline</i>
      {/if}
    </div>
  {/if}
</section>

<style>
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th { text-align: left; font-weight: 600; border-bottom: 1px solid var(--line); }
  td, th { padding: 2px 6px; }
  .n { text-align: right; font-variant-numeric: tabular-nums; }
  tbody tr { cursor: pointer; }
  tbody tr:hover { background: var(--hover); }
  tr.sel { background: var(--accent-soft); }
  .muted { color: var(--muted); }
  .vs { margin-top: 0.4rem; font-size: 0.85rem; }
  h3 { margin: 0 0 0.25rem; font-size: 0.95rem; }
</style>
