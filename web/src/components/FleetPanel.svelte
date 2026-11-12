<script lang="ts">
  /// What the change needs (runcut `fleet_summary` and `suggest_blocks`
  /// on the day type's date): the peak vehicles and the platform,
  /// revenue and layover hours as blocked, the trips no block carries,
  /// and the suggested blocks that would carry them — adopted with one
  /// event, their edits applied by the host.
  import type { FleetSummary } from "../types/FleetSummary";
  import type { Suggestion } from "../types/transit_runs/Suggestion";
  import { clock, fixed } from "./designFormat";

  let {
    fleet,
    suggestion,
    date,
    framed,
    onadopt,
    onroute,
  }: {
    fleet: FleetSummary | null;
    suggestion: Suggestion | null;
    date: string;
    framed: boolean;
    onadopt: () => void;
    onroute: (route: string) => void;
  } = $props();

  const unassigned = $derived(suggestion ? suggestion.blocks.reduce((n, b) => n + b.trips.length, 0) + suggestion.unblocked.length : 0);
</script>

<section class="fleet" aria-label="Vehicles and hours" data-testid="fleet-panel">
  <p class="muted">As blocked on {date}, a date this day type runs.</p>
  {#if fleet}
    <div class="cards">
      <div><small>Peak vehicles</small><b>{fleet.peak_vehicles}</b><span>{fleet.peak_at_secs != null ? `at ${clock(fleet.peak_at_secs)}` : ""}</span></div>
      <div><small>Platform hours</small><b>{fixed(fleet.platform_hours)}</b><span>{fleet.blocks} blocks</span></div>
      <div><small>Revenue hours</small><b>{fixed(fleet.revenue_hours)}</b></div>
      <div><small>Layover hours</small><b>{fixed(fleet.layover_hours)}</b></div>
    </div>
  {/if}
  {#if suggestion}
    <h3>Trips on no block</h3>
    {#if unassigned === 0}
      <p class="muted" data-testid="all-blocked">Every trip of the day is on a block.</p>
    {:else}
      <p data-testid="unblocked">
        {unassigned} trips are on no block, so no vehicle is counted for them.
        Suggested: {suggestion.blocks.length} blocks, {suggestion.added_vehicles > 0 ? `${suggestion.added_vehicles} more vehicles at the peak (${suggestion.peak_vehicles})` : "no more vehicles at the peak"}.
        {#if suggestion.unblocked.length}{suggestion.unblocked.length} cannot be blocked.{/if}
      </p>
      <ul class="blocks">
        {#each suggestion.blocks as b (b.block_id)}
          <li>
            <strong>{b.block_id}</strong>
            {b.trips.length} trips, {clock(b.trips[0].start_secs)}–{clock(b.trips.at(-1)?.end_secs)},
            {#each [...new Set(b.trips.map((t) => t.route))] as r (r)}<button type="button" class="chip" onclick={() => onroute(r)}>{r}</button>{/each}
          </li>
        {/each}
      </ul>
      <button type="button" class="adopt" disabled={!framed} onclick={onadopt} data-testid="adopt-blocks">Adopt the suggested blocks</button>
    {/if}
  {/if}
</section>

<style>
  .fleet { display: flex; flex-direction: column; gap: 0.6rem; font-size: 0.85rem; }
  .cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr)); border: 1px solid var(--line); border-radius: 8px; background: var(--surface); }
  .cards div { padding: 0.6rem 0.9rem; border-right: 1px solid var(--line); display: flex; flex-direction: column; }
  small { color: var(--muted); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; }
  b { font-size: 1.2rem; font-variant-numeric: tabular-nums; }
  span { color: var(--muted); font-size: 0.75rem; }
  h3 { margin: 0.4rem 0 0; font-size: 0.85rem; }
  p { margin: 0; }
  .blocks { list-style: none; margin: 0; padding: 0; }
  .blocks li { padding: 0.2rem 0; border-bottom: 1px solid var(--line); }
  .chip { font: inherit; font-size: 0.72rem; border: 1px solid var(--accent); color: var(--accent); background: none; border-radius: 4px; margin-left: 0.25rem; padding: 0 0.3rem; cursor: pointer; }
  .adopt { align-self: flex-start; font: inherit; padding: 0.4rem 0.9rem; border-radius: 6px; border: 0; background: var(--accent); color: var(--surface); font-weight: 600; cursor: pointer; }
  .adopt:disabled { opacity: 0.5; cursor: default; }
  .muted { color: var(--muted); }
</style>
