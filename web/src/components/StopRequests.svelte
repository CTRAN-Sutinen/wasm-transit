<script lang="ts">
  /// A stop's requests for work (D-159): the planning team's
  /// submissions — C-TRAN's Bus Stop Sign & Shelter Installation
  /// Worksheet — each tracking the work order the work order system
  /// opens for it. The list (each its job, status, and work order),
  /// and one form that starts a request or edits one, laid out as the
  /// worksheet is. The lists its answers come from are cap-requests
  /// `vocabulary`'s; what a request must carry is the library's to say
  /// (its refusals come back as props). A RENDER component.
  import type { IdRequest } from "../types/cap_requests/IdRequest";
  import type { RequestRefusal } from "../types/RequestRefusal";
  import type { StopRequest } from "../types/StopRequest";
  import Attachments from "./Attachments.svelte";
  import AttachmentThumbs from "./AttachmentThumbs.svelte";
  import { answerWords, requestProblemWords } from "./stopWords";

  let {
    stopId,
    requests,
    vocab,
    date,
    busy,
    refusals,
    onsave,
    workOrders,
    onworkorder,
    attachments = null,
  }: {
    stopId: string;
    requests: IdRequest[];
    vocab: { jobs: string[]; statuses: string[]; sides: string[]; directions: string[]; markings: string[] };
    date: string;
    busy: boolean;
    refusals: RequestRefusal[];
    /// Save one request under its id; true when it is saved.
    onsave: (id: string, request: StopRequest, done: string) => Promise<boolean>;
    /// Whether a work order system is connected (D-165), and asking it
    /// to open a request's work order or read its status. Without one,
    /// a work order is entered by hand on the form.
    workOrders: boolean;
    onworkorder: (id: string, action: "open" | "refresh") => void;
    /// Adding and showing photos and the site drawing, where the
    /// deployment keeps them (D-162).
    attachments?: { onadd: (file: File) => Promise<{ hash: string; type: string; name?: string; caption?: string; taken_on?: string } | null>; src: (hash: string) => Promise<string | null> } | null;
  } = $props();

  /// The form's values, every one a string as typed ("" not given).
  type Form = {
    id: string;
    job: string;
    status: string;
    requested_by: string;
    requested_on: string;
    routes: string;
    direction: string;
    street: string;
    cross_street: string;
    offset_ft: string;
    side: string;
    city: string;
    county: string;
    township: string;
    range: string;
    section: string;
    parcel: string;
    marked_by: string;
    marked_note: string;
    ticket: string;
    called_on: string;
    called_by: string;
    utilities: string;
    wo_system: string;
    wo_id: string;
    wo_url: string;
    wo_status: string;
    wo_read_on: string;
    completed_on: string;
    completed_by: string;
    note: string;
    attachments: { hash: string; type: string; name?: string; caption?: string; taken_on?: string }[];
  };

  let editing = $state<string | null>(null); // "" a new request, else its id
  let f = $state<Form>(blank());
  /// The request being edited as it was, so fields this form does not
  /// show are kept.
  let base: StopRequest | null = null;

  function blank(): Form {
    return {
      id: "", job: "new", status: "drafted", requested_by: "", requested_on: date, routes: "", direction: "", street: "",
      cross_street: "", offset_ft: "", side: "", city: "", county: "", township: "", range: "", section: "", parcel: "",
      marked_by: "", marked_note: "", ticket: "", called_on: "", called_by: "", utilities: "", wo_system: "", wo_id: "",
      wo_url: "", wo_status: "", wo_read_on: "", completed_on: "", completed_by: "", note: "", attachments: [],
    };
  }

  function start(id: string | null) {
    const held = requests.find((r) => r.id === id)?.request ?? null;
    base = held;
    const r = held;
    f = r
      ? {
          id: id ?? "", job: r.job, status: r.status, requested_by: r.requested_by ?? "", requested_on: r.requested_on ?? "",
          routes: (r.routes ?? []).join(", "), direction: r.direction ?? "", street: r.street ?? "", cross_street: r.cross_street ?? "",
          offset_ft: r.offset_ft == null ? "" : String(r.offset_ft), side: r.side ?? "", city: r.city ?? "", county: r.county ?? "",
          township: r.township ?? "", range: r.range ?? "", section: r.section ?? "", parcel: r.parcel ?? "",
          marked_by: r.marked_by ?? "", marked_note: r.marked_note ?? "", ticket: r.locate?.ticket ?? "",
          called_on: r.locate?.called_on ?? "", called_by: r.locate?.called_by ?? "", utilities: (r.locate?.utilities ?? []).join(", "),
          wo_system: r.work_order?.system ?? "", wo_id: r.work_order?.id ?? "", wo_url: r.work_order?.url ?? "",
          wo_status: r.work_order?.status ?? "", wo_read_on: r.work_order?.read_on ?? "", completed_on: r.completed_on ?? "",
          completed_by: r.completed_by ?? "", note: r.note ?? "", attachments: [...(r.attachments ?? [])],
        }
      : { ...blank(), id: `${stopId}-${date.replaceAll("-", "")}` };
    editing = id ?? "";
  }

  const list = (s: string) => s.split(",").map((x) => x.trim()).filter(Boolean);
  const opt = (s: string) => (s.trim() ? s.trim() : undefined);

  async function save() {
    const locate = { ticket: opt(f.ticket), called_on: opt(f.called_on), called_by: opt(f.called_by), utilities: list(f.utilities) };
    const hasLocate = locate.ticket || locate.called_on || locate.called_by || locate.utilities.length;
    const wo = { system: opt(f.wo_system), id: f.wo_id.trim(), url: opt(f.wo_url), status: opt(f.wo_status), read_on: opt(f.wo_read_on) };
    const hasWo = wo.system || wo.id || wo.url || wo.status || wo.read_on;
    const offset = f.offset_ft.trim() === "" ? undefined : Number(f.offset_ft);
    const request: StopRequest = {
      ...(base ?? {}),
      stop: stopId,
      job: f.job,
      status: f.status,
      requested_by: opt(f.requested_by),
      requested_on: opt(f.requested_on),
      routes: list(f.routes),
      direction: opt(f.direction),
      street: opt(f.street),
      cross_street: opt(f.cross_street),
      offset_ft: offset,
      side: opt(f.side),
      city: opt(f.city),
      county: opt(f.county),
      township: opt(f.township),
      range: opt(f.range),
      section: opt(f.section),
      parcel: opt(f.parcel),
      marked_by: opt(f.marked_by),
      marked_note: opt(f.marked_note),
      locate: hasLocate ? { ...(base?.locate ?? {}), ...locate } : undefined,
      work_order: hasWo ? { ...(base?.work_order ?? {}), ...wo } : undefined,
      completed_on: opt(f.completed_on),
      completed_by: opt(f.completed_by),
      note: opt(f.note),
      attachments: f.attachments.length ? f.attachments : undefined,
    };
    // absent, not undefined: the document carries only what is given
    for (const k of Object.keys(request) as (keyof StopRequest)[]) if (request[k] === undefined) delete request[k];
    const id = f.id.trim();
    if (await onsave(id, request, `Request ${id} saved.`)) editing = null;
  }

  const jobWords: Record<string, string> = {
    new: "New",
    reinstall: "Reinstall",
    relocate_remove: "Relocate/Remove",
    repair_improve: "Repair/Improve",
    temporary: "Temporary",
  };
  const statusWords: Record<string, string> = {
    drafted: "Draft",
    submitted: "Submitted",
    ordered: "Work order open",
    completed: "Completed",
    cancelled: "Cancelled",
  };
</script>

<section class="requests" data-testid="stop-requests">
  <div class="head">
    <h3>Requests</h3>
    {#if editing === null}
      <button type="button" disabled={busy} onclick={() => start(null)} data-testid="request-new">New request</button>
    {/if}
  </div>
  {#if requests.length}
    <ul class="list">
      {#each requests as r (r.id)}
        <li data-testid="request-row">
          <strong>{r.id}</strong>
          <span>{jobWords[r.request.job] ?? r.request.job}</span>
          <span class="status {r.request.status}">{statusWords[r.request.status] ?? r.request.status}</span>
          {#if r.request.work_order?.id}
            <span class="muted">
              {r.request.work_order.system ? `${r.request.work_order.system} ` : ""}{#if r.request.work_order.url}<a href={r.request.work_order.url} target="_blank" rel="noopener">{r.request.work_order.id}</a>{:else}{r.request.work_order.id}{/if}{r.request.work_order.status ? ` · ${r.request.work_order.status}${r.request.work_order.read_on ? ` (${r.request.work_order.read_on})` : ""}` : ""}
            </span>
          {/if}
          {#if editing === null}<button type="button" class="link" disabled={busy} onclick={() => start(r.id)} data-testid="request-edit">Edit</button>{/if}
          {#if attachments && r.request.attachments?.length}<AttachmentThumbs items={r.request.attachments} src={attachments.src} />{/if}
          {#if workOrders && editing === null && r.request.status !== "completed" && r.request.status !== "cancelled"}
            {#if r.request.work_order?.id}
              <button type="button" class="link" disabled={busy} onclick={() => onworkorder(r.id, "refresh")} data-testid="request-wo-refresh">Check status</button>
            {:else if r.request.status !== "drafted"}
              <button type="button" class="link" disabled={busy} onclick={() => onworkorder(r.id, "open")} data-testid="request-wo-open">Open work order</button>
            {/if}
          {/if}
        </li>
      {/each}
    </ul>
  {:else if editing === null}
    <p class="muted">No requests for this stop.</p>
  {/if}

  {#if editing !== null}
    <form
      class="wide-form"
      data-testid="request-form"
      onsubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <h3>{editing ? `Request ${editing}` : "New request"}</h3>
      <div class="grid">
        <label>Request id <input type="text" bind:value={f.id} required readonly={!!editing} data-testid="request-id" /></label>
        <label>Job
          <select bind:value={f.job} data-testid="request-job">{#each vocab.jobs as j (j)}<option value={j}>{jobWords[j] ?? j}</option>{/each}</select>
        </label>
        <label>Status
          <select bind:value={f.status} data-testid="request-status">{#each vocab.statuses as s (s)}<option value={s}>{statusWords[s] ?? s}</option>{/each}</select>
        </label>
        <label>Requested by <input type="text" bind:value={f.requested_by} data-testid="request-by" /></label>
        <label>Requested on <input type="date" bind:value={f.requested_on} data-testid="request-on" /></label>
      </div>

      <fieldset>
        <legend>Service</legend>
        <div class="grid">
          <label>Routes <input type="text" bind:value={f.routes} placeholder="37, 71" data-testid="request-routes" /></label>
          <label>Direction
            <select bind:value={f.direction} data-testid="request-direction"><option value="">not given</option>{#each vocab.directions as d (d)}<option value={d}>{answerWords(d)}</option>{/each}</select>
          </label>
        </div>
      </fieldset>

      <fieldset>
        <legend>Location</legend>
        <div class="grid">
          <label>Street <input type="text" bind:value={f.street} data-testid="request-street" /></label>
          <label>Cross street or address <input type="text" bind:value={f.cross_street} data-testid="request-cross" /></label>
          <label>Feet from its center <input type="text" inputmode="decimal" pattern="[0-9]*\.?[0-9]*" bind:value={f.offset_ft} data-testid="request-offset" /></label>
          <label>Side of street
            <select bind:value={f.side} data-testid="request-side"><option value="">not given</option>{#each vocab.sides as x (x)}<option value={x}>{answerWords(x)}</option>{/each}</select>
          </label>
          <label>City <input type="text" bind:value={f.city} /></label>
          <label>County <input type="text" bind:value={f.county} /></label>
          <label>Township <input type="text" bind:value={f.township} /></label>
          <label>Range <input type="text" bind:value={f.range} /></label>
          <label>Section <input type="text" bind:value={f.section} /></label>
          <label>Parcel <input type="text" bind:value={f.parcel} /></label>
          <label>Marked by
            <select bind:value={f.marked_by} data-testid="request-marked"><option value="">not given</option>{#each vocab.markings as m (m)}<option value={m}>{answerWords(m)}</option>{/each}</select>
          </label>
          <label>Marking note <input type="text" bind:value={f.marked_note} /></label>
        </div>
      </fieldset>

      <fieldset>
        <legend>Utility locate</legend>
        <div class="grid">
          <label>Locate ticket <input type="text" bind:value={f.ticket} data-testid="request-ticket" /></label>
          <label>Called in on <input type="date" bind:value={f.called_on} /></label>
          <label>Called in by <input type="text" bind:value={f.called_by} /></label>
          <label class="wide">Utilities called <input type="text" bind:value={f.utilities} placeholder="City of Vancouver, NW Natural, …" /></label>
        </div>
      </fieldset>

      <fieldset>
        <legend>Work order</legend>
        <div class="grid">
          <label>System <input type="text" bind:value={f.wo_system} placeholder="Maintenance" data-testid="request-wo-system" /></label>
          <label>Work order id <input type="text" bind:value={f.wo_id} data-testid="request-wo-id" /></label>
          <label>Link <input type="url" bind:value={f.wo_url} /></label>
          <label>Its status <input type="text" bind:value={f.wo_status} data-testid="request-wo-status" /></label>
          <label>Status read on <input type="date" bind:value={f.wo_read_on} /></label>
        </div>
      </fieldset>

      <fieldset>
        <legend>Completion</legend>
        <div class="grid">
          <label>Completed on <input type="date" bind:value={f.completed_on} data-testid="request-completed-on" /></label>
          <label>Completed by <input type="text" bind:value={f.completed_by} /></label>
        </div>
      </fieldset>

      {#if attachments}
        <Attachments bind:items={f.attachments} onadd={attachments.onadd} src={attachments.src} testid="request-attachments" />
      {/if}
      <label class="note">Note <input type="text" bind:value={f.note} data-testid="request-note" /></label>
      {#if refusals.length}
        <ul class="refused" data-testid="request-refused">
          {#each refusals as r, i (i)}<li>Not saved: {requestProblemWords(r.problem)}.</li>{/each}
        </ul>
      {/if}
      <div class="row">
        <button type="submit" disabled={busy || !f.id.trim()} data-testid="request-save">Save request</button>
        <button type="button" onclick={() => (editing = null)}>Cancel</button>
      </div>
    </form>
  {/if}
</section>

<style>
  .head { display: flex; justify-content: space-between; align-items: baseline; gap: 0.6rem; }
  h3 { font-size: 0.95rem; margin: 0.8rem 0 0.3rem; }
  form h3 { margin-top: 0; }
  .list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 0.2rem; }
  .list li { display: flex; flex-wrap: wrap; gap: 0.2rem 0.6rem; align-items: baseline; }
  .status.cancelled { color: var(--muted); text-decoration: line-through; }
  .status.completed { color: var(--ok); }
  .status.ordered, .status.submitted { color: var(--accent); }
  .wide-form { display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.5rem; padding: 0.6rem; border: 1px solid var(--line); border-radius: 6px; }
  fieldset { border: 1px solid var(--line); border-radius: 6px; margin: 0; padding: 0.3rem 0.6rem 0.5rem; min-width: 0; }
  legend { padding: 0 0.25rem; font-size: 0.85rem; color: var(--muted); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(17rem, 1fr)); gap: 0.3rem 1.2rem; }
  label { display: grid; grid-template-columns: 8.5rem minmax(0, 1fr); align-items: center; gap: 0.5rem; font-size: 0.85rem; min-width: 0; }
  label.wide { grid-column: 1 / -1; }
  input, select { font: inherit; font-size: 0.9rem; width: 100%; min-width: 0; }
  input[readonly] { color: var(--muted); }
  .row { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .refused { color: var(--bad); padding-left: 1rem; margin: 0; }
  .link { background: none; border: none; padding: 0; color: var(--accent); cursor: pointer; text-decoration: underline; font: inherit; }
  @media (pointer: coarse) {
    input, select { font-size: 1rem; min-height: 2.5rem; }
    .row button { min-height: 2.75rem; padding: 0 0.9rem; }
  }
</style>
