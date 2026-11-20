<script lang="ts">
  /// The disruptions UI's layout: PLACEMENT ONLY. A bar across the top
  /// (the name, the agency's date and clock, the views, whether the
  /// times are live, sign-in) over a panel beside a large map. The
  /// panel shows the open disruption, else the view: Status (the search
  /// and the affected routes) or Dispatch (late now, detours, cancelled
  /// trips, today's notes). Every value is the host's (host.svelte.ts).
  import { dz, setView, setQuery, setPick, openDisruption, showBus, retry, why, type View } from "./host.svelte";
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
        <button type="button" class:on={dz.view === v.id && !dz.open} aria-current={dz.view === v.id ? "page" : undefined} onclick={() => (dz.open ? openDisruption(null) : null, setView(v.id))}>{v.label}</button>
      {/each}
    </nav>
    <span class="live" class:off={!dz.live} data-testid="live-state" title={dz.liveError}>{dz.live ? `Live · ${dz.liveAt}` : "No live feed"}</span>
    <SignInControl available={signin.available} name={signin.name} error={signin.error} onsignin={signIn} onsignout={signOut} />
  </header>
  {#if dz.status || dz.editsStale}
    <p class="status" class:alert={dz.alert} role="status">{dz.status || dz.editsStale}</p>
  {/if}
  </div>

  <main class="body">
    <aside class="panel">
      {#if dz.open}
        <DisruptionDetail
          disruption={dz.openItem}
          route={dz.openRoute}
          tripStops={dz.openStops}
          notes={dz.openNotes}
          zone={dz.zone}
          date={dz.date}
          notesOff={why("OpenNotes")}
          onclose={() => openDisruption(null)}
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
    <section class="map">
      <DisruptionsMap live={dz.live} day={dz.day} {focus} path={dz.openPath} open={dz.openItem} bus={dz.bus} ondisruption={openDisruption} />
    </section>
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
  .status { margin: 0; padding: 0.35rem 1rem; font-size: 0.85rem; background: var(--hover); }
  .status.alert { color: var(--bad); }
  .body { display: grid; grid-template-columns: 30rem 1fr; min-height: 0; }
  .panel { overflow: auto; padding: 1rem; border-right: 1px solid var(--line); }
  .gap { height: 0.9rem; }
  .map { position: relative; min-height: 0; display: grid; }
</style>
