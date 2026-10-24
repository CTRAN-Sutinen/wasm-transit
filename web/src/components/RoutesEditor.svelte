<script>
  import { getNetworkDoc } from "../state.svelte.js";
  import { addRoute, updateRoute, removeRoute, modes } from "../planner.svelte.js";

  const doc = $derived(getNetworkDoc());

  let filter = $state("");
  let limit = $state(100);
  const matches = $derived.by(() => {
    const all = doc ? Object.entries(doc.routes ?? {}) : [];
    const q = filter.trim().toLowerCase();
    return q
      ? all.filter(([id, r]) => id.toLowerCase().includes(q) || r.name.toLowerCase().includes(q))
      : all;
  });
  const rows = $derived(matches.slice(0, limit));
</script>

<section>
  <h2>Routes</h2>
  <p>
    <input aria-label="Filter routes" placeholder="filter by id or name…" bind:value={filter}>
    {#if doc}<span class="muted">{Object.keys(doc.routes ?? {}).length} total</span>{/if}
  </p>
  {#each rows as [id, r] (id)}
    <div class="ent-row">
      <code>{id}</code>
      <input aria-label={`Name of route ${id}`} value={r.name}
             onchange={(e) => updateRoute(id, { name: e.currentTarget.value })}>
      <select aria-label={`Mode of route ${id}`} value={r.mode}
              onchange={(e) => updateRoute(id, { mode: e.currentTarget.value })}>
        {#each modes() as m (m)}
          <option value={m}>{m}</option>
        {/each}
      </select>
      <button aria-label={`Remove route ${id}`} onclick={() => removeRoute(id)}>remove</button>
    </div>
  {/each}
  {#if matches.length > limit}
    <p><button onclick={() => (limit += 100)}>Show 100 more</button>
       <span class="muted">({limit} of {matches.length} shown)</span></p>
  {/if}
  <p><button onclick={addRoute}>Add route</button></p>
</section>
