<script lang="ts">
  /// The proposal's scorecard under one group (analysis
  /// `service_type_compare`): trips, revenue hours and miles, routes
  /// changed, the vehicles the group's day needs (runcut
  /// `suggest_blocks`, D-46) — and the measures the bus cannot answer
  /// yet, each named with what it waits on.
  import Unavailable from "./Unavailable.svelte";
  import type { ServiceTypeComparison } from "../types/ServiceTypeComparison";
  import type { Suggestion } from "../types/transit_runs/Suggestion";
  import { change, count, fixed } from "./designFormat";

  let {
    comparison,
    vehicles,
    vehiclesError = "",
  }: { comparison: ServiceTypeComparison; vehicles: Suggestion | null; vehiclesError?: string } = $props();
  const b = $derived(comparison.base);
  const p = $derived(comparison.proposal);
</script>

<section class="score" aria-label="Scorecard" data-testid="scorecard">
  <div class="cards">
    <div class="card"><small>Trips</small><b>{change(count(b.trips), count(p.trips))}</b></div>
    <div class="card"><small>Revenue hours</small><b>{change(fixed(b.revenue_hours), fixed(p.revenue_hours))}</b></div>
    <div class="card"><small>Revenue miles</small><b>{change(count(Math.round(b.revenue_miles)), count(Math.round(p.revenue_miles)))}</b></div>
    <div class="card"><small>Routes changed</small><b>{comparison.routes.length}</b><span>of {p.routes}</span></div>
    {#if vehicles}
      <div class="card" data-testid="score-vehicles">
        <small>Peak vehicles</small>
        <b>{change(String(vehicles.authored_peak_vehicles), String(vehicles.peak_vehicles))}</b>
        <span>{vehicles.added_vehicles > 0 ? `+${vehicles.added_vehicles} for trips no block carries, suggested` : vehicles.blocks.length ? "trips no block carries fit under the peak" : "every trip on a block"}{vehicles.unblocked.length ? `; ${vehicles.unblocked.length} trips not blockable` : ""}</span>
      </div>
    {/if}
  </div>
  <div class="blocked">
    {#if vehiclesError}<Unavailable title="Peak vehicles" error={vehiclesError} />{/if}
    <Unavailable title="Hours against budget" reason="Not shown yet. A proposal is measured in platform hours against an hours budget, not dollars; what a budget is, and where it is kept, is not decided." blockers={["D-108"]} />
    <Unavailable title="Coverage gained and lost" reason="Not shown yet. It waits on a rule for matching this plan's service groups to the baseline's when a fork renames or splits a service type." blockers={["D-43", "QAF-20"]} />
    <Unavailable title="Connections made and broken" reason="Not shown yet: which timed connections this proposal makes or breaks against the baseline. Like coverage, it waits on the rule for matching this plan's service groups to the baseline's." blockers={["D-43"]} />
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
