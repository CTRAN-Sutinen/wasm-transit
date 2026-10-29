<script lang="ts">
  /// One group on a stop's display — a RENDER component over one
  /// `grouped_board` group: the route (its authored colors), where it
  /// is going, and its next departures, each as the row's `wait` says
  /// it, live or scheduled, and (at a station) the bay it leaves from.
  /// A group with none left says why: the edits took it from this
  /// stop, or it has stopped for the day.
  import type { DepartureGroup } from "../types/DepartureGroup";
  import { routeColor, statusText, statusTone, waitText } from "./rideFormat";

  let {
    group,
    variant,
    bays = false,
  }: {
    group: DepartureGroup;
    /// "row" (a line of a station's list), "hero" (one route, large),
    /// "card" (one of several, in a grid).
    variant: "row" | "hero" | "card";
    /// Show each departure's bay.
    bays?: boolean;
  } = $props();

  const bg = $derived(routeColor(group.route_color) ?? "var(--accent)");
  const fg = $derived(routeColor(group.route_text_color) ?? "#fff");
</script>

<article class="group {variant}" data-testid="display-group" data-route={group.route}>
  <header>
    <span class="badge" style:background={bg} style:color={fg}>{group.route_name}</span>
    <div class="where">
      <span class="dest" data-testid="display-dest">{group.destination}</span>
      {#if group.route_long_name && variant !== "row"}<span class="long">{group.route_long_name}</span>{/if}
    </div>
  </header>
  {#if group.departures.length}
    <ol class="times">
      {#each group.departures as d (d.trip + d.start_secs + d.stop)}
        <li class={statusTone(d.status)} data-testid="display-time">
          <span class="wait">{waitText(d.wait)}</span>
          <span class="meta">
            {#if d.status !== "no_prediction"}<span class="live" aria-label="live">●</span>{/if}
            {statusText(d.status, d.delay_secs)}
            {#if bays}<span class="bay">{d.platform ? `Bay ${d.platform}` : d.stop_name}</span>{/if}
          </span>
        </li>
      {/each}
    </ol>
  {:else if group.skipped.length}
    <p class="none detour" data-testid="display-none">Detoured — not serving this stop</p>
  {:else}
    <p class="none" data-testid="display-none">No more service today</p>
  {/if}
  {#if group.departures.length && group.skipped.length}
    <p class="note">Some trips detoured away from this stop</p>
  {/if}
</article>

<style>
  .group { display: grid; gap: 0.6em; min-width: 0; }
  header { display: flex; align-items: center; gap: 0.6em; min-width: 0; }
  .badge {
    flex: none; min-width: 2.6em; padding: 0.15em 0.45em; border-radius: 0.3em;
    font-weight: 800; text-align: center; font-variant-numeric: tabular-nums;
  }
  .where { display: grid; min-width: 0; }
  .dest { font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .long { color: var(--muted); font-size: 0.6em; }
  .times { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 0.5em; }
  .times li { display: grid; gap: 0.1em; }
  .wait { font-weight: 800; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .meta { font-size: 0.5em; color: var(--muted); display: flex; flex-wrap: wrap; gap: 0.4em; align-items: center; }
  .live { color: var(--ok); }
  li.warn .meta { color: var(--warn); }
  li.bad .meta { color: var(--bad); }
  li.ok .meta { color: var(--ok); }
  .bay { border: 1px solid var(--line); border-radius: 0.3em; padding: 0 0.35em; color: var(--text); font-weight: 700; }
  .none { margin: 0; color: var(--muted); font-weight: 600; }
  .none.detour, .note { color: var(--warn); }
  .note { margin: 0; font-size: 0.5em; }

  /* a line of a station's list: route, destination, then the times */
  .row { grid-template-columns: minmax(0, 1.1fr) minmax(0, 1.4fr); align-items: center; font-size: clamp(1rem, 2.6vh, 2.2rem); padding: 0.45em 0; border-bottom: 1px solid var(--line); }
  .row .note { grid-column: 2; }
  /* one route, large */
  .hero { font-size: clamp(1.4rem, 6vh, 5rem); gap: 0.4em; }
  .hero .times { gap: 1em; }
  .hero .meta { font-size: 0.32em; }
  .hero .long { font-size: 0.45em; }
  .hero .none { font-size: 0.6em; }
  /* one of several */
  .card { font-size: clamp(1rem, 3.2vh, 2.6rem); padding: 0.6em 0.7em; border: 1px solid var(--line); border-radius: 0.4em; align-content: start; }
  .card .dest { white-space: normal; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .card .wait { font-size: 0.85em; }
</style>
