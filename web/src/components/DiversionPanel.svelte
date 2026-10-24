<script lang="ts">
  import { app, getEditDocument, getStreets } from "../state.svelte";
  import {
    startDiversion, cancelDiversion, setDiversionAnchor, toggleSkipped,
    applyDiversion, toggleDiversion, toggleDrawing, clearDetour, syncEdits,
    importStreetsFile,
  } from "../actions";

  const routes = $derived(app.gtfs ? app.gtfs.routes() : []);
  const draft = $derived(app.diversionDraft);

  // The document: transit-sync EditRecords (id, diversion, status,
  // rev, updated_at). Display is derived here; truth lives in the doc.
  const records = $derived(getEditDocument());

  const routeName = $derived.by(() => {
    const m = new Map();
    for (const r of routes) m.set(r.route_id, r.name);
    return m;
  });
  const stopName = $derived.by(() => {
    const m = new Map();
    if (app.gtfs) for (const s of app.gtfs.stops()) m.set(s.stop_id, s.name);
    return m;
  });

  const reportOf = (id: string) => app.editReports.find((r) => r.id === id);

  // The pattern the anchors are picked from: the route's longest trip.
  const sequence = $derived.by(() => {
    if (!draft?.routeId || !app.gtfs) return [];
    const dir = draft.directionId === "" ? null : Number(draft.directionId);
    return app.gtfs.route_stop_sequence(draft.routeId, dir);
  });

  // Positions of the anchors within that pattern.
  const firstPos = $derived(sequence.findIndex((s) => s.stop_id === draft?.firstKept));
  const lastPos = $derived(sequence.findIndex((s) => s.stop_id === draft?.lastKept));

  // Stops strictly between the anchors: the skippable interior.
  const interior = $derived.by(() => {
    if (firstPos < 0 || lastPos <= firstPos) return [];
    return sequence.slice(firstPos + 1, lastPos);
  });

  // Interior stops are skipped by default (a detour usually bypasses
  // them); when anchors change, stale skips are pruned.
  $effect(() => {
    const d = draft;
    if (!d) return;
    const ids = interior.map((s) => s.stop_id);
    if (d.firstKept && d.lastKept && ids.length && d.skipped.size === 0) {
      for (const id of ids) d.skipped.add(id);
    }
    for (const id of [...d.skipped]) {
      if (!ids.includes(id)) d.skipped.delete(id);
    }
  });

  function isInterior(stopId: string) {
    return interior.some((s) => s.stop_id === stopId);
  }

  async function onStreetsFile(ev: Event): Promise<void> {
    const input = ev.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (file) await importStreetsFile(file);
    input.value = ""; // same file can be re-imported after an edit
  }

  let streetsInput = $state<HTMLInputElement | null>(null); // bind:this target
</script>

{#if app.gtfs}
<section>
  <h2>Diversions</h2>
  <p class="muted">Operator edits: reroute around a closure. Boards, blocks and plans re-derive locally.</p>

  {#if records.length}
    <ul class="diversion-list">
      {#each records as d (d.id)}
        {@const rep = reportOf(d.id)}
        {@const retired = d.status === "retired"}
        <li class:inactive={retired}>
          <label class="chip diversion-chip">
            <input
              type="checkbox"
              checked={!retired}
              onchange={() => toggleDiversion(d.id)}
              title={retired ? "Retired (tombstone) — check to restore" : "In force — uncheck to retire"}
            >
            <strong>{routeName.get(d.diversion.route_id) ?? d.diversion.route_id}</strong>
            {stopName.get(d.diversion.first_kept_stop) ?? d.diversion.first_kept_stop}
            → {stopName.get(d.diversion.last_kept_stop) ?? d.diversion.last_kept_stop}
            {#if d.diversion.skipped_stops.length}
              <span class="chip bad" title="stops no longer served">
                skips {d.diversion.skipped_stops.map((s) => stopName.get(s) ?? s).join(", ")}
              </span>
            {/if}
            {#if d.diversion.added_secs}+{Math.round(d.diversion.added_secs / 60)} min{/if}
            {#if d.diversion.detour?.length}<span class="chip svc">detour drawn</span>{/if}
            <span class="muted">
              {d.diversion.start_date} → {d.diversion.end_date}
              · {rep?.affected_trips ?? "?"} trips · {d.status}
            </span>
            {#if rep && !rep.valid}
              <span class="chip bad" title={rep.error}>invalid — {rep.error}</span>
            {/if}
          </label>
        </li>
      {/each}
    </ul>
    <p class="muted">Unchecking retires an edit (it stays as a sync tombstone); checking restores it.</p>
  {/if}

  {#if draft}
    <div class="diversion-editor">
      <p>
        <label for="div-route">Route</label>
        <select id="div-route" bind:value={draft.routeId}>
          <option value="" disabled>Pick a route…</option>
          {#each routes as r (r.route_id)}
            <option value={r.route_id}>{r.name} ({r.route_type})</option>
          {/each}
        </select>
        <label for="div-dir">Direction</label>
        <select id="div-dir" bind:value={draft.directionId}>
          <option value="">any</option>
          <option value="0">0</option>
          <option value="1">1</option>
        </select>
      </p>

      {#if sequence.length}
        {#if !draft.firstKept}
          <p class="muted">Click the first stop the diversion keeps:</p>
        {:else if !draft.lastKept}
          <p class="muted">Now the last kept stop (on the list or the map):</p>
        {:else}
          <p class="muted">Anchors set. Interior stops can be un-skipped if the detour still serves them.</p>
        {/if}
        <ol class="stop-seq">
          {#each sequence as s, i (s.stop_id)}
            <li
              class:anchor-first={i === firstPos}
              class:anchor-last={i === lastPos}
              class:skipped={draft.skipped.has(s.stop_id)}
            >
              <button class="link" onclick={() => setDiversionAnchor(s.stop_id)}>{s.name}</button>
              {#if isInterior(s.stop_id)}
                <label class="chip">
                  <input
                    type="checkbox"
                    checked={draft.skipped.has(s.stop_id)}
                    onchange={() => toggleSkipped(s.stop_id)}
                  >
                  skip
                </label>
              {/if}
            </li>
          {/each}
        </ol>
      {/if}

      <p>
        <label for="div-added">Added time (min)</label>
        <input type="number" id="div-added" min="0" step="1" bind:value={draft.addedMins}>
        <label for="div-start">From</label>
        <input type="date" id="div-start" bind:value={draft.start}>
        <label for="div-end">To</label>
        <input type="date" id="div-end" bind:value={draft.end}>
      </p>

      <p>
        <button class:primary={app.drawingDetour} onclick={toggleDrawing}>
          {app.drawingDetour ? "Done drawing" : "Draw detour on map"}
        </button>
        {#if draft.detour.length}
          <span class="muted">{draft.detour.length} points</span>
          <button class="link" onclick={clearDetour}>Clear</button>
        {/if}
      </p>

      <p>
        <input
          type="file"
          id="streets-file"
          accept=".geojson,.json,application/geo+json"
          hidden
          bind:this={streetsInput}
          onchange={onStreetsFile}
        >
        <button onclick={() => streetsInput?.click()}>Import street network (GeoJSON)</button>
        {#if getStreets()}
          {@const count = getStreets()?.network.segmentCount ?? 0}
          <span class="chip svc" title="Detour clicks snap to street segments and stops">
            {count} street segments — snapping on
          </span>
        {:else}
          <span class="muted">without one, detour clicks snap to stops only</span>
        {/if}
      </p>

      <p>
        <button class="primary" onclick={applyDiversion}>Apply diversion</button>
        <button onclick={cancelDiversion}>Cancel</button>
      </p>
    </div>
  {:else}
    <p>
      <button onclick={startDiversion}>New diversion</button>
      <button onclick={() => syncEdits()} title="Push local edits, pull the server's, merge (last writer wins)">
        Sync with server
      </button>
    </p>
  {/if}
</section>
{/if}
