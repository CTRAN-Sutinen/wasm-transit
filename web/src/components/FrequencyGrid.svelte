<script lang="ts">
  /// The frequency grid (analysis `route_frequency`): each route a
  /// row, each service hour a cell — the longest gap starting in the
  /// hour, and the shortest beneath it when the trips vary. The gaps
  /// are the trips' own; nothing is rounded to a grid. Shading
  /// deepens continuously as the longest wait shortens — no tier
  /// boundaries (D-52 is open). The route's numbers ride at the right;
  /// a row opens the route.
  import type { RouteFrequency } from "../types/RouteFrequency";
  import type { RouteHourGaps } from "../types/RouteHourGaps";
  import { clock, count, fixed } from "./designFormat";

  let {
    frequency,
    window,
    changed,
    selected,
    onroute,
  }: {
    frequency: RouteFrequency;
    window: { from: number; to: number } | null;
    changed: string[];
    selected: string;
    onroute: (route: string) => void;
  } = $props();

  const hours = $derived.by(() => {
    const all = frequency.routes.flatMap((r) => r.hours.map((h) => h.hour));
    if (!all.length) return [];
    const [lo, hi] = [Math.min(...all), Math.max(...all)];
    return Array.from({ length: hi - lo + 1 }, (_, i) => lo + i);
  });
  const inWindow = (h: number) => !!window && h >= window.from && h < window.to;
  /// Strength 0..1 from the longest wait: 10 min or less is full,
  /// 75 min or more is none — drawing, not a standard.
  const strength = (secs: number) => Math.max(0, Math.min(1, (75 * 60 - secs) / (65 * 60)));
  function cell(h: RouteHourGaps | undefined) {
    if (!h) return { text: "", sub: "", bg: "transparent", fg: "var(--muted)", title: "" };
    if (h.longest_gap_secs == null) {
      return { text: "·", sub: `${h.departures} dep`, bg: "transparent", fg: "var(--muted)", title: `${h.departures} departures, no gap starts in the hour` };
    }
    const s = strength(h.longest_gap_secs);
    const longest = Math.round(h.longest_gap_secs / 60);
    const shortest = Math.round((h.shortest_gap_secs ?? h.longest_gap_secs) / 60);
    return {
      text: String(longest),
      sub: shortest !== longest ? `from ${shortest}` : "",
      bg: `color-mix(in oklab, #1d4ed8 ${Math.round(s * 100)}%, var(--surface))`,
      fg: s > 0.55 ? "#fff" : "var(--text)",
      title: `${h.hour}:00 — ${h.departures} departures; gaps ${shortest}–${longest} min${h.median_gap_secs != null ? `, median ${Math.round(h.median_gap_secs / 60)}` : ""}`,
    };
  }
</script>

<section class="grid-panel" aria-label="Frequency by hour" data-testid="frequency-grid">
  <header>
    <h2>Frequency by hour</h2>
    <span class="muted">each cell: the longest wait in minutes (gaps starting in the hour, at the route's busiest stop) — the shortest beneath it when trips vary</span>
  </header>
  <div class="scroll">
    <table style:--cols={hours.length}>
      <thead>
        <tr>
          <th class="route">Route</th>
          {#each hours as h (h)}<th class="hr" class:win={inWindow(h)}>{h % 24}</th>{/each}
          <th class="n">Trips</th><th class="n">Span</th><th class="n">Rev h</th><th class="n">Rev mi</th>
        </tr>
      </thead>
      <tbody>
        {#each frequency.routes as r (r.route_id)}
          {@const byHour = new Map(r.hours.map((h) => [h.hour, h]))}
          <tr class:changed={changed.includes(r.route_id)} class:sel={r.route_id === selected} onclick={() => onroute(r.route_id)}>
            <th class="route" scope="row">
              <button type="button" onclick={() => onroute(r.route_id)}>{r.name}</button>
              {#if changed.includes(r.route_id)}<span class="dot" title="changed against the baseline"></span>{/if}
            </th>
            {#each hours as h (h)}
              {@const c = cell(byHour.get(h))}
              <td class="hr" class:win={inWindow(h)} title={c.title}>
                <span class="cell" style:background={c.bg} style:color={c.fg}><b>{c.text}</b>{#if c.sub}<i>{c.sub}</i>{/if}</span>
              </td>
            {/each}
            <td class="n">{count(r.stats.trips)}</td>
            <td class="n muted">{clock(r.stats.first_start_secs)}–{clock(r.stats.last_arrival_secs)}</td>
            <td class="n">{fixed(r.stats.revenue_hours)}</td>
            <td class="n">{count(Math.round(r.stats.revenue_miles))}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <footer class="muted">
    <span class="key"><i style:background="color-mix(in oklab, #1d4ed8 100%, var(--surface))"></i>≤ 10 min</span>
    <span class="key"><i style:background="color-mix(in oklab, #1d4ed8 50%, var(--surface))"></i>≈ 40</span>
    <span class="key"><i style:background="var(--surface)" style:border="1px solid var(--line)"></i>≥ 75</span>
    <span class="key"><i class="changed-key"></i>changed against the baseline</span>
    <span>· one departure, no gap in the hour</span>
  </footer>
</section>

<style>
  .grid-panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: baseline; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  .scroll { overflow: auto; max-height: 70vh; }
  table { border-collapse: separate; border-spacing: 0; width: 100%; font-variant-numeric: tabular-nums; font-size: 0.8rem; }
  thead th { position: sticky; top: 0; z-index: 2; background: var(--surface); font-weight: 500; color: var(--muted); font-size: 0.7rem; padding: 0.35rem 0.2rem; border-bottom: 1px solid var(--line); }
  th.win { color: var(--accent); background: var(--accent-soft); font-weight: 700; }
  .route { position: sticky; left: 0; z-index: 1; background: var(--surface); text-align: left; padding: 0 0.6rem; white-space: nowrap; min-width: 6rem; }
  thead .route { z-index: 3; }
  .route button { font: inherit; font-weight: 600; border: 0; background: none; padding: 0; cursor: pointer; color: var(--text); font-family: ui-monospace, monospace; }
  td.hr { padding: 1px; min-width: 2.3rem; }
  td.hr.win { background: var(--accent-soft); }
  .cell { display: flex; flex-direction: column; align-items: center; justify-content: center; height: 2.1rem; border-radius: 3px; line-height: 1.05; }
  .cell b { font-size: 0.8rem; }
  .cell i { font-style: normal; font-size: 0.62rem; opacity: 0.85; }
  td.n { text-align: right; padding: 0 0.5rem; white-space: nowrap; border-bottom: 1px solid var(--line); }
  th.n { text-align: right; padding: 0.35rem 0.5rem; }
  tbody tr { cursor: pointer; }
  tbody tr:hover .route, tbody tr:hover td.n { background: var(--hover); }
  tr.sel .route { color: var(--accent); }
  tr.changed .route { box-shadow: inset 3px 0 0 var(--accent); }
  .dot { display: inline-block; width: 0.45rem; height: 0.45rem; border-radius: 50%; background: var(--accent); margin-left: 0.3rem; }
  footer { display: flex; flex-wrap: wrap; gap: 0.9rem; align-items: center; padding: 0.5rem 0.9rem; border-top: 1px solid var(--line); }
  .key { display: inline-flex; gap: 0.3rem; align-items: center; }
  .key i { display: inline-block; width: 0.9rem; height: 0.9rem; border-radius: 3px; }
  .changed-key { box-shadow: inset 3px 0 0 var(--accent); border: 1px solid var(--line); }
</style>
