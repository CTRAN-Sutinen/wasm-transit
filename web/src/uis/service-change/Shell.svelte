<script lang="ts">
  /// The service-change UI's layout: PLACEMENT ONLY. The network map
  /// fills the page; the side panel holds the worklist, the proposal,
  /// and the plans; a route or a hub opens in a drawer over the map,
  /// with the watched findings beside the change. Every prop is the
  /// host's answers state; every event calls a host function.
  import {
    sc,
    planName,
    groupMembers,
    periodStart,
    openPlan,
    setCompare,
    fork,
    loadFeedBytes,
    setPanel,
    setCenter,
    selectGroup,
    setEditType,
    setWindow,
    openRoute,
    openStop,
    closeDrawer,
    selectDirection,
    selectTimetableStop,
    selectTrip,
    toggleEdit,
    toggleWatch,
    retry,
    gestures,
    undoChange,
    redoChange,
    exportPlan,
    publishPlan,
  } from "./host.svelte";
  import NetworkMap from "../../components/NetworkMap.svelte";
  import Worklist from "../../components/Worklist.svelte";
  import WatchList from "../../components/WatchList.svelte";
  import DayTypePicker from "../../components/DayTypePicker.svelte";
  import HourWindowPicker from "../../components/HourWindowPicker.svelte";
  import PlanScore from "../../components/PlanScore.svelte";
  import FrequencyGrid from "../../components/FrequencyGrid.svelte";
  import RouteHeader from "../../components/RouteHeader.svelte";
  import DirectionTabs from "../../components/DirectionTabs.svelte";
  import StringLine from "../../components/StringLine.svelte";
  import TimetableGrid from "../../components/TimetableGrid.svelte";
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
  // The other combinations the edited service type runs in (D-95's
  // warning), as the day types name them.
  const alsoIn = $derived(
    sc.dayTypes.flatMap((t) => t.groups).filter((g) => g.id !== sc.group && g.members.includes(sc.editType)).map((g) => g.id),
  );
  const shade = $derived(sc.hour != null ? { from: sc.hour, to: sc.hour + 1 } : sc.window);
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
      <button type="button" class="health" class:bad={sc.issues.length > 0} class:warn={!sc.issues.length && sc.warnings.length > 0} onclick={() => setPanel("proposal")}>
        {sc.issues.length} issues · {sc.warnings.length} warnings
      </button>
      <div class="jump">
        <Combobox label="Open a route" bind:value={jump} options={routeOptions} placeholder="Open a route…" onchange={(v) => { if (v) openRoute(v); jump = ""; }} />
      </div>
    </div>
  </header>
  <p class="status" class:alert={sc.alert} role="status" data-testid="status">{sc.busy ? "… " : ""}{sc.status}</p>

  <div class="body">
    <aside class="panel">
      <div class="tabs" role="tablist">
        {#each [["worklist", "Worklist"], ["proposal", "Proposal"], ["plans", "Plans"]] as [id, label] (id)}
          <button type="button" role="tab" aria-selected={sc.panel === id} class:on={sc.panel === id} onclick={() => setPanel(id as "worklist" | "proposal" | "plans")}>{label}</button>
        {/each}
      </div>
      <div class="panel-body">
        {#if sc.panel === "worklist"}
          {#if !framed}
            <div class="note" data-testid="base-note">
              You are looking at <strong>{planName(sc.activeKey)}</strong>. To change it, start a plan from it in
              <button type="button" class="link" onclick={() => setPanel("plans")}>Plans</button>.
            </div>
          {/if}
          {#if sc.noService}
            <Unavailable title="No service in this plan's period"
              reason={`${planName(sc.activeKey)} runs nothing from ${periodStart()} on: its calendars end before then. Start the plan again with "Runs through" set (Plans), which carries the weekly service to that day.`} />
          {/if}
          {#if sc.findings && (sc.watch.length > 0)}
            <WatchList watched={sc.findings.watched} baselineName={planName(sc.compareKey)} onunwatch={toggleWatch} />
          {/if}
          {#if sc.failed.Worklist}{@render failure("Worklist", "Worklist")}
          {:else if sc.findings}
            <Worklist
              findings={sc.findings}
              baselineName={sc.compareKey ? planName(sc.compareKey) : ""}
              watching={sc.watch}
              onhole={(route, hour) => openRoute(route, hour)}
              onwait={openStop}
              onmiss={openStop}
              onwatch={toggleWatch}
            />
          {:else if !sc.noService}
            <p class="muted">Reading the network…</p>
          {/if}
        {:else if sc.panel === "proposal"}
          {#if !sc.compareKey}
            <Unavailable title="The proposal" reason="Choose a plan to compare against, in the bar at the top." />
          {:else if sc.failed.Proposal}{@render failure("Proposal", "The proposal")}
          {:else if sc.planCompare}
            <PlanScore dayTypes={sc.planCompare} selected={sc.group} onselect={selectGroup} onroute={(r) => openRoute(r)} />
          {/if}
          {#if sc.failed.Readiness}{@render failure("Readiness", "Ready to publish?")}{:else}
            <Readiness issues={sc.issues} warnings={sc.warnings} {framed} exported={sc.exported} exportError={sc.failed.Export ?? ""} onexport={exportPlan}
              published={sc.published} publishRefused={sc.publishRefused} publishError={sc.failed.Publish ?? ""} onpublish={publishPlan}
              onhold={gestures.retime} />
          {/if}
        {:else}
          <PlanList plans={sc.plans} activeKey={sc.activeKey} compareKey={sc.compareKey} feed={sc.feed} onopen={openPlan} oncompare={setCompare} onfork={fork}
            onfeed={async (f) => loadFeedBytes(new Uint8Array(await f.arrayBuffer()), f.name)} />
        {/if}
      </div>
    </aside>

    <main class="center">
      <div class="center-tools">
        <div class="seg" role="group" aria-label="Show">
          <button type="button" class:on={sc.center === "map"} aria-pressed={sc.center === "map"} onclick={() => setCenter("map")}>Map</button>
          <button type="button" class:on={sc.center === "grid"} aria-pressed={sc.center === "grid"} onclick={() => setCenter("grid")}>Frequency grid</button>
        </div>
        <div class="hub-pick">
          <Combobox label="Open a stop or hub" bind:value={hub} options={stopOptions} placeholder="Open a stop or hub…" onchange={(v) => { if (v) openStop(v); hub = ""; }} />
        </div>
      </div>
      {#if sc.panel === "plans"}
        <div class="timeline">
          {#if sc.failed.Plans}{@render failure("Plans", "Pick timeline")}
          {:else if sc.chain}
            <PickTimeline periods={sc.chain.periods} names={Object.fromEntries(sc.plans.map((p) => [p.id, p.name]))} inForce={sc.chain.inForce}
              issues={sc.chain.issues} today={today()} activeId={sc.plans.find((p) => p.key === sc.activeKey)?.id ?? ""} />
          {/if}
        </div>
      {/if}
      {#if sc.center === "map"}
        {#if sc.failed.Map}{@render failure("Map", "The network")}{:else}
          <div class="map"><NetworkMap routes={sc.lines} {changed} selected={sc.route} onroute={(r) => openRoute(r)} /></div>
        {/if}
      {:else}
        <div class="grid">
          {#if sc.failed.Grid}{@render failure("Grid", "Frequency by hour")}
          {:else if sc.frequency}<FrequencyGrid frequency={sc.frequency} window={sc.window} {changed} selected={sc.route} onroute={(r) => openRoute(r)} />{/if}
        </div>
      {/if}

      {#if sc.route}
        <section class="drawer" aria-label="Route {routeRow?.name ?? sc.route}" data-testid="route-drawer">
          <div class="drawer-head">
            <RouteHeader route={sc.route} routeName={routeRow?.name ?? sc.route} longName={routeRow?.long_name ?? null} groupName={sc.group}
              frequency={sc.frequency} comparison={sc.comparison} editing={sc.editing} onedit={toggleEdit} />
            <button type="button" class="close" aria-label="Close" onclick={closeDrawer}>×</button>
          </div>
          <div class="drawer-body">
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
                  <StringLine timetable={sc.timetable} window={shade} editable={sc.editing && framed} selected={sc.selectedTrip} onselect={selectTrip}
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
              {:else}
                <p class="muted">This direction runs nothing under this day type.</p>
              {/if}
            </div>
            <aside class="beside">
              {#if sc.findings}<WatchList watched={sc.findings.watched} baselineName={planName(sc.compareKey)} onunwatch={toggleWatch} />{/if}
              <ChangeLedger comparison={sc.comparison} baselineName={planName(sc.compareKey)} canUndo={sc.canUndo} canRedo={sc.canRedo}
                onundo={undoChange} onredo={redoChange} onroute={(r) => openRoute(r)} />
            </aside>
          </div>
        </section>
      {:else if sc.stop}
        <section class="drawer" aria-label="Stop" data-testid="hub-drawer">
          <div class="drawer-head">
            <h2>{sc.gaps?.name ?? sc.stop}</h2>
            <button type="button" class="close" aria-label="Close" onclick={closeDrawer}>×</button>
          </div>
          {#if sc.failed.Hub}{@render failure("Hub", "The hub")}{:else}
            <div class="hub-body">
              {#if sc.gaps}<StopWaits gaps={sc.gaps} window={sc.window} />{/if}
              {#if sc.missed}<NearMisses missed={sc.missed} stopName={sc.gaps?.name ?? sc.stop} onroute={(r) => openRoute(r)} />{/if}
              {#if sc.connections}<HubMatrix connections={sc.connections} stopName={sc.gaps?.name ?? sc.stop} onroute={(r) => openRoute(r)} />{/if}
              {#if sc.findings}<WatchList watched={sc.findings.watched} baselineName={planName(sc.compareKey)} onunwatch={toggleWatch} />{/if}
            </div>
          {/if}
        </section>
      {/if}
    </main>
  </div>
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
  .status { margin: 0; padding: 0.25rem 0.9rem; font-size: 0.78rem; color: var(--muted); border-bottom: 1px solid var(--line); }
  .status.alert { color: var(--bad); }
  .body { display: grid; grid-template-columns: minmax(20rem, 26rem) 1fr; min-height: 0; }
  .panel { border-right: 1px solid var(--line); display: flex; flex-direction: column; min-height: 0; background: var(--surface); }
  .tabs { display: flex; border-bottom: 1px solid var(--line); }
  .tabs button { flex: 1; font: inherit; font-size: 0.85rem; padding: 0.5rem; border: 0; border-bottom: 2px solid transparent; background: none; color: var(--muted); cursor: pointer; }
  .tabs button.on { color: var(--text); border-bottom-color: var(--accent); font-weight: 600; }
  .panel-body { overflow: auto; padding: 0.7rem 0.9rem; display: flex; flex-direction: column; gap: 0.8rem; }
  .note { font-size: 0.8rem; padding: 0.5rem 0.7rem; border: 1px solid var(--warn); border-radius: 6px; }
  .link { background: none; border: 0; padding: 0; font: inherit; color: var(--accent); cursor: pointer; text-decoration: underline; }
  .center { position: relative; min-height: 0; display: flex; flex-direction: column; }
  .center-tools { position: absolute; z-index: 2; top: 0.6rem; left: 0.6rem; right: 3.2rem; display: flex; gap: 0.6rem; align-items: center; pointer-events: none; }
  .center-tools > * { pointer-events: auto; }
  .seg { display: flex; background: var(--surface); border: 1px solid var(--line); border-radius: 6px; overflow: hidden; box-shadow: 0 1px 3px rgb(0 0 0 / 0.15); }
  .seg button { font: inherit; font-size: 0.8rem; border: 0; background: none; padding: 0.3rem 0.7rem; cursor: pointer; color: var(--text); }
  .seg button.on { background: var(--accent); color: var(--surface); }
  .hub-pick { width: 16rem; background: var(--surface); border-radius: 6px; }
  .timeline { padding: 3.2rem 0.8rem 0.6rem; border-bottom: 1px solid var(--line); background: var(--surface); }
  .map { flex: 1; min-height: 0; }
  .grid { flex: 1; overflow: auto; padding: 3rem 0.8rem 0.8rem; }
  .drawer {
    position: absolute; z-index: 3; left: 0.6rem; right: 0.6rem; bottom: 0.6rem; max-height: 62%;
    display: flex; flex-direction: column; background: var(--surface); border: 1px solid var(--line); border-radius: 10px;
    box-shadow: 0 6px 24px rgb(0 0 0 / 0.25); overflow: hidden;
  }
  .drawer-head { display: flex; align-items: flex-start; gap: 0.5rem; border-bottom: 1px solid var(--line); }
  .drawer-head > :global(:first-child) { flex: 1; }
  .drawer-head h2 { margin: 0; padding: 0.7rem 0.9rem; font-size: 1rem; }
  .close { font-size: 1.4rem; line-height: 1; background: none; border: 0; padding: 0.4rem 0.7rem; cursor: pointer; color: var(--muted); }
  .drawer-body { display: grid; grid-template-columns: 1fr minmax(16rem, 22rem); gap: 0.8rem; padding: 0.7rem 0.9rem; overflow: auto; }
  .work { display: flex; flex-direction: column; gap: 0.6rem; min-width: 0; }
  .beside { display: flex; flex-direction: column; gap: 0.8rem; }
  .dirs { display: flex; justify-content: space-between; align-items: center; }
  .mode { font-size: 0.8rem; color: var(--muted); }
  .hub-body { display: grid; grid-template-columns: repeat(auto-fit, minmax(20rem, 1fr)); gap: 0.8rem; padding: 0.7rem 0.9rem; overflow: auto; }
  .muted { color: var(--muted); font-size: 0.8rem; }
  @media (max-width: 900px) {
    .body { grid-template-columns: 1fr; grid-template-rows: auto 1fr; }
    .panel { max-height: 40vh; border-right: 0; border-bottom: 1px solid var(--line); }
    .drawer-body { grid-template-columns: 1fr; }
  }
</style>
