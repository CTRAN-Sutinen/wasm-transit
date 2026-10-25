<script lang="ts">
  /// The planner persona's workspace: a screen stack, not a pile of
  /// lists. The overview answers the network-level questions (per
  /// service type — the design axis), and every drill-down pushes a
  /// screen — Back pops. The menu jumps to the authoring collections.
  /// All numbers come from the artifact's analysis ops; this file
  /// only renders them.
  import { app, getNetworkDoc } from "../state.svelte";
  import {
    deriveFromFeed, materializeNetwork, validateNetwork, loadNetworkText,
    loadNetworkFile, exportNetworkText, downloadNetwork, selectEntity,
    addStop, addRoute, addPattern, addServiceType, addSchedule, addBlock,
    plannerBack, plannerGo, togglePlannerMap,
  } from "../planner";
  import type { PlannerScreen } from "../planner";
  import RouteForm from "./RouteForm.svelte";
  import StopForm from "./StopForm.svelte";
  import PatternForm from "./PatternForm.svelte";
  import ScheduleForm from "./ScheduleForm.svelte";
  import ServiceTypeForm from "./ServiceTypeForm.svelte";
  import BlockForm from "./BlockForm.svelte";

  const doc = $derived(getNetworkDoc());
  const stack = $derived(app.plannerStack);
  const screen = $derived(stack[stack.length - 1]);

  let filter = $state("");
  const q = $derived(filter.trim().toLowerCase());
  const matches = (s: string): boolean => !q || s.toLowerCase().includes(q);

  const navItems: { view: PlannerScreen["view"]; label: string }[] = [
    { view: "overview", label: "Overview" },
    { view: "routes", label: "Routes" },
    { view: "stops", label: "Stops" },
    { view: "patterns", label: "Patterns" },
    { view: "schedules", label: "Schedules" },
    { view: "service_types", label: "Service types" },
    { view: "blocks", label: "Blocks" },
    { view: "document", label: "Document" },
  ];

  let pasteText = $state("");
  let networkFile: HTMLInputElement;
</script>

<nav class="planner-nav" aria-label="Planner screens">
  {#if stack.length > 1}
    <button class="primary" onclick={plannerBack}>← Back</button>
  {/if}
  {#each navItems as item (item.view)}
    <button class="chip" class:active={screen.view === item.view}
            onclick={() => plannerGo(item.view)}>{item.label}</button>
  {/each}
  <button class="chip" onclick={togglePlannerMap}>
    {app.plannerMapUp ? "Hide map" : "Show map"}
  </button>
</nav>

{#if !doc}
  <section>
    <h2>Network document</h2>
    <p class="muted">
      A planner works off an existing network: load a GTFS feed in
      the Rider or Dispatcher view, then derive the document from it here.
    </p>
    <p>
      <button class="primary" onclick={deriveFromFeed} disabled={!app.gtfs}>
        Derive from loaded feed
      </button>
    </p>
  </section>
{:else if screen.view === "overview"}
  <section>
    <h2>Network analysis</h2>

    {#if app.typeCoverage && app.typeCoverage.length > 0}
      <p class="chips">
        {#each app.typeCoverage as tc (tc.service_type)}
          <span class="chip ok" title={`coverage of ${tc.name} on its representative day (${tc.date})`}>
            {tc.name}: serves {tc.served_pct.toFixed(1)}% of population
          </span>
        {/each}
      </p>
    {:else if app.popLayer}
      <p class="muted">Coverage needs at least one service type that runs.</p>
    {:else}
      <p class="muted">
        Population coverage: import a census-block-centroid layer in Data sources
        to answer "what share of the population does this network serve?".
      </p>
    {/if}

    {#if app.networkNotes.length > 0}
      <h4>Import resolutions</h4>
      <ul class="issues">
        {#each app.networkNotes as note (JSON.stringify(note))}
          <li><code>{note.kind.replaceAll("_", " ")}</code> {JSON.stringify(
            Object.fromEntries(Object.entries(note).filter(([k]) => k !== "kind")),
          )}</li>
        {/each}
      </ul>
    {/if}

    {#if app.networkIssues.length === 0}
      <p><span class="chip ok">valid</span></p>
    {:else}
      <p>
        <span class="chip bad">{app.networkIssues.length} issue{app.networkIssues.length > 1 ? "s" : ""}</span>
        <button class="link" onclick={() => plannerGo("document")}>fix in Document</button>
      </p>
    {/if}
  </section>

  <section>
    <h2>Service types</h2>
    <p class="muted">
      The design axis: what planners author and link to days. Numbers
      are per type, on its representative day; coverage is per type
      when a population layer is loaded.
    </p>
    {#if app.typeOverview.length > 0}
      <div class="table-scroll">
        <table class="stat-table">
          <thead>
            <tr>
              <th>service type</th><th>trips</th><th>routes</th><th>stops</th><th>rev hrs</th>
              {#if app.typeCoverage}<th>serves</th>{/if}
            </tr>
          </thead>
          <tbody>
            {#each app.typeOverview as row (row.service_type)}
              <tr>
                <td>
                  <button class="link" aria-label={`Open service type ${row.service_type}`}
                          onclick={() => selectEntity("service_type", row.service_type)}>
                    <strong>{row.name}</strong>
                  </button>
                </td>
                <td>{row.trips}</td>
                <td>{row.active_routes}</td>
                <td>{row.active_stops}</td>
                <td>{row.revenue_hours.toFixed(1)}</td>
                {#if app.typeCoverage}
                  <td>
                    {#each app.typeCoverage.filter((tc) => tc.service_type === row.service_type) as tc (tc.service_type)}
                      {tc.served_pct.toFixed(1)}%
                    {/each}
                  </td>
                {/if}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <p class="muted">No service types yet.</p>
    {/if}
  </section>
{:else if screen.view === "routes"}
  <section>
    <h2>Routes</h2>
    <p><input aria-label="Filter routes" placeholder="filter routes…" bind:value={filter}></p>
    <ul class="rail-items">
      {#each Object.entries(doc.routes).filter(([id, r]) => matches(id + " " + r.name)) as [id, r] (id)}
        <li>
          <button class="rail-item" aria-label={`Open route ${id}`}
                  onclick={() => selectEntity("route", id)}>
            {r.name} <span class="muted">({id})</span>
          </button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addRoute()}>Add route</button></p>
  </section>
{:else if screen.view === "stops"}
  <section>
    <h2>Stops</h2>
    <p><input aria-label="Filter stops" placeholder="filter stops…" bind:value={filter}></p>
    <ul class="rail-items">
      {#each Object.entries(doc.stops).filter(([id, s]) => matches(id + " " + s.name)) as [id, s] (id)}
        <li>
          <button class="rail-item" aria-label={`Open stop ${id}`}
                  onclick={() => selectEntity("stop", id)}>
            {s.name}
            {#if s.transfer} <span class="chip svc">transfer</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addStop()}>Add stop</button></p>
  </section>
{:else if screen.view === "patterns"}
  <section>
    <h2>Patterns</h2>
    <p><input aria-label="Filter patterns" placeholder="filter patterns…" bind:value={filter}></p>
    <ul class="rail-items">
      {#each Object.entries(doc.patterns).filter(([id, p]) => matches(id + " " + p.name)) as [id, p] (id)}
        <li>
          <button class="rail-item" aria-label={`Open pattern ${id}`}
                  onclick={() => selectEntity("pattern", id)}>
            {p.name} <span class="muted">({doc.routes[p.route]?.name ?? p.route})</span>
          </button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addPattern()}>Add pattern</button></p>
  </section>
{:else if screen.view === "schedules"}
  <section>
    <h2>Schedules</h2>
    <p><input aria-label="Filter schedules" placeholder="filter schedules…" bind:value={filter}></p>
    <ul class="rail-items">
      {#each Object.entries(doc.schedules).filter(([id]) => matches(id)) as [id, s] (id)}
        <li>
          <button class="rail-item" aria-label={`Open schedule ${id}`}
                  onclick={() => selectEntity("schedule", id)}>
            {id}
          </button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addSchedule()}>Add schedule</button></p>
  </section>
{:else if screen.view === "service_types"}
  <section>
    <h2>Service types</h2>
    <ul class="rail-items">
      {#each Object.entries(doc.service_types).sort(([a], [b]) => (a < b ? -1 : 1)) as [id, t] (id)}
        <li>
          <button class="rail-item" aria-label={`Open service type ${id}`}
                  onclick={() => selectEntity("service_type", id)}>
            {t.name} <span class="muted">({id})</span>
          </button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addServiceType()}>Add service type</button></p>
  </section>
{:else if screen.view === "blocks"}
  <section>
    <h2>Blocks</h2>
    <p><input aria-label="Filter blocks" placeholder="filter blocks…" bind:value={filter}></p>
    <ul class="rail-items">
      {#each Object.entries(doc.blocks).filter(([id]) => matches(id)) as [id, b] (id)}
        <li>
          <button class="rail-item" aria-label={`Open block ${id}`}
                  onclick={() => selectEntity("block", id)}>
            {id} <span class="muted">({b.trips.length} trips)</span>
          </button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addBlock()}>Add block</button></p>
  </section>
{:else if screen.view === "document"}
  <section>
    <h2>Network document</h2>
    <p class="chips">
      <span class="chip">{app.networkLabel}</span>
      <span class="chip svc">{Object.keys(doc.stops).length} stops</span>
      <span class="chip svc">{Object.keys(doc.routes).length} routes</span>
      <span class="chip svc">{Object.keys(doc.patterns).length} patterns</span>
      <span class="chip svc">{Object.keys(doc.schedules).length} schedules</span>
      <span class="chip svc">{Object.keys(doc.blocks).length} blocks</span>
    </p>
    <p>
      <button onclick={deriveFromFeed} disabled={!app.gtfs}>Derive from loaded feed</button>
    </p>
    {#if app.networkIssues.length === 0}
      <p><span class="chip ok">valid</span> <button class="primary" onclick={materializeNetwork}>Load into app</button></p>
    {:else}
      <p><span class="chip bad">{app.networkIssues.length} issue{app.networkIssues.length > 1 ? "s" : ""}</span>
         <button onclick={validateNetwork}>Revalidate</button></p>
      <ul class="issues">
        {#each app.networkIssues.slice(0, 50) as issue (JSON.stringify(issue))}
          {@const [[variant, fields]] = Object.entries(issue)}
          <li><code>{variant}</code>
            {Object.entries(fields ?? {}).map(([k, v]) => `${k}: ${v}`).join(", ")}</li>
        {/each}
      </ul>
    {/if}
  </section>
  <details class="io-drawer">
    <summary>Import / export</summary>
    <p>
      <button onclick={() => networkFile.click()}>Open .json…</button>
      <button onclick={downloadNetwork}>Download .json</button>
      <button onclick={async () => { await navigator.clipboard.writeText(exportNetworkText()); app.status = "Document copied to the clipboard as JSON."; }}>Copy JSON</button>
      <input type="file" accept=".json,application/json" hidden bind:this={networkFile}
             onchange={(e) => { const f = e.currentTarget.files?.[0]; if (f) loadNetworkFile(f); e.currentTarget.value = ""; }}>
    </p>
    <textarea rows="3" cols="40" placeholder="…or paste a network document (JSON)"
              aria-label="Network JSON" bind:value={pasteText}></textarea>
    <p><button onclick={() => { if (loadNetworkText(pasteText, "pasted")) pasteText = ""; }}>Use pasted JSON</button></p>
  </details>
{:else if screen.view === "route"}
  <RouteForm />
{:else if screen.view === "stop"}
  <StopForm />
{:else if screen.view === "pattern"}
  <PatternForm />
{:else if screen.view === "schedule"}
  <ScheduleForm />
{:else if screen.view === "service_type"}
  <ServiceTypeForm />
{:else if screen.view === "block"}
  <BlockForm />
{/if}
