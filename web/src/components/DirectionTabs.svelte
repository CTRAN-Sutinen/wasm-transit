<script lang="ts">
  import type { DirectionGroup } from "../types/DirectionGroup";
  import { roving } from "./roving";

  let {
    directions,
    selected,
    onselect,
  }: {
    directions: DirectionGroup[];
    selected: string | null;
    onselect: (direction: string | null) => void;
  } = $props();
</script>

<div class="tabs" role="tablist" data-testid="direction-tabs" use:roving>
  {#each directions as d (d.direction ?? "")}
    <button
      role="tab"
      aria-selected={d.direction === selected}
      tabindex={d.direction === selected ? 0 : -1}
      class:sel={d.direction === selected}
      onclick={() => onselect(d.direction)}
    >
      {d.name} <span class="muted">{d.patterns.length} {d.patterns.length === 1 ? "pattern" : "patterns"}</span>
    </button>
  {/each}
</div>

<style>
  .tabs { display: flex; flex-wrap: wrap; gap: 4px; }
  button {
    border: 1px solid var(--line); background: var(--surface); color: inherit; border-radius: 4px;
    padding: 2px 8px; font: inherit; font-size: 0.85rem; cursor: pointer;
  }
  button:hover { background: var(--hover); }
  button.sel { background: var(--accent-soft); border-color: var(--accent); }
  .muted { color: var(--muted); font-size: 0.8rem; }
</style>
