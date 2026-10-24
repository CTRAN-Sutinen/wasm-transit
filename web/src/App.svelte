<script>
  import Loader from "./components/Loader.svelte";
  import MapView from "./components/MapView.svelte";
  import DepartureBoard from "./components/DepartureBoard.svelte";
  import BlocksPanel from "./components/BlocksPanel.svelte";
  import RtPanel from "./components/RtPanel.svelte";
  import TripPlanner from "./components/TripPlanner.svelte";
  import PlanOverlay from "./components/PlanOverlay.svelte";
  import DiversionPanel from "./components/DiversionPanel.svelte";
  import PlannerPanel from "./components/PlannerPanel.svelte";
  import { app } from "./state.svelte.js";
  import { loadRtFromUrl, loadVpFromUrl, saveSettings } from "./actions.svelte.js";

  // The map is the stage; the panel floats over it and can be hidden
  // for a full-map view. Persisted across refreshes.
  let panelOpen = $state(localStorage.getItem("transit:panel-open") !== "closed");

  const summary = $derived(app.gtfs ? app.gtfs.summary() : null);

  // Persona views: one workflow per tab, no clutter across them. The
  // rider plans trips and watches arrivals; the dispatcher runs the
  // operation (blocks, diversions, adherence); the planner authors
  // the network itself. The map and the feed underneath are shared.
  const personas = [
    { id: "rider", label: "Rider" },
    { id: "dispatcher", label: "Dispatcher" },
    { id: "planner", label: "Planner" },
  ];

  function setPersona(id) {
    app.persona = id;
    localStorage.setItem("transit:persona", id);
  }

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

<div class="shell">
  <MapView />

  <aside class="panel" class:hidden={!panelOpen}>
    <header class="panel-header">
      <h1>rustwasm transit</h1>
      <button onclick={togglePanel}>{panelOpen ? "Hide" : "Show"} panel</button>
    </header>

    <nav class="tabs" role="tablist" aria-label="View">
      {#each personas as p (p.id)}
        <button role="tab" class:active={app.persona === p.id} onclick={() => setPersona(p.id)}>{p.label}</button>
      {/each}
    </nav>

    <p class="muted">{app.status}</p>

    {#if summary}
      <p class="feed-line">
        {summary.stops} stops · {summary.routes} routes · {summary.trips} trips · {summary.stop_times} stop times
      </p>
    {/if}

    {#if app.persona === "rider"}
      <Loader />
      {#if app.gtfs}
        <DepartureBoard />
        <TripPlanner />
      {/if}
    {:else if app.persona === "dispatcher"}
      <Loader />
      {#if app.gtfs}
        <BlocksPanel />
        <DiversionPanel />
      {/if}
    {:else if app.persona === "planner"}
      <PlannerPanel />
    {/if}

    {#if app.rtOverview && app.persona !== "planner"}
      <RtPanel />
    {/if}
  </aside>

  {#if !panelOpen}
    <button class="panel-fab" onclick={togglePanel}>Show panel</button>
  {/if}

  <PlanOverlay />
</div>
