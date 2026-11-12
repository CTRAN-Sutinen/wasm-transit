<script lang="ts">
  /// The service-change UI's layout: PLACEMENT ONLY. One selection bar
  /// (plan, baseline, day type, hours, route) over the stage that is
  /// open — the plan and its calendar, the scan, one route's day with a
  /// hub beside it, vehicles and hours, the proposal — each with the
  /// page's full width. Every prop is the host's answers state; every
  /// event calls a host function.
  import {
    sc,
    STAGES,
    planName,
    groupMembers,
    periodStart,
    openPlan,
    setCompare,
    fork,
    loadFeedBytes,
    setStage,
    setScanView,
    selectGroup,
    setEditType,
    setWindow,
    openRoute,
    openHub,
    selectDirection,
    selectTimetableStop,
    selectTrip,
    toggleEdit,
    retry,
    gestures,
    calendar,
    adoptBlocks,
    undoChange,
    redoChange,
    exportPlan,
    publishPlan,
  } from "./host.svelte";
  import NetworkMap from "../../components/NetworkMap.svelte";
  import DayTypePicker from "../../components/DayTypePicker.svelte";
  import HourWindowPicker from "../../components/HourWindowPicker.svelte";
  import ServiceKeys from "../../components/ServiceKeys.svelte";
  import FleetPanel from "../../components/FleetPanel.svelte";
  import TripList from "../../components/TripList.svelte";
  import PlanScore from "../../components/PlanScore.svelte";
  import FrequencyGrid from "../../components/FrequencyGrid.svelte";
  import RouteHeader from "../../components/RouteHeader.svelte";
  import DirectionTabs from "../../components/DirectionTabs.svelte";
  import StringLine from "../../components/StringLine.svelte";
  import TimetableGrid from "../../components/TimetableGrid.svelte";
  import RunTimes from "../../components/RunTimes.svelte";
  import HourGaps from "../../components/HourGaps.svelte";
  import GesturePanel from "../../components/GesturePanel.svelte";
  import ChangeLedger from "../../components/ChangeLedger.svelte";
  import HubMatrix from "../../components/HubMatrix.svelte";
  import NearMisses from "../../components/NearMisses.svelte";
  import StopWaits from "../../components/StopWaits.svelte";
  import Readiness from "../../components/Readiness.svelte";
  import PlanList from "../../components/PlanList.svelte";
  import PickTimeline from "../../components/PickTimeline.svelte";
  import Combobox from "../../components/Combobox.svelte";
  import Unavailable from "../../components/Unavailable.svelte";
  import { today } from "../kit";

  const framed = $derived(!!sc.plans.find((p) => p.key === sc.activeKey)?.id);
  const changed = $derived(sc.comparison?.routes.map((r) => r.route_id) ?? []);
  const routeRow = $derived(sc.routes.find((r) => r.route_id === sc.route) ?? null);
  const routeOptions = $derived(sc.routes.map((r) => ({ value: r.route_id, label: r.name, sub: r.long_name ?? undefined })));
  const stopOptions = $derived(sc.stops.map((s) => ({ value: s.stop_id, label: s.name, sub: s.stop_id })));
  // The hubs to read beside the route: the stops of its timetable.
  const routeStops = $derived((sc.timetable?.stops ?? []).map((s) => ({ value: s.stop, label: s.name, sub: s.stop })));
  const alsoIn = $derived(
    sc.dayTypes.flatMap((t) => t.groups).filter((g) => g.id !== sc.group && g.members.includes(sc.editType)).map((g) => g.id),
  );
  const groupDate = $derived(sc.dayTypes.flatMap((t) => t.groups).find((g) => g.id === sc.group)?.dates[0] ?? "");
  let jump = $state("");
  let hub = $state("");
  let tableMode = $state(false);
</script>

{#snippet failure(frame: string, title: string)}
  <Unavailable {title} error={sc.failed[frame]} onretry={retry} />
{/snippet}

<div class="shell">
  <header class="bar">
    <label class="pick"><span>Plan</span>
      <select value={sc.activeKey} onchange={(e) => openPlan(e.currentTarget.value)} data-testid="plan-select">
        {#each sc.plans as p (p.key)}<option value={p.key}>{p.name}</option>{/each}
      </select>
    </label>
    <label class="pick"><span>against</span>
      <select value={sc.compareKey} onchange={(e) => setCompare(e.currentTarget.value)} data-testid="base-select">
        <option value="">nothing</option>
        {#each sc.plans.filter((p) => p.key !== sc.activeKey) as p (p.key)}<option value={p.key}>{p.name}</option>{/each}
      </select>
    </label>
    {#if sc.failed.DayTypes}{@render failure("DayTypes", "Day types")}{:else}
      <DayTypePicker dayTypes={sc.dayTypes} selected={sc.group} onselect={selectGroup} />
    {/if}
    <HourWindowPicker window={sc.window} onwindow={setWindow} />
    <div class="right">
      {#if sc.canUndo || sc.canRedo}
        <button type="button" onclick={undoChange} disabled={!sc.canUndo}>Undo</button>
        <button type="button" onclick={redoChange} disabled={!sc.canRedo}>Redo</button>
      {/if}
      <button type="button" class="health" class:bad={sc.issues.length > 0} class:warn={!sc.issues.length && sc.warnings.length > 0} onclick={() => setStage("proposal")}>
        {sc.issues.length} issues · {sc.warnings.length} warnings
      </button>
      <div class="jump">
        <Combobox label="Open a route" bind:value={jump} options={routeOptions} placeholder="Open a route…" onchange={(v) => { if (v) openRoute(v); jump = ""; }} />
      </div>
    </div>
  </header>
  <nav class="stages" aria-label="Stages">
    {#each STAGES as s, i (s.id)}
      <button type="button" class:on={sc.stage === s.id} aria-current={sc.stage === s.id ? "step" : undefined} onclick={() => setStage(s.id)}>
        <span class="n">{i + 1}</span>{s.label}{#if s.id === "route" && sc.route}<span class="sub">{routeRow?.name ?? sc.route}</span>{/if}
      </button>
    {/each}
    <p class="status" class:alert={sc.alert} role="status" data-testid="status">{sc.busy ? "… " : ""}{sc.status}</p>
  </nav>

  <main class="stage" data-testid="stage-{sc.stage}">
    {#if sc.noService && sc.stage !== "calendar"}
      <Unavailable title="No service in this plan's period"
        reason={`${planName(sc.activeKey)} runs nothing from ${periodStart()} on: its calendars end before then. Start the plan again with "Runs through" set, or extend its service keys' ranges (Plan & calendar).`} />
    {/if}

    {#if sc.stage === "calendar"}
      {#if sc.failed.Plans}{@render failure("Plans", "Pick timeline")}
      {:else if sc.chain}
        <PickTimeline periods={sc.chain.periods} names={Object.fromEntries(sc.plans.map((p) => [p.id, p.name]))} inForce={sc.chain.inForce}
          issues={sc.chain.issues} today={today()} activeId={sc.plans.find((p) => p.key === sc.activeKey)?.id ?? ""} />
      {/if}
      <div class="two">
        <PlanList plans={sc.plans} activeKey={sc.activeKey} compareKey={sc.compareKey} feed={sc.feed} onopen={openPlan} oncompare={setCompare} onfork={fork}
          onfeed={async (f) => loadFeedBytes(new Uint8Array(await f.arrayBuffer()), f.name)} />
        <div class="card">
          {#if sc.failed.Keys}{@render failure("Keys", "Service keys")}{:else}
            <ServiceKeys keys={sc.keys} dayTypes={sc.dayTypes} {framed}
              ontoggleday={calendar.toggleDay} onaddrange={calendar.addRange} onsetrange={calendar.setRange} onremoverange={calendar.removeRange}
              onsetexception={calendar.setException} onremoveexception={calendar.removeException} onrename={calendar.rename} onmove={calendar.move}
              onroute={openRoute} />
          {/if}
        </div>
      </div>

    {:else if sc.stage === "scan"}
      <div class="seg" role="group" aria-label="Show">
        <button type="button" class:on={sc.scanView === "grid"} aria-pressed={sc.scanView === "grid"} onclick={() => setScanView("grid")}>Frequency grid</button>
        <button type="button" class:on={sc.scanView === "map"} aria-pressed={sc.scanView === "map"} onclick={() => setScanView("map")}>Map</button>
      </div>
      {#if sc.scanView === "grid"}
        {#if sc.failed.Grid}{@render failure("Grid", "Frequency by hour")}
        {:else if sc.frequency}<FrequencyGrid frequency={sc.frequency} window={sc.window} {changed} selected={sc.route} onroute={openRoute} />{/if}
      {:else}
        {#if sc.failed.Map}{@render failure("Map", "The network")}{:else}
          <div class="map"><NetworkMap routes={sc.lines} {changed} selected={sc.route} onroute={openRoute} /></div>
        {/if}
      {/if}

    {:else if sc.stage === "route"}
      {#if !sc.route}
        <Unavailable title="No route open" reason="Open a route: a row of the frequency grid, a line on the map, a route a service key carries, or the route box at the top right." />
      {:else}
        <RouteHeader route={sc.route} routeName={routeRow?.name ?? sc.route} longName={routeRow?.long_name ?? null} groupName={sc.group}
          frequency={sc.frequency} comparison={sc.comparison} editing={sc.editing} onedit={toggleEdit} shortcut="" />
        <div class="route">
          <div class="work">
            {#if sc.failed.Route}{@render failure("Route", "The route")}{/if}
            {#if sc.routeDetail}
              <div class="dirs">
                <DirectionTabs directions={sc.routeDetail.directions} selected={sc.direction} onselect={selectDirection} />
                <label class="mode"><input type="checkbox" bind:checked={tableMode} /> timetable</label>
              </div>
            {/if}
            {#if sc.failed.StringLine}{@render failure("StringLine", "Timetable")}
            {:else if sc.timetable}
              {#if tableMode}
                <TimetableGrid timetable={sc.timetable} baselineName={planName(sc.compareKey)} selectedStop={sc.selectedStop} onstop={selectTimetableStop} />
              {:else}
                <StringLine timetable={sc.timetable} window={sc.window} editable={sc.editing && framed} selected={sc.selectedTrip} onselect={selectTrip}
                  onnudge={gestures.retime} selectedStop={sc.selectedStop} onstop={sc.editing ? selectTimetableStop : undefined} />
              {/if}
              {#if sc.editing}
                {#key `${sc.route}|${sc.direction}`}
                  <GesturePanel {framed} members={groupMembers()} editType={sc.editType} {alsoIn} onedittype={setEditType}
                    stops={sc.timetable.stops} allStops={stopOptions} selectedStop={sc.selectedStop} trips={sc.timetable.trips}
                    selectedTrip={sc.selectedTrip} window={sc.window} onspan={gestures.span} onrunning={gestures.running}
                    onretime={gestures.retime} oninsertstop={gestures.insertStop} onremovestop={gestures.removeStop} />
                {/key}
              {/if}
              <div class="row">
                <RunTimes timetable={sc.timetable} />
                {#if sc.frequency}<HourGaps frequency={sc.frequency} route={sc.route} window={sc.window} />{/if}
              </div>
              <TripList timetable={sc.timetable} selected={sc.selectedTrip} onselect={selectTrip} />
            {:else}
              <p class="muted">This direction runs nothing under this day type.</p>
            {/if}
          </div>
          <aside class="side">
            <ChangeLedger comparison={sc.comparison} baselineName={planName(sc.compareKey)} canUndo={sc.canUndo} canRedo={sc.canRedo}
              onundo={undoChange} onredo={redoChange} onroute={openRoute} />
            <div class="card hub" data-testid="route-hub">
              <h2>At a stop on this route</h2>
              <Combobox label="Read a stop or hub" bind:value={hub} options={routeStops} placeholder="Pick one of its stops…" onchange={(v) => { if (v) openHub(v); hub = ""; }} />
              {#if sc.stop}
                {#if sc.failed.Hub}{@render failure("Hub", "The stop")}{:else}
                  {#if sc.connections}<HubMatrix connections={sc.connections} stopName={sc.gaps?.name ?? sc.stop} onroute={openRoute} />{/if}
                  {#if sc.missed}<NearMisses missed={sc.missed} stopName={sc.gaps?.name ?? sc.stop} onroute={openRoute} />{/if}
                  {#if sc.gaps}<StopWaits gaps={sc.gaps} window={sc.window} />{/if}
                {/if}
              {/if}
            </div>
          </aside>
        </div>
      {/if}

    {:else if sc.stage === "resources"}
      {#if sc.failed.Fleet}{@render failure("Fleet", "Vehicles and hours")}{:else}
        <FleetPanel fleet={sc.fleet} suggestion={sc.suggestion} date={groupDate} {framed} onadopt={adoptBlocks} onroute={openRoute} />
      {/if}

    {:else}
      <div class="two">
        <div>
          {#if !sc.compareKey}
            <Unavailable title="The proposal" reason="Choose a plan to compare against, in the bar at the top." />
          {:else if sc.failed.Proposal}{@render failure("Proposal", "The proposal")}
          {:else if sc.planCompare}
            <PlanScore dayTypes={sc.planCompare} selected={sc.group} onselect={selectGroup} onroute={openRoute} />
          {/if}
        </div>
        {#if sc.failed.Readiness}{@render failure("Readiness", "Ready to publish?")}{:else}
          <Readiness issues={sc.issues} warnings={sc.warnings} {framed} exported={sc.exported} exportError={sc.failed.Export ?? ""} onexport={exportPlan}
            published={sc.published} publishRefused={sc.publishRefused} publishError={sc.failed.Publish ?? ""} onpublish={publishPlan}
            onhold={gestures.retime} />
        {/if}
      </div>
    {/if}
  </main>
</div>

<style>
  .shell { display: grid; grid-template-rows: auto auto 1fr; height: 100vh; background: var(--bg, var(--surface)); color: var(--text); }
  .bar { display: flex; flex-wrap: wrap; gap: 0.6rem 1rem; align-items: center; padding: 0.5rem 0.9rem; border-bottom: 1px solid var(--line); background: var(--surface); }
  .pick { display: flex; gap: 0.35rem; align-items: center; font-size: 0.8rem; }
  .pick span { color: var(--muted); }
  .pick select { font: inherit; padding: 0.25rem; border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--text); max-width: 14rem; }
  .right { margin-left: auto; display: flex; gap: 0.5rem; align-items: center; }
  .right > button { font: inherit; font-size: 0.8rem; border: 1px solid var(--line); background: var(--surface); color: var(--text); border-radius: 6px; padding: 0.25rem 0.6rem; cursor: pointer; }
  .health.bad { border-color: var(--bad); color: var(--bad); }
  .health.warn { border-color: var(--warn); color: var(--warn); }
  .jump { width: 14rem; }
  .stages { display: flex; flex-wrap: wrap; align-items: center; gap: 0.2rem; padding: 0 0.6rem; border-bottom: 1px solid var(--line); background: var(--surface); }
  .stages button { font: inherit; font-size: 0.85rem; display: flex; gap: 0.4rem; align-items: center; padding: 0.55rem 0.7rem; border: 0; border-bottom: 2px solid transparent; background: none; color: var(--muted); cursor: pointer; }
  .stages button.on { color: var(--text); border-bottom-color: var(--accent); font-weight: 600; }
  .n { display: inline-flex; width: 1.2rem; height: 1.2rem; border-radius: 50%; align-items: center; justify-content: center; font-size: 0.7rem; border: 1px solid var(--line); }
  .on .n { background: var(--accent); border-color: var(--accent); color: var(--surface); }
  .sub { font-weight: 400; color: var(--accent); }
  .status { margin: 0 0 0 auto; font-size: 0.78rem; color: var(--muted); max-width: 40rem; text-align: right; }
  .status.alert { color: var(--bad); }
  .stage { overflow: auto; padding: 0.9rem; display: flex; flex-direction: column; gap: 0.9rem; min-height: 0; }
  .two { display: grid; grid-template-columns: minmax(20rem, 1fr) 2fr; gap: 0.9rem; align-items: start; }
  .card { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; padding: 0.8rem 0.9rem; }
  .seg { display: flex; align-self: flex-start; background: var(--surface); border: 1px solid var(--line); border-radius: 6px; overflow: hidden; }
  .seg button { font: inherit; font-size: 0.8rem; border: 0; background: none; padding: 0.3rem 0.8rem; cursor: pointer; color: var(--text); }
  .seg button.on { background: var(--accent); color: var(--surface); }
  .map { flex: 1; min-height: 70vh; border-radius: 8px; overflow: hidden; border: 1px solid var(--line); }
  .route { display: grid; grid-template-columns: 1fr minmax(18rem, 26rem); gap: 0.9rem; align-items: start; }
  .work { display: flex; flex-direction: column; gap: 0.7rem; min-width: 0; }
  .side { display: flex; flex-direction: column; gap: 0.9rem; min-width: 0; }
  .hub { display: flex; flex-direction: column; gap: 0.6rem; }
  .hub h2 { margin: 0; font-size: 0.9rem; }
  .row { display: grid; grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr)); gap: 0.7rem; }
  .dirs { display: flex; justify-content: space-between; align-items: center; }
  .mode { font-size: 0.8rem; color: var(--muted); }
  .muted { color: var(--muted); font-size: 0.8rem; }
  @media (max-width: 1000px) {
    .two, .route { grid-template-columns: 1fr; }
    .status { margin: 0.3rem 0; text-align: left; flex-basis: 100%; }
  }
</style>
