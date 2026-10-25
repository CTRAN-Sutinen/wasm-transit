<script lang="ts">
  import { app, getNetworkDoc } from "../state.svelte";
  import {
    setScheduleRefs, removeSchedule, setScheduleStarts, addScheduleStart,
    removeScheduleStart, setBandField, addBand, removeBand,
    plannerBack, fmtHHMMSS, parseHHMMSS,
  } from "../planner";
  import Combobox from "./Combobox.svelte";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "schedule" ? app.plannerSelection.id : "");
  const schedule = $derived(id ? doc?.schedules?.[id] ?? null : null);
  const patternName = $derived(schedule ? doc?.patterns?.[schedule.pattern]?.name ?? schedule.pattern : "");
  const typeName = $derived(
    schedule ? doc?.service_types?.[schedule.service_type]?.name ?? schedule.service_type : "",
  );
  const patternOptions = $derived(
    doc ? Object.keys(doc.patterns ?? {}).sort().map((p) => ({ value: p, label: p })) : [],
  );
  const typeOptions = $derived(
    doc
      ? Object.entries(doc.service_types ?? {})
          .sort(([a], [b]) => (a < b ? -1 : 1))
          .map(([tid, t]) => ({ value: tid, label: `${t.name} (${tid})` }))
      : [],
  );

  function updateStart(index: number, text: string) {
    const secs = parseHHMMSS(text);
    if (secs === null) {
      app.status = `Not a valid time: "${text}" (use HH:MM:SS; hours may pass 24).`;
      return;
    }
    const sched = schedule?.schedule;
    if (sched?.kind !== "exact_times") return;
    const starts = sched.starts.map((s: number, i: number) => (i === index ? secs : s));
    setScheduleStarts(id, starts);
  }

  function updateBand(index: number, field: string, text: string, asMinutes = false) {
    const secs = asMinutes ? Math.round(Number(text) * 60) : parseHHMMSS(text);
    if (secs === null || Number.isNaN(secs) || secs < 0) {
      app.status = `Not a valid ${field === "headway_secs" ? "headway" : "time"}: "${text}"`;
      return;
    }
    if (schedule?.schedule?.kind !== "headway_bands") return;
    setBandField(id, index, field, secs);
  }
</script>

{#if schedule}
  <section>
    <h2>Schedule <code>{id}</code></h2>
    <p class="chips">
      <span class="chip svc">{patternName}</span>
      <span class="chip svc">on {typeName}</span>
    </p>
    <p class="ent-row">
      <Combobox label="Pattern" options={patternOptions} value={schedule.pattern}
                onchange={(p) => setScheduleRefs(id, { pattern: p })} />
      <Combobox label="Service type" options={typeOptions} value={schedule.service_type}
                onchange={(t) => setScheduleRefs(id, { service_type: t })} />
    </p>

    {#if schedule.schedule.kind === "exact_times"}
      <h4>Trips</h4>
      <ul class="svc-rows">
        {#each schedule.schedule.starts as secs, i (i)}
          <li>
            <input class="trip-start" aria-label={`Trip ${i + 1} start`} value={fmtHHMMSS(secs)}
                   onchange={(e) => updateStart(i, e.currentTarget.value)}>
            <button aria-label={`Remove trip ${i + 1}`}
                    disabled={schedule.schedule.starts.length <= 1}
                    onclick={() => removeScheduleStart(id, i)}>remove</button>
          </li>
        {/each}
      </ul>
      <p><button onclick={() => addScheduleStart(id)}>Add trip</button></p>
    {:else}
      <h4>Headway bands</h4>
      <ul class="svc-rows">
        {#each schedule.schedule.bands as band, i (i)}
          <li>
            <input class="band-start" aria-label={`Band ${i + 1} start`} value={fmtHHMMSS(band.start_secs)}
                   onchange={(e) => updateBand(i, "start_secs", e.currentTarget.value)}>
            <span aria-hidden="true">→</span>
            <input class="band-end" aria-label={`Band ${i + 1} end`} value={fmtHHMMSS(band.end_secs)}
                   onchange={(e) => updateBand(i, "end_secs", e.currentTarget.value)}>
            <label>every
              <input class="band-headway" aria-label={`Band ${i + 1} headway minutes`} type="number" min="1"
                     value={band.headway_secs / 60}
                     onchange={(e) => updateBand(i, "headway_secs", e.currentTarget.value, true)}>
              min
            </label>
            <button aria-label={`Remove band ${i}`}
                    disabled={schedule.schedule.bands.length <= 1}
                    onclick={() => removeBand(id, i)}>remove</button>
          </li>
        {/each}
      </ul>
      <p><button onclick={() => addBand(id)}>Add band</button></p>
    {/if}
    <p>
      <button aria-label="Remove this schedule"
              onclick={() => { removeSchedule(id); plannerBack(); }}>Remove schedule</button>
    </p>
  </section>
{:else}
  <p class="muted">Schedule not found (removed?).</p>
{/if}
