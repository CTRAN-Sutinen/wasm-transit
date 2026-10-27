<script lang="ts">
  /// The diversion editor — a RENDER component over cap-divert's
  /// answers: the edit document's rows (names resolved), the draft's
  /// view (the route's patterns and the default, each stop's role and
  /// skip, the flattened trace and its length, the problems), and the
  /// day views' validation reports. The form writes what the
  /// dispatcher entered into the draft; every rule is the module's.
  import { app, getStreets } from "../state.svelte";
  import {
    startDiversion, cancelDiversion, setDiversionAnchor, toggleServed,
    applyDiversion, setDiversionRetired, toggleDrawing, clearDetour,
    undoDetourSlice, syncEdits,
  } from "../actions";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import type { DiversionRow } from "../types/DiversionRow";
  import type { DraftView } from "../types/DraftView";
  import type { EditReport } from "../types/EditReport";
  import Combobox from "./Combobox.svelte";

  let {
    routes,
    rows,
    reports,
    view,
  }: {
    routes: DirectoryRoute[];
    rows: DiversionRow[];
    reports: EditReport[];
    view: DraftView | null;
  } = $props();

  const routeOptions = $derived(
    routes.map((r) => ({ value: r.route_id, label: r.name, sub: r.mode })),
  );
  const draft = $derived(app.diversionDraft);
  // Retired records (tombstones) are hidden until asked for.
  let showRetired = $state(false);
  const activeRows = $derived(rows.filter((r) => r.in_force));
  const retiredRows = $derived(rows.filter((r) => !r.in_force));
  const reportOf = (id: string) => reports.find((r) => r.id === id);

  function fmtLength(m: number) {
    return m >= 950 ? `${(m / 1000).toFixed(1)} km` : `${Math.round(m)} m`;
  }
</script>

<!-- ESC exits tracing mode — the reflex every drawing tool has.
The draft itself is untouched: Done (or Apply) still owns it. -->
<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && app.drawingDetour) toggleDrawing();
  }}
/>

{#if app.feedLoaded}
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
  {#if rows.length}
    <ul class="diversion-list">
      {#each activeRows as d (d.record_id)}
        {@const rep = reportOf(d.record_id)}
        <li>
          <label class="chip diversion-chip">
            <input
              type="checkbox"
              checked
              onchange={() => setDiversionRetired(d.record_id, true)}
              title="In force — uncheck to retire"
            >
            <strong>{d.route_name}</strong>
            {d.first_kept_name} → {d.last_kept_name}
            {#if d.skipped_names.length}
              <span class="chip bad" title="stops no longer served">
                skips {d.skipped_names.join(", ")}
              </span>
            {/if}
            {#if d.added_secs}+{Math.round(d.added_secs / 60)} min{/if}
            {#if d.detour_drawn}<span class="chip svc">detour drawn</span>{/if}
            <span class="muted">
              {d.start_date} → {d.end_date}
              · {rep?.affected_trips ?? "?"} trips · {d.status}
            </span>
            {#if rep && !rep.valid}
              <span class="chip bad" title={rep.error}>invalid — {rep.error}</span>
            {/if}
          </label>
        </li>
      {/each}
      {#if showRetired}
        {#each retiredRows as d (d.record_id)}
          <li class="inactive">
            <label class="chip diversion-chip">
              <input
                type="checkbox"
                onchange={() => setDiversionRetired(d.record_id, false)}
                title="Retired (sync tombstone) — check to restore"
              >
              <strong>{d.route_name}</strong>
              {d.first_kept_name} → {d.last_kept_name}
              {#if d.added_secs}+{Math.round(d.added_secs / 60)} min{/if}
              <span class="muted">retired — restore by checking</span>
            </label>
          </li>
        {/each}
      {/if}
    </ul>
    <p class="muted">
      Unchecking retires an edit (a sync tombstone — it leaves this list).
      {#if retiredRows.length}
        <button class="link" onclick={() => (showRetired = !showRetired)}>
          {showRetired ? "Hide" : "Show"} {retiredRows.length} retired
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
        <label for="div-pattern">Pattern</label>
        <select
          id="div-pattern"
          value={view?.pattern ?? ""}
          onchange={(e) => draft && (draft.patternId = e.currentTarget.value)}
        >
          {#each view?.patterns ?? [] as p (p.pattern)}
            <option value={p.pattern}>{p.name} ({p.stop_count} stops)</option>
          {/each}
        </select>
      </p>

      {#if view?.stops.length}
        {#if !draft.firstKept}
          <p class="muted">Click the first stop the diversion keeps:</p>
        {:else if !draft.lastKept}
          <p class="muted">Now the last kept stop (on the list or the map):</p>
        {:else if view.problems.length}
          <p class="muted"><span class="chip bad">{view.problems.join("; ")}</span></p>
        {:else}
          <p class="muted">Anchors set. Interior stops can be un-skipped if the detour still serves them.</p>
        {/if}
        <ol class="stop-seq">
          {#each view.stops as s (s.index)}
            <li
              class:anchor-first={s.role === "first_kept"}
              class:anchor-last={s.role === "last_kept"}
              class:skipped={s.skipped}
            >
              <button class="link" onclick={() => setDiversionAnchor(s.stop_id)}>{s.name}</button>
              {#if s.role === "interior"}
                <label class="chip">
                  <input
                    type="checkbox"
                    checked={s.skipped}
                    onchange={() => toggleServed(s.stop_id)}
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
        {#if draft.slices.length}
          <span class="muted">
            {view?.detour.length ?? 0} points{#if view && view.detour.length >= 2} · {fmtLength(view.detour_length_m)}{/if}
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
