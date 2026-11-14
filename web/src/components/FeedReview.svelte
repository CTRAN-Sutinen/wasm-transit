<script lang="ts">
  /// What the agency's feed did to the stops record (D-150: the feed
  /// applies, and the planner sees what it did), as a review page: the
  /// feed's first service date, then three tables — the changes the feed
  /// recorded (cap-stops `feed_events`), the stops in service it does not
  /// list, and the stops whose difference did not fit their history
  /// (`import_events` run without writing) — each stop opening its
  /// record, where a change is corrected. No GTFS control lives here.
  /// A RENDER component.
  import type { Event } from "../types/Event";
  import { EVENT_WORDS } from "./stopWords";

  let {
    feed,
    held,
    onstop,
  }: {
    feed: { on: string; recorded: { stop: string; event: Event }[]; unlisted: string[]; unapplied: string[] } | null;
    /// A stops record is kept (else there is nothing to compare).
    held: boolean;
    onstop: (stop: string) => void;
  } = $props();

  function what(e: Event): string {
    if (e.event === "changed") {
      const fields = Object.keys(e).filter((k) => !["on", "event", "note", "from_feed", "extra"].includes(k));
      return `changed (${fields.map((f) => f.replaceAll("_", " ")).join(", ")})`;
    }
    return (EVENT_WORDS[e.event] ?? e.event).toLowerCase();
  }
</script>

<section class="feed" data-testid="feed-notice">
  <header class="bar">
    <h1>Feed review</h1>
    {#if feed}
      <p class="muted">Feed in effect from {feed.on}: recorded {feed.recorded.length} {feed.recorded.length === 1 ? "change" : "changes"}; does not list {feed.unlisted.length} {feed.unlisted.length === 1 ? "stop in service" : "stops in service"}; {feed.unapplied.length} to check by hand.</p>
    {/if}
  </header>
  <div class="scroll">
    {#if !feed}
      <p class="muted" data-testid="feed-nothing">{held ? "No feed changes to review." : "No stops record yet."}</p>
    {:else}
      <h2>Changes recorded from the feed <span class="n">{feed.recorded.length}</span></h2>
      {#if feed.recorded.length}
        <table>
          <thead><tr><th>Stop</th><th>Change</th><th>Effective</th></tr></thead>
          <tbody>
            {#each feed.recorded as a, i (i)}
              <tr><td><button type="button" class="link" onclick={() => onstop(a.stop)}>{a.stop}</button></td><td>{a.stop} {what(a.event)}, from {a.event.on}</td><td class="num">{a.event.on}</td></tr>
            {/each}
          </tbody>
        </table>
      {:else}<p class="muted">None.</p>{/if}
      <h2>In service, not in the feed <span class="n">{feed.unlisted.length}</span></h2>
      <p class="muted small">Each keeps its history. Suspend or retire it on its record if the feed is right.</p>
      {#if feed.unlisted.length}
        <ul class="stops">{#each feed.unlisted as s (s)}<li><button type="button" class="link" onclick={() => onstop(s)}>{s}</button></li>{/each}</ul>
      {:else}<p class="muted">None.</p>{/if}
      <h2>To check by hand <span class="n">{feed.unapplied.length}</span></h2>
      <p class="muted small">The feed's record does not fit these stops' history, so nothing was recorded for them.</p>
      {#if feed.unapplied.length}
        <ul class="stops">{#each feed.unapplied as s (s)}<li><button type="button" class="link" onclick={() => onstop(s)}>{s}</button></li>{/each}</ul>
      {:else}<p class="muted">None.</p>{/if}
    {/if}
  </div>
</section>

<style>
  .feed { display: grid; grid-template-rows: auto minmax(0, 1fr); height: 100%; min-height: 0; }
  .bar { padding: 0.9rem 1.6rem; border-bottom: 1px solid var(--line); }
  h1 { margin: 0; font-size: 1.3rem; }
  .bar p { margin: 0.25rem 0 0; }
  .scroll { overflow: auto; padding: 1rem 1.6rem 2rem; max-width: 70rem; }
  h2 { font-size: 1rem; margin: 1.2rem 0 0.5rem; }
  h2:first-child { margin-top: 0; }
  .n { color: var(--muted); font-weight: 400; font-size: 0.85rem; }
  .small { font-size: 0.82rem; margin: 0 0 0.5rem; }
  table { width: 100%; border-collapse: collapse; font-size: 0.92rem; }
  th { text-align: left; color: var(--muted); font-size: 0.82rem; padding: 0.4rem 0.6rem; border-bottom: 1px solid var(--line); }
  td { padding: 0.45rem 0.6rem; border-bottom: 1px solid var(--line); }
  .num { font-variant-numeric: tabular-nums; }
  .stops { list-style: none; padding: 0; margin: 0; display: flex; flex-wrap: wrap; gap: 0.3rem 0.9rem; }
  .link { background: none; border: none; padding: 0; color: var(--accent); cursor: pointer; text-decoration: underline; font: inherit; }
</style>
