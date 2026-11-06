<script lang="ts">
  /// The service groups (analysis `service_groups`): one row per
  /// combination of service types that run together (D-55), with the
  /// dates it holds on and its members' pooled numbers. A row selects
  /// the group every view reads.
  import type { ServiceGroup } from "../types/ServiceGroup";
  import { count, fixed, spansText } from "./designFormat";

  let { groups, selected, onselect }: { groups: ServiceGroup[]; selected: string; onselect: (group: string) => void } = $props();
</script>

<section class="panel" aria-label="Service groups" data-testid="group-table">
  <header><h2>Service groups</h2><span class="muted">the service types that run together, and when</span></header>
  <div class="scroll">
    <table>
      <thead><tr><th>Group</th><th>Dates</th><th class="n">Days</th><th class="n">Trips</th><th class="n">Routes</th><th class="n">Rev. h</th><th class="n">Rev. mi</th></tr></thead>
      <tbody>
        {#each groups as g (g.id)}
          <tr class:sel={g.id === selected} onclick={() => onselect(g.id)}>
            <td><button type="button" onclick={() => onselect(g.id)}>{g.id}</button></td>
            <td class="muted" data-testid="group-dates">{spansText(g.spans)}</td>
            <td class="n">{g.dates.length}</td>
            <td class="n">{count(g.trips)}</td>
            <td class="n">{g.active_routes}</td>
            <td class="n">{fixed(g.revenue_hours)}</td>
            <td class="n">{count(Math.round(g.revenue_miles))}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; gap: 0.6rem; align-items: baseline; padding: 0.7rem 1rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.8rem; }
  .scroll { overflow-x: auto; }
  table { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; }
  th { text-align: left; font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); font-weight: 500; padding: 0.5rem 0.8rem; border-bottom: 1px solid var(--line); }
  td { padding: 0.45rem 0.8rem; border-bottom: 1px solid var(--line); font-size: 0.85rem; cursor: pointer; }
  .n { text-align: right; }
  tr:hover { background: var(--hover); }
  tr.sel { background: var(--accent-soft); }
  td button { font: inherit; font-weight: 600; border: 0; background: none; padding: 0; color: var(--text); cursor: pointer; font-family: ui-monospace, monospace; }
</style>
