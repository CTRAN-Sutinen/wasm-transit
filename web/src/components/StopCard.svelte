<script lang="ts">
  /// A stop at a glance — a RENDER component: its name, how far it is
  /// (the directory's `stops_near` answer, when it was asked from a
  /// point: the walk along the streets when one reaches it, else the
  /// distance), and its next few departures (a board's rows). Tapping it
  /// opens the stop.
  import type { BoardRow } from "../types/BoardRow";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import DepartureRows from "./DepartureRows.svelte";
  import { distanceText, walkText } from "./rideFormat";

  let {
    name,
    distance = null,
    walk = null,
    rows,
    pooled = false,
    routes,
    note = "",
    onopen,
  }: {
    name: string;
    distance?: number | null;
    /// Seconds on foot, along the streets.
    walk?: number | null;
    /// Null while the board is being asked.
    rows: BoardRow[] | null;
    pooled?: boolean;
    routes: DirectoryRoute[];
    /// A line under the rows (what this card cannot tell).
    note?: string;
    onopen: () => void;
  } = $props();
</script>

<article class="card" data-testid="stop-card">
  <button class="head" onclick={onopen}>
    <span class="name">{name}</span>
    {#if walk != null}<span class="dist" data-testid="walk">{walkText(walk)}</span>
    {:else if distance != null}<span class="dist">{distanceText(distance)}</span>{/if}
    <span class="chev" aria-hidden="true">›</span>
  </button>
  {#if rows === null}
    <div class="skeleton" aria-label="loading departures"><span></span><span></span></div>
  {:else if rows.length}
    <DepartureRows {rows} {routes} {pooled} compact />
  {:else}
    <p class="quiet">No more departures today.</p>
  {/if}
  {#if note}<p class="note">{note}</p>{/if}
</article>

<style>
  .card { background: var(--surface); border: 1px solid var(--line); border-radius: 14px; padding: 0.35rem 0.75rem 0.5rem; }
  .head {
    width: 100%; display: flex; align-items: baseline; gap: 0.5rem; border: 0; background: none; color: var(--text);
    font: inherit; text-align: left; padding: 0.4rem 0; cursor: pointer;
  }
  .name { font-weight: 700; flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dist { color: var(--muted); font-size: 0.8rem; font-variant-numeric: tabular-nums; }
  .chev { color: var(--muted); font-size: 1.2rem; line-height: 1; }
  .head:hover .name { color: var(--accent); }
  .quiet, .note { color: var(--muted); font-size: 0.82rem; margin: 0.2rem 0 0.3rem; }
  .note { font-size: 0.75rem; }
  .skeleton { display: grid; gap: 0.4rem; padding: 0.3rem 0 0.4rem; }
  .skeleton span { height: 1.4rem; border-radius: 6px; background: linear-gradient(90deg, var(--hover), var(--line), var(--hover)); background-size: 200% 100%; animation: shimmer 1.2s infinite; }
  @keyframes shimmer { to { background-position: -200% 0; } }
</style>
