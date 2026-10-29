<script lang="ts">
  /// One route — a RENDER component over the directory's
  /// `route_detail`: its directions, and in each the VARIANTS a rider
  /// rides (patterns over the same places in order, a station's bays
  /// one place — the op groups and names them, most trips first),
  /// drawn as a line diagram in the route's color. A place is an event.
  import type { RouteDetail } from "../types/RouteDetail";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import { routeColor } from "./rideFormat";

  let {
    route,
    info,
    off,
    onstop,
    onback,
  }: {
    /// The route as the directory lists it (name, long name, color).
    route: DirectoryRoute | null;
    info: { detail: RouteDetail | null } | null;
    off: boolean;
    onstop: (stop: { id: string; name: string }) => void;
    onback: () => void;
  } = $props();

  let direction = $state(0);
  $effect(() => {
    void route?.route_id;
    direction = 0;
  });

  const directions = $derived((info?.detail?.directions ?? []).filter((d) => d.variants.length));
  const color = $derived(routeColor(route?.color));
</script>

<section class="route" data-testid="route-view" style:--route={color}>
  <header>
    <button class="back" onclick={onback} aria-label="back">‹</button>
    {#if route}
      <span class="badge">{route.name}</span>
      <h2>{route.long_name ?? route.name}</h2>
    {/if}
  </header>

  {#if off}
    <p class="off">The route directory is unavailable right now.</p>
  {:else if !info}
    <div class="skeleton"><span></span><span></span><span></span></div>
  {:else if !info.detail}
    <p class="quiet">This route isn’t in the network.</p>
  {:else}
    <p class="hint">Its buses are highlighted on the map. Tap a stop to see what leaves it.</p>
    {#if directions.length > 1}
      <div class="tabs" role="tablist">
        {#each directions as d, i (d.direction ?? i)}
          <button role="tab" aria-selected={direction === i} class:on={direction === i} onclick={() => (direction = i)}>{d.name}</button>
        {/each}
      </div>
    {/if}
    {#each directions[direction]?.variants ?? [] as v, i (v.patterns.join(","))}
      <details class="variant" open={i === 0} data-testid="route-variant">
        <summary>
          <span class="vname">{v.name}</span>
          <span class="count">{v.stops.length} stops</span>
        </summary>
        <ol class="line">
          {#each v.stops as s, j (s.place + ":" + j)}
            <li class:end={j === 0 || j === v.stops.length - 1}>
              <button onclick={() => onstop({ id: s.place, name: s.name })}>{s.name}</button>
            </li>
          {/each}
        </ol>
      </details>
    {/each}
  {/if}
</section>

<style>
  header { display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.4rem; }
  h2 { font-size: 1.15rem; margin: 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .back { border: 0; background: none; color: var(--text); cursor: pointer; border-radius: 10px; width: 2.25rem; height: 2.25rem; font-size: 1.6rem; line-height: 1; flex: none; }
  .back:hover { background: var(--hover); }
  .badge { min-width: 2.8rem; padding: 0.2rem 0.45rem; border-radius: 8px; text-align: center; font-weight: 750; background: var(--route, var(--accent)); color: #fff; flex: none; }
  .hint { color: var(--muted); font-size: 0.85rem; margin: 0 0 0.6rem; }
  .tabs { display: flex; gap: 0.25rem; background: var(--hover); padding: 0.2rem; border-radius: 10px; margin-bottom: 0.6rem; }
  .tabs button {
    flex: 1; border: 0; background: none; color: var(--muted); font: inherit; font-size: 0.82rem; font-weight: 600;
    padding: 0.4rem; border-radius: 8px; cursor: pointer; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .tabs button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 3px rgb(0 0 0 / 0.12); }
  .variant { border: 1px solid var(--line); border-radius: 12px; padding: 0.3rem 0.6rem; margin-bottom: 0.5rem; }
  summary { cursor: pointer; display: flex; justify-content: space-between; gap: 0.5rem; padding: 0.35rem 0; font-weight: 650; }
  .count { color: var(--muted); font-weight: 500; font-size: 0.8rem; flex: none; }
  .vname { min-width: 0; }
  .line { list-style: none; margin: 0.2rem 0 0.3rem; padding: 0; position: relative; }
  .line::before { content: ""; position: absolute; left: 0.55rem; top: 0.8rem; bottom: 0.8rem; width: 4px; border-radius: 2px; background: var(--route, var(--accent)); }
  .line li { position: relative; padding-left: 1.6rem; }
  .line li::before {
    content: ""; position: absolute; left: 0.3rem; top: 50%; translate: 0 -50%; width: 8px; height: 8px; border-radius: 50%;
    background: var(--surface); border: 2.5px solid var(--route, var(--accent));
  }
  .line li.end::before { width: 11px; height: 11px; left: 0.22rem; background: var(--route, var(--accent)); }
  .line button { border: 0; background: none; color: var(--text); font: inherit; font-size: 0.88rem; padding: 0.3rem 0.3rem; cursor: pointer; text-align: left; border-radius: 6px; width: 100%; }
  .line li.end button { font-weight: 650; }
  .line button:hover { background: var(--hover); color: var(--accent); }
  .quiet { color: var(--muted); }
  .off { color: var(--bad); font-size: 0.9rem; }
  .skeleton { display: grid; gap: 0.5rem; padding: 0.75rem 0; }
  .skeleton span { height: 2.4rem; border-radius: 10px; background: linear-gradient(90deg, var(--hover), var(--line), var(--hover)); background-size: 200% 100%; animation: shimmer 1.2s infinite; }
  @keyframes shimmer { to { background-position: -200% 0; } }
</style>
