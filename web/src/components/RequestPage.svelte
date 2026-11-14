<script lang="ts">
  /// One request for work at a stop (D-159; C-TRAN's Bus Stop Sign &
  /// Shelter Installation Worksheet) as a page: its header (the job, its
  /// id, its stop, a status track — drafted, submitted, work order open,
  /// completed, or cancelled) with the one next step for that status
  /// beside it (submit; open the work order through the agency's system,
  /// D-165, or enter it by hand; check its status; complete it; record
  /// the change it carried out on the stop), and the worksheet below as
  /// sections under their own headings — Location, Service, Utility
  /// locate, Work order, Completion, Notes & photos — each edited in
  /// place. A new request is the same page with every section open and
  /// the job chosen first. The lists its answers come from are
  /// cap-requests `vocabulary`'s; what a request must carry is the
  /// library's to say (its refusals come back as props, shown beside
  /// Save). A RENDER component.
  import "./forms.css";
  import type { RequestRefusal } from "../types/RequestRefusal";
  import type { StopRequest } from "../types/StopRequest";
  import type { WorkOrder } from "../types/WorkOrder";
  import Attachments from "./Attachments.svelte";
  import AttachmentThumbs from "./AttachmentThumbs.svelte";
  import { JOB_WORDS, answerWords, jobWords, requestProblemWords, requestStatusWords } from "./stopWords";

  type Item = { hash: string; type: string; name?: string; caption?: string; taken_on?: string };
  let {
    id = "",
    request = null,
    stopId,
    stopName,
    taken = [],
    job = "",
    vocab,
    date,
    busy,
    workOrders,
    refusals,
    error,
    onsave,
    onworkorder,
    onhandorder,
    onrecord,
    onstop,
    onback,
    backLabel,
    attachments = null,
  }: {
    /// The request shown; none for a new one.
    id?: string;
    request?: StopRequest | null;
    stopId: string;
    stopName: string;
    /// The ids the stop's requests already have: a new request's
    /// suggested id is none of them.
    taken?: string[];
    /// A new request's job, chosen already (a field report: a repair; a
    /// new stop's installation).
    job?: string;
    vocab: { jobs: string[]; statuses: string[]; sides: string[]; directions: string[]; markings: string[] };
    date: string;
    busy: boolean;
    /// Whether the deployment connects a work order system (D-165).
    workOrders: boolean;
    refusals: RequestRefusal[];
    error: string;
    onsave: (id: string, request: StopRequest, done: string) => void;
    onworkorder: (id: string, action: "open" | "refresh") => void;
    onhandorder: (id: string, wo: WorkOrder) => void;
    onrecord: () => void;
    onstop: () => void;
    onback: () => void;
    backLabel: string;
    attachments?: { onadd: (file: File) => Promise<Item | null>; src: (hash: string) => Promise<string | null> } | null;
  } = $props();

  type Form = {
    id: string; job: string; requested_by: string; requested_on: string; routes: string; direction: string; street: string;
    cross_street: string; offset_ft: string; side: string; city: string; county: string; township: string; range: string;
    section: string; parcel: string; marked_by: string; marked_note: string; ticket: string; called_on: string; called_by: string;
    utilities: string; completed_on: string; completed_by: string; note: string; attachments: Item[];
  };

  function freeId(): string {
    const base = `${stopId}-${date.replaceAll("-", "")}`;
    let n = base;
    for (let i = 2; taken.includes(n); i++) n = `${base}-${i}`;
    return n;
  }

  function toForm(r: StopRequest | null): Form {
    return {
      id: r ? id : freeId(), job: r?.job ?? job, requested_by: r?.requested_by ?? "", requested_on: r?.requested_on ?? date,
      routes: (r?.routes ?? []).join(", "), direction: r?.direction ?? "", street: r?.street ?? "", cross_street: r?.cross_street ?? "",
      offset_ft: r?.offset_ft == null ? "" : String(r.offset_ft), side: r?.side ?? "", city: r?.city ?? "", county: r?.county ?? "",
      township: r?.township ?? "", range: r?.range ?? "", section: r?.section ?? "", parcel: r?.parcel ?? "", marked_by: r?.marked_by ?? "",
      marked_note: r?.marked_note ?? "", ticket: r?.locate?.ticket ?? "", called_on: r?.locate?.called_on ?? "", called_by: r?.locate?.called_by ?? "",
      utilities: (r?.locate?.utilities ?? []).join(", "), completed_on: r?.completed_on ?? "", completed_by: r?.completed_by ?? "",
      note: r?.note ?? "", attachments: [...(r?.attachments ?? [])],
    };
  }

  const fresh = $derived(!request);
  // svelte-ignore state_referenced_locally
  let f = $state<Form>(toForm(request));
  /// The section being edited (a held request edits one at a time).
  let editing = $state("");
  // A held request read again (saved, its work order answered) shows as it is.
  let seen = $state("");
  $effect(() => {
    const key = JSON.stringify(request);
    if (request && key !== seen) {
      seen = key;
      f = toForm(request);
      editing = "";
    }
  });
  const open = (k: string) => fresh || editing === k;

  const list = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);
  const opt = (s: string) => (s.trim() ? s.trim() : undefined);

  function build(status: string): StopRequest {
    const locate = { ticket: opt(f.ticket), called_on: opt(f.called_on), called_by: opt(f.called_by), utilities: list(f.utilities) };
    const hasLocate = locate.ticket || locate.called_on || locate.called_by || locate.utilities.length;
    const r: StopRequest = {
      ...(request ?? {}),
      stop: stopId, job: f.job, status,
      requested_by: opt(f.requested_by), requested_on: opt(f.requested_on), routes: list(f.routes), direction: opt(f.direction),
      street: opt(f.street), cross_street: opt(f.cross_street), offset_ft: f.offset_ft.trim() === "" ? undefined : Number(f.offset_ft),
      side: opt(f.side), city: opt(f.city), county: opt(f.county), township: opt(f.township), range: opt(f.range),
      section: opt(f.section), parcel: opt(f.parcel), marked_by: opt(f.marked_by), marked_note: opt(f.marked_note),
      locate: hasLocate ? { ...(request?.locate ?? {}), ...locate } : undefined,
      completed_on: opt(f.completed_on), completed_by: opt(f.completed_by), note: opt(f.note),
      attachments: f.attachments.length ? f.attachments : undefined,
    };
    for (const k of Object.keys(r) as (keyof StopRequest)[]) if (r[k] === undefined) delete r[k];
    return r;
  }

  const save = (status: string, done: string) => onsave(fresh ? f.id.trim() : id, build(status), done);
  const saveSection = () => save(request!.status, `Request ${id} saved.`);
  const cancelSection = () => ((f = toForm(request)), (editing = ""));

  // the next step's own fields: the work order by hand, the completion
  let step = $state("");
  let woId = $state("");
  let woSystem = $state("");
  let woUrl = $state("");
  let woStatus = $state("");
  function startHand() {
    const w = request?.work_order;
    woId = w?.id ?? "";
    woSystem = w?.system ?? "";
    woUrl = w?.url ?? "";
    woStatus = w?.status ?? "";
    step = "wo";
  }
  function recordHand() {
    const out: WorkOrder = { id: woId.trim() };
    if (woSystem.trim()) out.system = woSystem.trim();
    if (woUrl.trim()) out.url = woUrl.trim();
    if (woStatus.trim()) out.status = woStatus.trim();
    onhandorder(id, out);
    step = "";
  }
  function startComplete() {
    f.completed_on = f.completed_on || date;
    step = "complete";
  }

  const track = $derived(vocab.statuses.filter((s) => s !== "cancelled"));
  const at = $derived(request ? track.indexOf(request.status) : -1);
  const wo = $derived(request?.work_order);
  const live = $derived(!!request && request.status !== "completed" && request.status !== "cancelled");
  const problems = $derived([...refusals.map((r) => `Not saved: ${requestProblemWords(r.problem)}.`), ...(error ? [error] : [])]);
  const show = (v: string | undefined | null) => (v != null && String(v).trim() !== "" ? String(v) : "—");
</script>

{#snippet chips(value: string, options: string[], set: (v: string) => void, testid: string)}
  <div class="f-chips">
    {#each options as o (o)}
      <button type="button" aria-pressed={value === o} onclick={() => set(value === o ? "" : o)} data-testid="{testid}-{o}">{answerWords(o)}</button>
    {/each}
  </div>
{/snippet}

{#snippet head(title: string, key: string)}
  <header class="card-head">
    <h2>{title}</h2>
    {#if !fresh && request?.status !== "cancelled"}
      {#if editing === key}
        <button type="button" class="f-link" onclick={cancelSection}>Cancel</button>
        <button type="button" class="small primary" disabled={busy} onclick={saveSection} data-testid="request-save-{key}">Save</button>
      {:else if !editing}
        <button type="button" class="f-link" disabled={busy} onclick={() => (editing = key)} data-testid="request-edit-{key}">Edit</button>
      {/if}
    {/if}
  </header>
{/snippet}

{#snippet row(label: string, value: string | undefined | null)}
  <dt>{label}</dt><dd>{show(value)}</dd>
{/snippet}

<article class="request-page" data-testid="request-view">
  <header class="top">
    <button type="button" class="f-link back" onclick={onback} data-testid="request-back">← {backLabel}</button>
    <div class="title-row">
      <div class="titles">
        <h1>{fresh ? "New request" : jobWords(request!.job)} {#if !fresh}<span class="muted id">{id}</span>{/if}</h1>
        <p class="muted">At <button type="button" class="f-link" onclick={onstop} data-testid="request-stop">{stopName} · Stop {stopId}</button>{#if request?.requested_on} · requested {request.requested_on}{request.requested_by ? ` by ${request.requested_by}` : ""}{/if}</p>
      </div>
      {#if request}
        {#if request.status === "cancelled"}
          <p class="cancelled" data-testid="request-status">Cancelled</p>
        {:else}
          <ol class="track" data-testid="request-track">
            {#each track as s, i (s)}<li class:done={i < at} class:now={i === at} data-testid="request-track-{s}">{requestStatusWords(s)}</li>{/each}
          </ol>
        {/if}
      {/if}
    </div>
  </header>

  <div class="body">
    <div class="sheet">
      {#if fresh}
        <section class="card wide" data-testid="request-job">
          <header class="card-head"><h2>Job type</h2></header>
          <ul class="f-choices jobs" role="list">
            {#each vocab.jobs as j (j)}
              <li><button type="button" aria-pressed={f.job === j} onclick={() => (f.job = j)} data-testid="request-job-{j}"><strong>{jobWords(j)}</strong>{#if JOB_WORDS[j]}<span>{JOB_WORDS[j][1]}</span>{/if}</button></li>
            {/each}
          </ul>
        </section>
      {/if}

      <section class="card" data-testid="request-part-where">
        {@render head("Location", "where")}
        {#if open("where")}
          <div class="f-pair">
            <label class="f-field"><span class="f-label">Street</span><input type="text" bind:value={f.street} data-testid="request-street" /></label>
            <label class="f-field"><span class="f-label">Cross street</span><input type="text" bind:value={f.cross_street} data-testid="request-cross" /></label>
            <label class="f-field"><span class="f-label">Offset (ft)</span><input type="text" inputmode="decimal" bind:value={f.offset_ft} data-testid="request-offset" /></label>
          </div>
          <div class="f-field" role="group" aria-label="Side"><span class="f-label">Side</span>{@render chips(f.side, vocab.sides, (v) => (f.side = v), "request-side")}</div>
          <div class="f-pair">
            <label class="f-field"><span class="f-label">City</span><input type="text" bind:value={f.city} data-testid="request-city" /></label>
            <label class="f-field"><span class="f-label">County</span><input type="text" bind:value={f.county} data-testid="request-county" /></label>
          </div>
          <div class="f-pair">
            <label class="f-field"><span class="f-label">Township</span><input type="text" bind:value={f.township} /></label>
            <label class="f-field"><span class="f-label">Range</span><input type="text" bind:value={f.range} /></label>
            <label class="f-field"><span class="f-label">Section</span><input type="text" bind:value={f.section} /></label>
            <label class="f-field"><span class="f-label">Parcel</span><input type="text" bind:value={f.parcel} data-testid="request-parcel" /></label>
          </div>
          <div class="f-field" role="group" aria-label="Marking"><span class="f-label">Marking</span>{@render chips(f.marked_by, vocab.markings, (v) => (f.marked_by = v), "request-marked")}</div>
          <label class="f-field"><span class="f-label">Marking notes</span><input type="text" bind:value={f.marked_note} /></label>
        {:else}
          <dl>
            {@render row("Street", request?.street)}{@render row("Cross street", request?.cross_street)}
            {@render row("Offset (ft)", request?.offset_ft == null ? null : String(request.offset_ft))}{@render row("Side", request?.side && answerWords(request.side))}
            {@render row("City", request?.city)}{@render row("County", request?.county)}
            {@render row("Township / range / section", [request?.township, request?.range, request?.section].filter(Boolean).join(" / "))}{@render row("Parcel", request?.parcel)}
            {@render row("Marking", [request?.marked_by && answerWords(request.marked_by), request?.marked_note].filter(Boolean).join(" — "))}
          </dl>
        {/if}
      </section>

      <section class="card" data-testid="request-part-service">
        {@render head("Service", "service")}
        {#if open("service")}
          <label class="f-field"><span class="f-label">Routes</span><input type="text" bind:value={f.routes} placeholder="e.g. 37, 71" data-testid="request-routes" /></label>
          <div class="f-field" role="group" aria-label="Direction"><span class="f-label">Direction</span>{@render chips(f.direction, vocab.directions, (v) => (f.direction = v), "request-direction")}</div>
          <div class="f-pair">
            <label class="f-field"><span class="f-label">Requested by</span><input type="text" bind:value={f.requested_by} data-testid="request-by" /></label>
            <label class="f-field"><span class="f-label">Requested on</span><input type="date" bind:value={f.requested_on} data-testid="request-on" /></label>
          </div>
        {:else}
          <dl>
            {@render row("Routes", (request?.routes ?? []).join(", "))}{@render row("Direction", request?.direction && answerWords(request.direction))}
            {@render row("Requested by", request?.requested_by)}{@render row("Requested on", request?.requested_on)}
          </dl>
        {/if}
      </section>

      <section class="card" data-testid="request-part-locate">
        {@render head("Utility locate", "locate")}
        {#if open("locate")}
          <div class="f-pair">
            <label class="f-field"><span class="f-label">Ticket</span><input type="text" bind:value={f.ticket} data-testid="request-ticket" /></label>
            <label class="f-field"><span class="f-label">Called on</span><input type="date" bind:value={f.called_on} data-testid="request-called-on" /></label>
            <label class="f-field"><span class="f-label">Called by</span><input type="text" bind:value={f.called_by} /></label>
          </div>
          <label class="f-field"><span class="f-label">Utilities</span><input type="text" bind:value={f.utilities} placeholder="e.g. City of Vancouver, NW Natural" /></label>
        {:else}
          <dl>
            {@render row("Ticket", request?.locate?.ticket)}{@render row("Called on", request?.locate?.called_on)}
            {@render row("Called by", request?.locate?.called_by)}{@render row("Utilities", (request?.locate?.utilities ?? []).join(", "))}
          </dl>
        {/if}
      </section>

      {#if !fresh}
        <section class="card" data-testid="request-part-workorder">
          <header class="card-head"><h2>Work order</h2></header>
          <dl>
            {@render row("Number", wo?.id)}{@render row("System", wo?.system)}
            {@render row("Status", wo?.status ? `${wo.status}${wo.read_on ? ` (read ${wo.read_on})` : ""}` : null)}
          </dl>
          {#if wo?.url}<p><a href={wo.url} target="_blank" rel="noopener">Open in {wo.system ?? "the work order system"}</a></p>{/if}
        </section>
        <section class="card" data-testid="request-part-completion">
          <header class="card-head"><h2>Completion</h2></header>
          <dl>{@render row("Completed on", request?.completed_on)}{@render row("Completed by", request?.completed_by)}</dl>
        </section>
      {/if}

      <section class="card wide" data-testid="request-part-note">
        {@render head("Notes & photos", "note")}
        {#if open("note")}
          <label class="f-field"><span class="f-label">Notes</span><textarea rows="3" bind:value={f.note} data-testid="request-note"></textarea></label>
          {#if attachments}<Attachments bind:items={f.attachments} onadd={attachments.onadd} src={attachments.src} testid="request-attachments" />{/if}
        {:else}
          <p>{show(request?.note)}</p>
          {#if attachments && request?.attachments?.length}<AttachmentThumbs items={request.attachments} src={attachments.src} />{/if}
        {/if}
      </section>
    </div>

    <aside class="side" class:first={!fresh} data-testid="request-next-step">
      {#if fresh}
        <h2>Save</h2>
        <label class="f-field">
          <span class="f-label">Request ID</span>
          <input type="text" bind:value={f.id} required data-testid="request-id" />
          <span class="f-hint">Default: stop ID and date.</span>
        </label>
        {#if problems.length}<ul class="problems" role="alert" data-testid="request-problems">{#each problems as p, i (i)}<li>{p}</li>{/each}</ul>{/if}
        <div class="actions">
          <button type="button" class="primary" disabled={busy || !f.id.trim() || !f.job} onclick={() => save("submitted", `Request ${f.id.trim()} submitted.`)} data-testid="request-submit">Submit</button>
          <button type="button" disabled={busy || !f.id.trim() || !f.job} onclick={() => save("drafted", `Request ${f.id.trim()} saved as a draft.`)} data-testid="request-save-draft">Save as draft</button>
          <button type="button" class="f-link" onclick={onback}>Cancel</button>
        </div>
      {:else if request}
        <h2>Next step</h2>
        {#if request.status === "drafted"}
          <p>Draft.</p>
          <button type="button" class="primary" disabled={busy} onclick={() => onsave(id, { ...request, status: "submitted" }, `Request ${id} submitted.`)} data-testid="request-submit-now">Submit</button>
        {:else if request.status === "submitted"}
          <p>Submitted. No work order yet.</p>
          {#if workOrders}
            <button type="button" class="primary" disabled={busy} onclick={() => onworkorder(id, "open")} data-testid="request-wo-open">Open work order</button>
          {:else if step !== "wo"}
            <button type="button" class="primary" disabled={busy} onclick={startHand} data-testid="request-wo-hand">Enter work order</button>
          {/if}
        {:else if request.status === "ordered"}
          <p>Work order {wo?.id}{wo?.system ? ` in ${wo.system}` : ""}{wo?.status ? `: ${wo.status}` : ""}{wo?.read_on ? ` (read ${wo.read_on})` : ""}.</p>
          {#if step !== "complete"}
            <button type="button" class="primary" disabled={busy} onclick={startComplete} data-testid="request-complete">Complete</button>
            {#if workOrders}
              <button type="button" disabled={busy} onclick={() => onworkorder(id, "refresh")} data-testid="request-wo-refresh">Check status</button>
            {:else if step !== "wo"}
              <button type="button" disabled={busy} onclick={startHand} data-testid="request-wo-hand">Edit work order</button>
            {/if}
          {/if}
        {:else if request.status === "completed"}
          <p>Completed{request.completed_on ? ` ${request.completed_on}` : ""}{request.completed_by ? ` by ${request.completed_by}` : ""}.</p>
          <button type="button" class="primary" disabled={busy} onclick={onrecord} data-testid="request-record">Update stop record</button>
        {:else}
          <p class="muted">Cancelled.</p>
        {/if}

        {#if step === "wo"}
          <form class="step" data-testid="request-wo-form" onsubmit={(e) => (e.preventDefault(), recordHand())}>
            <label class="f-field"><span class="f-label">Work order #</span><input type="text" bind:value={woId} required data-testid="request-wo-id" /></label>
            <label class="f-field"><span class="f-label">System</span><input type="text" bind:value={woSystem} placeholder="e.g. Maintenance" data-testid="request-wo-system" /></label>
            <label class="f-field"><span class="f-label">Link</span><input type="url" bind:value={woUrl} /></label>
            <label class="f-field"><span class="f-label">Status</span><input type="text" bind:value={woStatus} data-testid="request-wo-status" /></label>
            <div class="actions row"><button type="submit" class="primary" disabled={busy || !woId.trim()} data-testid="request-wo-save">Save</button><button type="button" onclick={() => (step = "")}>Cancel</button></div>
          </form>
        {:else if step === "complete"}
          <form class="step" data-testid="request-complete-form" onsubmit={(e) => (e.preventDefault(), save("completed", `Request ${id} completed.`), (step = ""))}>
            <label class="f-field"><span class="f-label">Completed on</span><input type="date" bind:value={f.completed_on} required data-testid="request-completed-on" /></label>
            <label class="f-field"><span class="f-label">Completed by</span><input type="text" bind:value={f.completed_by} data-testid="request-completed-by" /></label>
            <div class="actions row"><button type="submit" class="primary" disabled={busy || !f.completed_on} data-testid="request-save">Complete</button><button type="button" onclick={() => (step = "")}>Cancel</button></div>
          </form>
        {/if}
        {#if problems.length}<ul class="problems" role="alert" data-testid="request-problems">{#each problems as p, i (i)}<li>{p}</li>{/each}</ul>{/if}

        {#if live}
          {#if step === "cancel"}
            <div class="confirm" data-testid="request-cancel-confirm">
              <p><strong>Cancel this request?</strong></p>
              <div class="actions row">
                <button type="button" class="danger" disabled={busy} onclick={() => onsave(id, { ...request, status: "cancelled" }, `Request ${id} cancelled.`)} data-testid="request-cancel-yes">Cancel request</button>
                <button type="button" onclick={() => (step = "")}>Back</button>
              </div>
            </div>
          {:else}
            <button type="button" class="f-link danger-link" disabled={busy} onclick={() => (step = "cancel")} data-testid="request-cancel">Cancel request</button>
          {/if}
        {/if}
      {/if}
    </aside>
  </div>
</article>

<style>
  .request-page { display: grid; grid-template-rows: auto minmax(0, 1fr); height: 100%; min-height: 0; }
  .top { padding: 0.8rem 1.6rem 1rem; border-bottom: 1px solid var(--line); }
  .back { font-size: 0.88rem; }
  .title-row { display: flex; flex-wrap: wrap; align-items: flex-end; gap: 0.8rem 2rem; margin-top: 0.4rem; }
  .titles { flex: 1; min-width: 16rem; }
  h1 { margin: 0; font-size: 1.35rem; }
  .id { font-size: 1rem; font-weight: 400; }
  .titles p { margin: 0.2rem 0 0; }
  .track { list-style: none; margin: 0; padding: 0; display: flex; gap: 0.2rem; min-width: 28rem; }
  .track li { flex: 1; text-align: center; padding: 0.35rem 0.6rem; font-size: 0.82rem; color: var(--muted); border-bottom: 3px solid var(--line); white-space: nowrap; }
  .track li.done { color: var(--text); border-bottom-color: color-mix(in srgb, var(--ok) 60%, var(--line)); }
  .track li.now { color: var(--text); font-weight: 600; border-bottom-color: var(--accent); }
  .cancelled { color: var(--muted); text-decoration: line-through; font-weight: 600; margin: 0; }
  .body { display: grid; grid-template-columns: minmax(0, 1fr) 22rem; min-height: 0; }
  .sheet { overflow: auto; padding: 1.2rem 1.6rem 3rem; display: grid; grid-template-columns: repeat(auto-fill, minmax(26rem, 1fr)); gap: 1.2rem; align-content: start; }
  .card { border: 1px solid var(--line); border-radius: 8px; padding: 0.9rem 1.1rem; min-width: 0; }
  .card.wide { grid-column: 1 / -1; }
  .card-head { display: flex; align-items: baseline; gap: 0.8rem; margin-bottom: 0.7rem; }
  .card-head h2 { margin: 0; font-size: 1rem; flex: 1; }
  .jobs { display: grid; grid-template-columns: repeat(auto-fill, minmax(14rem, 1fr)); gap: 0.5rem; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 0.3rem 1rem; margin: 0; font-size: 0.92rem; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  .side { overflow: auto; padding: 1.2rem 1.4rem; border-left: 1px solid var(--line); display: flex; flex-direction: column; gap: 0.6rem; }
  .side h2 { margin: 0; font-size: 1rem; }
  .side p { margin: 0; }
  .actions { display: flex; flex-direction: column; gap: 0.5rem; }
  .actions.row { flex-direction: row; }
  .side button:not(.f-link), .card-head .small { min-height: 2.5rem; padding: 0 1rem; border-radius: 6px; }
  .card-head .small { min-height: 2rem; }
  .primary { background: var(--accent); color: var(--surface); border: 1px solid var(--accent); font-weight: 600; cursor: pointer; }
  .primary:disabled { opacity: 0.5; }
  .step { border-top: 1px solid var(--line); padding-top: 0.8rem; }
  .problems { color: var(--bad); padding-left: 1.1rem; margin: 0; font-size: 0.9rem; }
  .confirm { border: 1px solid var(--bad); border-radius: 8px; padding: 0.7rem; }
  .danger { color: var(--bad); border: 1px solid var(--bad); }
  .danger-link { color: var(--bad); font-size: 0.88rem; align-self: flex-start; margin-top: auto; }
  @media (max-width: 900px) {
    .top { padding: 0.6rem 0.9rem; }
    .track { min-width: 0; width: 100%; }
    .track li { padding: 0.3rem 0.1rem; font-size: 0.72rem; white-space: normal; }
    .body { grid-template-columns: 1fr; grid-template-rows: auto auto; overflow: auto; }
    .side { border-left: 0; border-top: 1px solid var(--line); overflow: visible; padding: 0.8rem 0.9rem; }
    .side.first { order: -1; border-top: 0; border-bottom: 1px solid var(--line); }
    .sheet { overflow: visible; padding: 0.9rem; grid-template-columns: 1fr; }
  }
</style>
