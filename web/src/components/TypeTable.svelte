<script lang="ts">
  /// The service GROUPS and what each family costs — cap-analysis's
  /// `service_groups` rows: one row per BASE type with the riders
  /// along folded in (QAF-65: the overlays are really part of the
  /// base's service). RENDER only. The baseline line shows the
  /// comparison plan's row for the SELECTED family beside this
  /// plan's; there is no diff here (a scenario diff is an op the bus
  /// does not answer yet — QAF-20).
  import type { ServiceGroup } from "../types/ServiceGroup";

  let {
    groups,
    baseline,
    baselineName,
    selected,
    onselect,
  }: {
    groups: ServiceGroup[];
    baseline: ServiceGroup[];
    baselineName: string;
    selected: string;
    onselect: (serviceType: string) => void;
  } = $props();

  const current = $derived(groups.find((g) => g.service_type === selected) ?? null);
  const base = $derived(baseline.find((g) => g.service_type === selected) ?? null);
</script>

<section class="type-table" data-testid="type-table">
  <h3>Service groups</h3>
  <table>
    <thead><tr><th>Group</th><th>Day</th><th class="n">Trips</th><th class="n">Routes</th><th class="n">Rev h</th></tr></thead>
    <tbody>
      {#each groups as g (g.service_type)}
        <tr class:sel={g.service_type === selected} onclick={() => onselect(g.service_type)}>
          <td>
            {g.name}
            {#if g.members.length > 1}
              <span class="riders" title={g.members.join(", ")}>+{g.members.length - 1}</span>
            {/if}
          </td>
          <td class="muted">{g.date}</td>
          <td class="n">{g.trips}</td>
          <td class="n">{g.active_routes}</td>
          <td class="n">{g.revenue_hours.toFixed(1)}</td>
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
  td, th { padding: 2px 4px; white-space: nowrap; }
  .n { text-align: right; font-variant-numeric: tabular-nums; }
  tbody tr { cursor: pointer; }
  tbody tr:hover { background: var(--hover); }
  tr.sel { background: var(--accent-soft); }
  .muted { color: var(--muted); }
  .riders {
    margin-left: 0.3rem;
    font-size: 0.7rem;
    color: var(--muted);
    border: 1px solid var(--line);
    border-radius: 0.5rem;
    padding: 0 0.25rem;
  }
  .vs { margin-top: 0.4rem; font-size: 0.85rem; }
  h3 { margin: 0 0 0.25rem; font-size: 0.95rem; }
</style>
