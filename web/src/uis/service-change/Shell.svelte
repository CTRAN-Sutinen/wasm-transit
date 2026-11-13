<script lang="ts">
  /// The service-change UI's layout: PLACEMENT ONLY. One selection bar
  /// (plan, day type, hours, route) over the stage that is open — the
  /// plan and its calendar, the scan, one route's day with a hub beside
  /// it, the stops (each on its own card, apart from any route),
  /// vehicles and hours, the proposal — each with the page's full
  /// width. The plan compared against is chosen on the Proposal, the
  /// stage it serves; the scan and the route mark the change only once
  /// one is chosen. Every prop is the host's answers state; every
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
    openStop,
    openVisit,
    readDate,
    setReadDate,
    closeStop,
    reopenStop,
    closeRoute,
    openBlock,
    showBlock,
    scanGroup,
    renamePlan,
    setPickDates,
    runsToday,
    deletePlan,
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
  import StopMap from "../../components/StopMap.svelte";
  import StopRecord from "../../components/StopRecord.svelte";
  import StopEvents from "../../components/StopEvents.svelte";
  import { issueWords } from "../../components/designFormat";
  import DayTypePicker from "../../components/DayTypePicker.svelte";
  import HourWindowPicker from "../../components/HourWindowPicker.svelte";
  import ServiceKeys from "../../components/ServiceKeys.svelte";
  import FleetPanel from "../../components/FleetPanel.svelte";
  import TripList from "../../components/TripList.svelte";
  import PlanScore from "../../components/PlanScore.svelte";
  import FrequencyGrid from "../../components/FrequencyGrid.svelte";
  import RouteHeader from "../../components/RouteHeader.svelte";
  import DirectionToggle from "../../components/DirectionToggle.svelte";
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
  import PickList from "../../components/PickList.svelte";
  import PickTimeline from "../../components/PickTimeline.svelte";
  import Combobox from "../../components/Combobox.svelte";
  import Unavailable from "../../components/Unavailable.svelte";
  import { today } from "../kit";

  // Can the open plan change? A published pick cannot (D-139): it forks.
  const openRow = $derived(sc.plans.find((p) => p.key === sc.activeKey));
  const framed = $derived(!!openRow && !openRow.published && !!openRow.id);
  const locked = $derived(
    openRow?.published
      ? `${openRow.name} is published, so it does not change: fork it (Plan & calendar) and change the fork.`
      : `${openRow?.name ?? "This plan"} has no pick date: set one (Plan & calendar) to change it.`,
  );
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
  const stopNames = $derived(new Map(sc.stops.map((s) => [s.stop_id, s.name])));
  // What the stops' closures do to the plan's trips (D-155): warnings,
  // never refusals.
  const servedWarnings = $derived(
    sc.warnings.map((w) => issueWords(w)).filter((w) => w.kind === "endpoint not served" || w.kind === "under two served"),
  );
  let jump = $state("");
  let hub = $state("");
  let stopFind = $state("");
  const openStation = $derived(sc.stopCard?.stop.parent ? (stopNames.get(sc.stopCard.stop.parent) ?? null) : null);
  let tableMode = $state(false);
</script>

{#snippet onDate()}
  <label class="mode" title="The day the stops and the timetable are read on: a stop closed that day is marked">
    Stops on <input type="date" value={readDate()} onchange={(e) => setReadDate(e.currentTarget.value)} data-testid="stops-on" />
  </label>
{/snippet}

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
      {#if s.id === "route" && sc.route}
        <button type="button" class="close" aria-label="Close route {routeRow?.name ?? sc.route}" title="Close the route" onclick={closeRoute} data-testid="close-route">×</button>
      {/if}
    {/each}
    <p class="status" class:alert={sc.alert} role="status" data-testid="status">{sc.busy ? "… " : ""}{sc.status}</p>
  </nav>

  <main class="stage" data-testid="stage-{sc.stage}">
    {#if sc.noService && sc.stage !== "calendar"}
      <Unavailable title="No service in this plan's period"
        reason={`${planName(sc.activeKey)} runs nothing from ${periodStart()} on: no service key has a date in its period. Give its keys ranges in the period, or fork the plan it came from again with "Runs through" set (Plan & calendar).`} />
    {/if}

    {#if sc.stage === "calendar"}
      {#if sc.failed.Plans}{@render failure("Plans", "Pick timeline")}
      {:else if sc.chain}
        <PickTimeline periods={sc.chain.periods} names={Object.fromEntries(sc.plans.map((p) => [p.id, p.name]))} inForce={sc.chain.inForce}
          issues={sc.chain.issues} today={today()} activeId={sc.plans.find((p) => p.key === sc.activeKey)?.id ?? ""} />
      {/if}
      <div class="stack">
        <PickList plans={sc.plans} activeKey={sc.activeKey} inForceId={runsToday()} onopen={openPlan} onfork={fork}
          onrename={renamePlan} ondate={setPickDates} ondelete={deletePlan}
          onfeed={async (f) => loadFeedBytes(new Uint8Array(await f.arrayBuffer()), f.name)} />
        <div class="card">
          {#if sc.failed.Keys}{@render failure("Keys", "Service keys")}{:else}
            <ServiceKeys keys={sc.keys} dayTypes={sc.dayTypes} {framed} {locked}
              ontoggleday={calendar.toggleDay} onaddrange={calendar.addRange} onsetrange={calendar.setRange} onremoverange={calendar.removeRange}
              onsetexception={calendar.setException} onremoveexception={calendar.removeException} onrename={calendar.rename} onmove={calendar.move}
              onadd={calendar.add} onremove={calendar.remove}
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

    {:else if sc.stage === "stops"}
      <div class="stops">
        <div class="stack">
          <div class="find">
            <Combobox label="Find a stop" bind:value={stopFind} options={stopOptions} placeholder="Find a stop by name or id…"
              onchange={(v) => { if (v) openStop(v); stopFind = ""; }} />
            {@render onDate()}
          </div>
          {#if sc.failed.StopStatus}{@render failure("StopStatus", "The stops on this date")}{/if}
          {#if servedWarnings.length}
            <ul class="warnings" data-testid="stop-warnings">
              {#each servedWarnings as w, i (i)}<li>{w.detail}</li>{/each}
            </ul>
          {/if}
          {#if sc.failed.Stops}{@render failure("Stops", "The stops")}{:else}
            <div class="map"><StopMap stops={sc.stops} lines={sc.lines} selected={sc.stopOpen} onstop={openStop} status={sc.stopStatus} /></div>
          {/if}
        </div>
        <aside class="side">
          {#if !sc.stopOpen}
            <p class="muted" data-testid="stop-none">Open a stop: click it on the map, or find it by name. A stop belongs to the network, not to a route: every route that visits it is listed on its card.</p>
          {:else if sc.failed.StopCard}{@render failure("StopCard", "The stop")}
          {:else if sc.stopCard}
            <div class="card"><StopRecord stopId={sc.stopOpen} detail={sc.stopCard} stationName={openStation} groupName={sc.group} onvisit={openVisit} /></div>
            <div class="card">
              {#if sc.failed.StopHistory}{@render failure("StopHistory", "The stop's history")}{:else}
                <StopEvents date={readDate()} held={sc.stopsHeld} history={sc.stopHistory} refusals={sc.stopsRefused} busy={sc.busy}
                  onclose={(from, until, note) => closeStop(sc.stopOpen, from, until, note)}
                  onreopen={(on, note) => reopenStop(sc.stopOpen, on, note)} />
              {/if}
            </div>
          {:else if sc.stops.length && !sc.stops.some((x) => x.stop_id === sc.stopOpen)}
            <Unavailable title="No such stop" reason={`${planName(sc.activeKey)} has no stop ${sc.stopOpen}.`} />
          {/if}
        </aside>
      </div>

    {:else if sc.stage === "route"}
      {#if !sc.route}
        <Unavailable title="No route open" reason="Open a route: a row of the frequency grid, a line on the map, a route a service key carries, or the route box at the top right." />
      {:else if sc.routes.length && !routeRow}
        <Unavailable title="No such route" reason={`${planName(sc.activeKey)} has no route ${sc.route}. Open one from the frequency grid, the map, or the route box at the top right.`} />
        <button type="button" class="closer" onclick={closeRoute}>Close route</button>
      {:else}
        <div class="routebar">
          <DirectionToggle directions={sc.routeDetail?.directions ?? null} selected={sc.direction} onselect={selectDirection} />
          <label class="mode"><input type="checkbox" bind:checked={tableMode} /> timetable</label>
          {@render onDate()}
          <button type="button" class="closer" onclick={closeRoute}>Close route</button>
        </div>
        <RouteHeader route={sc.route} routeName={routeRow?.name ?? sc.route} longName={routeRow?.long_name ?? null} groupName={sc.group}
          frequency={sc.frequency} comparison={sc.comparison} editing={sc.editing} onedit={toggleEdit} shortcut="" />
        <div class="route">
          <div class="work">
            {#if sc.failed.Route}{@render failure("Route", "The route")}{/if}
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
                  <GesturePanel {framed} {locked} members={groupMembers()} editType={sc.editType} {alsoIn} onedittype={setEditType}
                    stops={sc.timetable.stops} allStops={stopOptions} selectedStop={sc.selectedStop} trips={sc.timetable.trips}
                    selectedTrip={sc.selectedTrip} window={sc.window} onspan={gestures.span} onrunning={gestures.running}
                    onretime={gestures.retime} oninsertstop={gestures.insertStop} onremovestop={gestures.removeStop} />
                {/key}
              {/if}
              <div class="row">
                <RunTimes timetable={sc.timetable} />
                {#if sc.frequency}<HourGaps frequency={sc.frequency} route={sc.route} window={sc.window} />{/if}
              </div>
              <TripList timetable={sc.timetable} selected={sc.selectedTrip} onselect={selectTrip} onblock={showBlock} />
            {:else}
              {@const here = sc.routeDetail?.directions.find((d) => d.direction === sc.direction)}
              {@const others = (sc.routeDetail?.directions ?? []).filter((d) => d.direction !== sc.direction)}
              <p class="muted" data-testid="direction-empty">
                {here?.direction == null ? "These trips" : `Trips to ${here.name}`} run nothing under {sc.group}.
                {#each others as d (d.direction ?? "")}
                  <button type="button" class="closer" onclick={() => selectDirection(d.direction)}>Show {d.direction == null ? "trips in no direction" : `to ${d.name}`}</button>
                {/each}
              </p>
            {/if}
          </div>
          <aside class="side">
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
            {#if sc.compareKey}
              <ChangeLedger comparison={sc.comparison} baselineName={planName(sc.compareKey)} canUndo={sc.canUndo} canRedo={sc.canRedo}
                onundo={undoChange} onredo={redoChange} onroute={openRoute} />
            {/if}
          </aside>
        </div>
      {/if}

    {:else if sc.stage === "resources"}
      {#if sc.failed.Fleet}{@render failure("Fleet", "Vehicles and hours")}{:else}
        <FleetPanel fleet={sc.fleet} suggestion={sc.suggestion} date={groupDate} {framed} onadopt={adoptBlocks} onroute={openRoute}
          block={sc.block} blockTrips={sc.blockTrips} blockError={sc.failed.Block ?? ""} onblock={openBlock} stopName={(id) => stopNames.get(id) ?? id} />
      {/if}

    {:else}
      <div class="routebar">
        <label class="pick"><span>Compare {planName(sc.activeKey)} with</span>
          <select value={sc.compareKey} onchange={(e) => setCompare(e.currentTarget.value)} data-testid="base-select">
            <option value="">nothing</option>
            {#each sc.plans.filter((p) => p.key !== sc.activeKey) as p (p.key)}<option value={p.key}>{p.name}</option>{/each}
          </select>
        </label>
      </div>
      <div class="two">
        <div>
          {#if !sc.compareKey}
            <Unavailable title="The proposal" reason="Choose a plan above to compare it with." />
          {:else if sc.failed.Proposal}{@render failure("Proposal", "The proposal")}
          {:else if sc.planCompare}
            <PlanScore dayTypes={sc.planCompare} selected={sc.group} onselect={scanGroup} onroute={openRoute} />
          {/if}
        </div>
        {#if sc.failed.Readiness}{@render failure("Readiness", "Ready to publish?")}{:else}
          <Readiness issues={sc.issues} warnings={sc.warnings} {framed} {locked} exported={sc.exported} exportError={sc.failed.Export ?? ""} onexport={exportPlan}
            published={sc.published} publishRefused={sc.publishRefused} publishError={sc.failed.Publish ?? ""} onpublish={publishPlan}
            onhold={gestures.retime} onblock={showBlock} />
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
  .stages button.close { padding: 0.2rem 0.35rem; margin-left: -0.5rem; font-size: 1rem; line-height: 1; }
  .stages button.close:hover { color: var(--text); }
  .routebar { display: flex; flex-wrap: wrap; align-items: center; gap: 0.8rem; }
  .routebar .mode { margin-left: auto; }
  .closer { font: inherit; font-size: 0.78rem; border: 1px solid var(--line); background: var(--surface); color: var(--text); border-radius: 6px; padding: 0.2rem 0.6rem; cursor: pointer; }
  .status { margin: 0 0 0 auto; font-size: 0.78rem; color: var(--muted); max-width: 40rem; text-align: right; }
  .status.alert { color: var(--bad); }
  .stage { overflow: auto; padding: 0.9rem; display: flex; flex-direction: column; gap: 0.9rem; min-height: 0; }
  .stack { display: flex; flex-direction: column; gap: 0.9rem; min-width: 0; }
  .two { display: grid; grid-template-columns: minmax(20rem, 1fr) 2fr; gap: 0.9rem; align-items: start; }
  .card { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; padding: 0.8rem 0.9rem; }
  .seg { display: flex; align-self: flex-start; background: var(--surface); border: 1px solid var(--line); border-radius: 6px; overflow: hidden; }
  .seg button { font: inherit; font-size: 0.8rem; border: 0; background: none; padding: 0.3rem 0.8rem; cursor: pointer; color: var(--text); }
  .seg button.on { background: var(--accent); color: var(--surface); }
  .map { flex: 1; min-height: 70vh; border-radius: 8px; overflow: hidden; border: 1px solid var(--line); }
  .stops { display: grid; grid-template-columns: 1fr minmax(18rem, 26rem); gap: 0.9rem; align-items: start; }
  .find { max-width: 26rem; }
  /* A definite height: the map inside fills 100% of it (a min-height
     alone leaves the canvas at its own minimum). */
  .warnings { margin: 0; padding-left: 1.1rem; font-size: 0.85rem; color: var(--warn, #b45309); }
  .stops .map { flex: none; height: calc(100vh - 12rem); min-height: 24rem; }
  .route { display: grid; grid-template-columns: 1fr minmax(18rem, 26rem); gap: 0.9rem; align-items: start; }
  .work { display: flex; flex-direction: column; gap: 0.7rem; min-width: 0; }
  .side { display: flex; flex-direction: column; gap: 0.9rem; min-width: 0; }
  .hub { display: flex; flex-direction: column; gap: 0.6rem; }
  .hub h2 { margin: 0; font-size: 0.9rem; }
  .row { display: grid; grid-template-columns: repeat(auto-fit, minmax(18rem, 1fr)); gap: 0.7rem; }
  .mode { font-size: 0.8rem; color: var(--muted); }
  .muted { color: var(--muted); font-size: 0.8rem; }
  @media (max-width: 1000px) {
    .two, .route, .stops { grid-template-columns: 1fr; }
    .status { margin: 0.3rem 0; text-align: left; flex-basis: 100%; }
  }
</style>
