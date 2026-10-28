<script lang="ts">
  import Combobox from "./Combobox.svelte";

  type Option = { value: string; label: string; sub?: string };
  let { stopOptions, notice, onplan }: {
    /// The directory's stop list as picker options.
    stopOptions: Option[];
    /// What the router's notice tells the rider about the last plan
    /// (no trip, a later arrival, a diverted ride), if anything.
    notice: string | null;
    onplan: (from: string, to: string, depart: string) => void;
  } = $props();

  let from = $state("");
  let to = $state("");
  let departTime = $state("");
</script>

<section>
  <h2>Trip planner</h2>
  <p><Combobox label="From" placeholder="From…" options={stopOptions} bind:value={from} /></p>
  <p><Combobox label="To" placeholder="To…" options={stopOptions} bind:value={to} /></p>
  <p>
    <label for="plan-time">Leave</label>
    <input type="time" id="plan-time" bind:value={departTime}>
    <button class="primary" onclick={() => onplan(from, to, departTime)}>Plan trip</button>
  </p>
  {#if notice}
    <p class="plan-notice">{notice}</p>
  {/if}
  <p class="muted">Earliest arrival, up to 6 legs, walking transfers ≤ 300 m. The plan appears on the map.</p>
</section>
