<script>
  /// Persona tabs, shared by the floating panel and the planner
  /// workspace: one workflow per tab, the map and feed shared.
  import { app } from "../state.svelte.js";
  import { clearSelection } from "../planner.svelte.js";

  const personas = [
    { id: "rider", label: "Rider" },
    { id: "dispatcher", label: "Dispatcher" },
    { id: "planner", label: "Planner" },
  ];

  function setPersona(id) {
    // Leaving the planner workspace clears its map focus.
    if (app.persona === "planner" && id !== "planner") clearSelection();
    app.persona = id;
    localStorage.setItem("transit:persona", id);
  }
</script>

<nav class="tabs" role="tablist" aria-label="View">
  {#each personas as p (p.id)}
    <button role="tab" class:active={app.persona === p.id} onclick={() => setPersona(p.id)}>{p.label}</button>
  {/each}
</nav>
