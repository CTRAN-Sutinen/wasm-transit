<script lang="ts">
  /// The hub as a matrix (analysis `hub_connections`): routes in as
  /// rows, routes out as columns, each cell the day's timed
  /// connections and their wait range. A cell selects the pair.
  import type { RouteConnections } from "../types/RouteConnections";

  let {
    connections,
    stopName,
    onroute,
  }: { connections: RouteConnections[]; stopName: string; onroute: (route: string) => void } = $props();

  const routes = $derived([...new Set(connections.flatMap((c) => [c.from_route, c.to_route]))].sort());
  const byPair = $derived(new Map(connections.map((c) => [`${c.from_route}>${c.to_route}`, c])));
  const most = $derived(Math.max(1, ...connections.map((c) => c.connections)));
  let pick = $state<RouteConnections | null>(null);
  const m = (s: number) => Math.round(s / 60);
</script>

<section class="panel" aria-label="Connections at {stopName}" data-testid="hub-matrix">
  <header><h2>Connections at {stopName}</h2><span class="muted">timed transfers, 1–15 min, across the stop's bays</span></header>
  {#if connections.length === 0}
    <p class="muted pad">No timed connections here on the group's day.</p>
  {:else}
    <div class="body">
      <div class="scroll">
        <table>
          <thead><tr><th class="corner">from ↓ to →</th>{#each routes as r (r)}<th>{r}</th>{/each}</tr></thead>
          <tbody>
            {#each routes as a (a)}
              <tr>
                <th scope="row">{a}</th>
                {#each routes as b (b)}
                  {@const c = byPair.get(`${a}>${b}`)}
                  <td>
                    {#if c}
                      {@const s = c.connections / most}
                      <button
                        type="button"
                        class:on={pick === c}
                        style:background="color-mix(in oklab, #1d4ed8 {Math.round(15 + s * 85)}%, var(--surface))"
                        style:color={s > 0.45 ? "#fff" : "var(--text)"}
                        aria-label="{a} to {b}: {c.connections} connections, waits {m(c.shortest_wait_secs)} to {m(c.longest_wait_secs)} minutes"
                        onclick={() => (pick = c)}
                      ><b>{c.connections}</b><i>{m(c.shortest_wait_secs)}–{m(c.longest_wait_secs)}′</i></button>
                    {:else}
                      <span class="none" class:self={a === b}></span>
                    {/if}
                  </td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
      <aside class="pick">
        {#if pick}
          <h3>{pick.from_route} → {pick.to_route}</h3>
          <dl>
            <dt>Timed connections</dt><dd>{pick.connections}</dd>
            <dt>Wait</dt><dd>{m(pick.shortest_wait_secs)}–{m(pick.longest_wait_secs)} min</dd>
            <dt>Median wait</dt><dd>{pick.median_wait_secs != null ? `${m(pick.median_wait_secs)} min` : "fewer than four connections"}</dd>
          </dl>
          <div class="go">
            <button type="button" onclick={() => onroute(pick!.from_route)}>Open {pick.from_route}</button>
            <button type="button" onclick={() => onroute(pick!.to_route)}>Open {pick.to_route}</button>
          </div>
        {:else}
          <p class="muted">Pick a cell to read a pair.</p>
        {/if}
      </aside>
    </div>
  {/if}
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: baseline; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  .pad { padding: 0.8rem 0.9rem; margin: 0; }
  .body { display: flex; flex-wrap: wrap; gap: 0.8rem; padding: 0.7rem 0.9rem; }
  .scroll { overflow: auto; flex: 999 1 26rem; min-width: 0; }
  table { border-collapse: separate; border-spacing: 2px; font-variant-numeric: tabular-nums; }
  th { font: 500 0.75rem ui-monospace, monospace; color: var(--muted); padding: 0.2rem 0.3rem; }
  .corner { font: 0.65rem system-ui, sans-serif; white-space: nowrap; }
  td { padding: 0; }
  td button { display: flex; flex-direction: column; align-items: center; justify-content: center; width: 3.1rem; height: 2.6rem; border: 0; border-radius: 4px; cursor: pointer; font: inherit; line-height: 1.1; }
  td button.on { outline: 3px solid var(--accent); outline-offset: -1px; }
  td b { font-size: 0.85rem; }
  td i { font-style: normal; font-size: 0.65rem; }
  .none { display: block; width: 3.1rem; height: 2.6rem; border-radius: 4px; background: var(--hover); opacity: 0.4; }
  .none.self { opacity: 1; }
  .pick { flex: 1 1 13rem; border-left: 1px solid var(--line); padding-left: 0.8rem; font-size: 0.85rem; }
  h3 { margin: 0 0 0.4rem; font-family: ui-monospace, monospace; }
  dl { display: grid; grid-template-columns: auto 1fr; gap: 0.2rem 0.6rem; margin: 0; }
  dt { color: var(--muted); }
  dd { margin: 0; font-weight: 600; }
  .go { display: flex; gap: 0.4rem; margin-top: 0.6rem; }
  .go button { font: inherit; font-size: 0.8rem; padding: 0.2rem 0.6rem; border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--text); cursor: pointer; }
</style>
