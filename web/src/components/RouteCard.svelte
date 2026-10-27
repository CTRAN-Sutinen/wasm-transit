<script lang="ts">
  /// One route's composition — cap-directory `route_detail`: its
  /// patterns, the stops they visit, and its schedules grouped by
  /// service type with each schedule's trips. RENDER only; the group
  /// for the type under study opens first.
  import type { RouteDetail } from "../types/RouteDetail";

  let {
    routeId,
    detail,
    serviceType,
    selectedSchedule,
    onschedule,
  }: {
    routeId: string;
    detail: RouteDetail;
    serviceType: string;
    selectedSchedule: string;
    onschedule: (scheduleId: string) => void;
  } = $props();
</script>

<section class="route-card" data-testid="route-card">
  <h3>Route {routeId}</h3>
  <div class="muted">{detail.patterns.length} patterns · {detail.stops.length} stops</div>
  <ul class="patterns">
    {#each detail.patterns as p (p.id)}<li>{p.name}</li>{/each}
  </ul>
  {#each detail.schedules_by_type as g (g.service_type)}
    <details open={g.service_type === serviceType}>
      <summary>{g.name} <span class="muted">({g.schedules.length} schedules)</span></summary>
      <ul class="schedules">
        {#each g.schedules as s (s.id)}
          <li>
            <button class:sel={s.id === selectedSchedule} onclick={() => onschedule(s.id)}>
              <span class="ctx">{s.context ?? s.name}</span>
              <span class="n">{s.trips ?? "—"} trips</span>
            </button>
          </li>
        {/each}
      </ul>
    </details>
  {/each}
</section>

<style>
  h3 { margin: 0; font-size: 0.95rem; }
  .muted { color: var(--muted); font-size: 0.85rem; }
  ul { list-style: none; margin: 0.25rem 0; padding: 0; font-size: 0.85rem; }
  .patterns li { padding: 1px 0; }
  summary { cursor: pointer; font-size: 0.9rem; margin-top: 0.25rem; }
  .schedules button {
    display: flex; justify-content: space-between; gap: 0.5rem; width: 100%;
    background: none; border: 0; padding: 2px 6px; text-align: left; font: inherit; color: inherit; cursor: pointer;
  }
  .schedules button:hover { background: var(--hover); }
  .schedules button.sel { background: var(--accent-soft); }
  .ctx { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .n { white-space: nowrap; font-variant-numeric: tabular-nums; }
</style>
