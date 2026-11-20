<script lang="ts">
  /// One search box over routes and stops (directory `entities` and
  /// `stops`): a pick narrows the page to one route or one stop. The
  /// pick shows as a chip with a clear button. A RENDER component.
  import type { DirectoryStop } from "../types/cap_directory/DirectoryStop";
  import type { EntityRow } from "../types/EntityRow";

  type Pick = { kind: "route" | "stop"; id: string; label: string };

  let {
    query,
    found,
    pick,
    oninput,
    onpick,
  }: {
    query: string;
    found: { routes: EntityRow[]; stops: DirectoryStop[] } | null;
    pick: Pick | null;
    oninput: (text: string) => void;
    onpick: (pick: Pick | null) => void;
  } = $props();

  let timer: ReturnType<typeof setTimeout> | undefined;
  function typed(text: string) {
    clearTimeout(timer);
    timer = setTimeout(() => oninput(text), 150);
  }
</script>

<div class="search" data-testid="search">
  {#if pick}
    <span class="chip" data-testid="search-pick">
      {pick.kind === "route" ? "Route" : "Stop"} {pick.label}
      <button type="button" aria-label="Clear" onclick={() => onpick(null)}>×</button>
    </span>
  {:else}
    <input type="search" placeholder="Search" aria-label="Search routes and stops" value={query} oninput={(e) => typed(e.currentTarget.value)} />
    {#if found && (found.routes.length || found.stops.length)}
      <ul class="found" role="listbox">
        {#each found.routes as r (r.id)}
          <li><button type="button" onclick={() => onpick({ kind: "route", id: r.id, label: r.name })}><span class="kind">Route</span> {r.name}{#if r.long_name} <span class="muted">{r.long_name}</span>{/if}</button></li>
        {/each}
        {#each found.stops as s (s.stop_id)}
          <li><button type="button" onclick={() => onpick({ kind: "stop", id: s.stop_id, label: s.name })}><span class="kind">Stop</span> {s.name} <span class="muted">{s.stop_id}</span></button></li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .search { position: relative; }
  input { width: 100%; padding: 0.5rem 0.7rem; border: 1px solid var(--line); border-radius: 6px; font: inherit; background: var(--surface); color: var(--text); }
  .found {
    position: absolute; z-index: 10; left: 0; right: 0; top: calc(100% + 4px); list-style: none; margin: 0; padding: 0.25rem;
    background: var(--surface); border: 1px solid var(--line); border-radius: 6px; box-shadow: 0 4px 12px rgb(0 0 0 / 0.15);
    max-height: 22rem; overflow: auto;
  }
  .found button { width: 100%; text-align: left; border: 0; background: none; padding: 0.4rem 0.5rem; border-radius: 4px; cursor: pointer; font: inherit; color: inherit; }
  .found button:hover, .found button:focus { background: var(--hover); }
  .kind { display: inline-block; min-width: 2.6rem; font-size: 0.75rem; color: var(--muted); }
  .chip { display: inline-flex; align-items: center; gap: 0.4rem; padding: 0.3rem 0.4rem 0.3rem 0.7rem; border-radius: 999px; background: var(--accent-soft); font-weight: 600; }
  .chip button { border: 0; background: none; cursor: pointer; font-size: 1.1rem; line-height: 1; color: inherit; }
</style>
