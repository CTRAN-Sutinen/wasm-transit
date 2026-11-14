<script lang="ts">
  /// The stops the filters show on a date (cap-stops `stops_on`), each
  /// with its status, since when, until when, and its note — "every stop
  /// out of service today", or every one planned — or, asked from the
  /// device's location, the stops nearest it (cap-stops `stops_near`),
  /// each with how far it is. A click opens it. A RENDER component: the
  /// stops come in as props.
  import type { StopOn } from "../types/StopOn";
  import { STATUS_COLORS, STATUS_WORDS } from "./stopWords";

  let {
    stops,
    near = false,
    onstop,
  }: {
    stops: (StopOn & { distance_m?: number })[];
    /// The rows are the stops nearest the device, nearest first.
    near?: boolean;
    onstop: (stop: string) => void;
  } = $props();

  /// A long list is shown in pages, the first few hundred at once.
  let limit = $state(200);
  const far = (m: number) => (m < 1000 ? `${Math.round(m)} m` : `${(m / 1000).toFixed(1)} km`);
</script>

<section class="stop-list" data-testid={near ? "near-list" : "stop-list"}>
  <p class="muted">{near ? `${stops.length} nearest` : `${stops.length} ${stops.length === 1 ? "stop" : "stops"}`}</p>
  <ul>
    {#each stops.slice(0, limit) as s (s.stop_id)}
      <li>
        <button type="button" onclick={() => onstop(s.stop_id)} data-testid={near ? "near-row" : "stop-row"}>
          <i class="dot" style="--fill:{STATUS_COLORS[s.status][0]};--edge:{STATUS_COLORS[s.status][1]}"></i>
          <span class="name">{s.name || s.stop_id}</span>
          {#if s.distance_m != null}<span class="far">{far(s.distance_m)}</span>{/if}
          <span class="line">
            <span class="muted">{s.stop_id}{s.code && s.code !== s.stop_id ? ` · ${s.code}` : ""}</span>
            <span class="status {s.status}">{STATUS_WORDS[s.status]}{s.since ? ` since ${s.since}` : ""}{s.until ? `, until ${s.until}` : ""}</span>
          </span>
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
    width: 100%; text-align: left; background: none; border: 0; border-bottom: 1px solid var(--line); padding: 0.5rem 0.2rem;
    display: flex; flex-wrap: wrap; gap: 0.1rem 0.5rem; align-items: center; color: inherit; font: inherit; cursor: pointer;
  }
  li button:hover { background: var(--hover); }
  .dot { width: 10px; height: 10px; border-radius: 50%; border: 1.5px solid var(--edge); background: var(--fill); flex: none; }
  .name { font-weight: 600; flex: 1; min-width: 0; }
  .far { font-variant-numeric: tabular-nums; font-weight: 600; font-size: 0.9rem; }
  .line { flex-basis: 100%; display: flex; flex-wrap: wrap; gap: 0 0.6rem; padding-left: 1.1rem; }
  .status { font-size: 0.85rem; }
  .status.suspended, .status.retired { color: var(--bad); }
  .note { flex-basis: 100%; font-size: 0.85rem; padding-left: 1.1rem; }
  @media (pointer: coarse) { li button { padding: 0.75rem 0.2rem; } }
</style>
