<script lang="ts">
  /// The redline (analysis `service_type_compare`): every route the
  /// proposal changes under the group, before → after for trips,
  /// span, typical gap, and revenue hours and miles.
  import type { ServiceTypeComparison } from "../types/ServiceTypeComparison";
  import { change, clock, count, fixed, mins } from "./designFormat";

  let { comparison, onroute }: { comparison: ServiceTypeComparison; onroute: (route: string) => void } = $props();
</script>

<section class="panel" aria-label="What changes, route by route" data-testid="redline">
  <header>
    <h2>What changes, route by route</h2>
    <span class="muted">{comparison.name} · {comparison.routes.length} changed, {comparison.unchanged_routes} unchanged</span>
  </header>
  {#if comparison.routes.length}
    <div class="scroll">
      <table>
        <thead><tr><th>Route</th><th class="n">Trips</th><th class="n">Span</th><th class="n">Typical gap</th><th class="n">Rev. h</th><th class="n">Rev. mi</th></tr></thead>
        <tbody>
          {#each comparison.routes as r (r.route_id)}
            <tr>
              <td><button type="button" onclick={() => onroute(r.route_id)}>{r.name}</button>{#if !r.base} <span class="tag">new</span>{:else if !r.proposal} <span class="tag">gone</span>{/if}</td>
              <td class="n">{change(count(r.base?.trips), count(r.proposal?.trips))}</td>
              <td class="n">{change(`${clock(r.base?.first_start_secs)}–${clock(r.base?.last_arrival_secs)}`, `${clock(r.proposal?.first_start_secs)}–${clock(r.proposal?.last_arrival_secs)}`)}</td>
              <td class="n">{change(mins(r.base?.typical_headway_secs), mins(r.proposal?.typical_headway_secs))} min</td>
              <td class="n">{change(fixed(r.base?.revenue_hours), fixed(r.proposal?.revenue_hours))}</td>
              <td class="n">{change(count(r.base ? Math.round(r.base.revenue_miles) : null), count(r.proposal ? Math.round(r.proposal.revenue_miles) : null))}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="muted foot">A whole-day typical gap can hide an evening change — the Network view's hour cells show it.</p>
  {:else}
    <p class="muted foot">This plan changes no route under {comparison.name}.</p>
  {/if}
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: baseline; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  .foot { margin: 0; padding: 0.5rem 0.9rem; }
  .scroll { overflow: auto; max-height: 55vh; }
  table { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; font-size: 0.85rem; }
  th { text-align: left; font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); font-weight: 500; padding: 0.4rem 0.8rem; border-bottom: 1px solid var(--line); }
  td { padding: 0.45rem 0.8rem; border-bottom: 1px solid var(--line); }
  .n { text-align: right; white-space: nowrap; }
  button { font: inherit; font-weight: 600; font-family: ui-monospace, monospace; border: 0; background: none; padding: 0; color: var(--accent); cursor: pointer; }
  .tag { font-size: 0.7rem; padding: 0 0.3rem; border: 1px solid var(--line); border-radius: 4px; }
</style>
