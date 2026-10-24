<script>
  import { getNetworkDoc } from "../state.svelte.js";
  import { addStop, updateStop, removeStop } from "../planner.svelte.js";

  const doc = $derived(getNetworkDoc());

  // Real feeds carry thousands of stops: search narrows, and only a
  // window of rows renders (a form control per stop melts the page).
  let filter = $state("");
  let limit = $state(100);
  const matches = $derived.by(() => {
    const all = doc ? Object.entries(doc.stops ?? {}) : [];
    const q = filter.trim().toLowerCase();
    return q
      ? all.filter(([id, s]) => id.toLowerCase().includes(q) || s.name.toLowerCase().includes(q))
      : all;
  });
  const rows = $derived(matches.slice(0, limit));
</script>

<section>
  <h2>Stops</h2>
  <p>
    <input aria-label="Filter stops" placeholder="filter by id or name…" bind:value={filter}>
    {#if doc}<span class="muted">{Object.keys(doc.stops ?? {}).length} total</span>{/if}
  </p>
  {#each rows as [id, s] (id)}
    <div class="ent-row">
      <code>{id}</code>
      <input aria-label={`Name of stop ${id}`} value={s.name}
             onchange={(e) => updateStop(id, { name: e.currentTarget.value })}>
      <input aria-label={`Latitude of stop ${id}`} type="number" step="0.0001" value={s.lat}
             onchange={(e) => updateStop(id, { lat: Number(e.currentTarget.value) })}>
      <input aria-label={`Longitude of stop ${id}`} type="number" step="0.0001" value={s.lon}
             onchange={(e) => updateStop(id, { lon: Number(e.currentTarget.value) })}>
      <button aria-label={`Remove stop ${id}`} onclick={() => removeStop(id)}>remove</button>
    </div>
  {/each}
  {#if matches.length > limit}
    <p><button onclick={() => (limit += 100)}>Show 100 more</button>
       <span class="muted">({limit} of {matches.length} shown)</span></p>
  {/if}
  <p><button onclick={addStop}>Add stop</button></p>
  <p class="muted">Removing a stop still visited by a pattern shows as a validation issue, not a block.</p>
</section>
