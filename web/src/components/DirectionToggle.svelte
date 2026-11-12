<script lang="ts">
  /// The open route's direction, chosen: one button per direction the
  /// route names (cap-directory `route_detail` `directions` — named
  /// directions in id order, then the patterns in none), the open one
  /// pressed. Shown while the route is read, so the control never
  /// disappears. RENDER only: picking one is an event.
  import type { DirectionGroup } from "../types/DirectionGroup";

  let {
    directions,
    selected,
    onselect,
  }: {
    directions: DirectionGroup[] | null;
    selected: string | null;
    onselect: (direction: string | null) => void;
  } = $props();

  const named = $derived((directions ?? []).filter((d) => d.direction != null));
</script>

<div class="toggle" role="group" aria-label="Direction" data-testid="direction-toggle">
  <span class="label">Direction</span>
  {#if !directions}
    <span class="muted">reading the route…</span>
  {:else}
    {#each directions as d (d.direction ?? "")}
      <button type="button" aria-pressed={d.direction === selected} class:on={d.direction === selected} onclick={() => onselect(d.direction)}
        title="{d.patterns.length} {d.patterns.length === 1 ? 'pattern' : 'patterns'}">
        {d.direction == null ? (named.length ? "No direction" : "All trips") : `to ${d.name}`}
      </button>
    {/each}
    {#if named.length === 0}
      <span class="muted">This route's patterns name no direction, so its trips show together.</span>
    {/if}
  {/if}
</div>

<style>
  .toggle { display: flex; flex-wrap: wrap; align-items: center; gap: 0.35rem; }
  .label { font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); margin-right: 0.2rem; }
  button {
    font: inherit; font-size: 0.85rem; padding: 0.35rem 0.8rem; border-radius: 6px; cursor: pointer;
    border: 1px solid var(--line); background: var(--surface); color: var(--text);
  }
  button:hover { border-color: var(--accent); }
  button.on { background: var(--accent); border-color: var(--accent); color: var(--surface); font-weight: 600; }
  .muted { color: var(--muted); font-size: 0.8rem; }
</style>
