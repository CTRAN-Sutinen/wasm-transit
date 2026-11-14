<script lang="ts">
  /// The work across every stop (cap-requests `list`, D-159), as the
  /// table a planning team works from: each request's id, stop, job,
  /// status, request date, work order, and note; the statuses (with
  /// their counts, every request whatever is shown) and jobs as filters
  /// above it; a column heading sorts by it; a row opens the request. A
  /// RENDER component: the requests, the counts, and the stops they
  /// concern come in as props.
  import type { IdRequest } from "../types/cap_requests/IdRequest";
  import type { RequestCount } from "../types/RequestCount";
  import type { StopOn } from "../types/StopOn";
  import { jobWords, requestStatusWords } from "./stopWords";

  let {
    requests,
    counts,
    stops,
    statuses,
    jobs,
    vocab,
    onfilter,
    onopen,
  }: {
    requests: IdRequest[];
    counts: RequestCount[];
    /// The requests' stops on the date, to name each.
    stops: StopOn[];
    statuses: string[];
    jobs: string[];
    vocab: { jobs: string[]; statuses: string[] };
    onfilter: (statuses: string[], jobs: string[]) => void;
    onopen: (stop: string, request: string) => void;
  } = $props();

  const named = $derived(new Map(stops.map((s) => [s.stop_id, s.name || s.stop_id])));
  const count = (s: string) => counts.find((c) => c.status === s)?.requests ?? 0;
  const toggle = (list: string[], v: string, order: string[]) => (list.includes(v) ? list.filter((x) => x !== v) : order.filter((x) => x === v || list.includes(x)));

  type Col = "id" | "stop" | "job" | "status" | "requested" | "wo";
  let by = $state<Col>("requested");
  let down = $state(true);
  const key = (r: IdRequest, c: Col): string =>
    c === "id" ? r.id : c === "stop" ? (named.get(r.request.stop) ?? r.request.stop) : c === "job" ? jobWords(r.request.job) : c === "status" ? String(vocab.statuses.indexOf(r.request.status)).padStart(2, "0") : c === "requested" ? (r.request.requested_on ?? "") : (r.request.work_order?.id ?? "");
  const rows = $derived([...requests].sort((a, b) => (down ? -1 : 1) * key(a, by).localeCompare(key(b, by)) || a.id.localeCompare(b.id)));
  const sort = (c: Col) => (by === c ? (down = !down) : ((by = c), (down = c === "requested")));
  const COLS: [Col, string][] = [["id", "Request"], ["stop", "Stop"], ["job", "Job"], ["status", "Status"], ["requested", "Requested"], ["wo", "Work order"]];
</script>

<section class="work" data-testid="work-queue">
  <header class="bar">
    <h1>Work</h1>
    <div class="filters" role="group" aria-label="Status">
      {#each vocab.statuses as s (s)}
        <button type="button" class="filter" class:on={statuses.includes(s)} aria-pressed={statuses.includes(s)} onclick={() => onfilter(toggle(statuses, s, vocab.statuses), jobs)} data-testid="work-filter-{s}">
          {requestStatusWords(s)}<span class="n">{count(s)}</span>
        </button>
      {/each}
    </div>
    <div class="filters" role="group" aria-label="Job">
      <button type="button" class="filter" class:on={!jobs.length} aria-pressed={!jobs.length} onclick={() => onfilter(statuses, [])} data-testid="work-job-all">All jobs</button>
      {#each vocab.jobs as j (j)}
        <button type="button" class="filter" class:on={jobs.includes(j)} aria-pressed={jobs.includes(j)} onclick={() => onfilter(statuses, toggle(jobs, j, vocab.jobs))} data-testid="work-job-{j}">{jobWords(j)}</button>
      {/each}
    </div>
    <span class="muted total">{requests.length} {requests.length === 1 ? "request" : "requests"}</span>
  </header>

  <div class="scroll">
    <table>
      <thead>
        <tr>
          {#each COLS as [c, label] (c)}
            <th aria-sort={by === c ? (down ? "descending" : "ascending") : undefined}><button type="button" onclick={() => sort(c)}>{label}{by === c ? (down ? " ↓" : " ↑") : ""}</button></th>
          {/each}
          <th>Notes</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as r (r.id)}
          <tr onclick={() => onopen(r.request.stop, r.id)} data-testid="work-row">
            <td><button type="button" class="link" onclick={(e) => (e.stopPropagation(), onopen(r.request.stop, r.id))}>{r.id}</button></td>
            <td><strong>{named.get(r.request.stop) ?? r.request.stop}</strong> <span class="muted">{r.request.stop}</span></td>
            <td>{jobWords(r.request.job)}</td>
            <td><span class="req-status {r.request.status}">{requestStatusWords(r.request.status)}</span></td>
            <td class="num">{r.request.requested_on ?? "—"}</td>
            <td>{r.request.work_order?.id ?? "—"}{r.request.work_order?.status ? ` · ${r.request.work_order.status}` : ""}</td>
            <td class="note">{r.request.note ?? ""}</td>
          </tr>
        {:else}
          <tr><td colspan="7" class="muted">No matching requests.</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<style>
  .work { display: grid; grid-template-rows: auto minmax(0, 1fr); height: 100%; min-height: 0; }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 0.6rem 1.2rem; padding: 0.9rem 1.6rem; border-bottom: 1px solid var(--line); }
  h1 { margin: 0; font-size: 1.3rem; }
  .total { margin-left: auto; }
  .filters { display: flex; flex-wrap: wrap; gap: 0.3rem; }
  .filter {
    display: inline-flex; align-items: center; gap: 0.4rem; padding: 0.2rem 0.65rem; min-height: 2.1rem;
    border: 1px solid var(--line); border-radius: 6px; background: transparent; color: var(--muted); font: inherit; font-size: 0.86rem; cursor: pointer;
  }
  .filter:hover { background: var(--hover); }
  .filter.on { background: var(--hover); color: var(--text); border-color: color-mix(in srgb, var(--text) 30%, var(--line)); }
  .n { font-size: 0.75rem; color: var(--muted); font-variant-numeric: tabular-nums; }
  .scroll { overflow: auto; padding: 0 1.6rem 2rem; }
  table { width: 100%; border-collapse: collapse; font-size: 0.92rem; }
  thead th { position: sticky; top: 0; background: var(--surface); text-align: left; border-bottom: 1px solid var(--line); padding: 0; }
  th button { width: 100%; text-align: left; padding: 0.6rem 0.6rem; border: 0; background: none; color: var(--muted); font: inherit; font-size: 0.82rem; font-weight: 600; cursor: pointer; }
  thead th:last-child { padding: 0.6rem; color: var(--muted); font-size: 0.82rem; }
  td { padding: 0.55rem 0.6rem; border-bottom: 1px solid var(--line); vertical-align: top; }
  tbody tr { cursor: pointer; }
  tbody tr:hover { background: var(--hover); }
  .num { font-variant-numeric: tabular-nums; white-space: nowrap; }
  .note { color: var(--muted); max-width: 28rem; }
  .link { background: none; border: 0; padding: 0; color: var(--accent); font: inherit; cursor: pointer; text-decoration: underline; }
  .req-status { font-size: 0.82rem; padding: 0.05rem 0.5rem; border-radius: 999px; background: var(--hover); white-space: nowrap; }
  .req-status.submitted, .req-status.ordered { background: var(--accent-soft); }
  .req-status.completed { color: var(--ok); }
  .req-status.cancelled { color: var(--muted); text-decoration: line-through; }
  @media (max-width: 900px) {
    .bar, .scroll { padding-left: 0.9rem; padding-right: 0.9rem; }
    thead { display: none; }
    tbody tr { display: grid; grid-template-columns: 1fr auto; padding: 0.5rem 0; border-bottom: 1px solid var(--line); }
    td { border: 0; padding: 0.1rem 0; }
    td:nth-child(1), td:nth-child(5), td:nth-child(6) { font-size: 0.82rem; color: var(--muted); }
    .note { grid-column: 1 / -1; }
  }
</style>
