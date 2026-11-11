<script lang="ts">
  /// Where riders wait longest (analysis `longest_waits`): every stop
  /// ranked by its longest wait for the next departure on the group's
  /// day — inside the hour window when one is set, counting the wait
  /// from the window's start and to its end. A stop opens its hub.
  import type { StopWait } from "../types/StopWait";
  import { clock } from "./designFormat";

  let {
    waits,
    window,
    onstop,
  }: { waits: StopWait[]; window: { from: number; to: number } | null; onstop: (stop: string) => void } = $props();

  const m = (s: number) => Math.round(s / 60);
</script>

<section class="panel" aria-label="Longest waits" data-testid="longest-waits">
  <header>
    <h2>Longest waits across the network</h2>
    <span class="muted">{window ? `${window.from}:00–${window.to}:00, from the window's start to its end` : "the whole day, first to last departure"} · each bay on its own</span>
  </header>
  {#if waits.length === 0}
    <p class="muted pad">No stop has a wait to read.</p>
  {:else}
    <div class="scroll">
      <table>
        <thead><tr><th>Stop</th><th>Longest wait</th><th>From</th><th>Departures</th></tr></thead>
        <tbody>
          {#each waits as w (w.stop_id)}
            <tr>
              <td><button type="button" class="link" onclick={() => onstop(w.stop_id)}>{w.name || w.stop_id}</button></td>
              <td>{m(w.longest_wait_secs)} min</td>
              <td>{clock(w.wait_from_secs)}–{clock(w.wait_from_secs + w.longest_wait_secs)}</td>
              <td>{w.departures}</td>
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
  .scroll { max-height: 16rem; overflow: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 0.8rem; font-variant-numeric: tabular-nums; }
  th, td { text-align: left; padding: 0.3rem 0.9rem; border-bottom: 1px solid var(--line); }
  th { color: var(--muted); font-weight: 500; font-size: 0.7rem; position: sticky; top: 0; background: var(--surface); }
  .link { background: none; border: 0; padding: 0; color: var(--accent); cursor: pointer; font: inherit; text-align: left; }
</style>
