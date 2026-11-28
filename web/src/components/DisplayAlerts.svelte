<script lang="ts">
  import AlertList from "./AlertList.svelte";
  import type { LiveAlert } from "../types/LiveAlert";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";

  let {
    alerts,
    routes,
    off = false,
  }: {
    alerts: LiveAlert[];
    routes: DirectoryRoute[];
    off?: boolean;
  } = $props();
</script>

<section class="slot" data-testid="display-alerts">
  <h2>Service alerts</h2>
  {#if off}
    <p class="muted">
      Alerts are unavailable — live data could not be assembled.
    </p>
  {:else if alerts.length}
    <AlertList {alerts} {routes} />
  {:else}
    <p class="muted">No service alerts for this stop.</p>
  {/if}
</section>

<style>
  .slot {
    font-size: clamp(0.9rem, 2.2vh, 1.6rem);
    min-width: 0;
    overflow: auto;
  }
  h2 {
    margin: 0 0 0.3em;
    font-size: 0.75em;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }
  p {
    margin: 0;
  }
  .muted {
    color: var(--muted);
  }
</style>
