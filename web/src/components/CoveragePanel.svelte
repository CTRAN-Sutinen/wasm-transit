<script lang="ts">
  /// Who the network reaches (analysis `type_coverage`, judged in the
  /// hour window when one is set) with the op's parameters as inputs,
  /// and the unserved points most people first (`unserved_points`).
  /// The selected group's members are marked; coverage answers per
  /// service id (QAF-89).
  import type { TypeCoverage } from "../types/TypeCoverage";
  import type { UnservedPoint } from "../types/UnservedPoint";
  import { count } from "./designFormat";

  type Params = { radius_m: number; frequent_secs: number; min_departures: number };
  let {
    coverage,
    unserved,
    members,
    layer,
    params,
    window,
    selected,
    onparams,
    onlayer,
    onselect,
  }: {
    coverage: TypeCoverage[] | null;
    unserved: UnservedPoint[] | null;
    members: string[];
    layer: { points: number; total_pop: number; pop_field: string } | null;
    params: Params;
    window: { from: number; to: number } | null;
    selected: number | null;
    onparams: (p: Params) => void;
    onlayer: (file: File) => void;
    onselect: (i: number) => void;
  } = $props();

  const km = (m: number) => (m >= 1000 ? `${(m / 1000).toFixed(1)} km` : `${Math.round(m)} m`);
  const set = (patch: Partial<Params>) => onparams({ ...params, ...patch });
</script>

<aside class="panel" aria-label="Coverage" data-testid="coverage-panel">
  <section>
    <h2>Population layer</h2>
    {#if layer}
      <p class="muted">{count(layer.points)} points · {count(Math.round(layer.total_pop))} people ({layer.pop_field})</p>
    {:else}
      <p class="muted">Coverage needs a weighted point layer (GeoJSON).</p>
    {/if}
    <input type="file" accept=".geojson,.json" onchange={(e) => { const f = e.currentTarget.files?.[0]; if (f) onlayer(f); }} data-testid="layer-input" />
  </section>
  <section>
    <h2>Parameters</h2>
    <label>Walk radius (m)<input type="number" min="50" step="50" value={params.radius_m} onchange={(e) => set({ radius_m: Number(e.currentTarget.value) })} /></label>
    <label>Frequent: a bus every (min)<input type="number" min="1" value={params.frequent_secs / 60} onchange={(e) => set({ frequent_secs: Number(e.currentTarget.value) * 60 })} /></label>
    <label>Min departures<input type="number" min="1" value={params.min_departures} onchange={(e) => set({ min_departures: Number(e.currentTarget.value) })} /></label>
    <p class="muted">{window ? `Frequency judged ${window.from}:00–${window.to}:00 (the hours on the bar).` : "Frequency judged over the whole day — set hours on Network to narrow it."}</p>
  </section>
  {#if coverage}
    <section>
      <h2>Served, by service id</h2>
      <table>
        <thead><tr><th>Type</th><th class="n">Served</th><th class="n">Frequent</th></tr></thead>
        <tbody>
          {#each coverage as c (c.service_type)}
            <tr class:member={members.includes(c.service_type)}>
              <td>{c.name}</td>
              <td class="n">{c.served_pct.toFixed(1)}%</td>
              <td class="n">{c.frequent_pct.toFixed(1)}%</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="muted">Highlighted: the group's members. The group's pooled share is not answered yet (QAF-89).</p>
    </section>
  {/if}
  {#if unserved}
    <section class="list">
      <h2>Unserved, most people first</h2>
      <p class="muted">{count(unserved.length)} points beyond {params.radius_m} m of a served stop.</p>
      {#each unserved.slice(0, 50) as u, i (i)}
        <button type="button" class="row" class:on={i === selected} onclick={() => onselect(i)}>
          <b>{i + 1}</b><span>{count(Math.round(u.pop))} people<br /><small>{km(u.nearest_m)} to {u.nearest_stop ?? "no stop"}</small></span>
        </button>
      {/each}
    </section>
  {/if}
</aside>

<style>
  .panel { background: var(--surface); border-right: 1px solid var(--line); overflow-y: auto; font-size: 0.85rem; }
  section { padding: 0.75rem 1rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0 0 0.4rem; font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--muted); }
  .muted { color: var(--muted); font-size: 0.78rem; margin: 0.3rem 0; }
  label { display: flex; justify-content: space-between; align-items: center; gap: 0.5rem; padding: 0.2rem 0; }
  input[type="number"] { width: 5.5rem; font: inherit; padding: 0.15rem 0.35rem; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: var(--text); }
  table { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; }
  th { text-align: left; font-size: 0.68rem; color: var(--muted); font-weight: 500; padding: 0.2rem 0; }
  td { padding: 0.2rem 0; border-top: 1px solid var(--line); }
  .n { text-align: right; }
  tr.member td { font-weight: 600; color: var(--accent); }
  .row { display: grid; grid-template-columns: 1.6rem 1fr; gap: 0.4rem; width: 100%; text-align: left; font: inherit; border: 0; border-bottom: 1px solid var(--line); background: var(--surface); color: var(--text); padding: 0.4rem 0.2rem; cursor: pointer; }
  .row:hover { background: var(--hover); }
  .row.on { background: var(--accent-soft); }
  .row b { font-family: ui-monospace, monospace; color: var(--muted); font-size: 0.75rem; }
  .row small { color: var(--muted); }
</style>
