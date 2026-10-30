<script lang="ts">
  /// The pick timeline: each dated plan's governing period (plans
  /// `periods` — derived, never stored) as a bar on a date axis, with
  /// today marked and the plan in force (plans `resolve`) named. Bar
  /// placement is the only arithmetic here, and it is drawing.
  import type { PlanPeriod } from "../types/cap_plans/PlanPeriod";
  import type { PlanIssue } from "../types/cap_plans/PlanIssue";

  let {
    periods,
    names,
    inForce,
    issues,
    today,
    activeId,
  }: {
    periods: PlanPeriod[];
    names: Record<string, string>;
    inForce: string | null;
    issues: PlanIssue[];
    today: string;
    activeId: string;
  } = $props();

  const day = (iso: string) => Date.parse(`${iso}T00:00:00Z`) / 86_400_000;
  const span = $derived.by(() => {
    const dates = [today, ...periods.map((p) => p.effective_from), ...periods.flatMap((p) => (p.until ? [p.until] : []))].map(day);
    return { from: Math.min(...dates) - 30, to: Math.max(...dates) + 90 };
  });
  const pct = (d: number) => ((d - span.from) / (span.to - span.from)) * 100;
  const months = $derived.by(() => {
    const out: { label: string; left: number }[] = [];
    const start = new Date(span.from * 86_400_000);
    const d = new Date(Date.UTC(start.getUTCFullYear(), start.getUTCMonth() + 1, 1));
    while (d.getTime() / 86_400_000 < span.to) {
      out.push({ label: d.toLocaleDateString(undefined, { month: "short", year: d.getUTCMonth() === 0 ? "numeric" : undefined, timeZone: "UTC" }), left: pct(d.getTime() / 86_400_000) });
      d.setUTCMonth(d.getUTCMonth() + 1);
    }
    return out;
  });
</script>

<section class="panel" aria-label="Pick timeline" data-testid="pick-timeline">
  <header><h2>Pick timeline</h2><span class="muted">each plan governs from its pick until the next</span></header>
  {#if periods.length === 0}
    <p class="muted pad">No plan has a pick date yet. Fork the import into a dated plan to start the chain.</p>
  {:else}
    <div class="axis">
      {#each months as m (m.left)}<span class="tick" style:left="{m.left}%"><i>{m.label}</i></span>{/each}
      <span class="today" style:left="{pct(day(today))}%"><i>today</i></span>
    </div>
    {#each periods as p (p.id)}
      <div class="row">
        <span class="name" class:active={p.id === activeId}>
          {names[p.id] ?? p.id}
          {#if p.id === inForce}<em>in force</em>{/if}
        </span>
        <span class="track">
          <span
            class="bar"
            class:open={!p.until}
            class:active={p.id === activeId}
            style:left="{pct(day(p.effective_from))}%"
            style:width="{(p.until ? pct(day(p.until)) : 100) - pct(day(p.effective_from))}%"
            title="{p.effective_from} → {p.until ?? 'open'}"
          >{p.effective_from}{p.until ? ` → ${p.until}` : " →"}</span>
        </span>
      </div>
    {/each}
  {/if}
  {#if issues.length}
    <ul class="issues">
      {#each issues as issue, i (i)}<li>{JSON.stringify(issue)}</li>{/each}
    </ul>
  {/if}
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; }
  header { display: flex; gap: 0.6rem; align-items: baseline; padding: 0.7rem 1rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.8rem; }
  .pad { padding: 0.8rem 1rem; margin: 0; }
  .axis { position: relative; height: 1.6rem; margin: 0.4rem 1rem 0 12rem; }
  .tick, .today { position: absolute; top: 0; bottom: -100vh; border-left: 1px solid var(--line); pointer-events: none; }
  .tick i, .today i { position: absolute; left: 4px; font-style: normal; font-size: 0.7rem; color: var(--muted); white-space: nowrap; }
  .today { border-left: 2px solid var(--accent); }
  .today i { color: var(--accent); font-weight: 600; }
  section { overflow: hidden; }
  .row { display: flex; align-items: center; padding: 0.35rem 1rem; position: relative; }
  .name { width: 11rem; flex: 0 0 11rem; font-size: 0.85rem; font-weight: 500; }
  .name.active { color: var(--accent); }
  .name em { display: block; font-style: normal; font-size: 0.7rem; color: var(--ok); }
  .track { position: relative; flex: 1; height: 1.8rem; margin-left: 1rem; }
  .bar {
    position: absolute; top: 0; bottom: 0; border-radius: 4px; background: var(--hover); border: 1px solid var(--line);
    font-size: 0.72rem; padding: 0 0.4rem; display: flex; align-items: center; white-space: nowrap; overflow: hidden;
  }
  .bar.active { background: var(--accent-soft); border-color: var(--accent); }
  .bar.open { border-right-style: dashed; }
  .issues { margin: 0; padding: 0.5rem 1rem 0.8rem 2rem; color: var(--bad); font-size: 0.8rem; }
</style>
