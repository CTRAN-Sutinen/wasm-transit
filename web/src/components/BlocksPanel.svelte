<script lang="ts">
  /// The blocks view — a RENDER component over cap-runcut's answers
  /// (the day's blocks; one block's authored chain with the layover
  /// ripple and its health; each row names its trip by id). It
  /// formats and emits; it classifies nothing.
  import type { DayBlock } from "../types/DayBlock";
  import type { BlockChainTrip } from "../types/BlockChainTrip";
  import type { LayoverHealth } from "../types/LayoverHealth";
  import Combobox from "./Combobox.svelte";

  let {
    blocks,
    off,
    selected,
    chain,
    chainOff,
    date,
    onselect,
  }: {
    blocks: DayBlock[];
    off: boolean;
    selected: string;
    chain: { trips: BlockChainTrip[] } | null;
    chainOff: boolean;
    date: string;
    onselect: (blockId: string) => void;
  } = $props();

  const options = $derived(
    blocks.map((b) => ({ value: b.block_id, label: b.block_id, sub: `${b.trip_count} trips` })),
  );

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

  const FLAG: Record<LayoverHealth, { label: string; cls: string } | null> = {
    overlap: { label: "overlap", cls: "bad" },
    tight: { label: "tight", cls: "warn" },
    ok: null,
  };
</script>

{#if blocks.length}
<section>
  <h2>Blocks</h2>
  <p class="muted">One vehicle's day: consecutive trips, layovers included.</p>
  <p class="picker-row">
    <label for="block">Block:</label>
    <Combobox id="block" label="Block" {options} value={selected} onchange={onselect} />
  </p>
  {#if chainOff}
    <p class="muted">
      The chain needs the network and runcut capabilities; the bus
      did not assemble them, so the chain is off.
    </p>
  {:else if chain?.trips.length}
  <div class="table-scroll">
    <table>
    <tbody>
      <tr><th>#</th><th>start</th><th>end</th><th>route</th><th>trip</th><th>from → to</th><th>layover</th></tr>
      {#each chain.trips as t, i (t.trip + "@" + t.start_secs)}
        {@const flag = t.layover_health ? FLAG[t.layover_health] : null}
        <tr class:muted={!t.runs} title={t.runs ? "" : "this trip does not run today — the authored hole"}>
          <td>{i + 1}</td>
          <td>{fmtHMS(t.runs ? t.start_secs : null)}</td>
          <td>{fmtHMS(t.end_secs)}</td>
          <td>{t.route_name}</td>
          <td class="muted">{t.trip}</td>
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
  <p class="muted">{chain.trips.length} trips on {date} — schedule only.</p>
  {:else if chain}
    <p class="muted">
      No authored chain for this block in the network document — its
      trips are unrepresentable in the model (single-stop trips drop
      at import, reported in the import notes).
    </p>
  {/if}
</section>
{:else if off}
<section>
  <h2>Blocks</h2>
  <p class="muted">
    The blocks view needs the runcut capability; it did not assemble, so
    the view is off. Authoring and boards elsewhere are unaffected.
  </p>
</section>
{/if}
