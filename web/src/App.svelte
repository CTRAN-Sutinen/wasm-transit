<script>
  import Loader from "./components/Loader.svelte";
  import MapView from "./components/MapView.svelte";
  import DepartureBoard from "./components/DepartureBoard.svelte";
  import BlocksPanel from "./components/BlocksPanel.svelte";
  import RtPanel from "./components/RtPanel.svelte";
  import TripPlanner from "./components/TripPlanner.svelte";
  import PlanOverlay from "./components/PlanOverlay.svelte";
  import DiversionPanel from "./components/DiversionPanel.svelte";
  import { app } from "./state.svelte.js";
  import { loadRtFromUrl, loadVpFromUrl, saveSettings } from "./actions.svelte.js";

  // The map is the stage; the panel floats over it and can be hidden
  // for a full-map view. Persisted across refreshes.
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

<div class="shell">
  <MapView />

  <aside class="panel" class:hidden={!panelOpen}>
    <header class="panel-header">
      <h1>rustwasm transit</h1>
      <button onclick={togglePanel}>{panelOpen ? "Hide" : "Show"} panel</button>
    </header>

    <p class="muted">{app.status}</p>

    <Loader />

    {#if summary}
    <section>
      <h2>Feed summary</h2>
      <table>
        <tbody>
          <tr><td>agencies</td><td>{summary.agencies}</td></tr>
          <tr><td>routes</td><td>{summary.routes}</td></tr>
          <tr><td>stops</td><td>{summary.stops}</td></tr>
          <tr><td>trips</td><td>{summary.trips}</td></tr>
          <tr><td>stop times</td><td>{summary.stop_times}</td></tr>
        </tbody>
      </table>
    </section>
    {/if}

    {#if app.gtfs}
      <DepartureBoard />
      <TripPlanner />
      <BlocksPanel />
      <DiversionPanel />
    {/if}
    {#if app.rtOverview}
      <RtPanel />
    {/if}
  </aside>

  {#if !panelOpen}
    <button class="panel-fab" onclick={togglePanel}>Show panel</button>
  {/if}

  <PlanOverlay />
</div>
