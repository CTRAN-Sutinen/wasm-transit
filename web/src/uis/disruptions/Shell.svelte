<script lang="ts">
  /// The disruptions UI's layout: PLACEMENT ONLY. A bar across the top
  /// (the name, the agency's date and clock, the views, whether the
  /// times are live, sign-in) over a panel beside a large map. The
  /// panel shows the open disruption, else the view: Status (the search
  /// and the affected routes) or Dispatch (late now, detours, cancelled
  /// trips, today's notes), or Alerts (the disruptions with no alert,
  /// and the agency's alerts). An alert being written takes half the
  /// page, beside what riders see of it (its reach, entity by entity,
  /// and its map); the rider's board opens in a dialog. Every value is the host's
  /// (host.svelte.ts).
  import {
    dz,
    setView,
    setQuery,
    setPick,
    openDisruption,
    showBus,
    retry,
    why,
    newAlert,
    editAlert,
    endAlert,
    closeForm,
    changeAlert,
    fillTemplate,
    saveAlert,
    setPreviewStop,
    setPreviewQuery,
    downloadFeed,
    setStartsNow,
    openPreview,
    addStop,
    writeNote,
    startAlertFor,
    type View,
  } from "./host.svelte";
  import AlertsBoard from "../../components/AlertsBoard.svelte";
  import AlertEditor from "../../components/AlertEditor.svelte";
  import AlertPreview from "../../components/AlertPreview.svelte";
  import AlertReach from "../../components/AlertReach.svelte";
  import DisruptionsMap from "../../components/DisruptionsMap.svelte";
  import RouteStatus from "../../components/RouteStatus.svelte";
  import RouteStopSearch from "../../components/RouteStopSearch.svelte";
  import DispatchBoard from "../../components/DispatchBoard.svelte";
  import DisruptionDetail from "../../components/DisruptionDetail.svelte";
  import SignInControl from "../../components/SignInControl.svelte";
  import Unavailable from "../../components/Unavailable.svelte";
  import { dateText } from "../../components/disruptionsFormat";
  import { signin, signIn, signOut } from "../../signin.svelte";

  const VIEWS: { id: View; label: string }[] = [
    { id: "status", label: "Status" },
    { id: "alerts", label: "Alerts" },
    { id: "dispatch", label: "Dispatch" },
  ];
  const focus = $derived(dz.pick?.kind === "route" ? dz.pick.id : "");
</script>

<div class="page">
  <div class="top">
  <header class="bar">
    <strong class="name">Disruptions</strong>
    <span class="when" data-testid="clock">{dz.date ? dateText(dz.date) : ""} · {dz.clock}</span>
    <nav class="views" aria-label="Views">
      {#each VIEWS as v (v.id)}
        <button type="button" class:on={dz.view === v.id && !dz.open && !dz.form} aria-current={dz.view === v.id ? "page" : undefined} onclick={() => (dz.open ? openDisruption(null) : null, dz.form ? closeForm() : null, setView(v.id))}>{v.label}</button>
      {/each}
    </nav>
    <span class="live" class:off={!dz.live} class:partial={!!dz.liveError} data-testid="live-state" title={dz.liveError}>{dz.live ? `Live · ${dz.liveAt}` : "No live feed"}{dz.liveError ? " · A feed is not answering" : ""}</span>
    <SignInControl available={signin.available} name={signin.name} error={signin.error} onsignin={signIn} onsignout={signOut} />
  </header>
  {#if dz.status || dz.editsStale}
    <p class="status" class:alert={dz.alert} role="status">{dz.status || dz.editsStale}</p>
  {/if}
  </div>

  <main class="body" class:writing={!!dz.form}>
    <aside class="panel">
      {#if dz.form}
        <AlertEditor
          form={dz.form}
          templates={dz.templates}
          routes={dz.routes}
          stops={dz.formStops}
          refusals={dz.refusals}
          flags={dz.flags}
          zone={dz.zone}
          now={dz.now}
          busy={dz.busy}
          error={dz.writeError || why("Form")}
          query={dz.query}
          found={dz.found}
          oninput={setQuery}
          onchange={changeAlert}
          ontemplate={fillTemplate}
          onsave={saveAlert}
          oncancel={closeForm}
          onstartsnow={setStartsNow}
          onpreview={() => openPreview(true)}
        />
      {:else if dz.open}
        <DisruptionDetail
          disruption={dz.openItem}
          route={dz.openRoute}
          tripStops={dz.openStops}
          notes={dz.openNotes}
          zone={dz.zone}
          date={dz.date}
          notesOff={why("OpenNotes")}
          busy={dz.busy}
          error={dz.writeError}
          onclose={() => openDisruption(null)}
          onnote={(text, id) => (dz.open ? writeNote(dz.open, text, id) : Promise.resolve(false))}
          onstartalert={() => dz.open && startAlertFor(dz.open)}
        />
      {:else if dz.view === "alerts"}
        {#if dz.frames.Alerts}<Unavailable title="Service alerts" state={dz.frames.Alerts} onretry={retry} />{/if}
        <AlertsBoard
          gaps={dz.gaps}
          list={dz.alerts}
          routes={dz.routes}
          zone={dz.zone}
          busy={dz.busy}
          gapsOff={why("Gaps")}
          onstart={newAlert}
          onedit={editAlert}
          onend={endAlert}
        />
      {:else if dz.view === "status"}
        <RouteStopSearch query={dz.query} found={dz.found} pick={dz.pick} oninput={setQuery} onpick={setPick} />
        <div class="gap"></div>
        {#if dz.frames.Status}
          <Unavailable title="Affected routes" state={dz.frames.Status} onretry={retry} />
        {:else if dz.status_}
          <RouteStatus today={dz.status_} zone={dz.zone} date={dz.date} {focus} onopen={openDisruption} onfocus={(r) => setPick(r ? { kind: "route", ...r } : null)} />
        {/if}
      {:else}
        {#if dz.frames.Day}
          <Unavailable title="Today" state={dz.frames.Day} onretry={retry} />
        {/if}
        <DispatchBoard
          late={dz.late}
          live={dz.live}
          day={dz.day}
          notes={dz.notes}
          zone={dz.zone}
          date={dz.date}
          lateOff={why("Late")}
          notesOff={why("Notes")}
          onbus={showBus}
          onopen={openDisruption}
        />
      {/if}
    </aside>
    <section class="map" class:hidden={!!dz.form}>
      <DisruptionsMap live={dz.live} day={dz.day} {focus} path={dz.openPath} open={dz.openItem} bus={dz.bus} ondisruption={openDisruption} />
    </section>
    {#if dz.form}
      <aside class="side">
        <AlertReach reach={dz.reach} stops={dz.allStops} off={why("Reach")} onstop={addStop} />
      </aside>
    {/if}
    {#if dz.form && dz.previewOpen}
      <div class="scrim" role="presentation" onclick={(e) => e.target === e.currentTarget && openPreview(false)}>
        <div class="dialog" role="dialog" aria-modal="true" aria-label="Preview" data-testid="preview-dialog">
          <button type="button" class="close" aria-label="Close" onclick={() => openPreview(false)}>×</button>
          <AlertPreview
          stop={dz.previewStop}
          board={dz.preview}
          routes={dz.routes}
          canDownload={!!dz.previewFeed}
          off={why("Preview") || (dz.previewWaits ? "Fill in what Publish needs to see the preview" : "")}
          query={dz.previewQuery}
          found={dz.previewFound}
          oninput={setPreviewQuery}
          onstop={setPreviewStop}
          ondownload={downloadFeed}
        />
        </div>
      </div>
    {/if}
  </main>
</div>

<style>
  .page { height: 100dvh; display: grid; grid-template-rows: auto minmax(0, 1fr); }
  .bar { display: flex; align-items: center; gap: 1rem; padding: 0.5rem 1rem; border-bottom: 1px solid var(--line); }
  .name { font-size: 1.05rem; }
  .when { color: var(--muted); }
  .views { display: flex; gap: 0.25rem; margin-left: 1rem; }
  .views button { border: 0; background: none; padding: 0.4rem 0.8rem; border-radius: 6px; cursor: pointer; font: inherit; color: var(--muted); }
  .views button.on { background: var(--accent-soft); color: var(--text); font-weight: 600; }
  .live { margin-left: auto; font-size: 0.85rem; color: var(--ok); }
  .live.off { color: var(--muted); }
  .live.partial { color: var(--warn); }
  .status { margin: 0; padding: 0.35rem 1rem; font-size: 0.85rem; background: var(--hover); }
  .status.alert { color: var(--bad); }
  .body { display: grid; grid-template-columns: 30rem 1fr; min-height: 0; }
  .body.writing { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .map.hidden { display: none; }
  .scrim { position: fixed; inset: 0; background: rgb(0 0 0 / 0.45); display: grid; place-items: center; z-index: 20; }
  .dialog { position: relative; width: min(32rem, 92vw); max-height: 85vh; overflow: auto; background: var(--surface); border-radius: 10px; padding: 2.2rem 1.2rem 1.2rem; }
  .close { position: absolute; top: 0.5rem; right: 0.6rem; border: 0; background: none; font-size: 1.3rem; cursor: pointer; color: var(--muted); }
  .side { overflow: auto; padding: 1rem; border-left: 1px solid var(--line); }
  .panel { overflow: auto; padding: 1rem; border-right: 1px solid var(--line); }
  .gap { height: 0.9rem; }
  .map { position: relative; min-height: 0; display: grid; }
</style>
