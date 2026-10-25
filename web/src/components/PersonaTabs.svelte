<script lang="ts">
  /// Persona tabs, shared by the floating panel and the planner
  /// workspace: one workflow per tab, the map and feed shared.
  import { app } from "../state.svelte";
  import { clearSelection } from "../planner";

  const personas = [
    { id: "rider", label: "Rider" },
    { id: "dispatcher", label: "Dispatcher" },
    { id: "planner", label: "Planner" },
  ];

  function setPersona(id: "rider" | "dispatcher" | "planner") {
    // Leaving the planner workspace clears its map focus; leaving
    // the dispatcher mid-trace ends the trace (the map persists, the
    // modes do not).
    if (app.persona === "planner" && id !== "planner") clearSelection();
    if (app.persona === "dispatcher" && id !== "dispatcher") app.drawingDetour = false;
    app.persona = id;
    localStorage.setItem("transit:persona", id);
  }
</script>

<div class="tabs" role="tablist" aria-label="View">
  {#each personas as p (p.id)}
    <button role="tab" class:active={app.persona === p.id} onclick={() => setPersona(p.id as "rider" | "dispatcher" | "planner")}>{p.label}</button>
  {/each}
</div>
