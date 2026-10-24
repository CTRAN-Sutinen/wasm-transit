<script>
  import { app, getNetworkDoc } from "../state.svelte.js";
  import { updateRoute, removeRoute, clearSelection, modes } from "../planner.svelte.js";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "route" ? app.plannerSelection.id : null);
  const route = $derived(id ? doc?.routes?.[id] ?? null : null);
  const patternIds = $derived.by(() =>
    doc ? Object.entries(doc.patterns ?? {}).filter(([, p]) => p.route === id).map(([pid]) => pid) : [],
  );
</script>

{#if route}
  <h3>Route <code>{id}</code></h3>
  <p class="ent-row">
    <label>name
      <input aria-label="Route name" value={route.name}
             onchange={(e) => updateRoute(id, { name: e.currentTarget.value })}>
    </label>
    <select aria-label="Route mode" value={route.mode}
            onchange={(e) => updateRoute(id, { mode: e.currentTarget.value })}>
      {#each modes() as m (m)}
        <option value={m}>{m}</option>
      {/each}
    </select>
  </p>
  <p class="chips">
    {#each patternIds as pid (pid)}
      <button class="chip svc"
              aria-label={`Open pattern ${pid}`}
              onclick={() => app.plannerSelection = { kind: "pattern", id: pid }}>{pid}</button>
    {/each}
    {#if !patternIds.length}<span class="muted">no patterns yet</span>{/if}
  </p>
  <p><button aria-label="Remove this route"
             onclick={() => { removeRoute(id); clearSelection(); }}>Remove route</button></p>
{:else}
  <p class="muted">Route not found (removed?).</p>
{/if}
