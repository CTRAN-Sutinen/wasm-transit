<script>
  import { app } from "../state.svelte.js";
  import { planTrip } from "../actions.svelte.js";

  let from = $state("");
  let to = $state("");
  let departTime = $state("");

  const stops = $derived(app.gtfs ? app.gtfs.stops().slice(0, 2000) : []);
</script>

<section>
  <h2>Trip planner</h2>
  <p>
    <select aria-label="From" bind:value={from}>
      <option value="" disabled>From…</option>
      {#each stops as s (s.stop_id)}
        <option value={s.stop_id}>{s.name}</option>
      {/each}
    </select>
    <span aria-hidden="true">→</span>
    <select aria-label="To" bind:value={to}>
      <option value="" disabled>To…</option>
      {#each stops as s (s.stop_id)}
        <option value={s.stop_id}>{s.name}</option>
      {/each}
    </select>
  </p>
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
