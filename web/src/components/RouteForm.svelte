<script lang="ts">
  import { app, getNetworkDoc } from "../state.svelte";
  import { updateRoute, removeRoute, addPattern, selectEntity, plannerBack, refreshRouteDetail } from "../planner";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "route" ? app.plannerSelection.id : "");
  const route = $derived(id ? doc?.routes?.[id] ?? null : null);
  // The composition (patterns, the stops they visit, schedules by
  // service type with their trip counts) is cap-directory's answer,
  // re-asked whenever the document changes.
  $effect(() => {
    if (doc && id) void refreshRouteDetail(id);
  });
  const detail = $derived(app.routeDetail?.routeId === id ? app.routeDetail.detail : null);
</script>

{#if route}
  <section>
    <h2>Route <code>{id}</code></h2>
    <p class="ent-row">
      <label>name
        <input aria-label="Route name" value={route.name}
               onchange={(e) => updateRoute(id, { name: e.currentTarget.value })}>
      </label>
      <select aria-label="Route mode" value={route.mode}
              onchange={(e) => updateRoute(id, { mode: e.currentTarget.value as import("../types/Mode").Mode })}>
        {#each ["bus", "tram", "subway", "rail", "ferry", "cable_car", "gondola", "funicular", "trolleybus", "monorail", "other"] as m (m)}
          <option value={m}>{m}</option>
        {/each}
      </select>
    </p>

    <h4>Schedules by service type</h4>
    {#each detail?.pattern_trips_by_type ?? [] as group (group.service_type)}
      <p class="chips">
        <button class="chip" aria-label={`Open service type ${group.service_type}`}
                onclick={() => selectEntity("service_type", group.service_type)}>{group.name}</button>
        {#each group.pattern_trips as s (s.id)}
          <button class="chip svc" aria-label={`Open schedule ${s.id}`}
                  onclick={() => selectEntity("pattern_trips", s.id)}>{s.trips} trips</button>
        {/each}
      </p>
    {:else}
      <p class="muted">no schedules yet — add one from a service type or pattern</p>
    {/each}

    <h4>Patterns ({detail?.patterns.length ?? 0})</h4>
    <ul class="rail-items">
      {#each detail?.patterns ?? [] as p (p.id)}
        <li>
          <button class="rail-item" aria-label={`Open pattern ${p.id}`}
                  onclick={() => selectEntity("pattern", p.id)}>{p.name}</button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addPattern(id)}>Add pattern on this route</button></p>

    <h4>Stops ({detail?.stops.length ?? 0})</h4>
    <ul class="rail-items">
      {#each detail?.stops ?? [] as row (row.id)}
        <li>
          <button class="rail-item" aria-label={`Open stop ${row.id}`}
                  onclick={() => selectEntity("stop", row.id)}>
            {row.name}
            {#if row.transfer} <span class="chip svc">transfer</span>{/if}
          </button>
        </li>
      {/each}
    </ul>

    <p>
      <button aria-label="Remove this route"
              onclick={() => { removeRoute(id); plannerBack(); }}>Remove route</button>
    </p>
  </section>
{:else}
  <p class="muted">Route not found (removed?).</p>
{/if}
