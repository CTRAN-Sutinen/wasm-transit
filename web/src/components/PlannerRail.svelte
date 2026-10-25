<script lang="ts">
  /// The planner persona's sidebar: the network as a whole — the
  /// document (derive, validate, materialize, import/export) and the
  /// searchable entity rail. Drill-down editors live over the map
  /// (PlannerDrawer); selecting an entity there frames it.
  import { app, getNetworkDoc } from "../state.svelte";
  import {
    deriveFromFeed, materializeNetwork, validateNetwork, loadNetworkText,
    loadNetworkFile, exportNetworkText, downloadNetwork, selectEntity,
    addStop, addRoute, addPattern, addCalendar, addService, addBlock,
  } from "../planner";

  const doc = $derived(getNetworkDoc());
  const issues = $derived(app.networkIssues);

  const issueLines = $derived(
    issues.slice(0, 50).map((issue) => {
      const [[variant, fields]] = Object.entries(issue);
      const detail = Object.entries(fields ?? {})
        .map(([k, v]) => `${k}: ${v}`)
        .join(", ");
      return { variant, detail };
    }),
  );

  // The overview: every entity group, searchable, capped per group.
  let filter = $state("");
  const q = $derived(filter.trim().toLowerCase());
  const hits = (ids: string[]): string[] => {
    const all = [...ids];
    return q ? all.filter((id: string) => id.toLowerCase().includes(q)) : all;
  };
  const groups = $derived.by(() => {
    if (!doc) return [];
    const cap = 50;
    type Group = { kind: import("../planner").EntityKind; label: string; open: boolean; total: number; items: string[]; hidden: number };
    const mk = (kind: import("../planner").EntityKind, label: string, ids: string[], open = false): Group => ({
      kind, label, open,
      total: ids.length,
      items: hits(ids).slice(0, cap),
      hidden: Math.max(0, hits(ids).length - cap),
    });
    const ids = (o: Record<string, unknown> | undefined | null): string[] => Object.keys(o ?? {}).sort();
    return [
      mk("route", "Routes", ids(doc.routes)),
      mk("pattern", "Patterns", ids(doc.patterns)),
      mk("service", "Services", ids(doc.services)),
      mk("calendar", "Calendars", ids(doc.calendars)),
      mk("block", "Blocks", ids(doc.blocks)),
      mk("stop", "Stops", ids(doc.stops)),
    ];
  });

  const addItem: Partial<Record<import("../planner").EntityKind, () => void>> = {
    route: addRoute,
    pattern: addPattern,
    service: addService,
    calendar: addCalendar,
    block: addBlock,
    stop: addStop,
  };

  let pasteText = $state("");
  let networkFile: HTMLInputElement;
</script>

<section>
  <h2>Network document</h2>

  {#if !doc}
  <p class="muted">
    A planner works off an existing network: load a GTFS feed in
    the Rider or Dispatcher view, then derive the document from it here.
  </p>
{/if}

<p>
  <button class="primary" onclick={deriveFromFeed} disabled={!app.gtfs}>
    Derive from loaded feed
  </button>
</p>

{#if doc}
  <p class="chips">
    <span class="chip">{app.networkLabel}</span>
    <span class="chip svc">{Object.keys(doc.stops).length} stops</span>
    <span class="chip svc">{Object.keys(doc.routes).length} routes</span>
    <span class="chip svc">{Object.keys(doc.patterns).length} patterns</span>
    <span class="chip svc">{Object.keys(doc.services).length} services</span>
    <span class="chip svc">{Object.keys(doc.blocks).length} blocks</span>
  </p>

  {#if issues.length === 0}
    <p><span class="chip ok">valid</span> <button class="primary" onclick={materializeNetwork}>Load into app</button></p>
  {:else}
    <p><span class="chip bad">{issues.length} issue{issues.length > 1 ? "s" : ""}</span>
       <button onclick={validateNetwork}>Revalidate</button></p>
    <ul class="issues">
      {#each issueLines as issue (issue.variant + issue.detail)}
        <li><code>{issue.variant}</code> {issue.detail}</li>
      {/each}
    </ul>
  {/if}
{/if}

<details class="io-drawer">
  <summary>Import / export</summary>
  <p>
    <button onclick={() => networkFile.click()}>Open .json…</button>
    <button onclick={downloadNetwork} disabled={!doc}>Download .json</button>
    <button onclick={async () => { await navigator.clipboard.writeText(exportNetworkText()); app.status = "Document copied to the clipboard as JSON."; }}
            disabled={!doc}>Copy JSON</button>
    <input type="file" accept=".json,application/json" hidden bind:this={networkFile}
           onchange={(e) => { const f = e.currentTarget.files?.[0]; if (f) loadNetworkFile(f); e.currentTarget.value = ""; }}>
  </p>
  <textarea rows="3" cols="40" placeholder="…or paste a network document (JSON)"
            aria-label="Network JSON" bind:value={pasteText}></textarea>
  <p><button onclick={() => { if (loadNetworkText(pasteText, "pasted")) pasteText = ""; }}>Use pasted JSON</button></p>
</details>
</section>

{#if doc}
  <section>
    <h2>Network</h2>
    <p><input aria-label="Filter the network" placeholder="filter entities…" bind:value={filter}></p>
    {#each groups as group (group.kind)}
      <details class="rail-group" open={group.total <= 30}>
        <summary>{group.label} <span class="count">{group.total}</span></summary>
        <ul class="rail-items">
          {#each group.items as id (id)}
            <li>
              <button class="rail-item" class:active={app.plannerSelection?.kind === group.kind && app.plannerSelection?.id === id}
                      aria-label={`Open ${group.kind} ${id}`}
                      onclick={() => selectEntity(group.kind, id)}>{id}</button>
            </li>
          {/each}
          {#if group.hidden > 0}
            <li class="muted">+{group.hidden} more — use the filter</li>
          {/if}
        </ul>
        <p><button class="link" onclick={() => addItem[group.kind]?.()}>Add {group.kind}</button></p>
      </details>
    {/each}
  </section>
{/if}
