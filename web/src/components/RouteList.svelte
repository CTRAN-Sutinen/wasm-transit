<script lang="ts">
  /// The routes — a RENDER component over the directory's `entities`
  /// answer for routes, narrowed by the filter's words (the op
  /// matches; this emits what is typed, a beat after typing pauses).
  import type { EntityRow } from "../types/EntityRow";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import { routeColor } from "./rideFormat";

  let {
    rows,
    routes,
    query,
    off,
    onfilter,
    onopen,
    onback,
  }: {
    rows: EntityRow[] | null;
    /// The directory's routes — each badge takes its route's color.
    routes: DirectoryRoute[];
    query: string;
    off: boolean;
    onfilter: (text: string) => void;
    onopen: (routeId: string) => void;
    onback: () => void;
  } = $props();

  const colors = $derived(new Map(routes.map((r) => [r.route_id, routeColor(r.color)])));
  let timer: ReturnType<typeof setTimeout> | undefined;
  function input(e: Event) {
    const text = (e.currentTarget as HTMLInputElement).value;
    clearTimeout(timer);
    timer = setTimeout(() => onfilter(text), 120);
  }
</script>

<section class="routes" data-testid="route-list">
  <header>
    <button class="back" onclick={onback} aria-label="back">‹</button>
    <h2>Routes</h2>
  </header>
  <input type="search" aria-label="filter routes" placeholder="Filter by number or name" value={query} oninput={input} autocomplete="off" />

  {#if off}
    <p class="off">The route directory is unavailable right now.</p>
  {:else if !rows}
    <div class="skeleton"><span></span><span></span><span></span></div>
  {:else if rows.length === 0}
    <p class="quiet">No route matches “{query}”.</p>
  {:else}
    <ul>
      {#each rows as r (r.id)}
        <li>
          <button onclick={() => onopen(r.id)}>
            <span class="badge" style:--route={colors.get(r.id) ?? null}>{r.name}</span>
            <span class="name">{r.long_name ?? r.name}</span>
            <span class="chev" aria-hidden="true">›</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  header { display: flex; align-items: center; gap: 0.4rem; margin-bottom: 0.6rem; }
  h2 { font-size: 1.2rem; margin: 0; }
  .back { border: 0; background: none; color: var(--text); cursor: pointer; border-radius: 10px; width: 2.25rem; height: 2.25rem; font-size: 1.6rem; line-height: 1; }
  .back:hover { background: var(--hover); }
  input {
    width: 100%; font: inherit; padding: 0.6rem 0.8rem; border-radius: 12px; border: 1px solid var(--line);
    background: var(--hover); color: var(--text); outline: none;
  }
  input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft); background: var(--surface); }
  ul { list-style: none; margin: 0.6rem 0 0; padding: 0; }
  li + li { border-top: 1px solid var(--line); }
  li button {
    width: 100%; display: flex; align-items: center; gap: 0.75rem; border: 0; background: none; color: var(--text);
    font: inherit; text-align: left; padding: 0.6rem 0.25rem; cursor: pointer; border-radius: 10px;
  }
  li button:hover { background: var(--hover); }
  .badge {
    min-width: 2.8rem; padding: 0.2rem 0.4rem; border-radius: 8px; text-align: center; font-weight: 750;
    font-variant-numeric: tabular-nums; background: var(--route, var(--accent)); color: #fff;
  }
  .name { flex: 1; min-width: 0; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chev { color: var(--muted); font-size: 1.2rem; }
  .quiet { color: var(--muted); }
  .off { color: var(--bad); font-size: 0.9rem; }
  .skeleton { display: grid; gap: 0.5rem; padding: 0.75rem 0; }
  .skeleton span { height: 2.4rem; border-radius: 10px; background: linear-gradient(90deg, var(--hover), var(--line), var(--hover)); background-size: 200% 100%; animation: shimmer 1.2s infinite; }
  @keyframes shimmer { to { background-position: -200% 0; } }
</style>
