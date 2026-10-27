<script lang="ts">
  /// The planner's timetable — cap-directory `timetable`: one route,
  /// one direction, one service type, every trip across the
  /// direction's patterns. One row per trip in the op's order, one
  /// column per stop in the op's order; against a baseline the op
  /// marks `added` rows and lists `removed` trips. RENDER only — the
  /// pattern letters index the op's legend, and "timepoints only"
  /// hides the columns the op does not flag.
  import type { Timetable } from "../types/Timetable";

  let {
    timetable,
    baselineName,
    selectedStop,
    onstop,
  }: {
    timetable: Timetable;
    baselineName: string;
    selectedStop: string;
    onstop: (stopId: string) => void;
  } = $props();

  let timepointsOnly = $state(false);
  const columns = $derived(
    timetable.stops.map((s, i) => ({ ...s, i })).filter((s) => !timepointsOnly || s.timepoint),
  );
  const letter = (pattern: string) => {
    const i = timetable.patterns.findIndex((p) => p.pattern === pattern);
    return i < 0 ? "?" : String.fromCharCode(65 + i);
  };
  const added = $derived(timetable.trips.filter((t) => t.added).length);

  function hm(secs: number | null): string {
    if (secs == null) return "";
    return `${String(Math.floor(secs / 3600)).padStart(2, "0")}:${String(Math.floor((secs % 3600) / 60)).padStart(2, "0")}`;
  }
</script>

<section class="timetable" data-testid="timetable">
  <header>
    <h3>
      {timetable.route_name} · {timetable.direction_name} · {timetable.service_type_name}
      <span class="muted">{timetable.trips.length} trips</span>
    </h3>
    <label class="muted"><input type="checkbox" bind:checked={timepointsOnly} /> timepoints only</label>
  </header>
  <ul class="legend">
    {#each timetable.patterns as p, i (p.pattern)}
      <li><b>{String.fromCharCode(65 + i)}</b> {p.name} <span class="muted">{p.trips} trips{p.headway ? " · headway bands" : ""}</span></li>
    {/each}
  </ul>
  {#if baselineName && (added || timetable.removed.length)}
    <p class="delta" data-testid="timetable-delta">
      Against {baselineName}: <span class="add">{added} added</span>,
      <span class="gone">{timetable.removed.length} gone</span>
      {#if timetable.removed.length}
        <span class="muted">({timetable.removed.map((r) => `${hm(r.start_secs)} ${letter(r.pattern)}`).join(", ")})</span>
      {/if}
    </p>
  {/if}
  <div class="grid">
    <table>
      <thead>
        <tr>
          <th class="pat"></th>
          {#each columns as s (s.i)}
            <th class:tp={s.timepoint} class:sel={s.stop === selectedStop} title={s.name}>
              <button onclick={() => onstop(s.stop)}>{s.name}</button>
            </th>
          {/each}
          <th class="blk">Block</th>
        </tr>
      </thead>
      <tbody>
        {#each timetable.trips as t (t.pattern_trips + "@" + t.start_secs)}
          <tr class:added={t.added} data-start={t.start_secs}>
            <td class="pat">{letter(t.pattern)}</td>
            {#each columns as s (s.i)}<td class:sel={s.stop === selectedStop}>{hm(t.times[s.i])}</td>{/each}
            <td class="blk muted">{t.block ?? "—"}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<style>
  .timetable { display: flex; flex-direction: column; min-height: 0; height: 100%; }
  header { display: flex; justify-content: space-between; align-items: baseline; gap: 0.5rem; flex-wrap: wrap; }
  h3 { margin: 0; font-size: 0.95rem; }
  .muted { color: var(--muted); font-size: 0.8rem; font-weight: normal; }
  .legend { list-style: none; margin: 0.25rem 0; padding: 0; font-size: 0.8rem; display: flex; flex-wrap: wrap; gap: 0 1rem; }
  .delta { margin: 0 0 0.25rem; font-size: 0.85rem; }
  .add { color: var(--accent); font-weight: 600; }
  .gone { color: var(--bad); font-weight: 600; }
  .grid { overflow: auto; min-height: 0; flex: 1; border: 1px solid var(--line); }
  table { border-collapse: collapse; font-size: 0.78rem; font-variant-numeric: tabular-nums; }
  th, td { padding: 1px 6px; border-bottom: 1px solid var(--line); text-align: center; white-space: nowrap; }
  thead th {
    position: sticky; top: 0; background: var(--surface); z-index: 1; vertical-align: bottom;
    max-width: 6.5rem; font-weight: normal;
  }
  thead th button {
    writing-mode: vertical-rl; transform: rotate(180deg); max-height: 9rem; overflow: hidden; text-overflow: ellipsis;
    background: none; border: 0; color: inherit; font: inherit; cursor: pointer; padding: 2px 0;
  }
  thead th.tp { font-weight: 600; }
  th.sel, td.sel { background: var(--accent-soft); }
  .pat { position: sticky; left: 0; background: var(--surface); font-weight: 600; }
  thead .pat { z-index: 2; }
  tr.added td { background: var(--accent-soft); }
  tr.added td.pat { box-shadow: inset 3px 0 0 var(--accent); }
  .blk { text-align: left; }
</style>
