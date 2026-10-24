<script lang="ts">
  import { app, getNetworkDoc } from "../state.svelte";
  import {
    setServiceRefs, removeService, setServiceStarts, addServiceStart,
    removeServiceStart, setBandField, addBand, removeBand,
    clearSelection, fmtHHMMSS, parseHHMMSS,
  } from "../planner";
  import FilteredSelect from "./FilteredSelect.svelte";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "service" ? app.plannerSelection.id : "");
  const service = $derived(id ? doc?.services?.[id] ?? null : null);
  const patternName = $derived(service ? doc?.patterns?.[service.pattern]?.name ?? service.pattern : "");
  const patternOptions: [string, string][] = $derived(doc ? Object.keys(doc.patterns ?? {}).sort().map((p): [string, string] => [p, p]) : []);
  const calendarOptions: [string, string][] = $derived(doc ? Object.keys(doc.calendars ?? {}).sort().map((c): [string, string] => [c, c]) : []);

  function updateStart(index: number, text: string) {
    const secs = parseHHMMSS(text);
    if (secs === null) {
      app.status = `Not a valid time: "${text}" (use HH:MM:SS; hours may pass 24).`;
      return;
    }
    const sched = service?.schedule;
    if (sched?.kind !== "exact_times") return;
    const starts = sched.starts.map((s: number, i: number) => (i === index ? secs : s));
    setServiceStarts(id, starts);
  }

  function updateBand(index: number, field: string, text: string, asMinutes = false) {
    const secs = asMinutes ? Math.round(Number(text) * 60) : parseHHMMSS(text);
    if (secs === null || Number.isNaN(secs) || secs < 0) {
      app.status = `Not a valid ${field === "headway_secs" ? "headway" : "time"}: "${text}"`;
      return;
    }
    if (service?.schedule?.kind !== "headway_bands") return;
    setBandField(id, index, field, secs);
  }
</script>

{#if service}
  <h3>Service <code>{id}</code></h3>
  <p class="chips"><span class="chip svc">{patternName}</span></p>
  <p class="ent-row">
    <FilteredSelect label="Service pattern" value={service.pattern} options={patternOptions} cap={200}
                    onchange={(p) => setServiceRefs(id, { pattern: p })} />
    <FilteredSelect label="Service calendar" value={service.calendar} options={calendarOptions} cap={200}
                    onchange={(c) => setServiceRefs(id, { calendar: c })} />
  </p>

  {#if service.schedule.kind === "exact_times"}
    <h4>Trips</h4>
    <ul class="svc-rows">
      {#each service.schedule.starts as secs, i (i)}
        <li>
          <input class="trip-start" aria-label={`Trip ${i + 1} start`} value={fmtHHMMSS(secs)}
                 onchange={(e) => updateStart(i, e.currentTarget.value)}>
          <button aria-label={`Remove trip ${i + 1}`}
                  disabled={service.schedule.starts.length <= 1}
                  onclick={() => removeServiceStart(id, i)}>remove</button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addServiceStart(id)}>Add trip</button></p>
  {:else}
    <h4>Headway bands</h4>
    <ul class="svc-rows">
      {#each service.schedule.bands as band, i (i)}
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
                  disabled={service.schedule.bands.length <= 1}
                  onclick={() => removeBand(id, i)}>remove</button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addBand(id)}>Add band</button></p>
  {/if}
  <p>
    <button aria-label="Remove this service"
            onclick={() => { removeService(id); clearSelection(); }}>Remove service</button>
  </p>
{:else}
  <p class="muted">Service not found (removed?).</p>
{/if}
