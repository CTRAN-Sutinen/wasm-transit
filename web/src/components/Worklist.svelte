<script lang="ts">
  /// What is weak (analysis `network_findings`, D-136): frequency
  /// holes, long waits, and costly near misses at the hubs, each kind
  /// as the op ranked it. Against a baseline each finding says whether
  /// the plan made it or how it moved, and the baseline's findings the
  /// plan resolved close the list. A row opens its route or hub; the
  /// star watches it across changes.
  import type { NetworkFindings } from "../types/NetworkFindings";
  import { clock } from "./designFormat";

  let {
    findings,
    baselineName,
    watching,
    onhole,
    onwait,
    onmiss,
    onwatch,
  }: {
    findings: NetworkFindings;
    baselineName: string;
    watching: string[];
    onhole: (route: string, hour: number) => void;
    onwait: (stop: string) => void;
    onmiss: (hub: string) => void;
    onwatch: (key: string) => void;
  } = $props();

  const m = (s: number | null | undefined) => (s == null ? "—" : `${Math.round(s / 60)} min`);
  const total = $derived(findings.holes.length + findings.waits.length + findings.misses.length);
  const resolved = $derived(findings.resolved.holes.length + findings.resolved.waits.length + findings.resolved.misses.length);
</script>

{#snippet star(key: string)}
  <button type="button" class="star" aria-pressed={watching.includes(key)} aria-label="Watch this finding" title="Watch it across changes" onclick={() => onwatch(key)}>
    {watching.includes(key) ? "★" : "☆"}
  </button>
{/snippet}

{#snippet moved(now: number, base: number | null | undefined, hasBase: boolean)}
  {#if hasBase}
    {#if base == null}<span class="tag new">new in this plan</span>
    {:else if base !== now}<span class="tag">was {m(base)}</span>{/if}
  {/if}
{/snippet}

<section class="worklist" aria-label="Worklist" data-testid="worklist">
  <p class="muted lead">
    {total === 0 ? "Nothing stands out under these thresholds" : `${total} to look at`}{baselineName ? `, against ${baselineName}` : ""}.
    Read on {findings.date}, a date this service runs.
  </p>

  <h3>Frequency holes <span class="muted">an hour much longer between buses than the hours beside it</span></h3>
  {#if findings.holes.length === 0}<p class="muted none">None.</p>{/if}
  <ul>
    {#each findings.holes as row (row.now.key)}
      <li>
        {@render star(row.now.key)}
        <button type="button" class="open" onclick={() => onhole(row.now.route_id, row.now.hour)}>
          <strong>{row.now.name}</strong> at {row.now.hour % 24}:00 — a {m(row.now.longest_gap_secs)} gap, {m(row.now.around_gap_secs)} around it
        </button>
        {@render moved(row.now.longest_gap_secs, row.base?.longest_gap_secs, !!baselineName)}
      </li>
    {/each}
  </ul>

  <h3>Long waits <span class="muted">the longest wait for the next bus at a stop</span></h3>
  {#if findings.waits.length === 0}<p class="muted none">None.</p>{/if}
  <ul>
    {#each findings.waits as row (row.now.key)}
      <li>
        {@render star(row.now.key)}
        <button type="button" class="open" onclick={() => onwait(row.now.stop_id)}>
          <strong>{row.now.name}</strong> — {m(row.now.longest_wait_secs)} from {clock(row.now.wait_from_secs)}
        </button>
        {@render moved(row.now.longest_wait_secs, row.base?.longest_wait_secs, !!baselineName)}
      </li>
    {/each}
  </ul>

  <h3>Missed connections <span class="muted">at hubs: a bus leaves just before another arrives, and the next one is long after</span></h3>
  {#if findings.misses.length === 0}<p class="muted none">None.</p>{/if}
  <ul>
    {#each findings.misses as row (row.now.key)}
      <li>
        {@render star(row.now.key)}
        <button type="button" class="open" onclick={() => onmiss(row.now.hub_id)}>
          <strong>{row.now.hub_name}</strong>: {row.now.from_route} → {row.now.to_route}, {row.now.misses}× —
          {row.now.worst_wait_secs != null ? `up to ${m(row.now.worst_wait_secs)} for the next` : ""}{row.now.stranded ? `${row.now.worst_wait_secs != null ? ", " : ""}${row.now.stranded} with no later bus` : ""}
        </button>
        {#if baselineName}
          {#if row.base == null}<span class="tag new">new in this plan</span>
          {:else if row.base.misses !== row.now.misses}<span class="tag">was {row.base.misses}×</span>{/if}
        {/if}
      </li>
    {/each}
  </ul>

  {#if resolved > 0}
    <h3 class="fixed">Resolved against {baselineName}</h3>
    <ul class="resolved" data-testid="resolved">
      {#each findings.resolved.holes as f (f.key)}<li>{f.name} at {f.hour % 24}:00 — the {m(f.longest_gap_secs)} gap</li>{/each}
      {#each findings.resolved.waits as f (f.key)}<li>{f.name} — the {m(f.longest_wait_secs)} wait</li>{/each}
      {#each findings.resolved.misses as f (f.key)}<li>{f.hub_name}: {f.from_route} → {f.to_route}</li>{/each}
    </ul>
  {/if}
</section>

<style>
  .worklist { display: flex; flex-direction: column; gap: 0.2rem; font-size: 0.82rem; }
  .lead { margin: 0 0 0.4rem; }
  h3 { margin: 0.8rem 0 0.2rem; font-size: 0.8rem; display: flex; flex-direction: column; gap: 0.1rem; }
  h3 .muted { font-weight: 400; font-size: 0.72rem; }
  h3.fixed { color: var(--ok); }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; align-items: baseline; gap: 0.35rem; padding: 0.25rem 0; border-bottom: 1px solid var(--line); }
  .open { flex: 1; text-align: left; background: none; border: 0; padding: 0; font: inherit; color: var(--text); cursor: pointer; }
  .open:hover { color: var(--accent); }
  .star { background: none; border: 0; padding: 0 0.15rem; cursor: pointer; color: var(--accent); font-size: 0.95rem; line-height: 1; }
  .tag { font-size: 0.68rem; white-space: nowrap; color: var(--muted); border: 1px solid var(--line); border-radius: 4px; padding: 0 0.3rem; }
  .tag.new { color: var(--warn); border-color: var(--warn); }
  .none { margin: 0; }
  .resolved li { color: var(--ok); border-bottom-style: dashed; }
  .muted { color: var(--muted); }
</style>
