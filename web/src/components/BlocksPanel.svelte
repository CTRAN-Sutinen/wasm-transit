<script lang="ts">
  import { app, getEditDocument } from "../state.svelte";
  import { refreshNow, serviceDate, feedBytes } from "../actions";
  import {
    runcutCap,
    runcutModule,
    ensureRuncutBoarding,
    ensureRuncutEdits,
  } from "../capabilities.svelte";
  import type { DayBlock } from "../types/DayBlock";
  import type { BlockChainTrip } from "../types/BlockChainTrip";
  import Combobox from "./Combobox.svelte";

  // The blocks LIST answers through cap-runcut — the runcut
  // capability's index facet (the monolith's op is deleted). Only
  // the blocks with authored trips running on the service day (a
  // parked bus is not a pickable block; trip counts are that day's;
  // a block whose only trips were unrepresentable in the model is
  // not offered — the honest hole). Async with race guards; when the
  // capability cannot assemble the view is OFF and says so — never
  // silently.
  let blocks = $state<DayBlock[]>([]);
  let blocksOff = $state(false);
  let blocksSeq = 0;
  $effect(() => {
    void app.clock; // the service day can roll over
    void app.feedEpoch; // the effective feed changed (reload, materialize)
    const date = serviceDate();
    const seq = ++blocksSeq;
    const bytes = feedBytes();
    if (!app.feedLoaded || !bytes) {
      blocks = [];
      return;
    }
    void (async () => {
      try {
        const boarding = await ensureRuncutBoarding(bytes);
        if (seq !== blocksSeq) return;
        if (!boarding) {
          blocks = [];
          blocksOff = true;
          app.status =
            "runcut capability unavailable — the blocks view is off until it loads (the capability bus did not assemble).";
          return;
        }
        const editsHash = await ensureRuncutEdits(boarding.networkHash, getEditDocument());
        if (seq !== blocksSeq) return;
        const module = await runcutModule();
        if (!module || seq !== blocksSeq) return;
        blocksOff = false;
        blocks = runcutCap(module).blocks(boarding.networkHash, editsHash, date);
      } catch {
        if (seq === blocksSeq) blocks = [];
      }
    })();
  });

  const options = $derived(
    blocks.map((b) => ({ value: b.block_id, label: b.block_id, sub: `${b.trip_count} trips` })),
  );
  let selectedBlock = $state("");

  // -----------------------------------------------------------------
  // ONE BLOCK'S CHAIN — through cap-runcut: the AUTHORED runcut
  // (the model's blocks carry layovers GTFS never had), materialized
  // for the day with the operator edits applied. The ripple is IN
  // the answer: layover_remaining_secs is the authored layover minus
  // the trip's delay — negative means the next trip starts late (a
  // missed connection), under 5 minutes is tight. The old UI
  // computed the flag from raw times; the op now carries it.
  // -----------------------------------------------------------------
  let chain = $state<(BlockChainTrip & { agencyTripIds: string[] })[]>([]);
  let chainOff = $state(false);
  let chainSeq = 0;
  $effect(() => {
    void app.clock;
    void app.feedEpoch; // edits changed the effective day
    const blockId = selectedBlock;
    const seq = ++chainSeq;
    const bytes = feedBytes();
    chain = [];
    if (!blockId || !app.feedLoaded || !bytes) return;
    void (async () => {
      try {
        const boarding = await ensureRuncutBoarding(bytes);
        if (seq !== chainSeq || !boarding) {
          if (seq === chainSeq) chainOff = !boarding;
          return;
        }
        const editsHash = await ensureRuncutEdits(boarding.networkHash, getEditDocument());
        if (seq !== chainSeq) return;
        const module = await runcutModule();
        if (!module || seq !== chainSeq) return;
        const trips = runcutCap(module).blockTrips(
          boarding.networkHash,
          editsHash,
          blockId,
          serviceDate(),
        );
        // Agency ids for the trip column — the radio's vocabulary,
        // joined through the import's trip-id map.
        const agency = new Map(
          boarding.tripMap.entries.map((e) => [`${e.schedule}@${e.start_secs}`, e.agency_trip_ids]),
        );
        chainOff = false;
        chain = trips.map((t) => ({
          ...t,
          agencyTripIds: agency.get(`${t.schedule}@${t.start_secs}`) ?? [],
        }));
      } catch {
        if (seq === chainSeq) {
          chain = [];
          chainOff = true;
        }
      }
    })();
  });

  $effect(() => {
    // Pick the first block when the day's list changes.
    if (blocks.length && !blocks.some((b) => b.block_id === selectedBlock)) {
      selectedBlock = blocks[0].block_id;
    }
  });

  function fmtHMS(secs: number | null) {
    if (secs === null || secs === undefined) return "—";
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }

  function fmtLayover(s: number | null | undefined) {
    if (s === null || s === undefined) return "—";
    const min = Math.round(s / 60);
    return min >= 1 ? `${min} min` : `${s} s`;
  }

  // The connection-health flag, straight from the op's number:
  // negative = the delay ate the whole layover (the next trip
  // starts late — a missed connection); under 5 min = tight.
  function layoverFlag(t: BlockChainTrip) {
    const s = t.layover_remaining_secs;
    if (s === null || s === undefined) return null;
    const secs = Number(s);
    if (secs < 0) return { label: "overlap", cls: "bad" };
    if (secs < 300) return { label: "tight", cls: "warn" };
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
  {#if chainOff}
    <p class="muted">
      The chain needs the network and runcut capabilities; the bus
      did not assemble them, so the chain is off.
    </p>
  {:else if chain.length}
  <div class="table-scroll">
    <table>
    <tbody>
      <tr><th>#</th><th>start</th><th>end</th><th>route</th><th>trip</th><th>from → to</th><th>layover</th></tr>
      {#each chain as t, i (t.schedule + t.start_secs)}
        {@const flag = layoverFlag(t)}
        <tr class:muted={!t.runs} title={t.runs ? "" : "this trip does not run today — the authored hole"}>
          <td>{i + 1}</td>
          <td>{fmtHMS(t.runs ? t.start_secs : null)}</td>
          <td>{fmtHMS(t.end_secs)}</td>
          <td>{t.route_name}</td>
          <td class="muted">{t.agencyTripIds.join(", ")}</td>
          <td>{t.first_stop} → {t.last_stop}</td>
          <td>
            {fmtLayover(t.layover_remaining_secs === null ? null : Number(t.layover_remaining_secs))}
            {#if flag}<span class="chip {flag.cls}">{flag.label}</span>{/if}
          </td>
        </tr>
      {/each}
    </tbody>
    </table>
  </div>
  <p class="muted">{chain.length} trips on {serviceDate()} — schedule only.</p>
  {:else}
    <p class="muted">
      No authored chain for this block in the network document — its
      trips are unrepresentable in the model (single-stop trips drop
      at import, reported in the import notes).
    </p>
  {/if}
</section>
{:else if blocksOff}
<section>
  <h2>Blocks</h2>
  <p class="muted">
    The blocks view needs the runcut capability; it did not assemble, so
    the view is off. Authoring and boards elsewhere are unaffected.
  </p>
</section>
{/if}
