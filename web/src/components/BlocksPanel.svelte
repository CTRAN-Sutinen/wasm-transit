<script lang="ts">
  import { app } from "../state.svelte";
  import { refreshNow, serviceDate } from "../actions";
  import Combobox from "./Combobox.svelte";

  // Only the blocks running on the service day — a parked bus is not
  // a pickable block (the artifact scopes the list; trip counts are
  // that day's).
  const blocks = $derived.by(() => {
    void app.clock; // the service day can roll over
    void app.feedEpoch;
    return app.gtfs ? app.gtfs.blocks(serviceDate()) : [];
  });
  const options = $derived(
    blocks.map((b) => ({ value: b.block_id, label: b.block_id, sub: `${b.trip_count} trips` })),
  );
  let selectedBlock = $state("");

  const chain = $derived.by(() => {
    void app.clock; // time moved
    void app.feedEpoch; // the effective feed changed (edits, reload)
    if (!app.gtfs || !selectedBlock) return [];
    return app.gtfs.block_trips(selectedBlock, serviceDate());
  });

  $effect(() => {
    // Pick the first block when the day's list changes.
    if (blocks.length && !blocks.some((b) => b.block_id === selectedBlock)) {
      selectedBlock = blocks[0].block_id;
    }
  });

  function fmtLayover(s: number | null | undefined) {
    if (s === null || s === undefined) return "—";
    const min = Math.round(s / 60);
    return min >= 1 ? `${min} min` : `${s} s`;
  }

  // A diverted trip ending later eats into the vehicle's next layover.
  // Flag tight connections (< 5 min) and overlaps (the next trip now
  // starts before the previous one ends — a missed connection).
  function layoverFlag(chain: any[], i: number) {
    if (i === 0) return null;
    const prev = chain[i - 1];
    const t = chain[i];
    if (prev.end_time != null && t.start_time != null && t.start_time < prev.end_time) {
      return { label: "overlap", cls: "bad" };
    }
    if (t.layover_secs != null && t.layover_secs < 300) {
      return { label: "tight", cls: "warn" };
    }
    return null;
  }
</script>

{#if blocks.length}
<section>
  <h2>Blocks</h2>
  <p class="muted">One vehicle's day: consecutive trips, layovers included.</p>
  <p class="picker-row">
    <label for="block">Block:</label>
    <Combobox id="block" label="Block" options={options} bind:value={selectedBlock} />
  </p>
  <div class="table-scroll">
    <table>
    <tbody>
      <tr><th>#</th><th>start</th><th>end</th><th>route</th><th>trip</th><th>service</th><th>from → to</th><th>layover</th></tr>
      {#each chain as t, i (t.trip_id)}
        {@const flag = layoverFlag(chain, i)}
        <tr>
          <td>{i + 1}</td>
          <td>{t.start_time_hhmmss}</td>
          <td>{t.end_time_hhmmss}</td>
          <td>{t.route_name}</td>
          <td class="muted">{t.trip_id}</td>
          <td class="muted">{t.service_id}</td>
          <td>{t.first_stop_name} → {t.last_stop_name}</td>
          <td>
            {fmtLayover(t.layover_secs)}
            {#if flag}<span class="chip {flag.cls}">{flag.label}</span>{/if}
          </td>
        </tr>
      {/each}
    </tbody>
    </table>
  </div>
  <p class="muted">{chain.length} trips on {serviceDate()} — schedule only.</p>
</section>
{/if}
