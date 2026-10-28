<script lang="ts">
  import type { HeadwayBand } from "../types/HeadwayBand";
  import { app, getNetworkDoc } from "../state.svelte";
  import {
    setScheduleRefs, removeSchedule, retimeTrip, addTrip, removeTrip, setBandField, addBand, removeBand,
    plannerBack, fmtHHMMSS, parseHHMMSS,
  } from "../planner";
  import Combobox from "./Combobox.svelte";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "pattern_trips" ? app.plannerSelection.id : "");
  const trips = $derived(id ? doc?.pattern_trips?.[id] ?? null : null);
  // Names and pickers are cap-directory's rows (app.pickers).
  const nameOf = (kind: "pattern" | "service_type", rid: string) =>
    app.pickers?.[kind].find((r) => r.id === rid)?.name ?? rid;
  const patternName = $derived(trips ? nameOf("pattern", trips.pattern) : "");
  const typeName = $derived(trips ? nameOf("service_type", trips.service_type) : "");
  const patternOptions = $derived((app.pickers?.pattern ?? []).map((r) => ({ value: r.id, label: r.id })));
  const typeOptions = $derived(
    (app.pickers?.service_type ?? []).map((r) => ({ value: r.id, label: `${r.name} (${r.id})` })),
  );

  let addTime = $state("");

  function updateStart(tripId: string, text: string) {
    const secs = parseHHMMSS(text);
    if (secs === null) {
      app.status = `Not a valid time: "${text}" (use HH:MM:SS; hours may pass 24).`;
      return;
    }
    retimeTrip(tripId, secs);
  }

  function updateBand(tripId: string, index: number, field: keyof HeadwayBand, text: string, asMinutes = false) {
    const secs = asMinutes ? Math.round(Number(text) * 60) : parseHHMMSS(text);
    if (secs === null || Number.isNaN(secs) || secs < 0) {
      app.status = `Not a valid ${field === "headway_secs" ? "headway" : "time"}: "${text}"`;
      return;
    }
    setBandField(tripId, index, field, secs);
  }
</script>

{#if trips}
  <section>
    <h2>Schedule <code>{id}</code></h2>
    <p class="chips">
      <span class="chip svc">{patternName}</span>
      <span class="chip svc">on {typeName}</span>
    </p>
    <p class="ent-row">
      <Combobox label="Pattern" options={patternOptions} value={trips.pattern}
                onchange={(p) => setScheduleRefs(id, { pattern: p })} />
      <Combobox label="Service type" options={typeOptions} value={trips.service_type}
                onchange={(t) => setScheduleRefs(id, { service_type: t })} />
    </p>

    <h4>Trips</h4>
    <ul class="svc-rows">
      {#each trips.trips as trip, i (trip.id)}
        <li>
          <code>{trip.id}</code>
          {#if trip.departs.kind === "at"}
            <input class="trip-start" aria-label={`Trip ${i + 1} start`} value={fmtHHMMSS(trip.departs.start_secs)}
                   onchange={(e) => updateStart(trip.id, e.currentTarget.value)}>
          {:else}
            <ul class="svc-rows">
              {#each trip.departs.bands as band, b (b)}
                <li>
                  <input class="band-start" aria-label={`Trip ${i + 1} band ${b + 1} start`} value={fmtHHMMSS(band.start_secs)}
                         onchange={(e) => updateBand(trip.id, b, "start_secs", e.currentTarget.value)}>
                  <span aria-hidden="true">→</span>
                  <input class="band-end" aria-label={`Trip ${i + 1} band ${b + 1} end`} value={fmtHHMMSS(band.end_secs)}
                         onchange={(e) => updateBand(trip.id, b, "end_secs", e.currentTarget.value)}>
                  <label>every
                    <input class="band-headway" aria-label={`Trip ${i + 1} band ${b + 1} headway minutes`} type="number" min="1"
                           value={band.headway_secs / 60}
                           onchange={(e) => updateBand(trip.id, b, "headway_secs", e.currentTarget.value, true)}>
                    min
                  </label>
                  <button aria-label={`Remove trip ${i + 1} band ${b + 1}`}
                          disabled={trip.departs.kind === "every" && trip.departs.bands.length <= 1}
                          onclick={() => removeBand(trip.id, b)}>remove</button>
                </li>
              {/each}
            </ul>
            <button onclick={() => addBand(trip.id)}>Add band</button>
          {/if}
          <button aria-label={`Remove trip ${i + 1}`} onclick={() => removeTrip(trip.id)}>remove</button>
        </li>
      {/each}
    </ul>
    <p class="ent-row">
      <input aria-label="New trip start" placeholder="HH:MM:SS" bind:value={addTime}>
      <button disabled={parseHHMMSS(addTime) === null}
              onclick={() => { addTrip(id, parseHHMMSS(addTime)); addTime = ""; }}>Add trip</button>
    </p>
    <p>
      <button aria-label="Remove this schedule"
              onclick={() => { removeSchedule(id); plannerBack(); }}>Remove schedule</button>
    </p>
  </section>
{:else}
  <p class="muted">Schedule not found (removed?).</p>
{/if}
