<script lang="ts">
  /// Service alerts — a RENDER component over the live document's
  /// alerts in force (all of them, or the ones cap-board's `stop_board`
  /// says speak about a stop). Which alerts apply is the op's; this
  /// shows each one's header, text, link, and what it names.
  import type { AlertEffect } from "../types/AlertEffect";
  import type { LiveAlert } from "../types/LiveAlert";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";

  let { alerts, routes, compact = false }: { alerts: LiveAlert[]; routes: DirectoryRoute[]; compact?: boolean } = $props();

  const names = $derived(new Map(routes.map((r) => [r.route_id, r.name])));

  const EFFECT: Partial<Record<AlertEffect, string>> = {
    no_service: "No service",
    reduced_service: "Reduced service",
    significant_delays: "Delays",
    detour: "Detour",
    additional_service: "Added service",
    modified_service: "Service change",
    stop_moved: "Stop moved",
    no_effect: "Notice",
    accessibility_issue: "Accessibility",
  };

  function effect(a: LiveAlert): string {
    return EFFECT[a.effect] ?? "Alert";
  }

  /// The routes and stops an alert names, as the rider reads them.
  function scope(a: LiveAlert): string {
    if (a.scopes.some((s) => !s.route && !s.stop && !s.trip)) return "All service";
    const rs = [...new Set(a.scopes.map((s) => s.route).filter((r): r is string => !!r))].map((r) => names.get(r) ?? r);
    return rs.length ? `Route${rs.length > 1 ? "s" : ""} ${rs.join(", ")}` : "";
  }
</script>

<ul class="alerts" class:compact data-testid="alerts">
  {#each alerts as a (a.id)}
    <li class={a.severity}>
      <details open={!compact}>
        <summary>
          <span class="kind">{effect(a)}</span>
          {#if scope(a)}<span class="scope">{scope(a)}</span>{/if}
          <span class="header">{a.header}</span>
        </summary>
        {#if a.description}<p>{a.description}</p>{/if}
        {#if a.url}<a href={a.url} target="_blank" rel="noopener">More from the agency ↗</a>{/if}
      </details>
    </li>
  {/each}
</ul>

<style>
  .alerts { list-style: none; margin: 0.5rem 0; padding: 0; display: grid; gap: 0.5rem; }
  li { border-radius: 12px; padding: 0.6rem 0.75rem; background: color-mix(in srgb, var(--warn) 12%, transparent); border-left: 4px solid var(--warn); }
  li.severe { background: color-mix(in srgb, var(--bad) 12%, transparent); border-left-color: var(--bad); }
  summary { cursor: pointer; list-style: none; display: grid; gap: 0.1rem; }
  summary::-webkit-details-marker { display: none; }
  .kind { font-size: 0.7rem; font-weight: 750; text-transform: uppercase; letter-spacing: 0.06em; color: var(--warn); }
  li.severe .kind { color: var(--bad); }
  .scope { font-size: 0.75rem; color: var(--muted); }
  .header { font-weight: 650; font-size: 0.92rem; }
  p { margin: 0.4rem 0 0; font-size: 0.85rem; white-space: pre-line; }
  a { display: inline-block; margin-top: 0.4rem; font-size: 0.82rem; color: var(--accent); }
</style>
