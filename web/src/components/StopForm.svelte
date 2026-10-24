<script>
  import { app, getNetworkDoc } from "../state.svelte.js";
  import { updateStop, removeStop, clearSelection } from "../planner.svelte.js";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "stop" ? app.plannerSelection.id : null);
  const stop = $derived(id ? doc?.stops?.[id] ?? null : null);
</script>

{#if stop}
  <h3>Stop <code>{id}</code></h3>
  <p class="ent-row">
    <label>name
      <input aria-label="Stop name" value={stop.name}
             onchange={(e) => updateStop(id, { name: e.currentTarget.value })}>
    </label>
  </p>
  <p class="ent-row">
    <label>lat
      <input aria-label="Stop latitude" type="number" step="0.0001" value={stop.lat}
             onchange={(e) => updateStop(id, { lat: Number(e.currentTarget.value) })}>
    </label>
    <label>lon
      <input aria-label="Stop longitude" type="number" step="0.0001" value={stop.lon}
             onchange={(e) => updateStop(id, { lon: Number(e.currentTarget.value) })}>
    </label>
  </p>
  <p>
    <button aria-label="Remove this stop"
            onclick={() => { removeStop(id); clearSelection(); }}>Remove stop</button>
    <span class="muted">Patterns still visiting it show as validation issues.</span>
  </p>
{:else}
  <p class="muted">Stop not found (removed?).</p>
{/if}
