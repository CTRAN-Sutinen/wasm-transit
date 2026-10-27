<script lang="ts">
  /// The departure board — a RENDER component. Everything it shows is
  /// cap-board's answer (`stop_board`: pooling, realtime-first order,
  /// each row's expected time, delay, and status under the live
  /// document's window); it formats and emits, and decides nothing.
  import type { StopBoard } from "../types/StopBoard";
  import type { BoardRow } from "../types/BoardRow";
  import type { LiveStatus } from "../types/LiveStatus";
  import Combobox from "./Combobox.svelte";

  let {
    stop,
    stopOptions,
    board,
    off,
    services,
    selected,
    onstop,
    onrefresh,
    onselect,
  }: {
    stop: string | null;
    stopOptions: { value: string; label: string; sub?: string }[];
    board: StopBoard | null;
    /// The bus could not assemble the board.
    off: boolean;
    services: string[];
    /// The selected trip's key ("schedule@start").
    selected: string | null;
    onstop: (stopId: string) => void;
    onrefresh: () => void;
    onselect: (row: BoardRow) => void;
  } = $props();

  function fmtClock(secs: number) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
  }

  const CHIP: Record<LiveStatus, string | null> = {
    early: "warn",
    on_time: "ok",
    late: "warn",
    very_late: "bad",
    no_prediction: null,
  };

  function chipLabel(row: BoardRow) {
    if (row.status === "on_time") return "on time";
    const min = Math.round(Math.abs(row.delay_secs ?? 0) / 60);
    return row.status === "early" ? `−${min} min` : `+${min} min`;
  }
</script>

<section>
  <h2>Departures</h2>
  <p class="picker-row">
    <Combobox label="Stop" options={stopOptions} value={stop ?? ""} onchange={onstop} />
    <button onclick={onrefresh}>Refresh</button>
    {#each services as s (s)}
      <span class="chip svc" title="service key running today">{s}</span>
    {/each}
  </p>

  {#if board && board.rows.length}
    <table>
      <tbody>
        {#each board.rows as d (d.pattern_trips + d.start_secs + d.stop)}
          <tr
            class="clickable-row"
            class:selected={!board.pooled && selected === `${d.pattern_trips}@${d.start_secs}`}
            onclick={() => onselect(d)}
          >
            <td class="time">
              {fmtClock(d.departure_secs)}
              {#if d.expected_secs != null && d.expected_secs !== d.departure_secs}
                <span class="expected" class:late={d.status === "late" || d.status === "very_late"} class:early={(d.delay_secs ?? 0) < 0}>
                  → {fmtClock(d.expected_secs)}
                </span>
              {/if}
            </td>
            <td><span class="route-badge">{d.route_name}</span></td>
            <td>{d.destination}</td>
            {#if board.pooled}
              <td class="muted" title="the concrete bay this leaves from">{d.stop}</td>
            {/if}
            <td>{#if CHIP[d.status]}<span class="chip {CHIP[d.status]}">{chipLabel(d)}</span>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if board.pooled}
      <p class="muted">
        Station board — every bay at this stop's group, tagged by bay.
        {#if board.live}Times include live delays.{/if}
      </p>
    {:else}
      <p class="muted">Click a departure to draw its path. {#if board.live}Times include live delays.{/if}</p>
    {/if}
  {:else if off && stop}
    <p class="muted">
      The board needs the network and board capabilities; the bus
      did not assemble them, so the board is off. Other views are
      unaffected.
    </p>
  {:else if stop}
    <p class="muted">Nothing upcoming at this stop right now.</p>
  {/if}
</section>
