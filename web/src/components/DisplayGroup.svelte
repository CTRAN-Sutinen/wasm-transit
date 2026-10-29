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
    /// "row" (a line of a station's list), "hero" (one route filling
    /// the screen), "band" (one of a street stop's few, across the
    /// screen) — hero and band size to the size container their
    /// placement gives them.
    variant: "row" | "hero" | "band";
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
  /* one route, large: the route above its three times, spread over
     the height its placement gives it; the type follows that box, and
     a time never outgrows its third of the width */
  .hero {
    height: 100%; align-content: space-evenly; gap: 0.4em;
    font-size: clamp(1.4rem, min(11cqh, 4.6cqw), 9rem); padding: 0 0.3em;
  }
  .hero .times { gap: 0.6em; }
  .hero .wait { font-size: min(1.8em, 6.6cqw, 24cqh); }
  .hero .meta { font-size: max(0.4em, 1rem); }
  .hero .long { font-size: max(0.42em, 0.85rem); }
  .hero .none { font-size: 0.8em; }
  .hero header { flex-wrap: wrap; row-gap: 0.2em; }
  .hero .dest { white-space: normal; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; line-height: 1.1; }
  /* narrow: the times stack down the height, each with its status
     beside it */
  @container (max-width: 40rem) {
    .hero { font-size: clamp(1rem, min(6cqh, 8cqw), 4rem); }
    .hero .times { grid-template-columns: minmax(0, 1fr); gap: 0.5em; }
    .hero .times li { grid-template-columns: auto minmax(0, 1fr); align-items: baseline; column-gap: 0.6em; }
    .hero .wait { font-size: min(1.7em, 14cqw, 11cqh); }
  }
  /* one of a street stop's few: route and destination, then the
     times, across the band; the type follows the band's height */
  .band {
    height: 100%; grid-template-columns: minmax(0, 1fr) minmax(0, 1.5fr); align-items: center; column-gap: 1.2em;
    font-size: clamp(1rem, min(15cqh, 3.4cqw), 7rem); padding: 0 0.8em;
    border: 1px solid var(--line); border-radius: 0.3em;
  }
  .band header { flex-wrap: wrap; row-gap: 0.2em; }
  .band .dest { white-space: normal; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; line-height: 1.1; }
  .band .long { font-size: max(0.42em, 0.85rem); }
  .band .times { gap: 0.8em; }
  .band .meta { font-size: max(0.36em, 0.85rem); }
  .band .none { font-size: 0.7em; }
  .band .note { grid-column: 2; font-size: max(0.36em, 0.85rem); }
  @container (max-width: 40rem) {
    .band { grid-template-columns: minmax(0, 1fr); align-content: center; row-gap: 0.4em; font-size: clamp(1rem, min(11cqh, 7cqw), 4rem); }
    .band .note { grid-column: 1; }
  }
</style>
