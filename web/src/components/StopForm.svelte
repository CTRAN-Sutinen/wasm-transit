<script lang="ts">
  import { app, getNetworkDoc } from "../state.svelte";
  import { updateStop, removeStop, selectEntity, plannerBack } from "../planner";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "stop" ? app.plannerSelection.id : "");
  const stop = $derived(id ? doc?.stops?.[id] ?? null : null);
  // The routes serving this stop (doc-composed), linked for drill-down.
  const servingRoutes = $derived.by(() => {
    if (!doc) return [];
    const routes = new Set<string>();
    for (const p of Object.values(doc.patterns)) {
      if (p.stops.some((ps) => ps.stop === id)) routes.add(p.route);
    }
    return [...routes].sort();
  });
</script>

{#if stop}
  <section>
    <h2>Stop <code>{id}</code></h2>
    <p class="chips">
      <span class="chip svc">{servingRoutes.length} routes serve this stop</span>
      {#if stop.transfer}<span class="chip svc">transfer point</span>{/if}
    </p>
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
      <label class="ent-row">
        <input type="checkbox" aria-label="Transfer point"
               checked={stop.transfer}
               onchange={(e) => updateStop(id, { transfer: e.currentTarget.checked })}>
        transfer point
      </label>
    </p>

    {#if stop.transfer}
      <p class="muted">
        Flagged as a transfer point: routes meeting across its walking
        group (siblings sharing a parent, the parent itself — across a
        street or down a block) compose it. Timed-connection counts
        live in the service type screens' stop tables.
      </p>
    {/if}

    <h4>Served by ({servingRoutes.length})</h4>
    <p class="chips">
      {#each servingRoutes as rid (rid)}
        <button class="chip svc" aria-label={`Open route ${rid}`}
                onclick={() => selectEntity("route", rid)}>{doc?.routes?.[rid]?.name ?? rid}</button>
      {/each}
      {#if servingRoutes.length === 0}<span class="muted">no patterns visit this stop</span>{/if}
    </p>

    <p>
      <button aria-label="Remove this stop"
              onclick={() => { removeStop(id); plannerBack(); }}>Remove stop</button>
      <span class="muted">Patterns still visiting it show as validation issues.</span>
    </p>
  </section>
{:else}
  <p class="muted">Stop not found (removed?).</p>
{/if}
