<script lang="ts">
  /// The service-design UI — one screen, drilled left to right:
  /// the plan bar (which plan, against which baseline, its place in
  /// the chain) over three columns — the service types (cost, what
  /// the plan changes against the baseline, coverage, fleet), the
  /// routes under the selected type, and the selected route's
  /// timetable (one direction under the type, marked against the
  /// baseline) with the changes a planner states against it.
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
    <TypeTable types={wb.types} baseline={wb.compareTypes} baselineName={compareName} selected={wb.selectedType} onselect={selectType} />
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
    {#if wb.typeDetail}
      <RouteTable
        routes={wb.typeDetail.routes}
        typeName={wb.typeDetail.name}
        date={wb.typeDetail.date}
        selected={wb.selectedRoute}
        onselect={selectRoute}
      />
    {/if}
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
      <div class="foot">
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
        <div class="map"><FocusMap focus={wb.focus} label={mapLabel} /></div>
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
    grid-template-columns: minmax(280px, 22rem) minmax(300px, 24rem) minmax(420px, 1fr);
    grid-template-areas: "bar bar bar" "types routes route";
  }
  .bar { grid-area: bar; }
  .col { overflow-y: auto; padding: 0.5rem 0.75rem; min-height: 0; min-width: 0; }
  .types { grid-area: types; display: flex; flex-direction: column; gap: 1rem; border-right: 1px solid var(--line); }
  .routes { grid-area: routes; border-right: 1px solid var(--line); overflow: hidden; padding-bottom: 0; }
  .route {
    grid-area: route; overflow: hidden; display: grid; gap: 0.5rem;
    grid-template-rows: auto minmax(0, 1fr) minmax(180px, 34%);
  }
  .table { min-height: 0; }
  .foot { display: grid; grid-template-columns: minmax(0, 1.3fr) minmax(200px, 1fr); gap: 0.75rem; min-height: 0; }
  .changes { overflow-y: auto; }
  .map { position: relative; min-height: 180px; }
  .fleet summary { cursor: pointer; font-weight: 600; font-size: 0.95rem; }
  /* BlocksPanel titles itself; here the fleet summary already does. */
  .fleet :global(h2) { display: none; }
  .muted { color: var(--muted); font-size: 0.85rem; }

  /* A narrow screen stacks the drill-down in the same order. */
  @media (max-width: 1000px) {
    .shell {
      height: auto;
      grid-template-columns: 1fr;
      grid-template-rows: auto;
      grid-template-areas: "bar" "types" "routes" "route";
    }
    :global(body) { overflow: auto; }
    .col { overflow: visible; border-right: 0; }
    .route { grid-template-rows: auto 60vh auto; }
    .foot { grid-template-columns: 1fr; }
    .map { height: 260px; }
  }
</style>
