<script lang="ts">
  /// A searchable picker over possibly thousands of options: typing
  /// filters (debounced a beat, so a fast typist doesn't pay for a
  /// search per keystroke), only `cap` rows render, and long labels
  /// ellipsize instead of spilling. Replaces every native <select>
  /// whose option list is data, not a short enum.
  type Opt = { value: string; label: string; sub?: string };

  let {
    label,
    id,
    value = $bindable(""),
    options = [],
    onchange,
    placeholder = "search…",
    cap = 100,
  }: {
    label: string;
    id?: string;
    value?: string;
    options: Opt[];
    onchange?: (v: string) => void;
    placeholder?: string;
    cap?: number;
  } = $props();

  const uid = `combo-${crypto.randomUUID()}`;

  let open = $state(false);
  let query = $state("");
  let settled = $state("");
  let highlight = $state(0);
  let rootEl: HTMLElement;
  let inputEl: HTMLInputElement;

  // Debounce: the filtered list follows the query a beat after
  // typing stops (cleanup cancels a pending recompute on each
  // keystroke). The highlight follows the selected row while the
  // query hasn't actually changed (e.g. on open, pre-filled).
  $effect(() => {
    const q = query;
    const t = setTimeout(() => {
      if (settled !== q) highlight = 0;
      settled = q;
    }, 120);
    return () => clearTimeout(t);
  });

  const hits = $derived.by(() => {
    const q = settled.trim().toLowerCase();
    if (!q) return options;
    return options.filter(
      (o) => o.label.toLowerCase().includes(q) || o.value.toLowerCase().includes(q),
    );
  });
  const shown = $derived(hits.slice(0, cap));
  const selected = $derived(options.find((o) => o.value === value));

  function openList(): void {
    open = true;
    // The search starts FROM the selection: the box never blanks out
    // to an empty query (the current value stays readable), and the
    // list opens positioned around it. Typing replaces the
    // pre-filled text — select it first.
    query = selected?.label ?? "";
    settled = query;
    const at = shown.findIndex((o) => o.value === value);
    highlight = at >= 0 ? at : 0;
    inputEl?.select();
  }

  function close(): void {
    open = false;
    query = "";
    settled = "";
  }

  function pick(v: string): void {
    value = v;
    onchange?.(v);
    close();
    // The pick is made: focus leaves the box, so the page's own keys
    // (a view's shortcuts) work again and typing no longer lands here.
    inputEl?.blur();
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === "Escape" && open) {
      // Consumed here: ESC closes the list and must not leak to
      // page-wide handlers.
      e.stopPropagation();
      close();
      return;
    }
    if (!open && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      openList();
      return;
    }
    if (!open) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      highlight = Math.min(highlight + 1, shown.length - 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      highlight = Math.max(highlight - 1, 0);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (shown[highlight]) pick(shown[highlight].value);
    }
  }
</script>

<svelte:window
  onclick={(e) => {
    if (open && rootEl && !rootEl.contains(e.target as Node)) close();
  }}
/>

<div class="combo" bind:this={rootEl}>
  <input
    bind:this={inputEl}
    id={id || undefined}
    role="combobox"
    aria-label={label}
    aria-expanded={open}
    aria-controls={`${uid}-list`}
    aria-autocomplete="list"
    placeholder={open ? placeholder : (selected?.label ?? placeholder)}
    value={open ? query : (selected?.label ?? "")}
    onfocus={openList}
    onclick={openList}
    onkeydown={onKey}
    oninput={(e) => (query = e.currentTarget.value)}
  />
  {#if open}
    <ul class="combo-list" id={`${uid}-list`} role="listbox" aria-label={label}>
      {#each shown as o, i (o.value)}
        <li
          role="option"
          aria-selected={o.value === value}
          class:active={i === highlight}
          onclick={() => pick(o.value)}
          onmousemove={() => (highlight = i)}
        >
          <span class="combo-label">{o.label}</span>
          {#if o.sub}<span class="combo-sub">{o.sub}</span>{/if}
        </li>
      {/each}
      {#if !shown.length}
        <li class="combo-empty">no matches</li>
      {:else if hits.length > shown.length}
        <li class="combo-more">{hits.length - shown.length} more — keep typing</li>
      {/if}
    </ul>
  {/if}
</div>

<style>
  /* The component owns its look (the list floats over the page, it
     never pushes it), so every page that places it gets the same. */
  .combo { position: relative; width: 100%; }
  input {
    width: 100%; box-sizing: border-box; height: 1.9rem; font: inherit; font-size: 0.85rem; padding: 0 0.55rem;
    border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--text);
  }
  input:focus { outline: 2px solid var(--accent); outline-offset: -1px; }
  .combo-list {
    position: absolute; z-index: 20; top: calc(100% + 2px); left: 0; right: 0; min-width: 16rem;
    max-height: 18rem; overflow-y: auto; margin: 0; padding: 4px; list-style: none;
    background: var(--surface); color: var(--text); border: 1px solid var(--line); border-radius: 8px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.22); font-size: 0.85rem;
  }
  li { display: flex; justify-content: space-between; gap: 0.5rem; padding: 0.22rem 0.45rem; border-radius: 6px; cursor: pointer; }
  li.active { background: var(--accent-soft); }
  .combo-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .combo-sub { color: var(--muted); white-space: nowrap; flex-shrink: 0; font-size: 0.78rem; }
  .combo-more, .combo-empty { color: var(--muted); font-size: 0.78rem; cursor: default; }
</style>
