<script lang="ts">
  /// One trip, as a timeline — a RENDER component over cap-router's
  /// itinerary: each ride (its route, the sign the bus shows, where you
  /// board and get off, and how far a live prediction moved each time)
  /// and each walk between. Nothing is added up here.
  import type { Itinerary } from "../types/Itinerary";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import { clock, clockText, delayText, routeColor, walkText } from "./rideFormat";

  let { itinerary, routes }: { itinerary: Itinerary; routes: DirectoryRoute[] } = $props();

  const colors = $derived(new Map(routes.map((r) => [r.route_id, routeColor(r.color)])));
  const first = $derived(itinerary.legs.find((l) => l.type === "transit"));
</script>

<div class="itinerary" data-testid="itinerary">
  <p class="span">
    {#if first && first.type === "transit"}
      <b>{clock(first.board_secs).hm}<small>{clock(first.board_secs).ap}</small></b>
    {:else}
      <b>{clock(itinerary.depart_secs).hm}<small>{clock(itinerary.depart_secs).ap}</small></b>
    {/if}
    <span class="arrow">→</span>
    <b>{clock(itinerary.arrival_secs).hm}<small>{clock(itinerary.arrival_secs).ap}</small></b>
  </p>

  <ol class="legs">
    {#each itinerary.legs as leg, i (i)}
      {#if leg.type === "transit"}
        {@const late = delayText(leg.board_delay_secs)}
        <li class="ride" style:--route={colors.get(leg.route_id) ?? null}>
          <div class="stopline">
            <span class="t">{clockText(leg.board_secs)}</span>
            <span class="place">{leg.board_stop_name}</span>
          </div>
          <div class="body">
            <span class="badge">{leg.route_name}</span>
            <span class="to">to {leg.headsign}</span>
            {#if late}<span class="late" class:early={(leg.board_delay_secs ?? 0) < 0}>{late}</span>
            {:else if leg.board_delay_secs === 0}<span class="ontime">On time</span>{/if}
          </div>
          <div class="stopline end">
            <span class="t">{clockText(leg.alight_secs)}</span>
            <span class="place">Get off at {leg.alight_stop_name}</span>
          </div>
        </li>
      {:else}
        <li class="walk">
          <span>{walkText(leg.walk_secs)} to {leg.to_stop_name}</span>
        </li>
      {/if}
    {/each}
  </ol>
  <p class="arrive">Arrive <b>{itinerary.to_stop_name}</b> at <b>{clockText(itinerary.arrival_secs)}</b></p>
</div>

<style>
  .span { font-size: 1.5rem; margin: 0 0 0.6rem; font-variant-numeric: tabular-nums; letter-spacing: -0.02em; display: flex; align-items: baseline; gap: 0.5rem; }
  .span small { font-size: 0.55em; color: var(--muted); margin-left: 0.1em; font-weight: 600; }
  .arrow { color: var(--muted); font-size: 1.1rem; }
  .legs { list-style: none; margin: 0; padding: 0; display: grid; }
  .ride { position: relative; padding: 0.1rem 0 0.1rem 1.4rem; }
  .ride::before {
    content: ""; position: absolute; left: 0.38rem; top: 0.6rem; bottom: 0.6rem; width: 5px; border-radius: 3px;
    background: var(--route, var(--accent));
  }
  .stopline { display: flex; gap: 0.6rem; align-items: baseline; position: relative; }
  .stopline::before {
    content: ""; position: absolute; left: -1.25rem; top: 0.35rem; width: 9px; height: 9px; border-radius: 50%;
    background: var(--surface); border: 2.5px solid var(--route, var(--accent));
  }
  .t { font-variant-numeric: tabular-nums; font-weight: 650; min-width: 4.4rem; font-size: 0.9rem; }
  .place { font-size: 0.9rem; }
  .end .place { color: var(--muted); }
  .body { display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap; margin: 0.45rem 0 0.45rem 5rem; }
  .badge { background: var(--route, var(--accent)); color: #fff; font-weight: 750; border-radius: 6px; padding: 0.05rem 0.4rem; font-size: 0.85rem; }
  .to { font-weight: 600; font-size: 0.9rem; }
  .late { color: var(--warn); font-size: 0.8rem; font-weight: 650; }
  .late.early { color: var(--bad); }
  .ontime { color: var(--ok); font-size: 0.8rem; font-weight: 650; }
  .walk { position: relative; padding: 0.55rem 0 0.55rem 6.4rem; color: var(--muted); font-size: 0.85rem; }
  .walk::before { content: ""; position: absolute; left: 0.5rem; top: 0.2rem; bottom: 0.2rem; border-left: 3px dotted var(--muted); }
  .arrive { margin: 0.7rem 0 0; font-size: 0.92rem; }
</style>
