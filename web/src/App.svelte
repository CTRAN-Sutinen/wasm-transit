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
  import PlannerDrawer from "./components/PlannerDrawer.svelte";
  import PersonaTabs from "./components/PersonaTabs.svelte";
  import { app } from "./state.svelte.js";
  import { loadRtFromUrl, loadVpFromUrl, saveSettings } from "./actions";

  // One shell, one persistent map. The sidebar carries the persona
  // (tabs + workflow); the map never remounts on a persona switch —
  // it is the same canvas, drawing different state. Collapsible when
  // the map is the whole point (rider following a trip, dispatcher
  // mid-trace).
  let panelOpen = $state(localStorage.getItem("transit:panel-open") !== "closed");

  const summary = $derived(app.gtfs ? app.gtfs.summary() : null);

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

    {#if summary}
      <p class="feed-line">
        {summary.stops} stops · {summary.routes} routes · {summary.trips} trips · {summary.stop_times} stop times
      </p>
    {/if}

    {#if app.persona === "planner"}
      <PlannerRail />
    {:else if app.gtfs}
      {#if app.persona === "rider"}
        <DepartureBoard />
        <TripPlanner />
      {:else}
        <BlocksPanel />
        <DiversionPanel />
      {/if}
    {/if}

    {#if app.persona !== "planner" && app.rtOverview}
      <RtPanel />
    {/if}
  </aside>

  <div class="map-side">
    <MapView />

    {#if app.persona === "planner"}
      <PlannerDrawer />
    {/if}

    <PlanOverlay />
  </div>
</div>

{#if !panelOpen}
  <button class="panel-fab" onclick={togglePanel}>Show panel</button>
{/if}

<!-- App config (feed + street network), shared by every persona:
     fixed-position, so it floats over the whole shell. -->
<DataSources />
