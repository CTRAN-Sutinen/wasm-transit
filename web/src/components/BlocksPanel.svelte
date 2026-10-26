<script lang="ts">
  import { app } from "../state.svelte";
  import { refreshNow, serviceDate, feedBytes } from "../actions";
  import { ensureGtfsFeed, gtfsCap, gtfsModule } from "../capabilities.svelte";
  import type { BlockSummary } from "../types/BlockSummary";
  import Combobox from "./Combobox.svelte";

  // The dispatcher's core view answers through the BUS (cap-gtfs) —
  // the monolith's op is deleted, this is the only blocks view. Only
  // the blocks running on the service day (a parked bus is not a
  // pickable block; trip counts are that day's). Async with race
  // guards; when the capability cannot assemble the view is OFF and
  // says so — never silently.
  let blocks = $state<BlockSummary[]>([]);
  let blocksOff = $state(false);
  let blocksSeq = 0;
  $effect(() => {
    void app.clock; // the service day can roll over
    void app.feedEpoch; // the effective feed changed (reload, materialize)
    const date = serviceDate();
    const seq = ++blocksSeq;
    const bytes = feedBytes();
    if (!app.gtfs || !bytes) {
      blocks = [];
      return;
    }
    void (async () => {
      try {
        const feedHash = await ensureGtfsFeed(bytes);
        if (seq !== blocksSeq) return;
        if (!feedHash) {
          blocks = [];
          blocksOff = true;
          app.status =
            "gtfs capability unavailable — the blocks view is off until it loads (the capability bus did not assemble).";
          return;
        }
        const module = await gtfsModule();
        if (!module || seq !== blocksSeq) return;
        blocksOff = false;
        blocks = gtfsCap(module).blocks(feedHash, date);
      } catch {
        if (seq === blocksSeq) blocks = [];
      }
    })();
  });

  const options = $derived(
    blocks.map((b) => ({ value: b.block_id, label: b.block_id, sub: `${b.trip_count} trips` })),
  );
  let selectedBlock = $state("");

  // One block's chain — MONOLITH-backed deliberately: the chain's
  // layovers ripple with diversions (t10 diverted +6 min shrinks the
  // next layover, flagged tight/overlap — the dispatcher's
  // connection-health read), and the diversion document has no bus
  // home yet. The chain moves to the bus with diversions; the blocks
  // LIST above is already bus-side.
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
{:else if blocksOff}
<section>
  <h2>Blocks</h2>
  <p class="muted">
    The blocks view needs the gtfs capability; it did not assemble, so
    the view is off. Authoring and boards elsewhere are unaffected.
  </p>
</section>
{/if}
