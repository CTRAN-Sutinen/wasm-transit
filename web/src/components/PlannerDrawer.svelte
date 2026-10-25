<script lang="ts">
  /// The planner's focused element editor: one entity at a time, over
  /// the map's right edge. Selection frames the element on the map
  /// (MapView draws app.plannerFocus).
  import { app } from "../state.svelte";
  import { clearSelection } from "../planner";
  import StopForm from "./StopForm.svelte";
  import RouteForm from "./RouteForm.svelte";
  import PatternForm from "./PatternForm.svelte";
  import CalendarForm from "./CalendarForm.svelte";
  import ServiceForm from "./ServiceForm.svelte";
  import BlockForm from "./BlockForm.svelte";

  const selection = $derived(app.plannerSelection);

  const drawerTitle: Record<string, string> = { stop: "Stop", route: "Route", pattern: "Pattern", service: "Service", calendar: "Calendar", block: "Block" };
</script>

{#if selection}
  <aside class="planner-drawer">
    <header class="drawer-header">
      <h2>{drawerTitle[selection.kind]} <code>{selection.id}</code></h2>
      <button aria-label="Close editor" onclick={clearSelection}>Close</button>
    </header>
    {#if selection.kind === "stop"}<StopForm />
    {:else if selection.kind === "route"}<RouteForm />
    {:else if selection.kind === "pattern"}<PatternForm />
    {:else if selection.kind === "service"}<ServiceForm />
    {:else if selection.kind === "calendar"}<CalendarForm />
    {:else if selection.kind === "block"}<BlockForm />
    {/if}
  </aside>
{/if}
