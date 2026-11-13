<script lang="ts">
  /// The stops UI's layout (D-157): PLACEMENT ONLY. The date, a search,
  /// and the status filters with their counts across the top; the map
  /// beside the list of the stops shown, or the open stop's card, or the
  /// new-stop form. Every value is the host's (host.svelte.ts).
  import {
    st,
    STATUSES,
    readDate,
    setDate,
    toggleStatus,
    openStop,
    closeStop,
    startPicking,
    picked,
    addEvent,
    correctEvent,
    newStop,
    retry,
  } from "./host.svelte";
  import StopLifeMap from "../../components/StopLifeMap.svelte";
  import StopLifeList from "../../components/StopLifeList.svelte";
  import StopLifeCard from "../../components/StopLifeCard.svelte";
  import NewStopForm from "../../components/NewStopForm.svelte";
  import Combobox from "../../components/Combobox.svelte";
  import Unavailable from "../../components/Unavailable.svelte";
  import { STATUS_COLORS, STATUS_WORDS, problemWords } from "../../components/stopWords";

  let find = $state("");
  const options = $derived(st.all.map((s) => ({ value: s.stop_id, label: s.name || s.stop_id, sub: s.code && s.code !== s.stop_id ? `${s.stop_id} · ${s.code}` : s.stop_id })));
  const count = (status: string) => st.counts.find((c) => c.status === status)?.stops ?? 0;
</script>

{#snippet failure(frame: string, title: string)}
  <Unavailable {title} error={st.failed[frame]} onretry={retry} />
{/snippet}

<div class="shell">
  <header class="bar">
    <h1>Stops</h1>
    <label class="date">On <input type="date" value={readDate()} onchange={(e) => setDate(e.currentTarget.value)} data-testid="stops-on" /></label>
    <div class="find">
      <Combobox label="Find a stop" bind:value={find} {options} placeholder="Find a stop by name, code, or id…" onchange={(v) => { if (v) openStop(v); find = ""; }} />
    </div>
    <div class="filters" role="group" aria-label="Statuses shown">
      {#each STATUSES as s (s)}
        <button type="button" class="chip" class:on={st.show.includes(s)} aria-pressed={st.show.includes(s)} onclick={() => toggleStatus(s)} data-testid="filter-{s}">
          <i class="dot" style="background:{STATUS_COLORS[s][0]};border-color:{STATUS_COLORS[s][1]}"></i>{STATUS_WORDS[s]} <span class="n">{count(s)}</span>
        </button>
      {/each}
    </div>
    <button type="button" class="new" disabled={st.busy || st.picking === "new"} onclick={() => { void closeStop(); startPicking("new"); }} data-testid="new-stop-start">New stop</button>
    <p class="status" class:alert={st.alert} role="status" data-testid="status">{st.busy ? "… " : ""}{st.status}</p>
  </header>

  <main class="body">
    <div class="map">
      {#if st.failed.Stops}{@render failure("Stops", "The stops")}{:else}
        <StopLifeMap stops={st.shown} selected={st.open} picking={!!st.picking} point={st.point} onstop={openStop} onpoint={picked} />
      {/if}
    </div>
    <aside class="side">
      {#if st.picking === "new"}
        <NewStopForm date={readDate()} point={st.point} busy={st.busy} onsave={newStop} oncancel={() => startPicking("")} />
      {:else if st.open}
        {#if st.failed.Card}{@render failure("Card", "The stop")}
        {:else if st.card}
          <StopLifeCard stopId={st.open} card={st.card} served={st.served} date={readDate()} busy={st.busy} refusals={st.refusals}
            moving={st.picking === "move"} point={st.point}
            onevent={(e, done) => addEvent(st.open, e, done)}
            oncorrect={(i, e, done) => correctEvent(st.open, i, e, done)}
            onmove={(start) => startPicking(start ? "move" : "")}
            onclose={closeStop} />
        {/if}
      {:else}
        <StopLifeList stops={st.shown} onstop={openStop} />
      {/if}
      {#if st.picking === "new" && st.refusals.length}
        <ul class="refused">{#each st.refusals as r, i (i)}<li>Not saved: {problemWords(r.problem.kind)}.</li>{/each}</ul>
      {/if}
    </aside>
  </main>
</div>

<style>
  .shell { display: grid; grid-template-rows: auto 1fr; height: 100vh; }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem 0.9rem; padding: 0.5rem 0.9rem; border-bottom: 1px solid var(--line); background: var(--surface); }
  h1 { font-size: 1.05rem; margin: 0; }
  .find { min-width: 16rem; flex: 1; max-width: 26rem; }
  .filters { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .chip { display: inline-flex; align-items: center; gap: 0.3rem; opacity: 0.7; }
  .chip.on { opacity: 1; }
  .n { color: var(--muted); font-variant-numeric: tabular-nums; }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; border: 1.5px solid; }
  .status { margin: 0 0 0 auto; color: var(--muted); font-size: 0.85rem; }
  .status.alert { color: var(--bad); }
  .body { display: grid; grid-template-columns: 1fr minmax(20rem, 28rem); min-height: 0; }
  .map { position: relative; min-height: 0; }
  .side { overflow: auto; padding: 0.9rem; border-left: 1px solid var(--line); }
  .refused { color: var(--bad); padding-left: 1rem; }
  @media (max-width: 800px) {
    .shell { height: auto; }
    .body { grid-template-columns: 1fr; }
    .map { height: 55vh; }
    .side { border-left: 0; border-top: 1px solid var(--line); }
  }
</style>
