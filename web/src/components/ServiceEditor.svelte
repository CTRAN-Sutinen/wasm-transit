<script>
  import { app, getNetworkDoc } from "../state.svelte.js";
  import {
    addService, setServiceRefs, removeService, setServiceStarts, addServiceStart,
    removeServiceStart, setBandField, addBand, removeBand,
    fmtHHMMSS, parseHHMMSS,
  } from "../planner.svelte.js";
  import FilteredSelect from "./FilteredSelect.svelte";

  const doc = $derived(getNetworkDoc());
  const serviceOptions = $derived(doc ? Object.keys(doc.services ?? {}).map((id) => [id, id]) : []);
  const patternOptions = $derived(doc ? Object.keys(doc.patterns ?? {}).sort().map((id) => [id, id]) : []);
  const calendarOptions = $derived(doc ? Object.keys(doc.calendars ?? {}).sort().map((id) => [id, id]) : []);

  let selected = $state("");
  const service = $derived(doc?.services?.[selected] ?? null);
  const patternName = $derived(doc?.patterns?.[service?.pattern]?.name ?? service?.pattern ?? "");

  function updateStart(index, text) {
    const secs = parseHHMMSS(text);
    if (secs === null) {
      app.status = `Not a valid time: "${text}" (use HH:MM:SS; hours may pass 24).`;
      return;
    }
    const starts = service.schedule.starts.map((s, i) => (i === index ? secs : s));
    setServiceStarts(selected, starts);
  }

  function updateBand(index, field, text, asMinutes = false) {
    const secs = asMinutes ? Math.round(Number(text) * 60) : parseHHMMSS(text);
    if (secs === null || Number.isNaN(secs) || secs < 0) {
      app.status = `Not a valid ${field === "headway_secs" ? "headway" : "time"}: "${text}"`;
      return;
    }
    setBandField(selected, index, field, secs);
  }
</script>

<section>
  <h2>Services</h2>
  <p class="ent-row">
    <FilteredSelect label="Service" value={selected} options={serviceOptions} cap={200}
                    onchange={(id) => (selected = id)} />
    <button onclick={addService}>Add service</button>
    {#if service}
      <button aria-label="Remove service" onclick={() => { removeService(selected); selected = ""; }}>remove</button>
    {/if}
  </p>

  {#if service}
    <p class="ent-row">
      <FilteredSelect label="Service pattern" value={service.pattern} options={patternOptions} cap={200}
                      onchange={(id) => setServiceRefs(selected, { pattern: id })} />
      <FilteredSelect label="Service calendar" value={service.calendar} options={calendarOptions} cap={200}
                      onchange={(id) => setServiceRefs(selected, { calendar: id })} />
    </p>
    <p class="chips"><span class="chip svc">{patternName}</span></p>

    {#if service.schedule.kind === "exact_times"}
      <ul class="svc-rows">
        {#each service.schedule.starts as secs, i (i)}
          <li>
            <input class="trip-start" aria-label={`Trip ${i + 1} start`} value={fmtHHMMSS(secs)}
                   onchange={(e) => updateStart(i, e.currentTarget.value)}>
            <button aria-label={`Remove trip ${i + 1}`}
                    disabled={service.schedule.starts.length <= 1}
                    onclick={() => removeServiceStart(selected, i)}>remove</button>
          </li>
        {/each}
      </ul>
      <p><button onclick={() => addServiceStart(selected)}>Add trip</button></p>
    {:else}
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
                    onclick={() => removeBand(selected, i)}>remove</button>
          </li>
        {/each}
      </ul>
      <p><button onclick={() => addBand(selected)}>Add band</button></p>
    {/if}
    <p class="muted">Edits revalidate live; "Load into app" applies them to the feed the other views run on.</p>
  {/if}
</section>
