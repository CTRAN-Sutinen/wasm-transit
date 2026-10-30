<script lang="ts">
  /// Shared corridors (analysis `route_overlap`): route pairs that
  /// share stops on the group's day, with the shared segments and the
  /// stop where their departures stack. Whether the departures are
  /// spread along a corridor has no op yet.
  import type { RouteOverlap } from "../types/RouteOverlap";

  let { overlaps, onroute, onstop }: { overlaps: RouteOverlap[]; onroute: (r: string) => void; onstop: (s: string) => void } = $props();
</script>

<section class="panel" aria-label="Shared corridors" data-testid="corridors">
  <header><h2>Shared corridors</h2><span class="muted">route pairs sharing stops — {overlaps.length} pairs</span></header>
  <div class="scroll">
    <table>
      <thead><tr><th>Routes</th><th class="n">Shared segments</th><th class="n">Shared stops</th><th>Where departures stack</th><th class="n">Combined</th></tr></thead>
      <tbody>
        {#each overlaps as o (`${o.a}>${o.b}`)}
          <tr>
            <td><button type="button" onclick={() => onroute(o.a)}>{o.a}</button> + <button type="button" onclick={() => onroute(o.b)}>{o.b}</button></td>
            <td class="n">{o.shared_segments}</td>
            <td class="n">{o.shared_stops}</td>
            <td>{#if o.stack_stop}<button type="button" class="stop" onclick={() => onstop(o.stack_stop!)}>{o.stack_stop}</button>{/if}</td>
            <td class="n">{o.stack_departures}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: baseline; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  .scroll { overflow: auto; max-height: 60vh; }
  table { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; font-size: 0.85rem; }
  thead th { position: sticky; top: 0; background: var(--surface); text-align: left; font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); font-weight: 500; padding: 0.4rem 0.8rem; border-bottom: 1px solid var(--line); }
  td { padding: 0.35rem 0.8rem; border-bottom: 1px solid var(--line); }
  .n { text-align: right; }
  button { font: inherit; font-family: ui-monospace, monospace; font-weight: 600; border: 0; background: none; padding: 0; color: var(--accent); cursor: pointer; }
  button.stop { font-weight: 400; }
</style>
