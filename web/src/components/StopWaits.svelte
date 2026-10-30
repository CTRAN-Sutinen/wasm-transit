<script lang="ts">
  /// One stop's waits (analysis `stop_gaps`): the longest gap starting
  /// in each hour as a bar, the hour window marked, and the day's
  /// longest wait named.
  import type { StopGaps } from "../types/StopGaps";
  import { clock } from "./designFormat";

  let { gaps, window }: { gaps: StopGaps; window: { from: number; to: number } | null } = $props();

  const most = $derived(Math.max(1, ...gaps.hourly.map((h) => h.max_gap_secs ?? 0)));
  const inWindow = (h: number) => !!window && h >= window.from && h < window.to;
</script>

<section class="panel" aria-label="Waits at {gaps.name}" data-testid="stop-waits">
  <header><h2>Waits at {gaps.name}</h2></header>
  <div class="bars">
    {#each gaps.hourly as h (h.hour)}
      <div class="col" class:in={inWindow(h.hour)} title="{h.hour}:00 — {h.departures} departures, longest gap {h.max_gap_secs != null ? Math.round(h.max_gap_secs / 60) : '—'} min">
        <i style:height="{((h.max_gap_secs ?? 0) / most) * 100}%"></i>
        <span>{h.hour % 24}</span>
      </div>
    {/each}
  </div>
  <p class="muted">
    {gaps.departures} departures, {clock(gaps.first_departure_secs)}–{clock(gaps.last_departure_secs)}.
    {#if gaps.max_gap_secs != null}Longest wait {Math.round(gaps.max_gap_secs / 60)} min, after {clock(gaps.max_gap_after_secs)}.{/if}
    Bars: the longest gap starting in each hour.
  </p>
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; }
  header { padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.78rem; margin: 0; padding: 0.5rem 0.9rem 0.7rem; }
  .bars { display: flex; gap: 2px; height: 6rem; padding: 0.6rem 0.9rem 0; align-items: stretch; }
  .col { flex: 1; display: flex; flex-direction: column; justify-content: flex-end; min-width: 0; }
  .col i { display: block; background: var(--muted); opacity: 0.55; border-radius: 2px 2px 0 0; min-height: 1px; }
  .col.in i { background: var(--accent); opacity: 1; }
  .col span { font-size: 0.62rem; color: var(--muted); text-align: center; }
</style>
