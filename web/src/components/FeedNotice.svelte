<script lang="ts">
  /// What the agency's feed did to the stops record (D-150: the feed
  /// applies, and the planner sees what it did). One line — the feed's
  /// first service date, how many changes it recorded (cap-stops
  /// `feed_events`), how many stops in service it does not list — and,
  /// opened, each change and each stop, linked to its card, where a
  /// change is fixed (a move that was a survey correction made a
  /// correction of the earlier point). No GTFS control lives here.
  /// A RENDER component.
  import type { Event } from "../types/Event";
  import { EVENT_WORDS } from "./stopWords";

  let {
    feed,
    onstop,
  }: {
    feed: { on: string; recorded: { stop: string; event: Event }[]; unlisted: string[]; unapplied: string[] };
    onstop: (stop: string) => void;
  } = $props();

  let open = $state(false);
  const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
  const parts = $derived(
    [
      feed.recorded.length ? `recorded ${plural(feed.recorded.length, "change", "changes")}` : "",
      feed.unlisted.length ? `does not list ${plural(feed.unlisted.length, "stop in service", "stops in service")}` : "",
      feed.unapplied.length ? `has ${plural(feed.unapplied.length, "stop to check by hand", "stops to check by hand")}` : "",
    ].filter(Boolean),
  );

  /// What one change recorded, in a few words.
  function what(e: Event): string {
    if (e.event === "moved") return `moved, from ${e.on}`;
    if (e.event === "in_service") return `in service, from ${e.on}`;
    if (e.event === "changed") {
      const fields = Object.keys(e).filter((k) => !["on", "event", "note", "from_feed", "extra"].includes(k));
      return `changed (${fields.map((f) => f.replaceAll("_", " ")).join(", ")}), from ${e.on}`;
    }
    return `${(EVENT_WORDS[e.event] ?? e.event).toLowerCase()}, from ${e.on}`;
  }
</script>

<section class="feed-notice" data-testid="feed-notice">
  <p>
    <strong>The agency's feed</strong> (service from {feed.on}) {parts.join("; ")}.
    <button type="button" class="link" onclick={() => (open = !open)} aria-expanded={open} data-testid="feed-review">{open ? "Hide" : "Review"}</button>
  </p>
  {#if open}
    {#if feed.recorded.length}
      <h4>Changes the feed recorded</h4>
      <p class="muted small">Open a stop to fix one: a move that was a correction is fixed on the earlier point.</p>
      <ul>
        {#each feed.recorded as a, i (i)}
          <li><button type="button" class="link" onclick={() => onstop(a.stop)}>{a.stop}</button> {what(a.event)}</li>
        {/each}
      </ul>
    {/if}
    {#if feed.unlisted.length}
      <h4>In service here, not in the feed</h4>
      <p class="muted small">Each keeps its history. Suspend or retire it on its card if the feed is right.</p>
      <ul class="inline">
        {#each feed.unlisted as s (s)}<li><button type="button" class="link" onclick={() => onstop(s)}>{s}</button></li>{/each}
      </ul>
    {/if}
    {#if feed.unapplied.length}
      <h4>To check by hand</h4>
      <p class="muted small">The feed's record does not fit these stops' history (a retired stop it still lists, for example), so nothing was recorded for them.</p>
      <ul class="inline">
        {#each feed.unapplied as s (s)}<li><button type="button" class="link" onclick={() => onstop(s)}>{s}</button></li>{/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .feed-notice { border: 1px solid color-mix(in srgb, var(--warn) 50%, var(--line)); border-radius: 6px; padding: 0.4rem 0.6rem; margin-bottom: 0.6rem; font-size: 0.9rem; }
  p { margin: 0.1rem 0; }
  h4 { font-size: 0.85rem; margin: 0.5rem 0 0.15rem; color: var(--muted); }
  ul { margin: 0.1rem 0 0.4rem; padding-left: 1rem; }
  ul.inline { list-style: none; padding: 0; display: flex; flex-wrap: wrap; gap: 0.2rem 0.6rem; }
  .small { font-size: 0.8rem; }
  .link { background: none; border: none; padding: 0; color: var(--accent); cursor: pointer; text-decoration: underline; font: inherit; }
</style>
