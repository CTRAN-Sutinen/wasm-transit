<script lang="ts">
  /// Each trip's running time by its start (directory `timetable`'s
  /// `run_secs`): a dot per trip, added trips in blue — where the day
  /// runs slower, and a trip out of step with its band.
  import type { Timetable } from "../types/Timetable";
  import { clock } from "./designFormat";

  let { timetable }: { timetable: Timetable } = $props();

  const W = 400;
  const H = 130;
  const bounds = $derived.by(() => {
    const starts = timetable.trips.map((t) => t.start_secs);
    const runs = timetable.trips.map((t) => t.run_secs / 60);
    const lo = Math.floor(Math.min(...starts, 6 * 3600) / 3600) * 3600;
    const hi = Math.ceil(Math.max(...starts, lo + 3600) / 3600) * 3600;
    const rlo = Math.max(0, Math.floor(Math.min(...runs, 10) / 5) * 5);
    const rhi = Math.ceil(Math.max(...runs, rlo + 5) / 5) * 5;
    return { lo, hi, rlo, rhi };
  });
  const x = (s: number) => 8 + ((s - bounds.lo) / (bounds.hi - bounds.lo)) * (W - 16);
  const y = (m: number) => H - 8 - ((m - bounds.rlo) / (bounds.rhi - bounds.rlo)) * (H - 16);
  const lines = $derived(Array.from({ length: (bounds.rhi - bounds.rlo) / 5 + 1 }, (_, i) => bounds.rlo + i * 5));
</script>

<section class="panel" aria-label="Running time by start" data-testid="run-times">
  <header><h2>Running time by start</h2><span class="muted">end to end, minutes; gridlines every 5</span></header>
  <svg viewBox="0 0 {W} {H}" role="img" aria-label="Each trip's running time by its start">
    {#each lines as m (m)}<line x1="0" x2={W} y1={y(m)} y2={y(m)}></line><text x="2" y={y(m) - 2}>{m}</text>{/each}
    {#each timetable.trips as t (`${t.trip}@${t.start_secs}`)}
      <circle cx={x(t.start_secs)} cy={y(t.run_secs / 60)} r="2.6" class:added={t.added}><title>{t.trip} {clock(t.start_secs)} — {Math.round(t.run_secs / 60)} min</title></circle>
    {/each}
  </svg>
  <div class="ticks"><span>{clock(bounds.lo)}</span><span>{clock(bounds.hi)}</span></div>
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: baseline; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  svg { display: block; width: 100%; height: 9rem; padding: 0.4rem 0.9rem 0; box-sizing: border-box; }
  line { stroke: var(--line); }
  text { font-size: 9px; fill: var(--muted); }
  circle { fill: var(--text); opacity: 0.75; }
  circle.added { fill: #2563eb; opacity: 1; }
  .ticks { display: flex; justify-content: space-between; padding: 0 0.9rem 0.5rem; font-size: 0.68rem; color: var(--muted); }
</style>
