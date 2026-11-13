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
    readDraft,
    keepDraft,
    survey,
    locate,
    putRequest,
    workOrder,
    addAttachment,
    attachmentSrc,
  } from "./host.svelte";
  import StopLifeMap from "../../components/StopLifeMap.svelte";
  import StopLifeList from "../../components/StopLifeList.svelte";
  import StopLifeCard from "../../components/StopLifeCard.svelte";
  import NewStopForm from "../../components/NewStopForm.svelte";
  import StopRequests from "../../components/StopRequests.svelte";
  import SignInControl from "../../components/SignInControl.svelte";
  import { signin, signIn, signOut } from "../../signin.svelte";
  import FeedNotice from "../../components/FeedNotice.svelte";
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
        <button type="button" class="filter" class:on={st.show.includes(s)} aria-pressed={st.show.includes(s)} onclick={() => toggleStatus(s)} data-testid="filter-{s}">
          <i class="dot" style="--fill:{STATUS_COLORS[s][0]};--edge:{STATUS_COLORS[s][1]}"></i>{STATUS_WORDS[s]}<span class="n">{count(s)}</span>
        </button>
      {/each}
    </div>
    <button type="button" class="new" disabled={st.busy || st.picking === "new"} onclick={() => { void closeStop(); startPicking("new"); }} data-testid="new-stop-start">New stop</button>
    <SignInControl available={signin.available} name={signin.name} error={signin.error} onsignin={signIn} onsignout={signOut} />
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
        <NewStopForm date={readDate()} point={st.point} busy={st.busy} onpoint={picked} onlocate={locate} onsave={newStop} oncancel={() => startPicking("")} />
      {:else if st.open}
        {#if st.failed.Card}{@render failure("Card", "The stop")}
        {:else if st.card}
          <StopLifeCard stopId={st.open} card={st.card} served={st.served} date={readDate()} busy={st.busy} refusals={st.refusals}
            point={st.point} onpoint={picked} onlocate={locate}
            questions={st.questions} requests={st.requests} draft={readDraft(st.open)} ondraft={(d) => keepDraft(st.open, d)}
            onsurvey={(e, done) => survey(st.open, e, done)}
            attachments={st.attachments ? { onadd: addAttachment, src: attachmentSrc } : null}
            onevent={(e, done) => addEvent(st.open, e, done)}
            oncorrect={(i, e, done) => correctEvent(st.open, i, e, done)}
            onmove={(start) => startPicking(start ? "move" : "")}
            onclose={closeStop} />
          {#if st.failed.Requests}{@render failure("Requests", "The stop's requests")}
          {:else}
            <StopRequests stopId={st.open} requests={st.requests} vocab={st.requestVocab} date={readDate()} busy={st.busy}
              refusals={st.requestRefusals} onsave={putRequest} workOrders={!!st.workOrders} onworkorder={workOrder}
              attachments={st.attachments ? { onadd: addAttachment, src: attachmentSrc } : null} />
          {/if}
        {/if}
      {:else}
        {#if st.feed}<FeedNotice feed={st.feed} onstop={openStop} />{/if}
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
  /* A status filter: on is filled and in the text colour, off is flat
     and muted with a hollow dot — the state reads without the count. */
  .filter {
    display: inline-flex; align-items: center; gap: 0.4rem; padding: 0.2rem 0.6rem;
    border: 1px solid var(--line); border-radius: 6px; background: transparent;
    color: var(--muted); font: inherit; font-size: 0.85rem; cursor: pointer;
  }
  .filter:hover { background: var(--hover); }
  .filter:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .filter.on { background: var(--hover); color: var(--text); border-color: color-mix(in srgb, var(--text) 30%, var(--line)); }
  .n { font-size: 0.75rem; color: var(--muted); font-variant-numeric: tabular-nums; }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; border: 1.5px solid var(--edge); background: var(--fill); }
  .filter:not(.on) .dot { background: transparent; border-color: var(--muted); }
  .status { margin: 0 0 0 auto; color: var(--muted); font-size: 0.85rem; }
  .status.alert { color: var(--bad); }
  .body { display: grid; grid-template-columns: 1fr minmax(20rem, 28rem); min-height: 0; }
  /* A survey is a long form: while one is open the side takes most of
     the width, and the map keeps a strip to place the stop. */
  .body:has(:global(.survey)), .body:has(:global(.wide-form)) { grid-template-columns: minmax(16rem, 1fr) minmax(28rem, 62%); }
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
