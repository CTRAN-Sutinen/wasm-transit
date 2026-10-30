<script lang="ts">
  /// The selected group's numbers against the baseline (analysis
  /// `service_type_compare`: both sides on their representative
  /// days), with the answers the bus cannot give yet named as such.
  import type { ServiceTypeComparison } from "../types/ServiceTypeComparison";
  import type { ServiceGroup } from "../types/ServiceGroup";
  import { count, fixed, change } from "./designFormat";

  let {
    group,
    comparison,
    baselineName,
  }: { group: ServiceGroup | null; comparison: ServiceTypeComparison | null; baselineName: string } = $props();

  const b = $derived(comparison?.base);
  const p = $derived(comparison?.proposal);
</script>

<section class="ledger" aria-label="Group ledger" data-testid="group-ledger">
  {#if p && b}
    <div class="stat"><small>Trips</small><b>{change(count(b.trips), count(p.trips))}</b></div>
    <div class="stat"><small>Routes</small><b>{change(String(b.routes), String(p.routes))}</b><span class="muted">{comparison!.routes.length} changed</span></div>
    <div class="stat"><small>Revenue hours</small><b>{change(fixed(b.revenue_hours), fixed(p.revenue_hours))}</b></div>
    <div class="stat"><small>Revenue miles</small><b>{change(count(Math.round(b.revenue_miles)), count(Math.round(p.revenue_miles)))}</b></div>
  {:else if group}
    <div class="stat"><small>Trips</small><b>{count(group.trips)}</b></div>
    <div class="stat"><small>Routes</small><b>{group.active_routes}</b></div>
    <div class="stat"><small>Revenue hours</small><b>{fixed(group.revenue_hours)}</b></div>
    <div class="stat"><small>Revenue miles</small><b>{count(Math.round(group.revenue_miles))}</b></div>
  {/if}
  <div class="stat"><small>Peak vehicles</small><b class="muted">—</b><span class="blocker">D-46</span></div>
  <div class="stat"><small>Cost</small><b class="muted">—</b><span class="blocker">D-51</span></div>
  <p class="note muted">
    {#if p && b}against {baselineName}, each on its representative day ({b.date} → {p.date}){:else if group}{group.name} on {group.date}{/if}
  </p>
</section>

<style>
  .ledger { display: flex; flex-wrap: wrap; background: var(--surface); border: 1px solid var(--line); border-radius: 8px; font-variant-numeric: tabular-nums; }
  .stat { flex: 1 1 9rem; padding: 0.6rem 0.9rem; border-right: 1px solid var(--line); display: flex; flex-direction: column; gap: 0.1rem; }
  .stat small { color: var(--muted); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; }
  .stat b { font-size: 1.05rem; font-weight: 600; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  .blocker { font-size: 0.7rem; padding: 0 0.35rem; border: 1px dashed var(--line); border-radius: 4px; color: var(--muted); align-self: flex-start; }
  .note { flex-basis: 100%; margin: 0; padding: 0.35rem 0.9rem; border-top: 1px solid var(--line); }
</style>
