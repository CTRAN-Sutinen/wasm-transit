<script lang="ts">
  import AlertList from "./AlertList.svelte";
  import DepartureRows from "./DepartureRows.svelte";
  import RouteStopSearch from "./RouteStopSearch.svelte";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import type { DirectoryStop } from "../types/cap_directory/DirectoryStop";
  import type { EntityRow } from "../types/EntityRow";
  import type { StopBoard } from "../types/StopBoard";

  let {
    stop,
    board,
    routes,
    canDownload,
    off,
    query,
    found,
    oninput,
    onstop,
    ondownload,
  }: {
    stop: { id: string; name: string } | null;
    board: StopBoard | null;
    routes: DirectoryRoute[];
    canDownload: boolean;
    off: string;
    query: string;
    found: { routes: EntityRow[]; stops: DirectoryStop[] } | null;
    oninput: (text: string) => void;
    onstop: (stop: { id: string; name: string } | null) => void;
    ondownload: () => void;
  } = $props();
</script>

<section class="preview" data-testid="alert-preview">
  <header>
    <h2>Preview</h2>
    <button
      type="button"
      disabled={!canDownload}
      onclick={ondownload}
      data-testid="download-pb"
    >
      Download .pb
    </button>
  </header>
  <RouteStopSearch
    {query}
    {found}
    pick={stop ? { kind: "stop", id: stop.id, label: stop.name } : null}
    {oninput}
    onpick={(p) => onstop(p ? { id: p.id, name: p.label } : null)}
  />
  {#if off}
    <p class="muted">{off}</p>
  {:else if !stop}
    <p class="muted">Pick a stop</p>
  {:else if board}
    <div class="board" data-testid="preview-board">
      <strong class="name">{stop.name}</strong>
      {#if board.alerts.length}<AlertList
          alerts={board.alerts}
          {routes}
        />{:else}<p class="muted">No alert here now</p>{/if}
      {#if board.rows.length}<DepartureRows
          rows={board.rows}
          {routes}
          pooled={board.pooled}
          compact
        />{/if}
    </div>
  {/if}
</section>

<style>
  .preview {
    display: grid;
    gap: 0.6rem;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h2 {
    font-size: 0.95rem;
    margin: 0;
  }
  button {
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    border-radius: 6px;
    padding: 0.3rem 0.7rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.85rem;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .board {
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 0.7rem;
    display: grid;
    gap: 0.3rem;
  }
  .name {
    font-size: 1rem;
  }
</style>
