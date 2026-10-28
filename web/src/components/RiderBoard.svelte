<script lang="ts">
  /// One stop, live — a RENDER component over cap-board's `stop_board`:
  /// the service alerts about it on top, its departures realtime-first
  /// (each with the expected time and its status under the live
  /// document's window), and what a detour took away from it. Picking
  /// a departure is an event (the map draws its route and its bus).
  import type { StopBoard } from "../types/StopBoard";
  import type { BoardRow } from "../types/BoardRow";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import DepartureRows from "./DepartureRows.svelte";
  import AlertList from "./AlertList.svelte";
  import { clock } from "./rideFormat";

  let {
    name,
    stopId,
    board,
    error,
    routes,
    off,
    liveOff,
    saved,
    picked,
    onpick,
    onsave,
    onfrom,
    onto,
    onback,
  }: {
    name: string;
    stopId: string;
    board: StopBoard | null;
    /// Asking the board failed (the capability loaded).
    error: string;
    routes: DirectoryRoute[];
    /// The board capability did not assemble.
    off: boolean;
    /// The realtime capability did not assemble.
    liveOff: boolean;
    saved: boolean;
    picked: { trip: string; start_secs: number } | null;
    onpick: (row: BoardRow) => void;
    onsave: () => void;
    onfrom: () => void;
    onto: () => void;
    onback: () => void;
  } = $props();
</script>

<section class="stop" data-testid="rider-board">
  <header>
    <button class="back" onclick={onback} aria-label="back">‹</button>
    <div class="title">
      <h2>{name}</h2>
      <span class="id">Stop #{stopId}</span>
    </div>
    <button class="save" class:on={saved} onclick={onsave} aria-pressed={saved} aria-label={saved ? "Remove from saved" : "Save this stop"} title={saved ? "Saved" : "Save"}>
      <svg viewBox="0 0 24 24" width="22" height="22"><path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z" fill={saved ? "currentColor" : "none"} stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" /></svg>
    </button>
  </header>
  <div class="actions">
    <button onclick={onfrom}>Directions from here</button>
    <button onclick={onto}>Directions to here</button>
  </div>

  {#if off}
    <p class="off">Departures are unavailable right now — the board capability did not load.</p>
  {:else if error}
    <p class="off">Couldn’t load departures for this stop right now ({error}).</p>
  {:else if !board}
    <div class="skeleton" aria-label="loading departures"><span></span><span></span><span></span></div>
  {:else}
    {#if board.alerts.length}
      <AlertList alerts={board.alerts} {routes} compact />
    {/if}

    {#if board.rows.length}
      <div data-testid="board-rows">
        <DepartureRows rows={board.rows} {routes} pooled={board.pooled} {picked} onpick={(r) => onpick(r as BoardRow)} />
      </div>
    {:else}
      <p class="quiet">No more departures from here today.</p>
    {/if}

    {#if board.skipped.length}
      <div class="detour" data-testid="board-skipped">
        <h3><span class="sign" aria-hidden="true">⤳</span> Detour — not stopping here</h3>
        <ul>
          {#each board.skipped as s (s.trip + s.start_secs + s.stop)}
            {@const c = clock(s.departure_secs)}
            <li><span class="t">{c.hm}<small>{c.ap}</small></span> <b>{s.route_name}</b> {s.destination}</li>
          {/each}
        </ul>
      </div>
    {/if}

    <p class="source">
      {#if board.live}
        <span class="dot"></span> Live predictions. Tap a departure to see its bus.
      {:else if liveOff}
        Live data is unavailable right now — times are the schedule.
      {:else}
        Times are the schedule — no live predictions right now.
      {/if}
    </p>
  {/if}
</section>

<style>
  header { display: flex; align-items: center; gap: 0.4rem; }
  .back, .save {
    border: 0; background: none; color: var(--text); cursor: pointer; border-radius: 10px;
    width: 2.25rem; height: 2.25rem; display: grid; place-items: center; font-size: 1.6rem; line-height: 1; flex: none;
  }
  .back:hover, .save:hover { background: var(--hover); }
  .save { color: var(--muted); }
  .save.on { color: var(--accent); }
  .title { flex: 1; min-width: 0; }
  h2 { font-size: 1.2rem; margin: 0; line-height: 1.2; }
  .id { color: var(--muted); font-size: 0.78rem; }
  .actions { display: flex; gap: 0.4rem; margin: 0.6rem 0 0.4rem; flex-wrap: wrap; }
  .actions button {
    border: 1px solid var(--line); background: var(--surface); color: var(--text); font: inherit; font-size: 0.85rem;
    padding: 0.35rem 0.75rem; border-radius: 999px; cursor: pointer;
  }
  .actions button:hover { border-color: var(--accent); color: var(--accent); }
  .detour { margin-top: 0.75rem; padding: 0.6rem 0.75rem; border-radius: 12px; background: color-mix(in srgb, var(--warn) 12%, transparent); }
  .detour h3 { font-size: 0.85rem; margin: 0 0 0.3rem; }
  .detour ul { list-style: none; margin: 0; padding: 0; font-size: 0.85rem; display: grid; gap: 0.15rem; }
  .detour .t { font-variant-numeric: tabular-nums; font-weight: 600; display: inline-block; min-width: 4rem; }
  .detour small { color: var(--muted); margin-left: 0.1em; }
  .sign { color: var(--warn); }
  .quiet { color: var(--muted); margin: 1rem 0; }
  .source { color: var(--muted); font-size: 0.78rem; margin: 0.75rem 0 0; display: flex; align-items: center; gap: 0.4rem; }
  .dot { width: 0.5rem; height: 0.5rem; border-radius: 50%; background: var(--ok); box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 25%, transparent); }
  .off { color: var(--bad); font-size: 0.9rem; }
  .skeleton { display: grid; gap: 0.5rem; padding: 0.75rem 0; }
  .skeleton span { height: 2.6rem; border-radius: 10px; background: linear-gradient(90deg, var(--hover), var(--line), var(--hover)); background-size: 200% 100%; animation: shimmer 1.2s infinite; }
  @keyframes shimmer { to { background-position: -200% 0; } }
</style>
