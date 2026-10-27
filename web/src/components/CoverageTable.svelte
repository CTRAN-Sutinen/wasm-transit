<script lang="ts">
  /// Who each service type reaches — cap-analysis `type_coverage`
  /// against a hash-addressed population layer. RENDER only: the
  /// walk radius and the frequency bar are OP PARAMETERS the reader
  /// sets here and the host sends; nothing is classified in the view.
  import type { TypeCoverage } from "../types/TypeCoverage";

  type Params = { radius_m: number; frequent_secs: number; min_departures: number };

  let {
    coverage,
    layer,
    params,
    selected,
    onparams,
    onlayer,
  }: {
    coverage: TypeCoverage[] | null;
    layer: { points: number; total_pop: number } | null;
    params: Params;
    selected: string;
    onparams: (p: Params) => void;
    onlayer: (file: File) => void;
  } = $props();

  const num = (e: Event) => Number((e.currentTarget as HTMLInputElement).value);
</script>

<section class="coverage" data-testid="coverage">
  <h3>Coverage</h3>
  {#if !layer}
    <label class="pick">Population layer (GeoJSON)
      <input type="file" accept=".geojson,.json" onchange={(e) => {
        const f = (e.currentTarget as HTMLInputElement).files?.[0];
        if (f) onlayer(f);
      }} />
    </label>
  {:else}
    <div class="params">
      <span class="muted">{layer.total_pop.toLocaleString()} people, {layer.points.toLocaleString()} points</span>
      <label>walk <input type="number" min="100" step="50" value={params.radius_m}
        onchange={(e) => onparams({ ...params, radius_m: num(e) })} /> m</label>
      <label>frequent ≤ <input type="number" min="5" step="5" value={Math.round(params.frequent_secs / 60)}
        onchange={(e) => onparams({ ...params, frequent_secs: num(e) * 60 })} /> min</label>
    </div>
    {#if coverage}
      <table>
        <thead><tr><th>Type</th><th class="n">Served</th><th class="n">Frequent</th><th class="n">Stops</th></tr></thead>
        <tbody>
          {#each coverage as c (c.service_type)}
            <tr class:sel={c.service_type === selected}>
              <td>{c.name}</td>
              <td class="n">{c.served_pct.toFixed(1)}%</td>
              <td class="n">{c.frequent_pct.toFixed(1)}%</td>
              <td class="n">{c.served_stops}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  {/if}
</section>

<style>
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th { text-align: left; font-weight: 600; border-bottom: 1px solid var(--line); }
  td, th { padding: 2px 6px; }
  .n { text-align: right; font-variant-numeric: tabular-nums; }
  tr.sel { background: var(--accent-soft); }
  .muted { color: var(--muted); }
  .params { display: flex; flex-wrap: wrap; gap: 0.75rem; font-size: 0.8rem; margin-bottom: 0.25rem; }
  .params input { width: 4.5em; }
  .pick { font-size: 0.85rem; display: flex; flex-direction: column; gap: 0.25rem; }
  h3 { margin: 0 0 0.25rem; font-size: 0.95rem; }
</style>
