<script lang="ts">
  import { app, getNetworkDoc } from "../state.svelte";
  import {
    removeBlock, addBlockTrip, removeBlockTrip, setBlockLayover,
    plannerBack, fmtHHMMSS, parseHHMMSS,
  } from "../planner";
  import Combobox from "./Combobox.svelte";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "block" ? app.plannerSelection.id : "");
  const block = $derived(id ? doc?.blocks?.[id] ?? null : null);
  const scheduleOptions = $derived((app.pickers?.schedule ?? []).map((r) => ({ value: r.id, label: r.id })));

  let addScheduleId = $state("");
  let addTime = $state("");

  const layoverOf = (afterTrip: number) => block?.layovers.find((l) => l.after_trip === afterTrip);
</script>

{#if block}
  <h3>Block <code>{id}</code> <span class="muted">({block.trips.length} trips)</span></h3>
  <ul class="svc-rows">
    {#each block.trips as trip, i (i)}
      <li>
        <code>{i + 1}</code>
        <span class="muted">{trip.schedule}</span>
        <span class="time">{fmtHHMMSS(trip.start_secs)}</span>
        <button aria-label={`Remove trip ${i + 1}`} onclick={() => removeBlockTrip(id, i)}>remove</button>
      </li>
    {/each}
  </ul>
  <p class="ent-row">
    <Combobox label="Schedule to add" options={scheduleOptions} bind:value={addScheduleId} />
    <input aria-label="Start time to add" placeholder="HH:MM:SS" bind:value={addTime}>
    <button disabled={!addScheduleId || parseHHMMSS(addTime) === null}
            onclick={() => { addBlockTrip(id, addScheduleId, parseHHMMSS(addTime)); addTime = ""; }}>
      Add trip
    </button>
  </p>

  {#if block.trips.length >= 2}
    <h4>Layovers</h4>
    <ul class="svc-rows">
      {#each block.trips.slice(0, -1) as _, i (i)}
        {@const layover = layoverOf(i)}
        <li>
          <span class="muted">after trip {i + 1}</span>
          <label>
            <input aria-label={`Layover after trip ${i + 1}`} type="number" min="0"
                   value={layover ? layover.secs / 60 : ""}
                   onchange={(e) => {
                     const mins = e.currentTarget.value;
                     setBlockLayover(id, i, mins === "" ? null : Math.round(Number(mins) * 60));
                   }}>
            min
          </label>
        </li>
      {/each}
    </ul>
  {/if}
  <p><button aria-label="Remove this block"
             onclick={() => { removeBlock(id); plannerBack(); }}>Remove block</button></p>
{:else}
  <p class="muted">Block not found (removed?).</p>
{/if}
