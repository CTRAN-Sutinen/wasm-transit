<script lang="ts">
  import { openText, timeText, whenText } from "./disruptionsFormat";
  import type { AlertGap } from "../types/AlertGap";
  import type { AlertSource } from "../types/AlertSource";
  import type { AlertList } from "../types/AlertList";
  import type { AlertRow } from "../types/AlertRow";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import type { ServiceAlert } from "../types/ServiceAlert";
  import Tablist from "./primitives/Tablist.svelte";

  let {
    gaps,
    list,
    routes,
    zone,
    busy,
    gapsOff,
    onstart,
    onedit,
    onend,
  }: {
    gaps: AlertGap[];
    list: AlertList | null;
    routes: DirectoryRoute[];
    zone: string | undefined;
    busy: boolean;
    gapsOff: string;
    onstart: (source: AlertSource | null) => void;
    onedit: (id: string, alert: ServiceAlert) => void;
    onend: (id: string) => void;
  } = $props();

  type Tab = "in_force" | "upcoming" | "ended" | "drafts";
  const TABS: { id: Tab; label: string }[] = [
    { id: "in_force", label: "In force" },
    { id: "upcoming", label: "Upcoming" },
    { id: "drafts", label: "Drafts" },
    { id: "ended", label: "Ended" },
  ];
  let tab = $state<Tab>("in_force");
  const rows = $derived<AlertRow[]>(list ? list[tab] : []);
  const name = (id: string | undefined) =>
    id ? (routes.find((r) => r.route_id === id)?.name ?? id) : "";

  const EFFECT: Record<string, string> = {
    no_service: "No service",
    reduced_service: "Reduced service",
    significant_delays: "Delays",
    detour: "Detour",
    additional_service: "Added service",
    modified_service: "Service change",
    stop_moved: "Stop moved",
    no_effect: "Notice",
    accessibility_issue: "Accessibility",
    other: "Other",
    unknown: "Alert",
  };

  function gapTitle(g: AlertGap): string {
    if (g.source.kind === "stop")
      return `Stop ${g.stops?.[0] ?? g.source.stop} closed`;
    const route = name(g.informed.find((e) => e.route)?.route);
    if (g.trip)
      return `${route} · ${g.start_secs != null ? timeText(g.start_secs) : "Every"} trip cancelled`;
    return `${route} detour · ${g.stops?.length ?? 0} stops not served`;
  }

  function period(a: ServiceAlert): string {
    const p = a.communication?.[0];
    if (!p) return "No period";
    return `${p.start == null ? "Now" : whenText(p.start, zone)} – ${whenText(p.end, zone)}`;
  }
</script>

<div class="alerts-board" data-testid="alerts-board">
  <section data-testid="gaps">
    <header class="head">
      <h2>No alert <span class="count">{gaps.length}</span></h2>
      <button
        type="button"
        class="primary"
        onclick={() => onstart(null)}
        data-testid="new-alert"
      >
        New alert
      </button>
    </header>
    {#if gapsOff}<p class="muted">{gapsOff}</p>{:else if gaps.length === 0}<p
        class="muted"
      >
        Every disruption has an alert
      </p>{/if}
    {#each gaps as g (JSON.stringify(g.source))}
      <div class="row">
        <span>{gapTitle(g)}</span>
        <button
          type="button"
          onclick={() => onstart(g.source)}
          data-testid="start-alert"
        >
          Start alert
        </button>
      </div>
    {/each}
  </section>

  <section data-testid="alert-list">
    <h2>Service alerts</h2>
    <Tablist items={TABS} bind:selected={tab} label="Alerts" testid="alerts-tabs">

    </Tablist>
    <div class="tabs" role="tablist">
      {#each TABS as t (t.id)}
        <button
          type="button"
          role="tab"
          aria-selected={tab === t.id}
          class:on={tab === t.id}
          onclick={() => (tab = t.id)}
        >
          {t.label} · {list ? list[t.id].length : 0}
        </button>
      {/each}
    </div>
    {#if rows.length === 0}<p class="muted">None</p>{/if}
    {#each rows as r (r.id)}
      <div class="alert" data-testid="alert-row">
        <span class="effect">{EFFECT[r.alert.effect] ?? "Alert"}</span>
        <strong>
          {r.alert.header.find((h) => !h.lang || h.lang === "en")?.text ||
            r.alert.header[0]?.text ||
            r.id}
        </strong>
        <span class="sub">
          {period(
            r.alert,
          )}{#if r.open_secs != null}{` · ${openText(Number(r.open_secs))}`}{/if}
        </span>
        <span class="actions">
          <button type="button" onclick={() => onedit(r.id, r.alert)}>
            Edit
          </button>
          {#if tab === "in_force" || tab === "upcoming"}
            <button
              type="button"
              class="danger"
              disabled={busy}
              onclick={() => onend(r.id)}
              data-testid="end-alert"
            >
              End alert
            </button>
          {/if}
        </span>
      </div>
    {/each}
  </section>
</div>

<style>
  .alerts-board {
    display: grid;
    gap: 1.4rem;
  }
  .head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    font-size: 0.95rem;
    margin: 0 0 0.4rem;
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .count {
    font-weight: 600;
    background: var(--hover);
    border-radius: 999px;
    padding: 0 0.5rem;
    font-size: 0.8rem;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.6rem;
    padding: 0.45rem 0;
    border-top: 1px solid var(--line);
  }
  button {
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    border-radius: 6px;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
    white-space: nowrap;
  }
  button.primary {
    background: var(--accent);
    color: var(--on-accent);
    border-color: var(--accent);
  }
  button.danger {
    color: var(--bad);
  }
  .tabs {
    display: flex;
    gap: 0.25rem;
    margin-bottom: 0.4rem;
    flex-wrap: wrap;
  }
  .tabs button {
    border: 0;
    background: none;
    color: var(--muted);
  }
  .tabs button.on {
    background: var(--accent-soft);
    color: var(--text);
    font-weight: 600;
  }
  .alert {
    display: grid;
    gap: 0.15rem;
    padding: 0.55rem 0;
    border-top: 1px solid var(--line);
  }
  .effect {
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--warn);
  }
  .sub {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .actions {
    display: flex;
    gap: 0.4rem;
    margin-top: 0.2rem;
  }
</style>
