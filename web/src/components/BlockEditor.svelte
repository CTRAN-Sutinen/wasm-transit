<script>
  import { getNetworkDoc } from "../state.svelte.js";
  import {
    addBlock, removeBlock, addBlockTrip, removeBlockTrip, setBlockLayover,
    fmtHHMMSS, parseHHMMSS,
  } from "../planner.svelte.js";
  import FilteredSelect from "./FilteredSelect.svelte";

  const doc = $derived(getNetworkDoc());
  const blocks = $derived(doc ? Object.entries(doc.blocks ?? {}) : []);
  const serviceOptions = $derived(doc ? Object.keys(doc.services ?? {}).sort().map((id) => [id, id]) : []);

  // Per-block "add trip" form state.
  let addService = $state({});
  let addTime = $state({});

  const layoverOf = (block, afterTrip) =>
    block.layovers.find((l) => l.after_trip === afterTrip);
</script>

<section>
  <h2>Blocks</h2>
  {#each blocks as [id, block] (id)}
    <div class="block-editor">
      <p class="ent-row">
        <strong>{id}</strong>
        <span class="muted">{block.trips.length} trips</span>
        <button aria-label={`Remove block ${id}`} onclick={() => removeBlock(id)}>remove block</button>
      </p>
      <ul class="svc-rows">
        {#each block.trips as trip, i (i)}
          <li>
            <code>{i + 1}</code>
            <span class="muted">{trip.service}</span>
            <span class="time">{fmtHHMMSS(trip.start_secs)}</span>
            <button aria-label={`Remove trip ${i + 1} from block ${id}`}
                    onclick={() => removeBlockTrip(id, i)}>remove</button>
          </li>
        {/each}
      </ul>
      <p class="ent-row">
        <FilteredSelect label={`Service to add to block ${id}`} value={addService[id] ?? ""} options={serviceOptions} cap={200}
                        onchange={(sid) => (addService[id] = sid)} />
        <input aria-label={`Start time to add to block ${id}`} placeholder="HH:MM:SS"
               bind:value={addTime[id]}>
        <button disabled={!addService[id] || parseHHMMSS(addTime[id] ?? "") === null}
                onclick={() => {
                  addBlockTrip(id, addService[id], parseHHMMSS(addTime[id]));
                  addTime[id] = "";
                }}>
          Add trip
        </button>
      </p>

      {#if block.trips.length >= 2}
        <h3>Layovers</h3>
        <ul class="svc-rows">
          {#each block.trips.slice(0, -1) as _, i (i)}
            <li>
              <span class="muted">after trip {i + 1}</span>
              <label>
                <input aria-label={`Layover after trip ${i + 1} in block ${id}`} type="number" min="0"
                       value={layoverOf(block, i)?.secs / 60 ?? ""}
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
    </div>
  {/each}
  <p><button onclick={addBlock}>Add block</button></p>
  <p class="muted">Blocks chain one vehicle's trips; empty layover means none authored (the gap is still visible in the Dispatcher view).</p>
</section>
