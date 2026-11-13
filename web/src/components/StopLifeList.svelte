<script lang="ts">
  /// The stops the filters show on a date (cap-stops `stops_on`), each
  /// with its status, since when, until when, and its note — "every stop
  /// out of service today", or every one planned. A click opens it. A
  /// RENDER component: the stops come in as props.
  import type { StopOn } from "../types/StopOn";
  import { STATUS_WORDS } from "./stopWords";

  let { stops, onstop }: { stops: StopOn[]; onstop: (stop: string) => void } = $props();

  /// A long list is shown in pages, the first few hundred at once.
  let limit = $state(200);
</script>

<section class="stop-list" data-testid="stop-list">
  <p class="muted">{stops.length} {stops.length === 1 ? "stop" : "stops"}</p>
  <ul>
    {#each stops.slice(0, limit) as s (s.stop_id)}
      <li>
        <button type="button" onclick={() => onstop(s.stop_id)} data-testid="stop-row">
          <span class="name">{s.name || s.stop_id}</span>
          <span class="muted">{s.stop_id}{s.code && s.code !== s.stop_id ? ` · ${s.code}` : ""}</span>
          <span class="status {s.status}">{STATUS_WORDS[s.status]}{s.since ? ` since ${s.since}` : ""}{s.until ? `, until ${s.until}` : ""}</span>
          {#if s.note}<span class="muted note">{s.note}</span>{/if}
        </button>
      </li>
    {/each}
  </ul>
  {#if stops.length > limit}
    <button type="button" onclick={() => (limit += 200)}>Show more</button>
  {/if}
</section>

<style>
  ul { list-style: none; padding: 0; margin: 0.3rem 0; }
  li button {
    width: 100%; text-align: left; background: none; border: 0; border-bottom: 1px solid var(--line); padding: 0.4rem 0.2rem;
    display: flex; flex-wrap: wrap; gap: 0.1rem 0.6rem; align-items: baseline; color: inherit; font: inherit; cursor: pointer;
  }
  li button:hover { background: var(--hover); }
  .name { font-weight: 600; }
  .status { font-size: 0.85rem; }
  .status.suspended, .status.retired { color: var(--bad); }
  .note { flex-basis: 100%; font-size: 0.85rem; }
</style>
