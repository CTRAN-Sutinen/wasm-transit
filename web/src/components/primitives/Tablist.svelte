<script lang="ts">
  import { roving } from "./roving";
  import type { Snippet } from "svelte";

  type TabItem = { id: string; label?: string };
  let {
    items,
    selected,
    onchange,
    label = "",
    testid = "",
    tab,
    children,
  }: {
    items: TabItem[];
    selected: string;
    onchange?: (id: string) => void;
    label?: string;
    testid?: string;
    tab?: Snippet<[TabItem]>;
    children?: Snippet;
  } = $props();

  const uid = $props.id();
</script>

<div
  class="tabs"
  role="tablist"
  aria-label={label || undefined}
  data-testid={testid || undefined}
  use:roving
>
    {#each items as t (t.id)}
        <button
            type="button"
            role="tab"
            id="{uid}-tab-{t.id}"
            aria-controls="{uid}-panel"
            aria-selected={selected=== t.id}
            tabindex={selected === t.id ? 0 : -1}
            class:on={selected === t.id}
            onclick={() => onchange?.(t.id)}
        >
        {#if tab}
            {@render tab(t)}
        {:else}
            {t.label ?? t.id}
        {/if}</button>
    {/each}
</div>
<div id="{uid}-tab-panel" role="tabpanel" aria-labelledby="{uid}-tab-{selected}">
    {@render children?.()}
</div>