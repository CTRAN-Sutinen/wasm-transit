<script lang="ts">
  import { app, getNetworkDoc } from "../state.svelte";
  import { updateRoute, removeRoute, addPattern, selectEntity, plannerBack, scheduleTripCount } from "../planner";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "route" ? app.plannerSelection.id : "");
  const route = $derived(id ? doc?.routes?.[id] ?? null : null);
  const patternIds: string[] = $derived.by(() =>
    doc ? Object.entries(doc.patterns ?? {}).filter(([, p]) => p.route === id).map(([pid]) => pid) : [],
  );
  // The stops this route's patterns visit — the composed place list.
  const stopRows = $derived.by(() => {
    if (!doc) return [];
    const visits = new Set<string>();
    for (const pid of patternIds) {
      for (const ps of doc.patterns[pid]?.stops ?? []) visits.add(ps.stop);
    }
    return [...visits].map((sid) => ({
      sid,
      name: doc.stops[sid]?.name ?? sid,
      transfer: doc.stops[sid]?.transfer ?? false,
    }));
  });
  // This route's schedules, grouped by service type — the design
  // axis, straight from the document.
  const schedulesByType = $derived.by(() => {
    if (!doc) return [];
    const groups = new Map<string, { sid: string; trips: number }[]>();
    for (const [sid, s] of Object.entries(doc.schedules)) {
      if (!patternIds.includes(s.pattern)) continue;
      const list = groups.get(s.service_type) ?? [];
      list.push({ sid, trips: scheduleTripCount(s.schedule) });
      groups.set(s.service_type, list);
    }
    return [...groups.entries()]
      .sort(([a], [b]) => (a < b ? -1 : 1))
      .map(([tid, schedules]) => ({
        tid,
        name: doc.service_types[tid]?.name ?? tid,
        schedules,
      }));
  });
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
    {#each schedulesByType as group (group.tid)}
      <p class="chips">
        <button class="chip" aria-label={`Open service type ${group.tid}`}
                onclick={() => selectEntity("service_type", group.tid)}>{group.name}</button>
        {#each group.schedules as s (s.sid)}
          <button class="chip svc" aria-label={`Open schedule ${s.sid}`}
                  onclick={() => selectEntity("schedule", s.sid)}>{s.trips} trips</button>
        {/each}
      </p>
    {:else}
      <p class="muted">no schedules yet — add one from a service type or pattern</p>
    {/each}

    <h4>Patterns ({patternIds.length})</h4>
    <ul class="rail-items">
      {#each patternIds as pid (pid)}
        <li>
          <button class="rail-item" aria-label={`Open pattern ${pid}`}
                  onclick={() => selectEntity("pattern", pid)}>{doc?.patterns?.[pid]?.name ?? pid}</button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addPattern(id)}>Add pattern on this route</button></p>

    <h4>Stops ({stopRows.length})</h4>
    <ul class="rail-items">
      {#each stopRows as row (row.sid)}
        <li>
          <button class="rail-item" aria-label={`Open stop ${row.sid}`}
                  onclick={() => selectEntity("stop", row.sid)}>
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
