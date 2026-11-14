<script lang="ts">
  /// The stops UI's layout (D-157): PLACEMENT ONLY. A bar across the top
  /// (the views — Stops, Work, Feed review — the date, a search, New
  /// stop, sign-in, the status line) over one screen the width of the
  /// page, chosen by what is open: a survey's page; a request's page; a
  /// stop's page (a short change on it as a card under its header); the
  /// map with a narrow panel, only while a point is placed (a new stop, a
  /// move); else the view — the stops on a map beside their list or as a
  /// table, the work as a table, the feed's review. Every value is the
  /// host's (host.svelte.ts).
  import {
    st,
    STATUSES,
    readDate,
    setDate,
    setView,
    setMode,
    toggleStatus,
    setWork,
    openStop,
    closeStop,
    openRequest,
    startTask,
    endTask,
    picked,
    placing,
    addEvent,
    correctEvent,
    newStop,
    survey,
    readDraft,
    keepDraft,
    locate,
    nearMe,
    clearNear,
    putRequest,
    recordWorkOrder,
    workOrder,
    addAttachment,
    attachmentSrc,
    retry,
  } from "./host.svelte";
  import StopLifeMap from "../../components/StopLifeMap.svelte";
  import StopLifeList from "../../components/StopLifeList.svelte";
  import StopsTable from "../../components/StopsTable.svelte";
  import StopPage from "../../components/StopPage.svelte";
  import ChangeFlow from "../../components/ChangeFlow.svelte";
  import SurveyPage from "../../components/SurveyPage.svelte";
  import RequestPage from "../../components/RequestPage.svelte";
  import WorkTable from "../../components/WorkTable.svelte";
  import NewStopFlow from "../../components/NewStopFlow.svelte";
  import FeedReview from "../../components/FeedReview.svelte";
  import SignInControl from "../../components/SignInControl.svelte";
  import { signin, signIn, signOut } from "../../signin.svelte";
  import Combobox from "../../components/Combobox.svelte";
  import Unavailable from "../../components/Unavailable.svelte";
  import { STATUS_COLORS, STATUS_WORDS } from "../../components/stopWords";

  let find = $state("");
  const options = $derived(st.all.map((s) => ({ value: s.stop_id, label: s.name || s.stop_id, sub: s.code && s.code !== s.stop_id ? `${s.stop_id} · ${s.code}` : s.stop_id })));
  const count = (status: string) => st.counts.find((c) => c.status === status)?.stops ?? 0;
  const task = $derived(st.task);
  const kind = $derived(task.includes(":") ? task.slice(task.indexOf(":") + 1) : "");
  const stopName = $derived(st.card?.stop?.name || st.open);
  const held = $derived(st.requests.find((r) => r.id === st.request) ?? null);
  const files = $derived(st.attachments ? { onadd: addAttachment, src: attachmentSrc } : null);
  /// What fills the page.
  const screen = $derived(
    task === "new" || task === "change:move"
      ? "place"
      : st.open && task === "survey"
        ? "survey"
        : st.open && (task === "request" || task === "request:new" || task === "report")
          ? "new-request"
          : st.open && st.request && !task
            ? "request"
            : st.open
              ? "stop"
              : st.view,
  );
  const back = $derived(st.view === "work" ? "Work" : st.view === "feed" ? "Feed review" : "Stops");
  const choices = $derived(
    st.card ? [...st.card.next.filter((k) => k !== "retired"), ...(st.card.stop && st.card.stop.status !== "retired" ? ["move", "details"] : []), ...st.card.next.filter((k) => k === "retired")] : [],
  );
  const VIEWS: [typeof st.view, string][] = [
    ["stops", "Stops"],
    ["work", "Work"],
    ["feed", "Feed review"],
  ];
</script>

{#snippet failure(frame: string, title: string)}
  <Unavailable {title} error={st.failed[frame]} onretry={retry} />
{/snippet}

{#snippet map(focus: boolean, at: { lat: number; lon: number } | null = null)}
  {#if st.failed.Stops}{@render failure("Stops", "The stops")}{:else}
    <StopLifeMap stops={st.shown} selected={st.open} picking={placing()} point={st.point} me={st.near} {focus} {at}
      onstop={(id) => { if (!task) void openStop(id); }} onpoint={picked} />
  {/if}
{/snippet}

{#snippet change(kindOf: string)}
  {#if st.card}
    <ChangeFlow stopId={st.open} stop={st.card.stop} events={st.card.events} kind={kindOf} {choices} card={kindOf !== "move"}
      date={readDate()} busy={st.busy} routes={st.served?.routes.map((r) => r.name) ?? []}
      requests={st.requests.map((r) => ({ id: r.id, job: r.request.job }))} fromRequest={st.request}
      amenityNames={st.questions.amenities} point={st.point} onpoint={picked} onlocate={locate}
      refusals={st.refusals} error={st.taskError}
      onsave={(e, done) => addEvent(st.open, e, done)} oncorrect={(i, e, done) => correctEvent(st.open, i, e, done)} onclose={endTask} />
  {/if}
{/snippet}

<div class="shell" class:focused={screen === "survey"}>
  <header class="bar">
    <h1>Stops</h1>
    <nav class="views" aria-label="Views">
      {#each VIEWS as [v, label] (v)}
        <button type="button" class:on={st.view === v && screen === v} aria-current={st.view === v ? "page" : undefined}
          onclick={() => { void closeStop(); void setView(v); }} data-testid="view-{v}">
          {label}{#if v === "feed" && st.feed}<i class="flag" aria-label="changes to review"></i>{/if}
        </button>
      {/each}
    </nav>
    <label class="date">On <input type="date" value={readDate()} onchange={(e) => setDate(e.currentTarget.value)} data-testid="stops-on" /></label>
    <div class="find">
      <Combobox label="Find a stop" bind:value={find} {options} placeholder="Find a stop by name, code, or ID…" onchange={(v) => { if (v) openStop(v); find = ""; }} />
    </div>
    <button type="button" class="new" disabled={st.busy || task === "new"} onclick={() => { void openStop("").then(() => startTask("new")); }} data-testid="new-stop-start">New stop</button>
    <SignInControl available={signin.available} name={signin.name} error={signin.error} onsignin={signIn} onsignout={signOut} />
    <p class="status" class:alert={st.alert} role="status" data-testid="status">{st.busy ? "… " : ""}{st.status}</p>
  </header>

  <main class="screen">
    <!-- The stops' view stays mounted under the other screens, so its map
         is not rebuilt (and keeps where it was) on the way back. -->
    <div class="keep" class:hidden={screen !== "stops"}>
      <div class="stops-view">
          <header class="tools">
            <div class="filters" role="group" aria-label="Statuses shown">
              {#each STATUSES as s (s)}
                <button type="button" class="filter" class:on={st.show.includes(s)} aria-pressed={st.show.includes(s)} onclick={() => toggleStatus(s)} data-testid="filter-{s}">
                  <i class="dot" style="--fill:{STATUS_COLORS[s][0]};--edge:{STATUS_COLORS[s][1]}"></i>{STATUS_WORDS[s]}<span class="n">{count(s)}</span>
                </button>
              {/each}
            </div>
            {#if st.nearby}
              <button type="button" onclick={clearNear} data-testid="near-clear">All stops</button>
              <button type="button" disabled={st.locating} onclick={nearMe} data-testid="near-again">Refresh</button>
            {:else}
              <button type="button" class="near-me" disabled={st.locating} onclick={nearMe} data-testid="near-me">{st.locating ? "Locating…" : "Nearby stops"}</button>
            {/if}
            <div class="mode" role="group" aria-label="Show as">
              <button type="button" class:on={st.mode === "map"} aria-pressed={st.mode === "map"} onclick={() => setMode("map")} data-testid="mode-map">Map</button>
              <button type="button" class:on={st.mode === "table"} aria-pressed={st.mode === "table"} onclick={() => setMode("table")} data-testid="mode-table">Table</button>
            </div>
          </header>
          {#if st.mode === "table"}
            <StopsTable stops={st.nearby ?? st.shown} onstop={(id) => openStop(id)} />
          {:else}
            <div class="split">
              <aside class="list">
                {#if st.failed.Near}{@render failure("Near", "The stops near you")}
                {:else if st.nearby}<StopLifeList stops={st.nearby} near onstop={(id) => openStop(id)} />
                {:else}<StopLifeList stops={st.shown} onstop={(id) => openStop(id)} />{/if}
              </aside>
              <div class="map">{@render map(false)}</div>
            </div>
          {/if}
        </div>
    </div>
    {#if screen === "place"}
      <div class="split place">
        <div class="map">{@render map(task === "change:move")}</div>
        <aside class="panel">
          {#if task === "new"}
            <NewStopFlow date={readDate()} point={st.point} busy={st.busy} refusals={st.refusals} error={st.taskError}
              onpoint={picked} onlocate={locate} onsave={(id, e, req) => newStop(id, e, req)} onclose={endTask} />
          {:else}{@render change("move")}{/if}
        </aside>
      </div>
    {:else if screen === "survey" && st.card}
      <SurveyPage stopId={st.open} stop={st.card.stop} questions={st.questions} date={readDate()} busy={st.busy}
        draft={readDraft(st.open)} ondraft={(d) => keepDraft(st.open, d)} refusals={st.refusals} error={st.taskError}
        onsave={(e, done) => survey(st.open, e, done)} onclose={endTask} attachments={files} />
    {:else if screen === "new-request" || screen === "request"}
      {#if screen === "request" && !held}
        {#if st.failed.Requests}{@render failure("Requests", "The stop's requests")}{:else}<p class="pad muted">Reading request {st.request}… <button type="button" class="f-link" onclick={() => openRequest("")}>Back to the stop</button></p>{/if}
      {:else}
        {#key screen + st.request + task}
          <RequestPage id={held?.id ?? ""} request={screen === "request" ? (held?.request ?? null) : null} stopId={st.open} {stopName}
            taken={st.requests.map((r) => r.id)} job={task === "request:new" ? "new" : task === "report" ? "repair_improve" : ""}
            vocab={st.requestVocab} date={readDate()} busy={st.busy} workOrders={!!st.workOrders}
            refusals={st.requestRefusals} error={st.taskError}
            onsave={(id, r, done) => putRequest(id, r, done, screen === "new-request")}
            onworkorder={workOrder} onhandorder={(id, wo) => recordWorkOrder(id, st.open, wo)}
            onrecord={() => startTask("change")} onstop={() => openStop(st.open)}
            onback={() => (screen === "new-request" ? endTask() : st.view === "work" ? closeStop() : openRequest(""))}
            backLabel={screen === "request" && st.view === "work" ? "Work" : stopName} attachments={files} />
        {/key}
      {/if}
    {:else if screen === "stop"}
      {#if st.failed.Card}{@render failure("Card", "The stop")}
      {:else if !st.card}<p class="pad muted">Reading the stop…</p>
      {:else}
        {#snippet mini()}{@render map(true, st.card?.stop ?? null)}{/snippet}
        {#snippet changeCard()}{@render change(task.startsWith("fix:") ? task : kind)}{/snippet}
        <StopPage stopId={st.open} card={st.card} served={st.served} date={readDate()} busy={st.busy}
          questions={st.questions} requests={st.requests} draft={readDraft(st.open) !== null} attachments={files}
          map={mini} change={task.startsWith("change") || task.startsWith("fix:") ? changeCard : undefined} backLabel={back}
          ontask={startTask} onrequest={(id) => openStop(st.open, id)} onback={closeStop} />
      {/if}
    {:else if screen === "work"}
      {#if st.failed.Work}{@render failure("Work", "The work across the stops")}
      {:else}
        <WorkTable requests={st.work} counts={st.workCounts} stops={st.workStops} statuses={st.workStatuses} jobs={st.workJobs}
          vocab={st.requestVocab} onfilter={setWork} onopen={(stop, id) => openStop(stop, id)} />
      {/if}
    {:else if screen === "feed"}
      {#if st.failed.Feed}{@render failure("Feed", "What the feed did")}
      {:else}<FeedReview feed={st.feed} held={st.held} onstop={(id) => openStop(id)} />{/if}
    {/if}
  </main>
</div>

<style>
  .shell { display: grid; grid-template-rows: auto minmax(0, 1fr); height: 100dvh; }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem 0.9rem; padding: 0.5rem 1rem; border-bottom: 1px solid var(--line); background: var(--surface); }
  h1 { font-size: 1.05rem; margin: 0; }
  .views { display: flex; gap: 0.15rem; padding: 0.15rem; border-radius: 8px; background: var(--hover); }
  .views button { position: relative; border: 0; background: none; padding: 0.3rem 0.85rem; border-radius: 6px; color: var(--muted); font: inherit; font-size: 0.92rem; cursor: pointer; }
  .views button.on { background: var(--surface); color: var(--text); font-weight: 600; box-shadow: 0 1px 2px rgb(0 0 0 / 0.15); }
  .flag { position: absolute; top: 4px; right: 4px; width: 7px; height: 7px; border-radius: 50%; background: var(--accent); }
  .find { min-width: 14rem; flex: 1; max-width: 26rem; }
  .status { margin: 0 0 0 auto; color: var(--muted); font-size: 0.85rem; }
  .status.alert { color: var(--bad); }
  .screen { min-height: 0; overflow: hidden; }
  .keep { min-height: 0; height: 100%; }
  /* hidden, not destroyed: the map keeps its instance and its view */
  .hidden { display: none; }
  .pad { padding: 1.2rem 1.6rem; }
  .split { display: grid; grid-template-columns: minmax(20rem, 26rem) minmax(0, 1fr); height: 100%; min-height: 0; }
  .split.place { grid-template-columns: minmax(0, 1fr) minmax(24rem, 30rem); }
  .map { position: relative; min-height: 0; }
  .panel { border-left: 1px solid var(--line); min-height: 0; overflow: hidden; display: flex; flex-direction: column; }
  .panel > :global(*) { flex: 1; min-height: 0; }
  .list { overflow: auto; padding: 0.6rem 0.9rem; border-right: 1px solid var(--line); min-height: 0; }
  .stops-view { display: grid; grid-template-rows: auto minmax(0, 1fr); height: 100%; min-height: 0; }
  .tools { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem 0.8rem; padding: 0.5rem 1rem; border-bottom: 1px solid var(--line); }
  .filters { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .filter {
    display: inline-flex; align-items: center; gap: 0.4rem; padding: 0.2rem 0.6rem; min-height: 2rem;
    border: 1px solid var(--line); border-radius: 6px; background: transparent; color: var(--muted); font: inherit; font-size: 0.85rem; cursor: pointer;
  }
  .filter:hover { background: var(--hover); }
  .filter.on { background: var(--hover); color: var(--text); border-color: color-mix(in srgb, var(--text) 30%, var(--line)); }
  .n { font-size: 0.75rem; color: var(--muted); font-variant-numeric: tabular-nums; }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; border: 1.5px solid var(--edge); background: var(--fill); }
  .filter:not(.on) .dot { background: transparent; border-color: var(--muted); }
  .tools > button { min-height: 2rem; padding: 0 0.8rem; }
  .near-me { font-weight: 600; }
  .mode { margin-left: auto; display: flex; gap: 0.15rem; padding: 0.15rem; border-radius: 8px; background: var(--hover); }
  .mode button { border: 0; background: none; padding: 0.25rem 0.8rem; border-radius: 6px; color: var(--muted); font: inherit; cursor: pointer; }
  .mode button.on { background: var(--surface); color: var(--text); font-weight: 600; }
  @media (max-width: 900px) {
    .bar { gap: 0.4rem 0.6rem; padding: 0.45rem 0.7rem; }
    .find { flex-basis: 100%; max-width: none; order: 5; }
    .status { flex-basis: 100%; margin: 0; order: 6; }
    .screen { overflow: auto; }
    .split, .split.place { grid-template-columns: 1fr; grid-template-rows: 45dvh auto; height: auto; }
    .split .list { order: 2; border-right: 0; border-top: 1px solid var(--line); overflow: visible; }
    .panel { border-left: 0; border-top: 1px solid var(--line); overflow: visible; }
    .near-me { flex: 1; min-height: 2.9rem !important; }
    .mode { margin-left: 0; }
    /* a survey at the stop is the whole screen */
    .focused { grid-template-rows: minmax(0, 1fr); }
    .focused .bar { display: none; }
  }
</style>
