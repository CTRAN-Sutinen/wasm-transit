<script>
  import { app } from "../state.svelte.js";
  import { loadGtfsFromUrl, loadRtFromUrl, loadVpFromUrl, dropFile, clearCachedFeed } from "../actions.svelte.js";

  let dragActive = $state(false);
  let fileInput; // bind:this target — no need for reactivity

  async function handleFiles(files) {
    for (const file of files) await dropFile(file);
  }
</script>

<section>
  <h2>Load data</h2>

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
    ondrop={(e) => { e.preventDefault(); dragActive = false; handleFiles(e.dataTransfer.files); }}
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
      onchange={(e) => handleFiles(e.currentTarget.files)}
    />
  </div>

  <p>
    <label for="gtfs-url">GTFS static zip:</label><br>
    <input type="url" id="gtfs-url" size="60" bind:value={app.gtfsUrl}
           placeholder="https://agency.example/gtfs.zip">
    <button onclick={loadGtfsFromUrl}>Load feed</button>
  </p>
  <p>
    <label for="rt-url">GTFS-realtime protobuf:</label><br>
    <input type="url" id="rt-url" size="60" bind:value={app.rtUrl}
           placeholder="https://agency.example/trip-updates.pb">
    <button onclick={loadRtFromUrl}>Load</button>
    <label><input type="checkbox" bind:checked={app.rtPolling}> refresh every 30s</label>
  </p>
  <p>
    <label for="vp-url">Vehicle positions URL (many agencies serve it separately):</label><br>
    <input type="url" id="vp-url" size="60" bind:value={app.vpUrl}
           placeholder="https://agency.example/vehicle-positions.pb">
    <button onclick={loadVpFromUrl}>Load</button>
    <label><input type="checkbox" bind:checked={app.vpPolling}> refresh every 30s</label>
  </p>
  <p class="muted">URLs go through the local <code>/rt</code> proxy (CORS-free).</p>
</section>
