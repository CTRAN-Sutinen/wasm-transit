<script lang="ts">
  /// The planner persona's workspace: a screen stack, not a pile of
  /// lists. The overview answers the network-level questions (per
  /// service type — the design axis), and every drill-down pushes a
  /// screen — Back pops. The menu jumps to the authoring collections.
  /// All numbers come from the artifact's analysis ops; this file
  /// only renders them.
  import { app, getNetworkDoc, getAllPlans, getActivePlanId } from "../state.svelte";
  import {
    deriveFromFeed, materializeNetwork, validateNetwork, loadNetworkText,
    loadNetworkFile, exportNetworkText, downloadNetwork, selectEntity,
    addStop, addRoute, addPattern, addServiceType, addSchedule, addBlock,
    plannerBack, plannerGo, togglePlannerMap, forkPlan, switchPlan,
    deletePlan, updatePlanMeta, refreshPlanChain, refreshAnalysis, fmtHHMMSS,
  } from "../planner";
  import { serviceDate } from "../actions";
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
  const allPlans = $derived(getAllPlans());
  const activeKey = $derived(getActivePlanId());

  // The chain view and the analysis refresh when the workspace
  // opens (plan CRUD refreshes the chain itself; edits refresh the
  // analysis through setDoc).
  $effect(() => {
    if (app.persona === "planner") {
      refreshPlanChain();
      if (getNetworkDoc()) refreshAnalysis();
    }
  });

  // The fork form: "from" preselects the in-force plan.
  let forkFrom = $state("");
  let forkName = $state("");
  let forkDate = $state("");
  $effect(() => {
    if (!forkFrom && app.planChain.inForce) {
      const framed = Object.entries(getAllPlans()).find(
        ([, d]) => d.plan?.id === app.planChain.inForce,
      );
      if (framed) forkFrom = framed[0];
    }
  });

  let filter = $state("");
  const q = $derived(filter.trim().toLowerCase());
  const matches = (s: string): boolean => !q || s.toLowerCase().includes(q);

  const navItems: { view: PlannerScreen["view"]; label: string }[] = [
    { view: "plans", label: "Plans" },
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

{#if screen.view === "plans"}
  <section>
    <h2>Service plans</h2>
    <p class="muted">
      Agencies implement a few picks a year at defined dates — the only
      times permanent stop/route changes are allowed. Every plan below
      is a whole document (structure + schedules + runcut); the plan in
      force today is resolved by the artifact. Pick dates are targets:
      they move until they're real.
    </p>

    {#if app.planChain.issues.length > 0}
      <ul class="issues">
        {#each app.planChain.issues as issue (JSON.stringify(issue))}
          <li><code>{issue.kind.replaceAll("_", " ")}</code> {JSON.stringify(
            Object.fromEntries(Object.entries(issue).filter(([k]) => k !== "kind")),
          )}</li>
        {/each}
      </ul>
    {/if}

    {#if Object.keys(allPlans).length > 0}
      <div class="table-scroll">
        <table class="stat-table">
          <thead>
            <tr><th>plan</th><th>effective</th><th>period</th><th>status</th></tr>
          </thead>
          <tbody>
            {#each Object.entries(allPlans).sort(([a], [b]) => (a < b ? -1 : 1)) as [key, p] (key)}
              {@const meta = p.plan}
              {@const period = meta?.id ? app.planChain.periods.find((q) => q.id === meta.id) : null}
              {@const today = serviceDate()}
              {@const status =
                !meta?.effective_from ? "unversioned"
                : app.planChain.inForce === meta.id ? "in force"
                : meta.effective_from > today ? "upcoming"
                : "superseded"}
              <tr class:active-row={key === activeKey}>
                <td>
                  <button class="link" aria-label={`Open plan ${meta?.name || p.name}`}
                          onclick={() => switchPlan(key)}>
                    <strong>{meta?.name || p.name || key}</strong>
                  </button>
                </td>
                <td>{meta?.effective_from ?? "—"}</td>
                <td>
                  {#if period}{period.effective_from} → {period.until ?? "open"}{:else}—{/if}
                </td>
                <td>
                  <span class="chip {status === 'in force' ? 'ok' : 'svc'}">{status}</span>
                  {#if key === activeKey}<span class="chip svc">active</span>{/if}
                </td>
                <td>
                  <button class="link" aria-label={`Fork plan ${meta?.name || key}`}
                          onclick={() => { forkFrom = key; }}>Fork</button>
                  <button class="link" aria-label={`Remove plan ${meta?.name || key}`}
                          onclick={() => deletePlan(key)}>remove</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <h4>New pick from an existing plan</h4>
      <p class="ent-row">
        <label>from
          <select aria-label="Fork source plan" bind:value={forkFrom}>
            {#each Object.keys(allPlans) as key (key)}
              <option value={key}>{allPlans[key].plan?.name || allPlans[key].name || key}</option>
            {/each}
          </select>
        </label>
      </p>
      <p class="ent-row">
        <label>name
          <input aria-label="New pick name" bind:value={forkName} placeholder="Fall 2026 pick">
        </label>
        <label>effective
          <input type="date" aria-label="New pick effective date" bind:value={forkDate}>
        </label>
      </p>
      <p>
        <button class="primary" onclick={() => { forkPlan(forkFrom, forkName, forkDate); forkName = ""; forkDate = ""; }}>
          Fork into the next pick
        </button>
      </p>
    {:else}
      <p class="muted">
        No plans yet. A planner works off an existing network: load a
        GTFS feed in the Rider or Dispatcher view, then derive the
        first plan from it here.
      </p>
      <p>
        <button class="primary" onclick={deriveFromFeed} disabled={!app.feedLoaded}>
          Derive from loaded feed
        </button>
      </p>
    {/if}
  </section>
{:else if !doc}
  <section>
    <h2>Network document</h2>
    <p class="muted">
      No active plan — open or fork one on the Plans screen, or
      derive a network from the loaded feed.
    </p>
    <p>
      <button class="primary" onclick={deriveFromFeed} disabled={!app.feedLoaded}>
        Derive from loaded feed
      </button>
    </p>
  </section>
{:else if screen.view === "overview"}
  <section>
    <h2>Network analysis</h2>

    {#if app.dayStats}
      <div class="day-summary">
        <p class="chips">
          <label class="muted">
            day
            <input
              type="date"
              value={app.plannerDay}
              onchange={(e) => {
                app.plannerDay = e.currentTarget.value;
                refreshAnalysis();
              }}
            />
          </label>
          <span class="chip svc">{app.dayStats.trips} trips</span>
          <span class="chip svc">{app.dayStats.active_routes} routes</span>
          <span class="chip svc">{app.dayStats.active_stops} stops</span>
          <span class="chip svc" title="sum of trip runtimes, hours">
            {app.dayStats.revenue_hours.toFixed(1)} revenue hours
          </span>
          {#if app.dayStats.first_start_secs !== null && app.dayStats.last_arrival_secs !== null}
            <span class="chip svc">
              span {fmtHHMMSS(app.dayStats.first_start_secs)}–{fmtHHMMSS(app.dayStats.last_arrival_secs)}
            </span>
          {/if}
          {#if app.dayStats.transfer_stops > 0}
            <span class="chip svc">{app.dayStats.transfer_stops} transfer points</span>
          {/if}
        </p>
        {#if app.dayStats.trips === 0}
          <p class="muted">No service runs on {app.dayStats.date} — the day's calendar is empty.</p>
        {/if}
      </div>
    {:else}
      <p class="muted">
        The day summary needs the analysis capability; when it does not
        assemble, authoring continues without numbers.
      </p>
    {/if}

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

    {#if app.networkValidationOff}
      <p><span class="chip bad" title="the network capability did not assemble">validation off</span></p>
    {:else if app.networkIssues.length === 0}
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
    <h4>Plan frame</h4>
    <p class="ent-row">
      <label>pick name
        <input aria-label="Plan name" value={doc.plan?.name}
               onchange={(e) => updatePlanMeta({ name: e.currentTarget.value })}>
      </label>
      <label>effective
        <input type="date" aria-label="Plan effective date" value={doc.plan?.effective_from ?? ""}
               onchange={(e) => updatePlanMeta({ effectiveFrom: e.currentTarget.value })}>
      </label>
    </p>
    {#if !doc.plan?.effective_from}
      <p class="muted">
        Unversioned — set an effective date to frame this document as a
        pick in the chain. Dates are targets; they move until they're real.
      </p>
    {/if}
    <p class="chips">
      <span class="chip">{app.networkLabel}</span>
      <span class="chip svc">{Object.keys(doc.stops).length} stops</span>
      <span class="chip svc">{Object.keys(doc.routes).length} routes</span>
      <span class="chip svc">{Object.keys(doc.patterns).length} patterns</span>
      <span class="chip svc">{Object.keys(doc.schedules).length} schedules</span>
      <span class="chip svc">{Object.keys(doc.blocks).length} blocks</span>
    </p>
    <p>
      <button onclick={deriveFromFeed} disabled={!app.feedLoaded}>Derive from loaded feed</button>
    </p>
    {#if app.networkValidationOff}
      <p>
        <span class="chip bad" title="the network capability did not assemble">validation off</span>
        <button class="primary" onclick={materializeNetwork}>Load into app</button>
      </p>
    {:else if app.networkIssues.length === 0}
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
