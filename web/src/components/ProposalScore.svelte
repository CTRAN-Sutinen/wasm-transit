<script lang="ts">
  /// The proposal's scorecard under one group (analysis
  /// `service_type_compare`): trips, revenue hours and miles, routes
  /// changed — and the measures the bus cannot answer yet, each named
  /// with what it waits on.
  import Unavailable from "./Unavailable.svelte";
  import type { ServiceTypeComparison } from "../types/ServiceTypeComparison";
  import { change, count, fixed } from "./designFormat";

  let { comparison }: { comparison: ServiceTypeComparison } = $props();
  const b = $derived(comparison.base);
  const p = $derived(comparison.proposal);
</script>

<section class="score" aria-label="Scorecard" data-testid="scorecard">
  <div class="cards">
    <div class="card"><small>Trips</small><b>{change(count(b.trips), count(p.trips))}</b></div>
    <div class="card"><small>Revenue hours</small><b>{change(fixed(b.revenue_hours), fixed(p.revenue_hours))}</b></div>
    <div class="card"><small>Revenue miles</small><b>{change(count(Math.round(b.revenue_miles)), count(Math.round(p.revenue_miles)))}</b></div>
    <div class="card"><small>Routes changed</small><b>{comparison.routes.length}</b><span>of {p.routes}</span></div>
  </div>
  <div class="blocked">
    <Unavailable title="Peak vehicles" reason="Not shown yet. Trips a change adds are on no block, so the vehicles this proposal needs are not counted here." blockers={["D-46", "QAF-3"]} />
    <Unavailable title="Cost" reason="Not shown yet. Costing needs the agency's rates per hour, mile, and vehicle, which this software does not hold." blockers={["D-51"]} />
    <Unavailable title="Coverage gained and lost" reason="Not shown yet. It waits on a rule for matching this plan's service groups to the baseline's when a fork renames or splits a service type." blockers={["D-43", "QAF-20"]} />
    <Unavailable title="Connections made and broken" reason="Not available yet: which timed connections this proposal makes or breaks against the baseline." blockers={["Q19"]} />
  </div>
</section>

<style>
  .score { display: flex; flex-direction: column; gap: 0.6rem; }
  .cards { display: flex; flex-wrap: wrap; background: var(--surface); border: 1px solid var(--line); border-radius: 8px; font-variant-numeric: tabular-nums; }
  .card { flex: 1 1 10rem; padding: 0.7rem 1rem; border-right: 1px solid var(--line); }
  .card small { display: block; color: var(--muted); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; }
  .card b { display: block; font-size: 1.25rem; margin: 0.1rem 0; }
  .card span { color: var(--muted); font-size: 0.78rem; }
  .blocked { display: grid; grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr)); gap: 0.6rem; }
</style>
