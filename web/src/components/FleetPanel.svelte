<script lang="ts">
  /// What the change needs (runcut `fleet_summary` and `suggest_blocks`
  /// on the service combination: the trips its service keys run): the peak vehicles and the platform,
  /// revenue and layover hours as blocked, the trips no block carries,
  /// and the suggested blocks that would carry them — adopted with one
  /// event, their edits applied by the host. The blocks themselves: each
  /// running block's span drawn on the day (the fleet summary's
  /// outlines), and one opened to its chain (runcut `block_trips`) —
  /// the trips in order and the layover after each.
  import type { FleetSummary } from "../types/FleetSummary";
  import type { Suggestion } from "../types/transit_runs/Suggestion";
  import type { BlockChainTrip } from "../types/BlockChainTrip";
  import { clock, fixed, mins } from "./designFormat";

  let {
    fleet,
    suggestion,
    framed,
    onadopt,
    onroute,
    block = "",
    blockTrips = null,
    blockError = "",
    onblock,
    stopName = (id: string) => id,
  }: {
    fleet: FleetSummary | null;
    suggestion: Suggestion | null;
    framed: boolean;
    onadopt: () => void;
    onroute: (route: string) => void;
    block?: string;
    blockTrips?: BlockChainTrip[] | null;
    blockError?: string;
    onblock?: (block: string) => void;
    /// A stop's name as the directory lists it (its id when unknown).
    stopName?: (id: string) => string;
  } = $props();

  // The day's axis: the first block out to the last one in, on the hour.
  const spans = $derived(fleet?.spans ?? []);
  const axisFrom = $derived(spans.length ? Math.floor(Math.min(...spans.map((b) => b.first_start_secs)) / 3600) * 3600 : 0);
  const axisTo = $derived(spans.length ? Math.ceil(Math.max(...spans.map((b) => b.last_end_secs)) / 3600) * 3600 : 3600);
  const pct = (secs: number) => ((secs - axisFrom) / Math.max(1, axisTo - axisFrom)) * 100;
  const hours = $derived(Array.from({ length: (axisTo - axisFrom) / 3600 + 1 }, (_, i) => axisFrom + i * 3600));
  let filter = $state("");
  // The open block, wherever it was opened from, scrolled into view.
  function reveal(node: HTMLElement, on: boolean) {
    if (on) node.scrollIntoView({ block: "start" });
    return { update: (now: boolean) => { if (now) node.scrollIntoView({ block: "start" }); } };
  }
  const shown = $derived(filter ? spans.filter((b) => b.block_id.toLowerCase().includes(filter.toLowerCase())) : spans);

  const unassigned = $derived(suggestion ? suggestion.blocks.reduce((n, b) => n + b.trips.length, 0) + suggestion.unblocked.length : 0);
</script>

<section class="fleet" aria-label="Vehicles and hours" data-testid="fleet-panel">
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
  {#if spans.length}
    <div class="blockhead">
      <h3>Blocks <span class="muted">{spans.length} out; a bar is one vehicle, out to in. Open one for its trips.</span></h3>
      <input type="search" bind:value={filter} placeholder="Find a block…" aria-label="Find a block" />
    </div>
    <div class="gantt" data-testid="blocks">
      <div class="axis"><span></span><div class="ticks">{#each hours as h (h)}<i style="left:{pct(h)}%">{clock(h).replace(/:00$/, "")}</i>{/each}</div><span></span></div>
      {#each shown as b (b.block_id)}
        <button type="button" class="brow" use:reveal={b.block_id === block} class:on={b.block_id === block} aria-expanded={b.block_id === block} onclick={() => onblock?.(b.block_id)}>
          <span class="bid">{b.block_id}</span>
          <span class="track">
            <span class="bar" style="left:{pct(b.first_start_secs)}%; width:{pct(b.last_end_secs) - pct(b.first_start_secs)}%"
              title="{clock(b.first_start_secs)}–{clock(b.last_end_secs)}"></span>
            {#if b.longest_gap_secs != null && b.longest_gap_at_secs != null && b.longest_gap_secs >= 1800}
              <span class="gap" style="left:{pct(b.longest_gap_at_secs)}%; width:{pct(b.longest_gap_at_secs + b.longest_gap_secs) - pct(b.longest_gap_at_secs)}%"
                title="longest gap {mins(b.longest_gap_secs)} min from {clock(b.longest_gap_at_secs)}"></span>
            {/if}
          </span>
          <span class="bnum">{b.trips} trips · {clock(b.first_start_secs)}–{clock(b.last_end_secs)} · {fixed(b.span_secs / 3600)} h</span>
        </button>
        {#if b.block_id === block}
          <div class="chain" data-testid="block-chain">
            {#if blockError}<p class="bad">Could not read the block: {blockError}</p>
            {:else if !blockTrips}<p class="muted">Reading…</p>
            {:else}
              <table>
                <thead><tr><th>Route</th><th>Start</th><th>End</th><th>From</th><th>To</th><th>Layover after</th></tr></thead>
                <tbody>
                  {#each blockTrips as t, i (`${t.trip}|${t.start_secs}|${i}`)}
                    <tr class:off={!t.runs}>
                      <td><button type="button" class="chip" onclick={() => onroute(t.route)}>{t.route_name || t.route}</button></td>
                      <td>{clock(t.start_secs)}</td>
                      <td>{t.runs ? clock(t.end_secs) : "does not run"}</td>
                      <td title={t.first_stop}>{stopName(t.first_stop)}</td>
                      <td title={t.last_stop}>{stopName(t.last_stop)}</td>
                      <td>{t.layover_after_secs != null ? `${mins(t.layover_after_secs)} min` : ""}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
          </div>
        {/if}
      {/each}
    </div>
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
  h3 .muted { font-weight: 400; font-size: 0.78rem; }
  .bad { color: var(--bad); margin: 0; }
  .blockhead { display: flex; gap: 1rem; align-items: baseline; justify-content: space-between; }
  .blockhead input { font: inherit; font-size: 0.8rem; padding: 0.2rem 0.4rem; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: var(--text); }
  .gantt { border: 1px solid var(--line); border-radius: 8px; background: var(--surface); max-height: 60vh; overflow: auto; }
  .axis, .brow { display: grid; grid-template-columns: 7rem 1fr 15rem; gap: 0.6rem; align-items: center; }
  .brow { scroll-margin-top: 1.6rem; }
  .axis { position: sticky; top: 0; background: var(--surface); border-bottom: 1px solid var(--line); height: 1.4rem; z-index: 1; }
  .ticks, .track { position: relative; height: 100%; }
  .ticks i { position: absolute; top: 0.2rem; transform: translateX(-50%); font-style: normal; font-size: 0.65rem; color: var(--muted); }
  .brow { width: 100%; font: inherit; font-size: 0.78rem; border: 0; border-bottom: 1px solid var(--line); background: none; color: var(--text); padding: 0.15rem 0.6rem; height: 1.6rem; cursor: pointer; text-align: left; }
  .brow:hover, .brow.on { background: var(--accent-soft); }
  .bid { font-variant-numeric: tabular-nums; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bnum { color: var(--muted); font-variant-numeric: tabular-nums; white-space: nowrap; }
  .track { height: 0.7rem; }
  .bar { position: absolute; top: 0; bottom: 0; background: var(--accent); border-radius: 3px; }
  .gap { position: absolute; top: 0; bottom: 0; background: var(--surface); opacity: 0.75; }
  .chain { padding: 0.4rem 0.6rem 0.6rem 7.6rem; border-bottom: 1px solid var(--line); }
  .chain table { border-collapse: collapse; font-size: 0.78rem; }
  .chain th, .chain td { text-align: left; padding: 0.15rem 0.9rem 0.15rem 0; border-bottom: 1px solid var(--line); font-variant-numeric: tabular-nums; }
  .chain th { color: var(--muted); font-weight: 500; }
  tr.off td { color: var(--muted); }
</style>
