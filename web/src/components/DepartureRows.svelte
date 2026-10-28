<script lang="ts">
  /// Departure rows — a RENDER component over a board's rows (cap-board
  /// `stop_board`, realtime-first with each row's expected time and
  /// status) or a watched stop's departures (`watchlist_board`, the
  /// schedule). The order, the times, and the statuses are the op's;
  /// this formats them and emits the row picked.
  import type { BoardRow } from "../types/BoardRow";
  import type { BoardDeparture } from "../types/BoardDeparture";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import { clock, routeColor, statusText, statusTone } from "./rideFormat";

  type Row = BoardRow | BoardDeparture;
  let {
    rows,
    routes,
    pooled = false,
    compact = false,
    picked = null,
    onpick,
  }: {
    rows: Row[];
    /// The directory's routes — each badge takes its route's color.
    routes: DirectoryRoute[];
    /// A station's pooled board: each row names its bay.
    pooled?: boolean;
    compact?: boolean;
    picked?: { trip: string; start_secs: number } | null;
    onpick?: (row: Row) => void;
  } = $props();

  const colors = $derived(new Map(routes.map((r) => [r.route_id, routeColor(r.color)])));
  const isLive = (r: Row): r is BoardRow => "status" in r;
</script>

<ul class="rows" class:compact>
  {#each rows as r (r.trip + ":" + r.start_secs + ":" + r.stop)}
    {@const live = isLive(r)}
    {@const when = clock(live && r.expected_secs != null ? r.expected_secs : r.departure_secs)}
    {@const moved = live && r.expected_secs != null && r.expected_secs !== r.departure_secs}
    {@const tone = live ? statusTone(r.status) : "sched"}
    <li>
      <button
        class:picked={picked?.trip === r.trip && picked?.start_secs === r.start_secs}
        disabled={!onpick}
        onclick={() => onpick?.(r)}
      >
        <span class="badge" style:--route={colors.get(r.route) ?? null}>{r.route_name}</span>
        <span class="where">
          <span class="dest">{r.destination}</span>
          {#if pooled || (!compact && r.route_long_name)}
            <span class="sub">
              {#if pooled}{r.stop_name}{:else}{r.route_long_name}{/if}
            </span>
          {/if}
        </span>
        <span class="when">
          <span class="time {tone}">{when.hm}<small>{when.ap}</small></span>
          <span class="status {tone}" data-status={live ? r.status : "no_prediction"}>
            {#if live && r.alerts.length}<span class="alert-dot" title="An alert applies to this trip">!</span>{/if}
            {#if moved}<s>{clock(r.departure_secs).hm}</s>{/if}
            {live ? statusText(r.status, r.delay_secs) : "Scheduled"}
          </span>
        </span>
      </button>
    </li>
  {/each}
</ul>

<style>
  .rows { list-style: none; margin: 0; padding: 0; }
  li + li { border-top: 1px solid var(--line); }
  button {
    width: 100%; display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 0.75rem;
    border: 0; background: none; color: var(--text); font: inherit; text-align: left;
    padding: 0.65rem 0.25rem; cursor: pointer; border-radius: 10px;
  }
  button:disabled { cursor: default; }
  button:not(:disabled):hover { background: var(--hover); }
  button.picked { background: var(--accent-soft); }
  .compact button { padding: 0.4rem 0.15rem; gap: 0.6rem; }
  .badge {
    min-width: 2.6rem; padding: 0.2rem 0.4rem; border-radius: 8px; text-align: center;
    font-weight: 750; font-size: 0.95rem; font-variant-numeric: tabular-nums; letter-spacing: -0.01em;
    background: var(--route, var(--accent)); color: #fff;
  }
  .compact .badge { min-width: 2.2rem; font-size: 0.85rem; padding: 0.1rem 0.3rem; border-radius: 6px; }
  .where { display: grid; min-width: 0; }
  .dest { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { color: var(--muted); font-size: 0.8rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .when { display: grid; justify-items: end; }
  .time { font-size: 1.35rem; font-weight: 700; font-variant-numeric: tabular-nums; line-height: 1.1; letter-spacing: -0.02em; }
  .compact .time { font-size: 1.05rem; }
  .time small { font-size: 0.6em; font-weight: 600; margin-left: 0.15em; color: var(--muted); }
  .status { font-size: 0.78rem; font-weight: 600; white-space: nowrap; display: flex; gap: 0.3rem; align-items: center; }
  .status s { color: var(--muted); font-weight: 400; }
  .ok { color: var(--ok); }
  .warn { color: var(--warn); }
  .bad { color: var(--bad); }
  .sched { color: var(--muted); font-weight: 500; }
  .time.sched { color: var(--text); }
  .alert-dot {
    display: inline-grid; place-items: center; width: 1rem; height: 1rem; border-radius: 50%;
    background: var(--warn); color: #fff; font-size: 0.7rem; font-weight: 800;
  }
</style>
