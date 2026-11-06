<script lang="ts">
  /// Who the network reaches (analysis `type_coverage`, judged in the
  /// hour window when one is set) with the op's parameters as inputs,
  /// and the unserved places heaviest first (`unserved_points`). The
  /// layer is any weighted geography (D-47): the planner
  /// picks the field that is its weight and says what it means; the
  /// answer's reading states its resolution. Coverage is the selected
  /// service group's — the combination of service types that run
  /// together, pooled, named by its members with the dates it holds
  /// (D-55); the unserved places are what its stops leave out.
  import type { LayerField } from "../types/LayerField";
  import type { LayerReading } from "../types/LayerReading";
  import type { CombinationCoverage } from "../types/CombinationCoverage";
  import type { UnservedPoint } from "../types/UnservedPoint";
  import { count, spansText } from "./designFormat";

  type Params = { radius_m: number; frequent_secs: number; min_departures: number };
  type Layer = { features: number; points: number; polygons: number; fields: LayerField[]; field: string; label: string };
  let {
    coverage,
    reading,
    unserved,
    layer,
    params,
    window,
    selected,
    onparams,
    onlayer,
    onreading,
    onselect,
  }: {
    coverage: CombinationCoverage | null;
    reading: LayerReading | null;
    unserved: UnservedPoint[] | null;
    layer: Layer | null;
    params: Params;
    window: { from: number; to: number } | null;
    selected: number | null;
    onparams: (p: Params) => void;
    onlayer: (file: File) => void;
    onreading: (field: string, label: string) => void;
    onselect: (i: number) => void;
  } = $props();

  const km = (m: number) => (m >= 1000 ? `${(m / 1000).toFixed(1)} km` : `${Math.round(m)} m`);
  const set = (patch: Partial<Params>) => onparams({ ...params, ...patch });
</script>

<aside class="panel" aria-label="Coverage" data-testid="coverage-panel">
  <section>
    <h2>Layer</h2>
    {#if layer}
      <p class="muted">{count(layer.points)} points · {count(layer.polygons)} polygons</p>
      {#if layer.fields.length}
        <label>Weight
          <select value={layer.field} onchange={(e) => onreading(e.currentTarget.value, layer.label === layer.field ? e.currentTarget.value : layer.label)} data-testid="layer-field">
            {#each layer.fields as f (f.name)}<option value={f.name}>{f.name} ({count(Math.round(f.total))})</option>{/each}
          </select>
        </label>
        <label>Means<input type="text" value={layer.label} onchange={(e) => onreading(layer.field, e.currentTarget.value)} data-testid="layer-label" /></label>
      {:else}
        <p class="muted">The layer has no numeric property to weigh by.</p>
      {/if}
      {#if reading}
        <p class="muted" data-testid="layer-resolution">
          {#if reading.sample_spacing_m != null}Polygons weighted by area, sampled every {km(reading.sample_spacing_m)}.{/if}
          {#if reading.point_spacing_m != null}Points about {km(reading.point_spacing_m)} apart{reading.point_spacing_m > params.radius_m ? " — coarser than the walk radius: a point is served whole or not at all" : ""}.{/if}
        </p>
      {/if}
    {:else}
      <p class="muted">Coverage needs a weighted layer (GeoJSON points or polygons: people, jobs, households).</p>
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
    <section data-testid="group-coverage">
      <h2>Served by {coverage.group}</h2>
      <p class="muted" data-testid="coverage-dates">{spansText(coverage.spans)}</p>
      <table>
        <thead><tr><th></th><th class="n">{reading?.label ?? "Weight"}</th><th class="n">Share</th><th class="n">Stops</th></tr></thead>
        <tbody>
          <tr>
            <td>Served</td>
            <td class="n">{count(Math.round(coverage.served))}</td>
            <td class="n" data-testid="served-pct">{coverage.served_pct.toFixed(1)}%</td>
            <td class="n">{count(coverage.served_stops)}</td>
          </tr>
          <tr>
            <td>Frequent</td>
            <td class="n">{count(Math.round(coverage.frequent))}</td>
            <td class="n" data-testid="frequent-pct">{coverage.frequent_pct.toFixed(1)}%</td>
            <td class="n">{count(coverage.frequent_stops)}</td>
          </tr>
        </tbody>
      </table>
    </section>
  {/if}
  {#if unserved}
    <section class="list">
      <h2>Unserved, heaviest first</h2>
      <p class="muted">{count(unserved.length)} places beyond {params.radius_m} m of a served stop.</p>
      {#each unserved.slice(0, 50) as u, i (i)}
        <button type="button" class="row" class:on={i === selected} onclick={() => onselect(i)}>
          <b>{i + 1}</b><span>{count(Math.round(u.weight))} {reading?.label ?? ""}{u.polygon ? ` (${Math.round(u.share * 100)}% of its area)` : ""}<br /><small>{km(u.nearest_m)} to {u.nearest_stop ?? "no stop"}</small></span>
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
  select, input[type="text"] { width: 9rem; font: inherit; padding: 0.15rem 0.35rem; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: var(--text); }
  input[type="number"] { width: 5.5rem; font: inherit; padding: 0.15rem 0.35rem; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: var(--text); }
  table { width: 100%; border-collapse: collapse; font-variant-numeric: tabular-nums; }
  th { text-align: left; font-size: 0.68rem; color: var(--muted); font-weight: 500; padding: 0.2rem 0; }
  td { padding: 0.2rem 0; border-top: 1px solid var(--line); }
  .n { text-align: right; }
  .row { display: grid; grid-template-columns: 1.6rem 1fr; gap: 0.4rem; width: 100%; text-align: left; font: inherit; border: 0; border-bottom: 1px solid var(--line); background: var(--surface); color: var(--text); padding: 0.4rem 0.2rem; cursor: pointer; }
  .row:hover { background: var(--hover); }
  .row.on { background: var(--accent-soft); }
  .row b { font-family: ui-monospace, monospace; color: var(--muted); font-size: 0.75rem; }
  .row small { color: var(--muted); }
</style>
