<script lang="ts">
  /// What the open plan changes against its baseline under the group
  /// (analysis `service_type_compare`): each changed route, before →
  /// after; and undo / redo, which put back a document version.
  import type { ServiceTypeComparison } from "../types/ServiceTypeComparison";
  import { change, count, fixed, mins } from "./designFormat";

  let {
    comparison,
    baselineName,
    canUndo,
    canRedo,
    onundo,
    onredo,
    onroute,
  }: {
    comparison: ServiceTypeComparison | null;
    baselineName: string;
    canUndo: boolean;
    canRedo: boolean;
    onundo: () => void;
    onredo: () => void;
    onroute: (route: string) => void;
  } = $props();
</script>

<section class="panel" aria-label="Change ledger" data-testid="change-ledger">
  <header>
    <h2>Changes</h2>
    <span class="muted">{baselineName ? `vs ${baselineName}` : "no baseline"}</span>
    <span class="undo">
      <button type="button" onclick={onundo} disabled={!canUndo}>Undo</button>
      <button type="button" onclick={onredo} disabled={!canRedo}>Redo</button>
    </span>
  </header>
  {#if !comparison}
    <p class="muted pad">Choose a baseline to see what this plan changes.</p>
  {:else if comparison.routes.length === 0}
    <p class="muted pad">No route changes under {comparison.name}.</p>
  {:else}
    <ul>
      {#each comparison.routes as r (r.route_id)}
        <li>
          <button type="button" onclick={() => onroute(r.route_id)}>{r.name}</button>
          {#if !r.base}<span class="tag">new</span>{:else if !r.proposal}<span class="tag">gone</span>{/if}
          <dl>
            <dt>Trips</dt><dd>{change(count(r.base?.trips), count(r.proposal?.trips))}</dd>
            <dt>Rev. h</dt><dd>{change(fixed(r.base?.revenue_hours), fixed(r.proposal?.revenue_hours))}</dd>
            <dt>Typical gap</dt><dd>{change(mins(r.base?.typical_headway_secs), mins(r.proposal?.typical_headway_secs))} min</dd>
          </dl>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; }
  header { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: baseline; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  .pad { padding: 0.7rem 0.9rem; margin: 0; }
  .undo { margin-left: auto; display: flex; gap: 0.3rem; }
  .undo button { font: inherit; font-size: 0.78rem; padding: 0.15rem 0.5rem; border-radius: 6px; border: 1px solid var(--line); background: var(--surface); color: var(--text); cursor: pointer; }
  .undo button:disabled { opacity: 0.4; cursor: default; }
  ul { list-style: none; margin: 0; padding: 0; max-height: 22rem; overflow-y: auto; }
  li { padding: 0.5rem 0.9rem; border-bottom: 1px solid var(--line); font-size: 0.8rem; }
  li > button { font: inherit; font-weight: 600; font-family: ui-monospace, monospace; border: 0; background: none; padding: 0; color: var(--accent); cursor: pointer; }
  .tag { font-size: 0.7rem; padding: 0 0.3rem; border: 1px solid var(--line); border-radius: 4px; margin-left: 0.3rem; }
  dl { display: grid; grid-template-columns: auto 1fr; gap: 0.1rem 0.6rem; margin: 0.3rem 0 0; font-variant-numeric: tabular-nums; }
  dt { color: var(--muted); }
  dd { margin: 0; }
</style>
