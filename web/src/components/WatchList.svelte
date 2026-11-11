<script lang="ts">
  /// The findings the planner watches (analysis `network_findings`'
  /// `watched`), re-read after every change: each one now beside the
  /// baseline — "fixed" when this version no longer has it, "new" when
  /// the baseline did not. The loop's "did it work", kept on screen.
  import type { Watched } from "../types/Watched";
  import type { AnyFinding } from "../types/AnyFinding";
  import { clock } from "./designFormat";

  let { watched, baselineName, onunwatch }: { watched: Watched[]; baselineName: string; onunwatch: (key: string) => void } = $props();

  const m = (s: number) => `${Math.round(s / 60)} min`;

  function title(f: AnyFinding): string {
    if (f.kind === "hole") return `${f.name} at ${f.hour % 24}:00`;
    if (f.kind === "wait") return f.name;
    return `${f.hub_name}: ${f.from_route} → ${f.to_route}`;
  }

  function measure(f: AnyFinding): string {
    if (f.kind === "hole") return `${m(f.longest_gap_secs)} gap`;
    if (f.kind === "wait") return `${m(f.longest_wait_secs)} from ${clock(f.wait_from_secs)}`;
    return `${f.misses}× missed`;
  }
</script>

<section class="watch" aria-label="Watching" data-testid="watch-list">
  <h3>Watching <span class="muted">re-read after every change{baselineName ? `, against ${baselineName}` : ""}</span></h3>
  {#if watched.length === 0}
    <p class="muted">Star a finding in the worklist to follow it here while you change the plan.</p>
  {/if}
  <ul>
    {#each watched as w (w.key)}
      {@const f = w.now ?? w.base}
      <li class:fixed={!w.now} data-testid="watched">
        <span class="what">{f ? title(f) : w.key}</span>
        <span class="state">
          {#if !w.now}<strong>fixed</strong>{w.base ? ` (was ${measure(w.base)})` : ""}
          {:else}{measure(w.now)}{#if baselineName}{w.base ? (measure(w.base) !== measure(w.now) ? ` — was ${measure(w.base)}` : " — unchanged") : " — new"}{/if}{/if}
        </span>
        <button type="button" class="x" aria-label="Stop watching" onclick={() => onunwatch(w.key)}>×</button>
      </li>
    {/each}
  </ul>
</section>

<style>
  .watch { font-size: 0.8rem; }
  h3 { margin: 0 0 0.3rem; font-size: 0.8rem; }
  h3 .muted { font-weight: 400; font-size: 0.72rem; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; gap: 0.5rem; align-items: baseline; padding: 0.2rem 0; border-bottom: 1px solid var(--line); }
  .what { flex: 1; font-weight: 600; }
  .state { color: var(--muted); font-variant-numeric: tabular-nums; }
  li.fixed .state { color: var(--ok); }
  .x { background: none; border: 0; cursor: pointer; color: var(--muted); font-size: 1rem; line-height: 1; }
  .muted { color: var(--muted); }
  p { margin: 0; }
</style>
