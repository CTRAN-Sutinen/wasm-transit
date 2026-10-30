<script lang="ts">
  /// One route's gaps by hour (its `route_frequency` row): a bar from
  /// the shortest to the longest gap starting in each hour, a tick at
  /// the median where there is one — the target and the range the
  /// trips actually run, side by side.
  import type { RouteFrequency } from "../types/RouteFrequency";

  let {
    frequency,
    route,
    window,
  }: { frequency: RouteFrequency; route: string; window: { from: number; to: number } | null } = $props();

  const row = $derived(frequency.routes.find((r) => r.route_id === route) ?? null);
  const most = $derived(Math.max(15 * 60, ...(row?.hours.map((h) => h.longest_gap_secs ?? 0) ?? [])));
  const pct = (s: number) => (s / most) * 100;
  const inWindow = (h: number) => !!window && h >= window.from && h < window.to;
</script>

<section class="panel" aria-label="Gaps by hour" data-testid="hour-gaps">
  <header>
    <h2>Gaps by hour</h2>
    <span class="muted">{row?.headway_stop ? `at its busiest stop (${row.headway_stop}), both directions` : ""} · bar: shortest to longest, tick: median</span>
  </header>
  {#if row}
    <div class="bars">
      {#each row.hours as h (h.hour)}
        <div
          class="col"
          class:in={inWindow(h.hour)}
          title="{h.hour}:00 — {h.departures} departures{h.shortest_gap_secs != null ? `, gaps ${Math.round(h.shortest_gap_secs / 60)}–${Math.round(h.longest_gap_secs! / 60)} min` : ''}"
        >
          <div class="track">
            {#if h.shortest_gap_secs != null}
              <i class="range" style:bottom="{pct(h.shortest_gap_secs)}%" style:height="max(3px, {pct(h.longest_gap_secs! - h.shortest_gap_secs)}%)"></i>
            {/if}
            {#if h.median_gap_secs != null}<i class="mid" style:bottom="{pct(h.median_gap_secs)}%"></i>{/if}
          </div>
          <span>{h.hour % 24}</span>
        </div>
      {/each}
    </div>
    <p class="muted scale">top of the chart: {Math.round(most / 60)} min</p>
  {:else}
    <p class="muted pad">This route does not run under the group.</p>
  {/if}
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: baseline; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  .pad { padding: 0.8rem 0.9rem; margin: 0; }
  .bars { display: flex; gap: 3px; height: 8rem; padding: 0.6rem 0.9rem 0; }
  .col { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .track { position: relative; flex: 1; }
  .range { position: absolute; left: 22%; right: 22%; background: var(--muted); opacity: 0.45; border-radius: 2px; }
  .col.in .range { background: var(--accent); opacity: 0.7; }
  .mid { position: absolute; left: 0; right: 0; height: 2px; background: var(--text); }
  .col span { font-size: 0.62rem; color: var(--muted); text-align: center; }
  .scale { margin: 0; padding: 0.3rem 0.9rem 0.6rem; }
</style>
