<script lang="ts">
  import { app } from "../state.svelte";
  import { planTrip } from "../actions";
  import Combobox from "./Combobox.svelte";

  let from = $state("");
  let to = $state("");
  let departTime = $state("");

  const stops = $derived(app.gtfs ? app.gtfs.stops().slice(0, 2000) : []);
  const stopOptions = $derived(stops.map((s) => ({ value: s.stop_id, label: s.name, sub: s.stop_id })));
</script>

<section>
  <h2>Trip planner</h2>
  <p><Combobox label="From" placeholder="From…" options={stopOptions} bind:value={from} /></p>
  <p><Combobox label="To" placeholder="To…" options={stopOptions} bind:value={to} /></p>
  <p>
    <label for="plan-time">Leave</label>
    <input type="time" id="plan-time" bind:value={departTime}>
    <button class="primary" onclick={() => planTrip(from, to, departTime)}>Plan trip</button>
  </p>
  {#if app.planNotice}
    <p class="plan-notice">{app.planNotice}</p>
  {/if}
  <p class="muted">Earliest arrival, up to 6 legs, walking transfers ≤ 300 m. The plan appears on the map.</p>
</section>
