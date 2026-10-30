<script lang="ts">
  /// The selection every view reads, on one bar: the plan, its
  /// baseline, the service group, and — when set — the route, the
  /// stop, and the hour window, each clearable. The plan's health
  /// links to the proposal view, and the jump box finds a route.
  import Combobox from "./Combobox.svelte";

  type Opt = { value: string; label: string; sub?: string };
  let {
    plans,
    activeKey,
    compareKey,
    groups,
    group,
    routes,
    route,
    stopName,
    window,
    issues,
    warnings,
    status,
    busy,
    onplan,
    oncompare,
    ongroup,
    onroute,
    onclearroute,
    onclearstop,
    onclearwindow,
    onhealth,
  }: {
    plans: { key: string; name: string }[];
    activeKey: string;
    compareKey: string;
    groups: { service_type: string; name: string; members: string[] }[];
    group: string;
    routes: Opt[];
    route: string;
    stopName: string;
    window: { from: number; to: number } | null;
    issues: number;
    warnings: number;
    status: string;
    busy: boolean;
    onplan: (key: string) => void;
    oncompare: (key: string) => void;
    ongroup: (group: string) => void;
    onroute: (route: string) => void;
    onclearroute: () => void;
    onclearstop: () => void;
    onclearwindow: () => void;
    onhealth: () => void;
  } = $props();

  let jump = $state("");
</script>

<header class="bar" data-testid="context-bar">
  <label class="chip">
    <small>Plan</small>
    <select value={activeKey} onchange={(e) => onplan(e.currentTarget.value)} data-testid="plan-select">
      {#each plans as p (p.key)}<option value={p.key}>{p.name}</option>{/each}
    </select>
  </label>
  <label class="chip">
    <small>vs</small>
    <select value={compareKey} onchange={(e) => oncompare(e.currentTarget.value)} data-testid="baseline-select">
      <option value="">no baseline</option>
      {#each plans.filter((p) => p.key !== activeKey) as p (p.key)}<option value={p.key}>{p.name}</option>{/each}
    </select>
  </label>
  <label class="chip sel">
    <small>Group</small>
    <select value={group} onchange={(e) => ongroup(e.currentTarget.value)} data-testid="group-select">
      {#each groups as g (g.service_type)}
        <option value={g.service_type}>{g.name}{g.members.length > 1 ? ` +${g.members.length - 1}` : ""}</option>
      {/each}
    </select>
  </label>
  {#if route}
    <button type="button" class="chip sel" onclick={onclearroute} aria-label="Clear route {route}"><small>Route</small>{route} ✕</button>
  {/if}
  {#if stopName}
    <button type="button" class="chip sel" onclick={onclearstop} aria-label="Clear stop"><small>Stop</small>{stopName} ✕</button>
  {/if}
  {#if window}
    <button type="button" class="chip sel" onclick={onclearwindow} aria-label="Clear hours" data-testid="window-chip">
      <small>Hours</small>{window.from}:00–{window.to}:00 ✕
    </button>
  {/if}
  {#if issues || warnings}
    <button type="button" class="health" onclick={onhealth} data-testid="health">
      {issues} {issues === 1 ? "issue" : "issues"} · {warnings} {warnings === 1 ? "warning" : "warnings"}
    </button>
  {/if}
  <div class="jump">
    <Combobox label="Jump to route" bind:value={jump} options={routes} placeholder="Jump to route…" onchange={(v) => { if (v) onroute(v); jump = ""; }} />
  </div>
  <p class="status" class:busy aria-live="polite" data-testid="status">{status}</p>
</header>

<style>
  .bar {
    display: flex; flex-wrap: wrap; align-items: center; gap: 0.4rem;
    padding: 0.45rem 1rem; background: var(--surface); border-bottom: 1px solid var(--line);
  }
  .chip {
    display: inline-flex; align-items: center; gap: 0.35rem; height: 1.9rem; padding: 0 0.55rem;
    border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--text);
    font: inherit; font-size: 0.85rem; cursor: pointer;
  }
  .chip.sel { border-color: var(--accent); background: var(--accent-soft); }
  .chip small { color: var(--muted); font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; }
  .chip select { border: 0; background: transparent; font: inherit; color: inherit; max-width: 14rem; cursor: pointer; }
  .health {
    height: 1.9rem; padding: 0 0.6rem; border-radius: 6px; border: 0; cursor: pointer;
    background: color-mix(in srgb, var(--bad) 14%, var(--surface)); color: var(--bad); font: inherit; font-size: 0.85rem; font-weight: 600;
  }
  .jump { margin-left: auto; min-width: 14rem; }
  .jump :global(label) { position: absolute; left: -9999px; }
  .status { flex-basis: 100%; margin: 0; font-size: 0.75rem; color: var(--muted); }
  .status.busy { color: var(--accent); }
</style>
