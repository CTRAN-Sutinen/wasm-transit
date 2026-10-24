<script>
  import { app } from "../state.svelte.js";
  import { refreshNow, serviceDate } from "../actions.svelte.js";

  const blocks = $derived(app.gtfs ? app.gtfs.blocks() : []);
  let selectedBlock = $state("");

  const chain = $derived.by(() => {
    void app.clock; // time moved
    void app.feedEpoch; // the effective feed changed (edits, reload)
    if (!app.gtfs || !selectedBlock) return [];
    return app.gtfs.block_trips(selectedBlock, serviceDate());
  });

  $effect(() => {
    // Pick the first block when the feed changes.
    if (blocks.length && !blocks.some((b) => b.block_id === selectedBlock)) {
      selectedBlock = blocks[0].block_id;
    }
  });

  function fmtLayover(s) {
    if (s === null || s === undefined) return "—";
    const min = Math.round(s / 60);
    return min >= 1 ? `${min} min` : `${s} s`;
  }

  // A diverted trip ending later eats into the vehicle's next layover.
  // Flag tight connections (< 5 min) and overlaps (the next trip now
  // starts before the previous one ends — a missed connection).
  function layoverFlag(chain, i) {
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
  <p>
    <label for="block">Block:</label>
    <select id="block" bind:value={selectedBlock}>
      {#each blocks as b (b.block_id)}
        <option value={b.block_id}>{b.block_id} ({b.trip_count} trips)</option>
      {/each}
    </select>
  </p>
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
  <p class="muted">{chain.length} trips on {serviceDate()} — schedule only.</p>
</section>
{/if}
