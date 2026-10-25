<script lang="ts">
  import { app, getEditDocument, getStreets } from "../state.svelte";
  import {
    startDiversion, cancelDiversion, setDiversionAnchor, toggleSkipped,
    applyDiversion, toggleDiversion, toggleDrawing, clearDetour,
    undoDetourSlice, syncEdits, serviceDate,
  } from "../actions";
  import { pathLengthMeters } from "../streets";
  import Combobox from "./Combobox.svelte";

  const routes = $derived(app.gtfs ? app.gtfs.routes() : []);
  const routeOptions = $derived(
    routes.map((r) => ({ value: r.route_id, label: r.name, sub: r.route_type })),
  );
  const draft = $derived(app.diversionDraft);
  // Retired records (tombstones) are hidden until asked for.
  let showRetired = $state(false);
  // The document: transit-sync EditRecords (id, diversion, status,
  // rev, updated_at). Display is derived here; truth lives in the doc.
  const records = $derived(getEditDocument());
  const activeRecords = $derived(records.filter((d) => d.status !== "retired"));
  const retiredRecords = $derived(records.filter((d) => d.status === "retired"));

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

  // Route ghost: once a route is picked, show where it runs — the
  // alignment (shapes when the feed has them) the detour is drawn
  // against. Re-fetches on operator edits: a diverted route shows
  // its patched path.
  $effect(() => {
    void app.feedEpoch;
    const d = app.diversionDraft;
    const routeId = d?.routeId;
    if (!d || !routeId || !app.gtfs) {
      app.routeContext = null;
      return;
    }
    const dir = d.directionId === "" ? null : Number(d.directionId);
    app.routeContext = app.gtfs.route_path_geojson(routeId, dir, serviceDate());
  });

  // The drawn path's length — the number to reason about when
  // authoring the added time.
  const drawnLength = $derived.by(() => {
    const pts = app.diversionDraft?.detour ?? [];
    if (pts.length < 2) return null;
    const m = pathLengthMeters(pts);
    return m >= 950 ? `${(m / 1000).toFixed(1)} km` : `${Math.round(m)} m`;
  });
</script>

<!-- ESC exits tracing mode — the reflex every drawing tool has.
The draft itself is untouched: Done (or Apply) still owns it. -->
<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && app.drawingDetour) toggleDrawing();
  }}
/>

{#if app.gtfs}
<section>
  <h2>Diversions</h2>
  <p class="muted">Operator edits: reroute around a closure. Boards, blocks and plans re-derive locally.</p>

  <!-- The list the dispatcher works in: only in-force diversions.
  Retired records stay in the document — sync needs the tombstone to
  carry the deletion to replicas that still hold the record — but
  they leave the working list, revealable here to inspect or restore.
  Pruning tombstones is a sync-document lifecycle decision (the
  server's, after a grace window past a record's own dates), never
  a feed-artifact one: the feed is a codec, not the data model. -->
  {#if records.length}
    <ul class="diversion-list">
      {#each activeRecords as d (d.id)}
        {@const rep = reportOf(d.id)}
        <li>
          <label class="chip diversion-chip">
            <input
              type="checkbox"
              checked
              onchange={() => toggleDiversion(d.id)}
              title="In force — uncheck to retire"
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
      {#if showRetired}
        {#each retiredRecords as d (d.id)}
          <li class="inactive">
            <label class="chip diversion-chip">
              <input
                type="checkbox"
                onchange={() => toggleDiversion(d.id)}
                title="Retired (sync tombstone) — check to restore"
              >
              <strong>{routeName.get(d.diversion.route_id) ?? d.diversion.route_id}</strong>
              {stopName.get(d.diversion.first_kept_stop) ?? d.diversion.first_kept_stop}
              → {stopName.get(d.diversion.last_kept_stop) ?? d.diversion.last_kept_stop}
              {#if d.diversion.added_secs}+{Math.round(d.diversion.added_secs / 60)} min{/if}
              <span class="muted">retired — restore by checking</span>
            </label>
          </li>
        {/each}
      {/if}
    </ul>
    <p class="muted">
      Unchecking retires an edit (a sync tombstone — it leaves this list).
      {#if retiredRecords.length}
        <button class="link" onclick={() => (showRetired = !showRetired)}>
          {showRetired ? "Hide" : "Show"} {retiredRecords.length} retired
        </button>
      {/if}
    </p>
  {/if}

  {#if draft}
    <div class="diversion-editor">
      <p class="picker-row">
        <label for="div-route">Route</label>
        <Combobox
          id="div-route"
          label="Route"
          placeholder="Pick a route…"
          options={routeOptions}
          bind:value={draft.routeId}
        />
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

      <p class="muted">
        Trace the reroute: click to confirm each segment — the path routes along the
        imported street network and snaps to stops; move the mouse to see where the
        next click lands, drag to pan. It becomes the
        alignment the vehicle follows between the anchor stops; serving comes
        from the anchors and skip flags, the schedule effect from added time.
      </p>
      <p>
        <button class:primary={app.drawingDetour} onclick={toggleDrawing}>
          {app.drawingDetour ? "Done tracing" : "Trace path on map"}
        </button>
        {#if app.drawingDetour}
          <span class="chip svc">click confirms · drag pans</span>
        {/if}
        {#if draft.detour.length}
          <span class="muted">
            {draft.detour.length} points{#if drawnLength} · {drawnLength}{/if}
          </span>
          <button class="link" onclick={undoDetourSlice}>Undo</button>
          <button class="link" onclick={clearDetour}>Clear</button>
        {/if}
        {#if getStreets()}
          <span class="chip svc" title="Waypoints snap to the imported street geometry and stops; off-network clicks place nothing">
            snapping on — off-street clicks ignored
          </span>
        {:else if !app.drawingDetour}
          <span class="muted">points snap to stops only — streets: Data sources</span>
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
