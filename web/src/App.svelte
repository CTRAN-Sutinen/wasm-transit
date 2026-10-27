<script lang="ts">
  import DataSources from "./components/DataSources.svelte";
  import MapView from "./components/MapView.svelte";
  import DepartureBoard from "./components/DepartureBoard.svelte";
  import BlocksPanel from "./components/BlocksPanel.svelte";
  import RtPanel from "./components/RtPanel.svelte";
  import TripPlanner from "./components/TripPlanner.svelte";
  import PlanOverlay from "./components/PlanOverlay.svelte";
  import DiversionPanel from "./components/DiversionPanel.svelte";
  import PlannerRail from "./components/PlannerRail.svelte";
  import PersonaTabs from "./components/PersonaTabs.svelte";
  import { app, directory } from "./state.svelte.js";
  import { loadRtFromUrl, loadVpFromUrl, saveSettings, refreshFeedLine, refreshBoard, refreshServices, refreshBlocks, refreshBlockChain, refreshDiversionView, selectStop, refreshNow, showTrip, serviceDate, planTrip } from "./actions";

  // The feed line (cap-gtfs) and the directory's prime: re-asked on
  // a new feed.
  $effect(() => {
    void app.feedEpoch;
    void refreshFeedLine();
  });

  // The board and the day's service keys are the bus's answers; the
  // host re-asks when what they depend on moves (the stop, time, the
  // effective day, the live document) and the components only draw.
  $effect(() => {
    void app.selectedStop;
    void app.clock;
    void app.feedEpoch;
    void app.live;
    refreshBoard();
  });
  $effect(() => {
    void app.clock;
    void app.feedEpoch;
    refreshServices();
    refreshBlocks();
  });
  $effect(() => {
    void app.selectedBlock;
    void app.clock;
    void app.feedEpoch;
    refreshBlockChain();
  });
  // The diversion draft's view: re-asked on every change to what the
  // dispatcher entered (a deep read) and on a new feed.
  $effect(() => {
    void app.feedEpoch;
    void JSON.stringify(app.diversionDraft);
    refreshDiversionView();
  });
  const today = $derived.by(() => {
    void app.clock; // the service day can roll over
    return serviceDate();
  });
  const stopOptions = $derived(
    directory.stops.slice(0, 2000).map((s) => ({ value: s.stop_id, label: s.name, sub: s.stop_id })),
  );

  // One shell, one persistent map. The sidebar carries the persona
  // (tabs + workflow); the map never remounts on a persona switch —
  // it is the same canvas, drawing different state. Collapsible when
  // the map is the whole point (rider following a trip, dispatcher
  // mid-trace).
  let panelOpen = $state(localStorage.getItem("transit:panel-open") !== "closed");

  function togglePanel() {
    panelOpen = !panelOpen;
    localStorage.setItem("transit:panel-open", panelOpen ? "open" : "closed");
  }

  // Persist settings whenever they change (urls, poll toggles).
  $effect(() => {
    void app.gtfsUrl;
    void app.rtUrl;
    void app.vpUrl;
    void app.rtPolling;
    void app.vpPolling;
    saveSettings();
  });

  // Polling: intervals live and die with the checkbox state (restored
  // toggles from a previous session pick up automatically).
  $effect(() => {
    if (app.rtPolling) {
      const timer = setInterval(loadRtFromUrl, 30_000);
      loadRtFromUrl();
      return () => clearInterval(timer);
    }
  });
  $effect(() => {
    if (app.vpPolling) {
      const timer = setInterval(loadVpFromUrl, 30_000);
      loadVpFromUrl();
      return () => clearInterval(timer);
    }
  });
</script>

<div class="app-shell">
  <aside class="sidebar" class:hidden={!panelOpen}>
    <header class="panel-header">
      <h1>rustwasm transit</h1>
      <button onclick={togglePanel}>Hide panel</button>
    </header>

    <PersonaTabs />

    <p class="muted">{app.status}</p>

    {#if app.feedSummary}
      {@const summary = app.feedSummary}
      {@const feedInfo = app.feedInfo}
      <p class="feed-line">
        {summary.stops} stops · {summary.routes} routes · {summary.trips} trips · {summary.stop_times} stop times
        {#if feedInfo && (feedInfo.feed_publisher_name || feedInfo.feed_version)}
          · {feedInfo.feed_publisher_name ?? ""}{#if feedInfo.feed_version} {feedInfo.feed_version}{/if}
        {/if}
      </p>
    {/if}

    {#if app.persona === "planner"}
      <PlannerRail />
    {:else if app.feedLoaded}
      {#if app.persona === "rider"}
        <DepartureBoard
          stop={app.selectedStop}
          {stopOptions}
          board={app.board}
          off={app.boardOff}
          services={app.activeServices}
          selected={app.selectedTrip}
          onstop={selectStop}
          onrefresh={refreshNow}
          onselect={showTrip}
        />
        <TripPlanner {stopOptions} notice={app.planNotice} onplan={planTrip} />
      {:else}
        <BlocksPanel
          blocks={app.blocks}
          off={app.blocksOff}
          selected={app.selectedBlock}
          chain={app.blockChain}
          chainOff={app.chainOff}
          date={today}
          onselect={(id) => (app.selectedBlock = id)}
        />
        <DiversionPanel
          routes={directory.routes}
          rows={app.diversionRows}
          reports={app.editReports}
          view={app.diversionView}
        />
      {/if}
    {/if}

    {#if app.persona !== "planner" && (app.rtOverview || app.live)}
      <RtPanel summary={app.live?.summary ?? null} alerts={app.rtOverview?.alerts ?? 0} />
    {/if}
  </aside>

  <div class="map-side" class:hidden={app.persona === "planner" && !app.plannerMapUp}>
    <MapView />

    <PlanOverlay />
  </div>
</div>

{#if !panelOpen}
  <button class="panel-fab" onclick={togglePanel}>Show panel</button>
{/if}

<!-- App config (feed + street network), shared by every persona:
     fixed-position, so it floats over the whole shell. -->
<DataSources />
