<script>
  /// A select over possibly thousands of options: a filter box narrows
  /// the list, and only the first `cap` matches render. Without this,
  /// a real agency feed (thousands of services/patterns) turns every
  /// dropdown into tens of thousands of DOM nodes.
  let { label, value = "", options = [], onchange, cap = 200 } = $props();

  let filter = $state("");
  const shown = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    const hits = q
      ? options.filter(([id, text]) => id.toLowerCase().includes(q) || text.toLowerCase().includes(q))
      : options;
    // The current value always renders, filter or not.
    const list = hits.slice(0, cap);
    if (value && !list.some(([id]) => id === value)) {
      const current = options.find(([id]) => id === value);
      if (current) list.unshift(current);
    }
    return { list, total: hits.length };
  });
</script>

<span class="fselect">
  {#if options.length > cap}
    <input aria-label={`Filter ${label}`} placeholder="filter…" bind:value={filter}>
  {/if}
  <select aria-label={label} {value} onchange={(e) => onchange(e.currentTarget.value)}>
    {#each shown.list as [id, text] (id)}
      <option value={id}>{text}</option>
    {/each}
  </select>
</span>
