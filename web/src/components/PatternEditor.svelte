<script>
  import { app, getNetworkDoc } from "../state.svelte.js";
  import {
    addPattern, updatePattern, removePattern, addPatternStop, removePatternStop,
    movePatternStop, setPatternStop, togglePatternTimepoint,
    addTimingProfile, removeTimingProfile, setTimingProfileField, setTimepointTime,
    fmtHHMMSS, parseHHMMSS,
  } from "../planner.svelte.js";
  import FilteredSelect from "./FilteredSelect.svelte";

  const doc = $derived(getNetworkDoc());
  const patternOptions = $derived(
    doc ? Object.entries(doc.patterns ?? {}).map(([id, p]) => [id, `${id} — ${p.name}`]) : [],
  );
  const routeIds = $derived(doc ? Object.keys(doc.routes ?? {}).sort() : []);

  let selected = $state("");
  const pattern = $derived(doc?.patterns?.[selected] ?? null);
  const timepointNames = $derived.by(() => {
    if (!pattern) return [];
    const names = new Map(Object.entries(doc.stops ?? {}).map(([id, s]) => [id, s.name]));
    return pattern.stops.filter((s) => s.timepoint).map((s) => names.get(s.stop) ?? s.stop);
  });

  // Stop options for the row selects: one shared filter, capped. A
  // per-row dropdown over every stop in a real feed is the melt-down.
  let stopFilter = $state("");
  const stopOptions = $derived.by(() => {
    const ids = doc ? Object.keys(doc.stops ?? {}).sort() : [];
    const q = stopFilter.trim().toLowerCase();
    let hits = q ? ids.filter((id) => id.toLowerCase().includes(q)) : ids;
    hits = hits.slice(0, 200);
    // The pattern's current stops always render in their rows.
    if (pattern) {
      for (const s of pattern.stops) {
        if (!hits.includes(s.stop)) hits = [s.stop, ...hits];
      }
    }
    return hits.map((id) => [id, id]);
  });

  let addStopId = $state("");

  function parseTime(text) {
    const secs = parseHHMMSS(text);
    if (secs === null) app.status = `Not a valid time: "${text}" (HH:MM:SS; hours may pass 24).`;
    return secs;
  }
</script>

<section>
  <h2>Patterns</h2>
  <p class="ent-row">
    <FilteredSelect label="Pattern" value={selected} options={patternOptions} cap={200}
                    onchange={(id) => (selected = id)} />
    <button onclick={addPattern}>Add pattern</button>
    {#if pattern}
      <button aria-label="Remove pattern" onclick={() => { removePattern(selected); selected = ""; }}>remove</button>
    {/if}
  </p>

  {#if pattern}
    <p class="ent-row">
      <input aria-label="Pattern name" value={pattern.name}
             onchange={(e) => updatePattern(selected, { name: e.currentTarget.value })}>
      <select aria-label="Pattern route" value={pattern.route}
              onchange={(e) => updatePattern(selected, { route: e.currentTarget.value })}>
        {#each routeIds as id (id)}
          <option value={id}>{id}</option>
        {/each}
      </select>
    </p>

    <h3>Stop sequence</h3>
    {#if (doc ? Object.keys(doc.stops ?? {}).length : 0) > 200}
      <p><input aria-label="Filter stops" placeholder="filter stops…" bind:value={stopFilter}></p>
    {/if}
    <ul class="svc-rows">
      {#each pattern.stops as s, i (i)}
        <li>
          <code>{i + 1}</code>
          <select aria-label={`Stop at position ${i + 1}`} value={s.stop}
                  onchange={(e) => setPatternStop(selected, i, { stop: e.currentTarget.value })}>
            {#each stopOptions as [id] (id)}
              <option value={id}>{id}</option>
            {/each}
          </select>
          <label>
            <input type="checkbox" checked={s.timepoint} disabled={i === 0 || i === pattern.stops.length - 1}
                   aria-label={`Timepoint at position ${i + 1}`}
                   onchange={() => togglePatternTimepoint(selected, i)}>
            timepoint
          </label>
          <input aria-label={`Distance at position ${i + 1} (m)`} type="number" step="100" min="0" value={s.distance_m}
                 onchange={(e) => setPatternStop(selected, i, { distance_m: Number(e.currentTarget.value) })}>
          <button aria-label={`Move stop ${i + 1} up`} disabled={i <= 1}
                  onclick={() => movePatternStop(selected, i, -1)}>↑</button>
          <button aria-label={`Move stop ${i + 1} down`} disabled={i === 0 || i >= pattern.stops.length - 2}
                  onclick={() => movePatternStop(selected, i, 1)}>↓</button>
          <button aria-label={`Remove stop ${i + 1}`} disabled={i === 0 || i === pattern.stops.length - 1}
                  onclick={() => removePatternStop(selected, i)}>remove</button>
        </li>
      {/each}
    </ul>
    <p class="ent-row">
      <select aria-label="Stop to add" bind:value={addStopId}>
        <option value="" disabled>stop…</option>
        {#each stopOptions as [id] (id)}
          <option value={id}>{id}</option>
        {/each}
      </select>
      <button disabled={!addStopId} onclick={() => { addPatternStop(selected, addStopId); addStopId = ""; }}>
        Add stop before the end
      </button>
    </p>

    <h3>Timing profiles</h3>
    {#each pattern.timing as prof, pi (pi)}
      <div class="timing-profile">
        <p class="ent-row">
          <strong>Profile {pi + 1}</strong>
          <label>from
            <input aria-label={`Profile ${pi + 1} start`} value={fmtHHMMSS(prof.start_secs)}
                   onchange={(e) => setTimingProfileField(selected, pi, { start_secs: parseTime(e.currentTarget.value) })}>
          </label>
          <label>to
            <input aria-label={`Profile ${pi + 1} end`} value={fmtHHMMSS(prof.end_secs)}
                   onchange={(e) => setTimingProfileField(selected, pi, { end_secs: parseTime(e.currentTarget.value) })}>
          </label>
          <button aria-label={`Remove profile ${pi + 1}`} disabled={pattern.timing.length <= 1}
                  onclick={() => removeTimingProfile(selected, pi)}>remove</button>
        </p>
        <ul class="svc-rows">
          {#each prof.at_timepoints as secs, ti (ti)}
            <li>
              <code>{timepointNames[ti] ?? ti}</code>
              <input aria-label={`Profile ${pi + 1} time at ${timepointNames[ti] ?? ti}`}
                     class="tp-time" value={fmtHHMMSS(secs)}
                     onchange={(e) => setTimepointTime(selected, pi, ti, parseTime(e.currentTarget.value))}>
            </li>
          {/each}
        </ul>
      </div>
    {/each}
    <p><button onclick={() => addTimingProfile(selected)}>Add timing profile</button></p>
  {/if}
</section>
