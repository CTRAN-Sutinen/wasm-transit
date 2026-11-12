<script lang="ts">
  /// The proposal across every day type (analysis `plan_compare`):
  /// each type's regular service before → after, the routes it
  /// changes, and the other combinations of the type with theirs.
  /// A day type opens it.
  import type { DayTypeComparison } from "../types/DayTypeComparison";
  import { change, count, fixed, weekdaysText } from "./designFormat";

  let {
    dayTypes,
    selected,
    onselect,
    onroute,
  }: { dayTypes: DayTypeComparison[]; selected: string; onselect: (group: string) => void; onroute: (route: string) => void } = $props();
</script>

<section class="plan-score" aria-label="The proposal by day type" data-testid="plan-score">
  {#each dayTypes as t (t.groups[0].group.id)}
    {@const regular = t.groups[0]}
    {@const c = regular.comparison}
    {@const others = t.groups.slice(1)}
    <article class:on={t.groups.some((g) => g.group.id === selected)}>
      <button type="button" class="head" title="Open this day type on the Scan" onclick={() => onselect(regular.group.id)}>
        <strong>{weekdaysText(t.weekdays)}</strong>
        <span class="muted">{regular.group.dates.length} days</span>
      </button>
      {#if c}
        <dl>
          <div><dt>Trips</dt><dd>{change(count(c.base.trips), count(c.proposal.trips))}</dd></div>
          <div><dt>Revenue hours</dt><dd>{change(fixed(c.base.revenue_hours), fixed(c.proposal.revenue_hours))}</dd></div>
          <div><dt>Routes changed</dt><dd>{c.routes.length}</dd></div>
        </dl>
        {#if c.routes.length}
          <p class="routes">
            {#each c.routes as r (r.route_id)}<button type="button" class="chip" onclick={() => onroute(r.route_id)}>{r.route_id}</button>{/each}
          </p>
        {/if}
      {/if}
      {#if others.length}
        <p class="muted small">Also runs as {others.map((g) => `${g.group.id} (${g.group.dates.length} d, ${g.comparison?.routes.length ?? 0} routes changed)`).join("; ")}.</p>
      {/if}
    </article>
  {/each}
</section>

<style>
  .plan-score { display: flex; flex-direction: column; gap: 0.5rem; }
  article { border: 1px solid var(--line); border-radius: 8px; padding: 0.5rem 0.7rem; background: var(--surface); }
  article.on { border-color: var(--accent); }
  .head { display: flex; justify-content: space-between; width: 100%; background: none; border: 0; padding: 0; font: inherit; color: var(--text); cursor: pointer; }
  dl { display: grid; grid-template-columns: repeat(3, 1fr); gap: 0.3rem; margin: 0.4rem 0 0; font-variant-numeric: tabular-nums; }
  dt { font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); }
  dd { margin: 0; font-size: 0.85rem; font-weight: 600; }
  .routes { display: flex; flex-wrap: wrap; gap: 0.25rem; margin: 0.4rem 0 0; }
  .chip { font: inherit; font-size: 0.75rem; border: 1px solid var(--accent); color: var(--accent); background: none; border-radius: 4px; padding: 0 0.35rem; cursor: pointer; }
  .muted { color: var(--muted); }
  .small { font-size: 0.75rem; margin: 0.3rem 0 0; }
</style>
