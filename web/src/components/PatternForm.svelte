<script lang="ts">
  import { app, getNetworkDoc } from "../state.svelte";
  import {
    updatePattern, removePattern, addPatternStop, removePatternStop,
    movePatternStop, setPatternStop, togglePatternTimepoint,
    addTimingProfile, removeTimingProfile, setTimingProfileField, setTimepointTime,
    clearSelection, fmtHHMMSS, parseHHMMSS,
  } from "../planner";
  import Combobox from "./Combobox.svelte";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "pattern" ? app.plannerSelection.id : "");
  const pattern = $derived(id ? doc?.patterns?.[id] ?? null : null);
  const routeIds = $derived(doc ? Object.keys(doc.routes ?? {}).sort() : []);
  const timepointNames = $derived.by(() => {
    if (!pattern) return [];
    const names = new Map(Object.entries(doc?.stops ?? {}).map(([sid, s]) => [sid, s.name]));
    return pattern.stops.filter((s) => s.timepoint).map((s) => names.get(s.stop) ?? s.stop);
  });

  // Stop options for the comboboxes: names read, ids as the sub
  // label — the search matches both.
  const stopOptions = $derived.by(() => {
    const names = new Map(Object.entries(doc?.stops ?? {}).map(([sid, s]) => [sid, s.name]));
    return Object.keys(doc?.stops ?? {})
      .sort()
      .map((sid) => ({ value: sid, label: names.get(sid) ?? sid, sub: sid }));
  });

  let addStopId = $state("");

  function parseTime(text: string) {
    const secs = parseHHMMSS(text);
    if (secs === null) app.status = `Not a valid time: "${text}" (HH:MM:SS; hours may pass 24).`;
    return secs;
  }
</script>

{#if pattern}
  <h3>Pattern <code>{id}</code></h3>
  <p class="ent-row">
    <input aria-label="Pattern name" value={pattern.name}
           onchange={(e) => updatePattern(id, { name: e.currentTarget.value })}>
    <Combobox label="Pattern route" options={routeIds.map((rid) => ({ value: rid, label: rid }))}
              value={pattern.route}
              onchange={(v) => updatePattern(id, { route: v })} />
  </p>

  <h4>Stop sequence</h4>
  <ul class="svc-rows">
    {#each pattern.stops as s, i (i)}
      <li>
        <code>{i + 1}</code>
        <Combobox label={`Stop at position ${i + 1}`} options={stopOptions} value={s.stop}
                  onchange={(v) => setPatternStop(id, i, { stop: v })} />
        <label>
          <input type="checkbox" checked={s.timepoint} disabled={i === 0 || i === pattern.stops.length - 1}
                 aria-label={`Timepoint at position ${i + 1}`}
                 onchange={() => togglePatternTimepoint(id, i)}>
          tp
        </label>
        <input aria-label={`Distance at position ${i + 1} (m)`} type="number" step="100" min="0" value={s.distance_m}
               onchange={(e) => setPatternStop(id, i, { distance_m: Number(e.currentTarget.value) })}>
        <button aria-label={`Move stop ${i + 1} up`} disabled={i <= 1}
                onclick={() => movePatternStop(id, i, -1)}>↑</button>
        <button aria-label={`Move stop ${i + 1} down`} disabled={i === 0 || i >= pattern.stops.length - 2}
                onclick={() => movePatternStop(id, i, 1)}>↓</button>
        <button aria-label={`Remove stop ${i + 1}`} disabled={i === 0 || i === pattern.stops.length - 1}
                onclick={() => removePatternStop(id, i)}>remove</button>
      </li>
    {/each}
  </ul>
  <p class="ent-row">
    <Combobox label="Stop to add" placeholder="stop…" options={stopOptions} bind:value={addStopId} />
    <button disabled={!addStopId} onclick={() => { addPatternStop(id, addStopId); addStopId = ""; }}>
      Add stop before the end
    </button>
  </p>

  <h4>Timing profiles</h4>
  {#each pattern.timing as prof, pi (pi)}
    <div class="timing-profile">
      <p class="ent-row">
        <strong>{pi + 1}</strong>
        <label>from
          <input aria-label={`Profile ${pi + 1} start`} value={fmtHHMMSS(prof.start_secs)}
                 onchange={(e) => setTimingProfileField(id, pi, { start_secs: parseTime(e.currentTarget.value) })}>
        </label>
        <label>to
          <input aria-label={`Profile ${pi + 1} end`} value={fmtHHMMSS(prof.end_secs)}
                 onchange={(e) => setTimingProfileField(id, pi, { end_secs: parseTime(e.currentTarget.value) })}>
        </label>
        <button aria-label={`Remove profile ${pi + 1}`} disabled={pattern.timing.length <= 1}
                onclick={() => removeTimingProfile(id, pi)}>remove</button>
      </p>
      <ul class="svc-rows">
        {#each prof.at_timepoints as secs, ti (ti)}
          <li>
            <code>{timepointNames[ti] ?? ti}</code>
            <input aria-label={`Profile ${pi + 1} time at ${timepointNames[ti] ?? ti}`}
                   class="tp-time" value={fmtHHMMSS(secs)}
                   onchange={(e) => setTimepointTime(id, pi, ti, parseTime(e.currentTarget.value))}>
          </li>
        {/each}
      </ul>
    </div>
  {/each}
  <p>
    <button onclick={() => addTimingProfile(id)}>Add timing profile</button>
    <button aria-label="Remove this pattern"
            onclick={() => { removePattern(id); clearSelection(); }}>Remove pattern</button>
  </p>
{:else}
  <p class="muted">Pattern not found (removed?).</p>
{/if}
