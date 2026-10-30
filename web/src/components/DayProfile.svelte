<script lang="ts">
  /// The day's shape (analysis `hourly_trips`): trips starting in each
  /// hour, as bars. Dragging across the bars sets the hour window
  /// every view reads; a click on a single bar selects that hour.
  import type { HourlyTrips } from "../types/HourlyTrips";

  let {
    hours,
    window,
    onwindow,
  }: {
    hours: HourlyTrips[];
    window: { from: number; to: number } | null;
    onwindow: (w: { from: number; to: number } | null) => void;
  } = $props();

  const max = $derived(Math.max(1, ...hours.map((h) => h.trips)));
  let anchor = $state<number | null>(null);
  let hover = $state<number | null>(null);
  const draft = $derived(
    anchor != null && hover != null ? { from: Math.min(anchor, hover), to: Math.max(anchor, hover) + 1 } : null,
  );
  const shown = $derived(draft ?? window);
  const inside = (h: number) => !!shown && h >= shown.from && h < shown.to;
  function end() {
    if (draft) onwindow(draft);
    anchor = null;
    hover = null;
  }
</script>

<svelte:window onpointerup={end} />

<section class="profile" aria-label="Trips per hour — drag to set the hour window" data-testid="day-profile">
  <header>
    <h2>Trips per hour</h2>
    <span class="muted">drag across the bars to set the hours every view reads</span>
    {#if window}<button type="button" onclick={() => onwindow(null)}>Clear hours</button>{/if}
  </header>
  <div class="bars">
    {#each hours as h (h.hour)}
      <button
        type="button"
        class="bar"
        class:in={inside(h.hour)}
        aria-label="{h.hour}:00 — {h.trips} trips"
        title="{h.hour}:00 — {h.trips} trips"
        onpointerdown={(e) => { e.preventDefault(); anchor = h.hour; hover = h.hour; }}
        onpointerenter={() => { if (anchor != null) hover = h.hour; }}
        onkeydown={(e) => { if (e.key === "Enter") onwindow({ from: h.hour, to: h.hour + 1 }); }}
      >
        <i style:height="{(h.trips / max) * 100}%"></i>
        <span>{h.hour % 24}</span>
      </button>
    {/each}
  </div>
</section>

<style>
  .profile { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; user-select: none; }
  header { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: baseline; padding: 0.55rem 0.9rem; }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  header button { margin-left: auto; font: inherit; font-size: 0.78rem; border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--text); padding: 0.15rem 0.5rem; cursor: pointer; }
  .bars { display: flex; gap: 2px; align-items: stretch; height: 5.5rem; padding: 0 0.9rem 0.5rem; }
  .bar { flex: 1; min-width: 0; display: flex; flex-direction: column; justify-content: flex-end; align-items: stretch; border: 0; background: transparent; padding: 0; cursor: ew-resize; font: inherit; }
  .bar i { display: block; background: var(--muted); opacity: 0.55; border-radius: 2px 2px 0 0; min-height: 2px; }
  .bar.in { background: var(--accent-soft); }
  .bar.in i { background: var(--accent); opacity: 1; }
  .bar span { font-size: 0.65rem; color: var(--muted); text-align: center; line-height: 1.3; }
</style>
