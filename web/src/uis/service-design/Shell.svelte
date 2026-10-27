<script lang="ts">
  /// The service-design UI — one screen, drilled left to right:
  /// the plan bar (which plan, against which baseline, its place in
  /// the chain) over three columns — the service types (cost,
  /// coverage, fleet), the routes under the selected type, and the
  /// selected route (where it runs, its trips by service type, the change).
  /// Every panel is a render component fed from the host's `wb`; this
  /// file only places them and routes their events to the host.
  import { wb, openPlan, setCompare, fork, loadFeedBytes, loadLayerBytes, selectType, selectRoute, selectSchedule, selectBlock, setCoverageParams, change } from "./host.svelte";
  import PlanBar from "../../components/PlanBar.svelte";
  import TypeTable from "../../components/TypeTable.svelte";
  import CoverageTable from "../../components/CoverageTable.svelte";
  import BlocksPanel from "../../components/BlocksPanel.svelte";
  import RouteTable from "../../components/RouteTable.svelte";
  import RouteCard from "../../components/RouteCard.svelte";
  import ScheduleChange from "../../components/ScheduleChange.svelte";
  import FocusMap from "../../components/FocusMap.svelte";

  const bytesOf = async (f: File) => new Uint8Array(await f.arrayBuffer());
  const compareName = $derived(wb.plans.find((p) => p.key === wb.compareKey)?.name ?? "");
  const framed = $derived(!!wb.plans.find((p) => p.key === wb.activeKey)?.id);
  const mapLabel = $derived(
    wb.selectedBlock ? `block ${wb.selectedBlock}` : wb.selectedSchedule ? wb.selectedSchedule : wb.selectedRoute ? `route ${wb.selectedRoute}` : "",
  );
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
    <CoverageTable
      coverage={wb.coverage}
      layer={wb.layer}
      params={wb.coverageParams}
      selected={wb.selectedType}
      onparams={setCoverageParams}
      onlayer={async (f) => loadLayerBytes(await bytesOf(f))}
    />
    {#if wb.typeDetail}
      <section class="fleet" data-testid="fleet">
        <h3>Fleet — {wb.blocks.length} blocks</h3>
        <BlocksPanel
          blocks={wb.blocks}
          off={false}
          selected={wb.selectedBlock}
          chain={wb.blockChain}
          chainOff={false}
          date={wb.typeDetail.date}
          onselect={selectBlock}
        />
      </section>
    {/if}
  </aside>

  <main class="col routes">
    {#if wb.typeDetail}
      <RouteTable
        routes={wb.typeDetail.routes}
        typeName={wb.typeDetail.name}
        date={wb.typeDetail.date}
        selected={wb.selectedRoute}
        onselect={selectRoute}
      />
    {/if}
  </main>

  <section class="col detail">
    <div class="map"><FocusMap focus={wb.focus} label={mapLabel} /></div>
    <div class="cards">
      {#if wb.routeDetail}
        <RouteCard
          routeId={wb.selectedRoute}
          detail={wb.routeDetail}
          serviceType={wb.selectedType}
          selectedSchedule={wb.selectedSchedule}
          onschedule={selectSchedule}
        />
      {:else}
        <p class="muted">Pick a route to see where it runs and its trips by service type.</p>
      {/if}
      {#if wb.selectedSchedule}
        <ScheduleChange
          scheduleId={wb.selectedSchedule}
          starts={wb.starts}
          {framed}
          onadd={() => change([{ edit: "add_start", pattern_trips: wb.selectedSchedule }])}
          onremove={(index) => change([{ edit: "remove_start", pattern_trips: wb.selectedSchedule, index }])}
        />
      {/if}
    </div>
  </section>
</div>

<style>
  .shell {
    display: grid;
    height: 100dvh;
    grid-template-rows: auto minmax(0, 1fr);
    grid-template-columns: minmax(300px, 24rem) minmax(320px, 1fr) minmax(360px, 1.15fr);
    grid-template-areas: "bar bar bar" "types routes detail";
  }
  .bar { grid-area: bar; }
  .col { overflow-y: auto; padding: 0.5rem 0.75rem; min-height: 0; min-width: 0; }
  .types { grid-area: types; display: flex; flex-direction: column; gap: 1rem; border-right: 1px solid var(--line); }
  .routes { grid-area: routes; border-right: 1px solid var(--line); overflow: hidden; padding-bottom: 0; }
  .detail { grid-area: detail; display: grid; grid-template-rows: minmax(220px, 45%) minmax(0, 1fr); padding: 0; overflow: hidden; }
  .map { position: relative; }
  .cards { overflow-y: auto; padding: 0.5rem 0.75rem; display: flex; flex-direction: column; gap: 0.75rem; }
  .fleet h3 { margin: 0 0 0.25rem; font-size: 0.95rem; }
  /* BlocksPanel titles itself; here the fleet heading already does. */
  .fleet :global(h2) { display: none; }
  .muted { color: var(--muted); font-size: 0.85rem; }

  /* A narrow screen stacks the drill-down in the same order. */
  @media (max-width: 900px) {
    .shell {
      height: auto;
      grid-template-columns: 1fr;
      grid-template-rows: auto;
      grid-template-areas: "bar" "types" "routes" "detail";
    }
    :global(body) { overflow: auto; }
    .col { overflow: visible; border-right: 0; }
    .detail { grid-template-rows: 300px auto; }
  }
</style>
