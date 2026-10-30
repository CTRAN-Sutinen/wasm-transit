<script lang="ts">
  /// The service-design UI — six views over one selection, chained:
  /// a rail of views (keys 1–6) and a context bar (plan, baseline,
  /// group, route, stop, hours) frame whichever view is open. Plans
  /// (the pick timeline and the plan set), Network (the group's
  /// numbers, the day's shape as the hour brush, the frequency grid),
  /// Coverage (who is reached, the unserved on the map), Hubs (the
  /// connection matrix, corridors, waits), Route (the string-line
  /// workbench and its gestures), and Proposal (the scorecard, the
  /// redline, readiness and export). A frame with no answer says why
  /// in place; the rest of the view keeps answering.
  /// Every panel is a render component fed from the host's `sd`;
  /// this file only places them and routes their events to the host.
  import {
    sd, VIEWS, setView, openPlan, setCompare, fork, loadFeedBytes, loadLayerBytes, selectGroup, openRoute, clearRoute,
    selectDirection, selectTimetableStop, openStop, clearStop, setWindow, setCoverageParams, gestures, undoChange,
    redoChange, exportPlan, toggleEdit, selectTrip, pickUnserved, retry, type View,
  } from "./host.svelte";
  import { today } from "../kit";
  import DesignRail from "../../components/DesignRail.svelte";
  import ContextBar from "../../components/ContextBar.svelte";
  import Unavailable from "../../components/Unavailable.svelte";
  import PickTimeline from "../../components/PickTimeline.svelte";
  import PlanList from "../../components/PlanList.svelte";
  import GroupTable from "../../components/GroupTable.svelte";
  import GroupLedger from "../../components/GroupLedger.svelte";
  import DayProfile from "../../components/DayProfile.svelte";
  import FrequencyGrid from "../../components/FrequencyGrid.svelte";
  import CoveragePanel from "../../components/CoveragePanel.svelte";
  import CoverageMap from "../../components/CoverageMap.svelte";
  import HubMatrix from "../../components/HubMatrix.svelte";
  import CorridorTable from "../../components/CorridorTable.svelte";
  import StopWaits from "../../components/StopWaits.svelte";
  import Combobox from "../../components/Combobox.svelte";
  import RouteHeader from "../../components/RouteHeader.svelte";
  import DirectionTabs from "../../components/DirectionTabs.svelte";
  import StringLine from "../../components/StringLine.svelte";
  import TimetableGrid from "../../components/TimetableGrid.svelte";
  import RunTimes from "../../components/RunTimes.svelte";
  import HourGaps from "../../components/HourGaps.svelte";
  import GesturePanel from "../../components/GesturePanel.svelte";
  import ChangeLedger from "../../components/ChangeLedger.svelte";
  import FocusMap from "../../components/FocusMap.svelte";
  import ProposalScore from "../../components/ProposalScore.svelte";
  import Redline from "../../components/Redline.svelte";
  import Readiness from "../../components/Readiness.svelte";

  const bytesOf = async (f: File) => new Uint8Array(await f.arrayBuffer());
  const planName = (key: string) => sd.plans.find((p) => p.key === key)?.name ?? "";
  const framed = $derived(!!sd.plans.find((p) => p.key === sd.activeKey)?.id);
  const group = $derived(sd.groups.find((g) => g.service_type === sd.group) ?? null);
  const routeRow = $derived(sd.routes.find((r) => r.route_id === sd.route) ?? null);
  const changed = $derived(sd.comparison?.routes.map((r) => r.route_id) ?? []);
  let tableMode = $state(false);

  function keys(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    if (target.closest("input, select, textarea") || e.metaKey || e.ctrlKey || e.altKey) return;
    const n = Number(e.key);
    if (n >= 1 && n <= VIEWS.length) setView(VIEWS[n - 1].id);
    else if ((e.key === "e" || e.key === "E") && sd.view === "route") toggleEdit();
  }
</script>

<svelte:window onkeydown={keys} />

{#snippet failure(frame: string, title: string)}
  <Unavailable {title} error={sd.failed[frame]} onretry={retry} />
{/snippet}

<div class="shell">
  <div class="rail"><DesignRail views={VIEWS} active={sd.view} onselect={(v) => setView(v as View)} /></div>
  <div class="main">
    <ContextBar
      plans={sd.plans}
      activeKey={sd.activeKey}
      compareKey={sd.compareKey}
      groups={sd.groups}
      group={sd.group}
      routes={sd.routes.map((r) => ({ value: r.route_id, label: r.name, sub: r.long_name ?? undefined }))}
      route={sd.route}
      stopName={sd.stop ? (sd.gaps?.name ?? sd.stop) : ""}
      window={sd.window}
      issues={sd.issues.length}
      warnings={sd.warnings.length}
      status={sd.status}
      busy={sd.busy}
      onplan={openPlan}
      oncompare={setCompare}
      ongroup={selectGroup}
      onroute={openRoute}
      onclearroute={clearRoute}
      onclearstop={clearStop}
      onclearwindow={() => setWindow(null)}
      onhealth={() => setView("proposal")}
    />

    <main class="view" data-testid="view-{sd.view}">
      {#if sd.view === "plans"}
        <h1>Plans <span>where each plan sits in the pick cycle</span></h1>
        {#if sd.failed.PickTimeline}
          {@render failure("PickTimeline", "Pick timeline")}
        {:else if sd.chain}
          <PickTimeline
            periods={sd.chain.periods}
            names={Object.fromEntries(sd.plans.map((p) => [p.id, p.name]))}
            inForce={sd.chain.inForce}
            issues={sd.chain.issues}
            today={today()}
            activeId={sd.plans.find((p) => p.key === sd.activeKey)?.id ?? ""}
          />
        {/if}
        <div class="row">
          <PlanList
            plans={sd.plans}
            activeKey={sd.activeKey}
            compareKey={sd.compareKey}
            feed={sd.feed}
            onopen={openPlan}
            oncompare={setCompare}
            onfork={fork}
            onfeed={async (f) => loadFeedBytes(await bytesOf(f), f.name)}
          />
          {#if sd.failed.Groups}
            {@render failure("Groups", "Service groups")}
          {:else}
            <GroupTable groups={sd.groups} selected={sd.group} onselect={selectGroup} />
          {/if}
        </div>
      {:else if sd.view === "network"}
        <h1>Network <span>{group ? `${group.name} on its representative day, ${group.date}` : ""}</span></h1>
        {#if sd.failed.Comparison}{@render failure("Comparison", "Against the baseline")}{/if}
        <GroupLedger {group} comparison={sd.comparison} baselineName={planName(sd.compareKey)} />
        {#if sd.failed.DayProfile}
          {@render failure("DayProfile", "Trips per hour")}
        {:else if sd.hourly}
          <DayProfile hours={sd.hourly} window={sd.window} onwindow={setWindow} />
        {/if}
        {#if sd.failed.FrequencyGrid}
          {@render failure("FrequencyGrid", "Frequency by hour")}
        {:else if sd.frequency}
          <FrequencyGrid frequency={sd.frequency} window={sd.window} {changed} selected={sd.route} onroute={openRoute} />
        {/if}
      {:else if sd.view === "coverage"}
        <div class="split">
          <CoveragePanel
            coverage={sd.coverage}
            unserved={sd.unserved}
            members={group?.members ?? []}
            layer={sd.layer}
            params={sd.coverageParams}
            window={sd.window}
            selected={sd.unservedPick}
            onparams={setCoverageParams}
            onlayer={async (f) => loadLayerBytes(await bytesOf(f))}
            onselect={pickUnserved}
          />
          <div class="map-area">
            {#if sd.failed.Coverage}{@render failure("Coverage", "Coverage")}{/if}
            {#if sd.failed.Unserved}{@render failure("Unserved", "Unserved points")}{/if}
            <Unavailable title="Gained and lost against the baseline" reason="No op compares two plans' coverage." blockers={["D-43", "QAF-20"]} />
            <div class="map"><CoverageMap points={sd.unserved ?? []} selected={sd.unservedPick} onselect={pickUnserved} /></div>
          </div>
        </div>
      {:else if sd.view === "hubs"}
        <h1>Hubs &amp; corridors <span>{group ? `${group.name}, ${group.date}` : ""}</span></h1>
        <div class="picker">
          {#if sd.failed.HubPicker}
            {@render failure("HubPicker", "Stops")}
          {:else}
            <Combobox
              label="Hub"
              value={sd.stop}
              options={sd.stops.map((s) => ({ value: s.stop_id, label: s.name, sub: s.stop_id }))}
              placeholder="Find a stop or station…"
              onchange={(v) => v && openStop(v)}
            />
          {/if}
        </div>
        <div class="row">
          {#if sd.failed.HubMatrix}
            {@render failure("HubMatrix", "Connections")}
          {:else if sd.connections && sd.stop}
            <HubMatrix connections={sd.connections} stopName={sd.gaps?.name ?? sd.stop} onroute={openRoute} />
          {:else}
            <Unavailable title="Connections" reason="Pick a stop or station to read who connects to whom there." />
          {/if}
          <div class="col">
            {#if sd.failed.StopWaits}
              {@render failure("StopWaits", "Waits")}
            {:else if sd.gaps && sd.gaps.departures === 0}
              <Unavailable title="Waits at {sd.gaps.name}" reason="Nothing departs from this stop itself; stop_gaps reads one stop, not a station's bays. Pick a bay." blockers={["QAF-92"]} />
            {:else if sd.gaps}
              <StopWaits gaps={sd.gaps} window={sd.window} />
            {/if}
            <Unavailable title="Just-missed connections" reason="Arrivals a minute after the departure leaves have no op yet." blockers={["Q9"]} />
            <Unavailable title="Longest waits across the network" reason="stop_gaps answers one stop at a time; no op ranks every stop." blockers={["Q8"]} />
          </div>
        </div>
        {#if sd.failed.Corridors}
          {@render failure("Corridors", "Shared corridors")}
        {:else if sd.overlaps}
          <CorridorTable overlaps={sd.overlaps} onroute={openRoute} onstop={openStop} />
        {/if}
      {:else if sd.view === "route"}
        {#if !sd.route}
          <Unavailable title="No route picked" reason="Pick a route from the Network grid, a hub, or the jump box." />
        {:else}
          <RouteHeader
            route={sd.route}
            routeName={routeRow?.name ?? sd.route}
            longName={routeRow?.long_name ?? null}
            groupName={group?.name ?? sd.group}
            frequency={sd.frequency}
            comparison={sd.comparison}
            editing={sd.editing}
            onedit={toggleEdit}
          />
          <div class="workbench">
            <div class="center">
              {#if sd.failed.RouteHeader}{@render failure("RouteHeader", "The route")}{/if}
              {#if sd.routeDetail}
                <div class="dirs">
                  <DirectionTabs directions={sd.routeDetail.directions} selected={sd.direction} onselect={selectDirection} />
                  <label class="mode"><input type="checkbox" bind:checked={tableMode} /> timetable</label>
                </div>
              {/if}
              {#if sd.failed.StringLine}
                {@render failure("StringLine", "Timetable")}
              {:else if sd.timetable}
                {#if tableMode}
                  <TimetableGrid timetable={sd.timetable} baselineName={planName(sd.compareKey)} selectedStop={sd.selectedStop} onstop={selectTimetableStop} />
                {:else}
                  <StringLine
                    timetable={sd.timetable}
                    window={sd.window}
                    editable={sd.editing && framed}
                    selected={sd.selectedTrip}
                    onselect={selectTrip}
                    onnudge={gestures.retime}
                  />
                {/if}
                <div class="row">
                  {#if sd.frequency}<HourGaps frequency={sd.frequency} route={sd.route} window={sd.window} />{/if}
                  <RunTimes timetable={sd.timetable} />
                </div>
                {#if sd.editing}
                  <GesturePanel
                    {framed}
                    stops={sd.timetable.stops}
                    selectedStop={sd.selectedStop}
                    trips={sd.timetable.trips}
                    selectedTrip={sd.selectedTrip}
                    window={sd.window}
                    onspan={gestures.span}
                    onrunning={gestures.running}
                    onretime={gestures.retime}
                    oninsertstop={gestures.insertStop}
                    onremovestop={gestures.removeStop}
                  />
                {/if}
              {:else}
                <Unavailable title="Timetable" reason="This direction runs nothing under the group." />
              {/if}
            </div>
            <aside class="side">
              <ChangeLedger
                comparison={sd.comparison}
                baselineName={planName(sd.compareKey)}
                canUndo={sd.canUndo}
                canRedo={sd.canRedo}
                onundo={undoChange}
                onredo={redoChange}
                onroute={openRoute}
              />
              {#if sd.failed.RouteMap}
                {@render failure("RouteMap", "Where it runs")}
              {:else}
                <div class="route-map"><FocusMap focus={sd.focus} label={`route ${sd.route}`} /></div>
              {/if}
            </aside>
          </div>
        {/if}
      {:else if sd.view === "proposal"}
        <h1>{planName(sd.activeKey)} <span>{sd.compareKey ? `against ${planName(sd.compareKey)}` : "no baseline"}</span></h1>
        {#if sd.failed.Comparison}
          {@render failure("Comparison", "Against the baseline")}
        {:else if sd.comparison}
          <ProposalScore comparison={sd.comparison} />
        {:else}
          <Unavailable title="Against the baseline" reason="Choose a baseline plan on the bar to compare against." />
        {/if}
        <div class="row">
          {#if sd.comparison}<Redline comparison={sd.comparison} onroute={openRoute} />{/if}
          {#if sd.failed.Readiness}
            {@render failure("Readiness", "Ready to publish?")}
          {:else}
            <Readiness issues={sd.issues} warnings={sd.warnings} {framed} exported={sd.exported} exportError={sd.failed.Export ?? ""} onexport={exportPlan} />
          {/if}
        </div>
      {/if}
    </main>
  </div>
</div>

<style>
  .shell { display: grid; grid-template-columns: 4.6rem minmax(0, 1fr); height: 100dvh; }
  .rail { min-height: 0; }
  .main { display: flex; flex-direction: column; min-height: 0; min-width: 0; background: color-mix(in srgb, var(--hover) 60%, var(--surface)); }
  .view { flex: 1; overflow-y: auto; padding: 1rem 1.2rem 2rem; display: flex; flex-direction: column; gap: 0.9rem; min-width: 0; }
  h1 { margin: 0; font-size: 1.2rem; }
  h1 span { font-size: 0.8rem; font-weight: 400; color: var(--muted); margin-left: 0.4rem; }
  .row { display: flex; flex-wrap: wrap; gap: 0.9rem; align-items: flex-start; }
  .row > :global(*) { flex: 1 1 24rem; min-width: 0; }
  .col { display: flex; flex-direction: column; gap: 0.6rem; }
  .split { display: flex; flex-wrap: wrap; margin: -1rem -1.2rem -2rem; flex: 1 1 0; min-height: 30rem; }
  .split > :global(aside) { flex: 0 1 22rem; min-width: 18rem; max-height: 100%; }
  .map-area { flex: 999 1 30rem; display: flex; flex-direction: column; gap: 0.5rem; padding: 0.6rem; min-width: 0; max-height: 100%; }
  .map { position: relative; flex: 1 1 0; min-height: 20rem; border-radius: 8px; overflow: hidden; border: 1px solid var(--line); }
  .map > :global(.coverage-map) { position: absolute; inset: 0; min-height: 0; }
  .picker { max-width: 28rem; }
  .workbench { display: flex; flex-wrap: wrap; gap: 0.9rem; align-items: flex-start; }
  .center { flex: 999 1 40rem; min-width: 0; display: flex; flex-direction: column; gap: 0.9rem; }
  .side { flex: 1 1 17rem; display: flex; flex-direction: column; gap: 0.9rem; min-width: 0; }
  .route-map { height: 18rem; border-radius: 8px; overflow: hidden; border: 1px solid var(--line); }
  .dirs { display: flex; align-items: center; gap: 0.8rem; flex-wrap: wrap; }
  .mode { margin-left: auto; font-size: 0.8rem; color: var(--muted); }
  @media (max-width: 720px) {
    .shell { grid-template-columns: 1fr; grid-template-rows: auto minmax(0, 1fr); }
  }
</style>
