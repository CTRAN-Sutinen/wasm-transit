<script lang="ts">
  /// A hub's just-missed connections (analysis `missed_connections`):
  /// each arrival that sees another route leave before the rider can
  /// make it, by how much, and when that route next leaves — a
  /// frequent route's miss reads as the short wait it is.
  import type { MissedConnection } from "../types/MissedConnection";
  import { clock } from "./designFormat";

  let {
    missed,
    stopName,
    onroute,
  }: { missed: MissedConnection[]; stopName: string; onroute: (route: string) => void } = $props();

  const m = (s: number) => Math.round(s / 60);
</script>

<section class="panel" aria-label="Just missed at {stopName}" data-testid="near-misses">
  <header><h2>Just missed at {stopName}</h2><span class="muted">a bus leaves up to 5 min before another arrives, or under a minute after</span></header>
  {#if missed.length === 0}
    <p class="muted pad">No near misses here on the group's day.</p>
  {:else}
    <div class="scroll">
      <table>
        <thead><tr><th>Arrives</th><th>Misses</th><th>By</th><th>Next one</th></tr></thead>
        <tbody>
          {#each missed as x, i (i)}
            <tr>
              <td><button type="button" class="link" onclick={() => onroute(x.from_route)}>{x.from_route}</button> {clock(x.arrival_secs)}</td>
              <td><button type="button" class="link" onclick={() => onroute(x.to_route)}>{x.to_route}</button> left {clock(x.departure_secs)}</td>
              <td>{m(x.missed_by_secs)} min</td>
              <td>{x.next_departure_secs != null ? `${clock(x.next_departure_secs)}, a ${m(x.next_departure_secs - x.arrival_secs)} min wait` : "none that day"}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; }
  header { padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); display: flex; flex-direction: column; gap: 0.15rem; }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.75rem; }
  .pad { margin: 0; padding: 0.6rem 0.9rem; }
  .scroll { max-height: 14rem; overflow: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 0.8rem; font-variant-numeric: tabular-nums; }
  th, td { text-align: left; padding: 0.3rem 0.9rem; border-bottom: 1px solid var(--line); }
  th { color: var(--muted); font-weight: 500; font-size: 0.7rem; position: sticky; top: 0; background: var(--surface); }
  .link { background: none; border: 0; padding: 0; color: var(--accent); cursor: pointer; font: inherit; font-weight: 600; }
</style>
