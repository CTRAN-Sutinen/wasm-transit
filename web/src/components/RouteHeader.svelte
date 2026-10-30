<script lang="ts">
  /// The route on top of its workbench: its name, the group it runs
  /// under, and its numbers there (its `route_frequency` row), before
  /// → after when the plan changes it (`service_type_compare`).
  import type { RouteFrequency } from "../types/RouteFrequency";
  import type { ServiceTypeComparison } from "../types/ServiceTypeComparison";
  import { change, clock, count, fixed } from "./designFormat";

  let {
    route,
    routeName,
    longName,
    groupName,
    frequency,
    comparison,
    editing,
    onedit,
  }: {
    route: string;
    routeName: string;
    longName: string | null;
    groupName: string;
    frequency: RouteFrequency | null;
    comparison: ServiceTypeComparison | null;
    editing: boolean;
    onedit: () => void;
  } = $props();

  const row = $derived(frequency?.routes.find((r) => r.route_id === route)?.stats ?? null);
  const delta = $derived(comparison?.routes.find((r) => r.route_id === route) ?? null);
  const pair = (f: (s: NonNullable<typeof row>) => string) =>
    delta?.base && delta.proposal ? change(f(delta.base), f(delta.proposal)) : row ? f(row) : "—";
</script>

<header class="route-head" data-testid="route-header">
  <div class="name">
    <h1><span class="id">{routeName}</span>{#if longName}<span class="long">{longName}</span>{/if}</h1>
    <span class="muted">{groupName}{delta ? " · changed in this plan" : ""}</span>
  </div>
  <div class="stat"><small>Trips</small><b>{pair((s) => count(s.trips))}</b></div>
  <div class="stat"><small>Span</small><b>{pair((s) => `${clock(s.first_start_secs)}–${clock(s.last_arrival_secs)}`)}</b></div>
  <div class="stat"><small>Rev. hours</small><b>{pair((s) => fixed(s.revenue_hours))}</b></div>
  <div class="stat"><small>Rev. miles</small><b>{pair((s) => count(Math.round(s.revenue_miles)))}</b></div>
  <button type="button" class:on={editing} onclick={onedit} aria-keyshortcuts="E" data-testid="edit-toggle">{editing ? "Done editing" : "Edit (E)"}</button>
</header>

<style>
  .route-head { display: flex; flex-wrap: wrap; align-items: center; gap: 1.2rem; padding: 0.7rem 1rem; background: var(--surface); border-bottom: 1px solid var(--line); font-variant-numeric: tabular-nums; }
  h1 { margin: 0; font-size: 1.15rem; }
  .id { font-family: ui-monospace, monospace; }
  .long { margin-left: 0.5rem; font-weight: 500; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  .stat small { display: block; color: var(--muted); font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.04em; }
  .stat b { font-size: 0.95rem; }
  button { margin-left: auto; font: inherit; font-weight: 600; padding: 0.35rem 0.9rem; border-radius: 6px; border: 1px solid var(--text); background: var(--text); color: var(--surface); cursor: pointer; }
  button.on { background: var(--accent); border-color: var(--accent); }
</style>
