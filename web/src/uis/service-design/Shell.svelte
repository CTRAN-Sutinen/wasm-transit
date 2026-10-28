<script lang="ts">
  /// The service-design UI — one screen, drilled left to right:
  /// the plan bar (which plan, against which baseline, its place in
  /// the chain) over three columns — the service types (cost, what
  /// the plan changes against the baseline, coverage, fleet), the
  /// routes under the selected type with the map of the selected
  /// one beneath them, and the selected route's timetable (one
  /// direction under the type, marked against the baseline) with the
  /// changes a planner states against it. The timetable scrolls
  /// inside its column; it never widens the page.
  /// Every panel is a render component fed from the host's `wb`; this
  /// file only places them and routes their events to the host.
  import {
    wb, openPlan, setCompare, fork, loadFeedBytes, loadLayerBytes, selectType, selectRoute, selectDirection,
    selectStop, selectBlock, setCoverageParams, gestures,
  } from "./host.svelte";
  import PlanBar from "../../components/PlanBar.svelte";
  import TypeTable from "../../components/TypeTable.svelte";
  import CompareTable from "../../components/CompareTable.svelte";
  import CoverageTable from "../../components/CoverageTable.svelte";
  import BlocksPanel from "../../components/BlocksPanel.svelte";
  import RouteTable from "../../components/RouteTable.svelte";
  import DirectionTabs from "../../components/DirectionTabs.svelte";
  import TimetableGrid from "../../components/TimetableGrid.svelte";
  import TimetableChange from "../../components/TimetableChange.svelte";
  import FocusMap from "../../components/FocusMap.svelte";

  const bytesOf = async (f: File) => new Uint8Array(await f.arrayBuffer());
  const compareName = $derived(wb.plans.find((p) => p.key === wb.compareKey)?.name ?? "");
  const framed = $derived(!!wb.plans.find((p) => p.key === wb.activeKey)?.id);
  const mapLabel = $derived(wb.selectedBlock ? `block ${wb.selectedBlock}` : wb.selectedRoute ? `route ${wb.selectedRoute}` : "");
</script>

<div class="shell">
  <div class="bar">
    <PlanBar
      feed={wb.feed}
      plans={wb.plans}
      activeKey={wb.activeKey}
      compareKey={wb.compareKey}
      chain={wb.chain}
      issues={wb.issues}
      warnings={wb.warnings}
      status={wb.status}
      busy={wb.busy}
      onopen={openPlan}
      oncompare={setCompare}
      onfork={fork}
      onfeed={async (f) => loadFeedBytes(await bytesOf(f), f.name)}
    />
  </div>

  <aside class="col types">
    <TypeTable groups={wb.groups} baseline={wb.compareGroups} baselineName={compareName} selected={wb.selectedType} onselect={selectType} />
    {#if wb.comparison}
      <CompareTable comparison={wb.comparison} baselineName={compareName} selected={wb.selectedRoute} onselect={selectRoute} />
    {/if}
    <CoverageTable
      coverage={wb.coverage}
      layer={wb.layer}
      params={wb.coverageParams}
      selected={wb.selectedType}
      onparams={setCoverageParams}
      onlayer={async (f) => loadLayerBytes(await bytesOf(f))}
    />
    {#if wb.typeDetail}
      <details class="fleet" data-testid="fleet">
        <summary>Fleet — {wb.blocks.length} blocks</summary>
        <BlocksPanel
          blocks={wb.blocks}
          off={false}
          selected={wb.selectedBlock}
          chain={wb.blockChain}
          chainOff={false}
          date={wb.typeDetail.date}
          onselect={selectBlock}
        />
      </details>
    {/if}
  </aside>

  <nav class="col routes">
    <div class="list">
      {#if wb.typeDetail}
        <RouteTable
          routes={wb.typeDetail.routes}
          typeName={wb.typeDetail.name}
          date={wb.typeDetail.date}
          selected={wb.selectedRoute}
          onselect={selectRoute}
        />
      {/if}
    </div>
    <div class="map"><FocusMap focus={wb.focus} label={mapLabel} /></div>
  </nav>

  <main class="col route">
    {#if wb.routeDetail}
      <div class="head">
        <DirectionTabs directions={wb.routeDetail.directions} selected={wb.selectedDirection} onselect={selectDirection} />
      </div>
      <div class="table">
        {#if wb.timetable}
          <TimetableGrid timetable={wb.timetable} baselineName={compareName} selectedStop={wb.selectedStop} onstop={selectStop} />
        {:else}
          <p class="muted">No timetable for this direction under the selected type.</p>
        {/if}
      </div>
      <div class="changes">
        {#if wb.timetable}
          <TimetableChange
            {framed}
            stops={wb.timetable.stops}
            selectedStop={wb.selectedStop}
            onheadway={gestures.headway}
            onspan={gestures.span}
            onrunning={gestures.running}
            oninsertstop={gestures.insertStop}
            onremovestop={gestures.removeStop}
          />
        {/if}
      </div>
    {:else}
      <p class="muted">Pick a route to see its timetable by direction.</p>
    {/if}
  </main>
</div>

<style>
  .shell {
    display: grid;
    height: 100dvh;
    grid-template-rows: auto minmax(0, 1fr);
    grid-template-columns: minmax(260px, 22rem) minmax(280px, 24rem) minmax(0, 1fr);
    grid-template-areas: "bar bar bar" "types routes route";
  }
  .bar { grid-area: bar; }
  .col { overflow-y: auto; padding: 0.5rem 0.75rem; min-height: 0; min-width: 0; }
  .types { grid-area: types; display: flex; flex-direction: column; gap: 1rem; border-right: 1px solid var(--line); }
  .routes {
    grid-area: routes; border-right: 1px solid var(--line); overflow: hidden; padding: 0;
    display: grid; grid-template-rows: minmax(0, 1fr) minmax(240px, 45%);
  }
  .list { min-height: 0; overflow: hidden; padding: 0.5rem 0.75rem 0; }
  .map { position: relative; min-height: 0; border-top: 1px solid var(--line); }
  .route {
    grid-area: route; overflow: hidden; display: grid; gap: 0.5rem;
    grid-template-rows: auto minmax(0, 1fr) auto;
  }
  .table { min-height: 0; min-width: 0; overflow: hidden; }
  .changes { max-height: 40vh; overflow-y: auto; border-top: 1px solid var(--line); padding-top: 0.4rem; }
  .fleet summary { cursor: pointer; font-weight: 600; font-size: 0.95rem; }
  /* BlocksPanel titles itself; here the fleet summary already does. */
  .fleet :global(h2) { display: none; }
  .muted { color: var(--muted); font-size: 0.85rem; }

  /* A laptop screen narrows the sidebars; the timetable keeps the rest. */
  @media (max-width: 1300px) {
    .shell { grid-template-columns: minmax(240px, 20rem) minmax(250px, 20rem) minmax(0, 1fr); }
  }

  /* A narrow screen stacks the drill-down in the same order. */
  @media (max-width: 1150px) {
    .shell {
      height: auto;
      grid-template-columns: 1fr;
      grid-template-rows: auto;
      grid-template-areas: "bar" "types" "routes" "route";
    }
    :global(body) { overflow: auto; }
    .col { overflow: visible; border-right: 0; }
    .routes { grid-template-rows: auto 300px; }
    .list { overflow: visible; }
    .route { grid-template-rows: auto 60vh auto; }
    .changes { max-height: none; }
  }
</style>
