<script lang="ts">
  /// One direction's trips by start (directory `timetable`): the key
  /// each runs under, its running time, its block, and whether the
  /// plan added or moved it — which key a change to a trip reaches.
  import type { Timetable } from "../types/Timetable";
  import { clock } from "./designFormat";

  let {
    timetable,
    selected,
    onselect,
    onblock,
  }: { timetable: Timetable; selected: string; onselect: (key: string) => void; onblock?: (block: string) => void } = $props();
</script>

<section class="panel" aria-label="Trips" data-testid="trip-list">
  <header><h2>Trips</h2><span class="muted">{timetable.trips.length} by start · the key each runs under</span></header>
  <div class="scroll">
    <table>
      <thead><tr><th>Start</th><th>Key</th><th>Runs</th><th>Block</th><th></th></tr></thead>
      <tbody>
        {#each timetable.trips as t (`${t.trip}@${t.start_secs}`)}
          {@const k = `${t.trip}@${t.start_secs}`}
          <tr class:on={selected === k} onclick={() => onselect(k)}>
            <td>{clock(t.start_secs)}</td>
            <td>{t.service_type}</td>
            <td>{Math.round(t.run_secs / 60)} min</td>
            <td class="muted">{#if t.block && onblock}<button type="button" class="blk" title="Open the block on Vehicles & hours" onclick={(e) => { e.stopPropagation(); onblock(t.block!); }}>{t.block}</button>{:else}{t.block ?? "none"}{/if}</td>
            <td class="mark">{t.added ? "added" : t.retimed_from_secs != null ? `moved from ${clock(t.retimed_from_secs)}` : ""}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; gap: 0.6rem; align-items: baseline; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .scroll { max-height: 18rem; overflow: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 0.78rem; font-variant-numeric: tabular-nums; }
  th, td { text-align: left; padding: 0.2rem 0.9rem; border-bottom: 1px solid var(--line); }
  th { color: var(--muted); font-weight: 500; position: sticky; top: 0; background: var(--surface); }
  tr { cursor: pointer; }
  tr.on { background: var(--accent-soft); }
  .mark { color: var(--accent); }
  .muted { color: var(--muted); }
  .blk { font: inherit; border: 0; background: none; padding: 0; color: var(--accent); cursor: pointer; }
</style>
