<script lang="ts">
  import DisplayAlerts from "../../components/DisplayAlerts.svelte";
  import DisplayGroup from "../../components/DisplayGroup.svelte";
  import DisplayHeader from "../../components/DisplayHeader.svelte";
  import DisplaySetup from "../../components/DisplaySetup.svelte";
  import { closeSetup, display, failure, label, loadFeedBytes, off, openSetup, pickStop, setLayout, status, typed } from "./host.svelte";
</script>

{#if !display.feed && !display.busy}
  <main>
    <label class="feed">
      <span role="status">{status() || "Load the agency's GTFS feed to begin"}</span>
      <input type="file" accept=".zip" aria-label="Agency GTFS zip" onchange={async (e) => {
        const f = e.currentTarget.files?.[0];
        if (f) await loadFeedBytes(new Uint8Array(await f.arrayBuffer()), f.name);
      }} />
    </label>
  </main>
{:else if display.setup}
  <main>
  <DisplaySetup
    layout={display.layout}
    found={display.found}
    limit={25}
    off={off("DisplaySetup")}
    current={display.stop}
    ontype={typed}
    onlayout={setLayout}
    onpick={pickStop}
    onclose={closeSetup}
  />
  {#if status()}<p class="status" data-testid="display-status">{status()}</p>{/if}
  </main>
{:else}
  <main class="screen {display.layout}" data-testid="display" data-layout={display.layout}>
    <DisplayHeader
      name={display.stop?.name ?? ""}
      stopId={display.stop?.id ?? ""}
      nowSecs={display.nowSecs}
      live={display.board?.live ?? false}
      onsetup={openSetup}
    />
    <div class="groups">
      {#if label("edits")}
        <p class="note" data-testid="display-stale">{label("edits")}</p>
      {/if}
      {#if display.board && (failure("DisplayGroup") || display.problem)}
        <p class="note" data-testid="display-note">
          {failure("DisplayGroup") ? `Departures could not be updated: ${failure("DisplayGroup")}` : display.problem}
        </p>
      {/if}
      {#if status()}
        <p class="status" data-testid="display-status">{status()}</p>
      {:else if off("DisplayGroup")}
        <p class="status">Departures are unavailable — the board capability could not be assembled.</p>
      {:else if failure("DisplayGroup") && !display.board}
        <p class="status">Departures could not be answered: {failure("DisplayGroup")}</p>
      {:else if display.board && display.board.groups.length === 0}
        <p class="status" data-testid="display-status">No routes serve this stop today.</p>
      {:else if display.board}
        {#each display.board.groups as group (group.route + "→" + group.destination)}
          <div class="slot">
            <DisplayGroup
              {group}
              variant={display.layout === "center" ? "row" : display.layout === "route" ? "hero" : "band"}
              bays={display.layout === "center"}
            />
          </div>
        {/each}
      {/if}
    </div>
    <div class="alerts">
      <DisplayAlerts alerts={display.board?.alerts ?? []} routes={display.routes} off={off("Live")} />
    </div>
  </main>
{/if}

<style>
  .screen { height: 100dvh; display: grid; grid-template-rows: auto minmax(0, 1fr) auto; overflow: hidden; }
  .groups { min-height: 0; overflow: auto; padding: 0.4rem 1rem; }
  .alerts { padding: 0.6rem 1rem; border-top: 1px solid var(--line); max-height: 28vh; overflow: auto; }
  .feed { display: grid; gap: 0.6rem; max-width: 40rem; margin: 2rem auto; padding: 0 1rem; color: var(--muted); }
  .note { margin: 0 0 0.4rem; color: var(--muted); font-size: clamp(0.8rem, 2vh, 1.2rem); }
  .status { padding: 1rem; color: var(--muted); font-size: clamp(1rem, 3vh, 2rem); }

  .center { grid-template-columns: minmax(0, 1fr) minmax(16rem, 28%); grid-template-rows: auto minmax(0, 1fr); }
  .center :global(.head) { grid-column: 1 / -1; }
  .center .alerts { border-top: 0; border-left: 1px solid var(--line); max-height: none; }
  /* The slot is the size container that the type scales to. */
  .route .groups { display: grid; grid-auto-rows: minmax(10rem, 1fr); gap: 1.5rem; padding: 1rem 2rem; }
  .routes .groups { display: grid; grid-auto-rows: minmax(8rem, 1fr); gap: 0.8rem; padding: 0.8rem 1rem; }
  .route .slot, .routes .slot { container-type: size; min-height: 0; }

  @media (max-width: 700px) {
    .center { grid-template-columns: minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr) auto; }
    .center .alerts { border-left: 0; border-top: 1px solid var(--line); max-height: 28vh; }
  }
</style>
