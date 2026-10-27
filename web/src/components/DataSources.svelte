<script lang="ts">
  import { app, getStreets } from "../state.svelte";
  import {
    dropFile, loadGtfsFromUrl, clearCachedFeed,
    importStreetsFile, loadStreetsFromUrl, clearStreets,
    importPopulationFile, loadPopulationFromUrl, clearPopulation,
    loadRtFromUrl, loadVpFromUrl,
  } from "../actions";
  import { refreshCoverage } from "../planner";
  import CapabilityDemo from "./CapabilityDemo.svelte";

  // App config, not workflow: loading a feed and importing reference
  // layers happen once. A MODAL, not a floating panel — it never
  // fights the persona rails for pixels (one laptop screen). Opens
  // by itself when nothing is loaded yet; afterwards on demand.
  let open = $state(!app.feedLoaded && !app.cachedFeed);
  let dragActive = $state(false);
  let fileInput: HTMLInputElement; // bind:this target
  let streetsInput: HTMLInputElement; // bind:this target
  let popInput: HTMLInputElement; // bind:this target

  // A feed load or IndexedDB restore closes the modal — loading is
  // one-time setup, not part of the daily workflow.
  $effect(() => {
    if (app.feedLoaded) open = false;
  });

  async function handleFiles(files: FileList) {
    for (const file of files) await dropFile(file);
  }

  async function onStreetsFile(ev: Event): Promise<void> {
    const input = ev.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (file) await importStreetsFile(file);
    input.value = ""; // same file can be re-imported after an edit
  }

  async function onPopFile(ev: Event): Promise<void> {
    const input = ev.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (file) {
      await importPopulationFile(file);
      refreshCoverage();
    }
    input.value = "";
  }

  async function onPopUrl(): Promise<void> {
    await loadPopulationFromUrl();
    refreshCoverage();
  }

  function clearPop(): void {
    clearPopulation();
    refreshCoverage();
  }
</script>

{#if open}
  <div class="ds-backdrop" onclick={() => (open = false)}></div>
{/if}
<button class="ds-fab" onclick={() => (open = !open)}>Data sources</button>

<aside class="ds-panel" class:hidden={!open} id="data-sources" role="dialog"
       aria-modal={open ? "true" : "false"} aria-label="Data sources">
  <header class="panel-header">
    <h2>Data sources</h2>
    <button class="link" onclick={() => (open = false)}>Close</button>
  </header>

  <h3>GTFS static feed</h3>
  {#if app.cachedFeed}
    <p class="muted">
      Cached feed: {app.cachedFeed} —
      <button class="link" onclick={clearCachedFeed}>clear</button>
    </p>
  {/if}
  <div
    class="dropzone"
    class:active={dragActive}
    role="button"
    tabindex="0"
    ondragover={(e) => { e.preventDefault(); dragActive = true; }}
    ondragleave={() => (dragActive = false)}
    ondrop={(e) => { e.preventDefault(); dragActive = false; if (e.dataTransfer) handleFiles(e.dataTransfer.files); }}
    onclick={() => fileInput.click()}
    onkeydown={(e) => e.key === "Enter" && fileInput.click()}
  >
    <p><strong>Drop files here</strong></p>
    <p class="muted">
      a GTFS static <code>.zip</code> — and optionally a GTFS-rt
      <code>.pb</code> / <code>.protobuf</code> file
    </p>
    <input
      type="file"
      accept=".zip,.pb,.protobuf"
      multiple
      hidden
      bind:this={fileInput}
      onchange={(e) => { if (e.currentTarget.files) handleFiles(e.currentTarget.files); }}
    />
  </div>
  <p>
    <label for="gtfs-url">GTFS static zip:</label><br>
    <input type="url" id="gtfs-url" size="45" bind:value={app.gtfsUrl}
           placeholder="https://agency.example/gtfs.zip">
    <button onclick={loadGtfsFromUrl}>Load feed</button>
  </p>

  <h3>Street network</h3>
  <p class="muted">For detour drawing: clicks snap to street geometry and stops.</p>
  <p>
    <input
      type="file"
      id="streets-file"
      accept=".geojson,.json,application/geo+json"
      hidden
      bind:this={streetsInput}
      onchange={onStreetsFile}
    >
    <button onclick={() => streetsInput.click()}>Import street network (GeoJSON)</button>
    {#if getStreets()}
      <button class="link" onclick={clearStreets}>clear</button>
    {/if}
  </p>
  <p>
    <label for="streets-url">Street network URL:</label><br>
    <input type="url" id="streets-url" size="45" bind:value={app.streetsUrl}
           placeholder="https://portal.example/…/streets.geojson">
    <button onclick={loadStreetsFromUrl}>Import</button>
  </p>
  {#if getStreets()}
    <p>
      <span class="chip svc" title="Detour clicks snap to street segments and stops">
        {getStreets()?.segmentCount} street segments — snapping on
      </span>
      <span class="muted">not persisted — re-imported per session</span>
    </p>
  {/if}

  <h3>Population layer</h3>
  <p class="muted">
    Census block centroids (GeoJSON points with a population property)
    drive the planner's coverage analysis. The population field is
    auto-detected (POP, POPULATION, …).
  </p>
  <p>
    <input
      type="file"
      id="population-file"
      accept=".geojson,.json,application/geo+json"
      hidden
      bind:this={popInput}
      onchange={onPopFile}
    >
    <button onclick={() => popInput.click()}>Import population layer (GeoJSON)</button>
    {#if app.popLayer}
      <button class="link" onclick={clearPop}>clear</button>
    {/if}
  </p>
  <p>
    <label for="population-url">Population layer URL:</label><br>
    <input type="url" id="population-url" size="45" bind:value={app.popUrl}
           placeholder="https://portal.example/…/blocks.geojson">
    <button onclick={onPopUrl}>Import layer</button>
  </p>
  {#if app.popLayer}
    <p>
      <span class="chip svc">
        {app.popLayer.points.toLocaleString()} points ·
        {Math.round(app.popLayer.total_pop).toLocaleString()} people ({app.popLayer.pop_field})
      </span>
      <span class="muted">not persisted — re-imported per session</span>
    </p>
  {/if}

  <h3>Realtime feeds</h3>
  <p>
    <label for="rt-url">GTFS-realtime protobuf:</label><br>
    <input type="url" id="rt-url" size="45" bind:value={app.rtUrl}
           placeholder="https://agency.example/trip-updates.pb">
    <button onclick={loadRtFromUrl}>Load</button>
    <label><input type="checkbox" bind:checked={app.rtPolling}> refresh every 30s</label>
  </p>
  <p>
    <label for="vp-url">Vehicle positions URL (many agencies serve it separately):</label><br>
    <input type="url" id="vp-url" size="45" bind:value={app.vpUrl}
           placeholder="https://agency.example/vehicle-positions.pb">
    <button onclick={loadVpFromUrl}>Load</button>
    <label><input type="checkbox" bind:checked={app.vpPolling}> refresh every 30s</label>
  </p>
  <p class="muted">All URLs go through the local <code>/rt</code> proxy (CORS-free).</p>

  <CapabilityDemo />
</aside>
