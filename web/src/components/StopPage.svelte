<script lang="ts">
  /// One stop in the stops document (cap-stops `stop`, D-157) as a page:
  /// its header (name, id and code, where it stands on the date, and the
  /// things done at a stop — Survey, Request work, Report issue, Record
  /// change, the last offering only what the stop may become that day,
  /// the op's `next`, and a move and new details), and its record below
  /// under headings, in as many columns as the screen holds: Location
  /// (the point, on a map of the stops around it), Details, Amenities,
  /// Routes (in the schedule running that day), Open work (its requests,
  /// D-159), Last survey (D-158), and History (every event, newest first,
  /// each correctable). A short change opens as a card under the header
  /// (the page places it, `change`). A RENDER component.
  import "./forms.css";
  import type { Snippet } from "svelte";
  import type { Event } from "../types/Event";
  import type { Neighbour } from "../types/Neighbour";
  import type { StopDetail } from "../types/StopDetail";
  import type { StopOn } from "../types/StopOn";
  import type { SiteQuestion } from "../types/cap_stops/SiteQuestion";
  import type { IdRequest } from "../types/cap_requests/IdRequest";
  import AttachmentThumbs from "./AttachmentThumbs.svelte";
  import { EVENT_WORDS, SITE_GROUPS, STATUS_COLORS, STATUS_WORDS, amenityWords, answerWords, changeWords, jobWords, refWords, requestStatusWords, siteWords } from "./stopWords";

  let {
    stopId,
    card,
    served,
    date,
    busy,
    questions,
    requests,
    draft,
    attachments = null,
    map,
    change,
    backLabel,
    ontask,
    onrequest,
    onback,
  }: {
    stopId: string;
    card: { stop: StopOn | null; events: Event[]; next: string[] };
    served: StopDetail | null;
    date: string;
    busy: boolean;
    questions: { site: SiteQuestion[]; amenities: string[] };
    requests: IdRequest[];
    /// A survey draft is kept on this device for the stop.
    draft: boolean;
    attachments?: { src: (hash: string) => Promise<string | null> } | null;
    /// The map of the stop and the stops around it (the page's).
    map: Snippet;
    /// The change under way, placed under the header (the page's).
    change?: Snippet;
    backLabel: string;
    /// A task begun: "change:<kind>", "fix:<index>", "survey",
    /// "request", or "report".
    ontask: (task: string) => void;
    onrequest: (id: string) => void;
    onback: () => void;
  } = $props();

  const s = $derived(card.stop);
  const name = $derived(s?.name || [...card.events].reverse().find((e) => e.name)?.name || stopId);
  const amenities = $derived(Object.entries(s?.amenities ?? {}));
  const siteGroups = $derived.by(() => {
    const site = s?.site ?? {};
    const named = new Set(SITE_GROUPS.flatMap((g) => g.keys));
    const groups = SITE_GROUPS.map((g) => ({ title: g.title, rows: g.keys.filter((k) => site[k]).map((k) => [k, site[k]] as [string, string]) }));
    const other = Object.entries(site).filter(([k, v]) => v && !named.has(k));
    return [...groups, { title: "Other", rows: other }].filter((g) => g.rows.length);
  });
  /// The stop's visits as a person reads them: one line per route,
  /// direction, and the stops either side (patterns that agree on those
  /// read as one).
  const visits = $derived.by(() => {
    const seen = new Map<string, { key: string; route: string; direction: string; timepoint: boolean; previous: Neighbour | null; next: Neighbour | null }>();
    for (const v of served?.visits ?? []) {
      const key = [v.route, v.direction ?? "", v.previous?.stop ?? "", v.next?.stop ?? ""].join("|");
      const was = seen.get(key);
      if (was) was.timepoint ||= v.timepoint;
      else seen.set(key, { key, route: v.route_name, direction: v.direction_name ?? "", timepoint: v.timepoint, previous: v.previous, next: v.next });
    }
    return [...seen.values()];
  });
  const metres = (n: Neighbour) => `${n.published ? "" : "~"}${n.metres < 1000 ? `${Math.round(n.metres)} m` : `${(n.metres / 1000).toFixed(1)} km`}`;
  const refs = $derived(Object.entries(s?.refs ?? {}).filter(([, v]) => v));
  const answerOf = (key: string, v: string) => (questions.site.find((q) => q.key === key)?.answer.kind === "one_of" ? answerWords(v) : v);
  const yesNo = (v: boolean | undefined) => (v == null ? "Not recorded" : v ? "Yes" : "No");
  const open = $derived(requests.filter((r) => r.request.status !== "completed" && r.request.status !== "cancelled"));
  const closed = $derived(requests.filter((r) => r.request.status === "completed" || r.request.status === "cancelled"));
  const changes = $derived(s && s.status !== "retired" ? [...card.next.filter((k) => k !== "retired"), "move", "details", ...card.next.filter((k) => k === "retired")] : card.next);
  let choosing = $state(false);
  let allHistory = $state(false);

  function what(e: Event): string {
    const parts: string[] = [];
    if (e.until) parts.push(`until ${e.until}`);
    if (e.request) parts.push(`(request ${e.request})`);
    if (e.event === "moved" && e.lat != null && e.lon != null) parts.push(`to ${e.lat}, ${e.lon}`);
    if (e.name && e.event === "changed") parts.push(`name "${e.name}"`);
    if (e.code && e.event === "changed") parts.push(`code ${e.code}`);
    if (e.wheelchair != null && e.event === "changed") parts.push(`wheelchair boarding ${yesNo(e.wheelchair).toLowerCase()}`);
    if (e.surveyed_by && e.event === "changed") {
      const n = Object.keys(e.site ?? {}).length + Object.keys(e.amenities ?? {}).length + Object.keys(e.refs ?? {}).length;
      parts.push(`by ${e.surveyed_by}: ${n} answer${n === 1 ? "" : "s"}`);
      return parts.join(", ");
    }
    for (const [k, v] of Object.entries(e.amenities ?? {})) parts.push(`${amenityWords(k).toLowerCase()} ${v ? "there" : "not there"}`);
    for (const [k, v] of Object.entries(e.site ?? {})) parts.push(`${siteWords(k).toLowerCase()} ${answerOf(k, v)}`);
    for (const [k, v] of Object.entries(e.refs ?? {})) parts.push(`${refWords(k)} ${v}`);
    return parts.join(", ");
  }
  const history = $derived([...card.events.keys()].reverse());
</script>

<article class="stop-page" data-testid="stop-card">
  <header class="top">
    <button type="button" class="f-link back" onclick={onback} data-testid="stop-close">← {backLabel}</button>
    <div class="title-row">
      <div class="titles">
        <h1>{name}</h1>
        <p class="standing" data-testid="stop-status">
          <span class="muted">Stop {stopId}{s?.code && s.code !== stopId ? ` · code ${s.code}` : ""}</span>
          {#if !s}
            <strong>Not there yet</strong> on {date}.
          {:else}
            <span class="badge" style="--fill:{STATUS_COLORS[s.status][0]};--edge:{STATUS_COLORS[s.status][1]}">{STATUS_WORDS[s.status]}</span>
            {#if s.since || s.until}<span>{[s.since && `since ${s.since}`, s.until && `until ${s.until}`].filter(Boolean).join(", ")}</span>{/if}
            {#if s.note}<span class="muted">— {s.note}</span>{/if}
          {/if}
        </p>
      </div>
      {#if s?.status !== "retired" && (s || card.next.length)}
        <div class="actions">
          {#if s}<button type="button" class="act primary" disabled={busy} onclick={() => ontask("survey")} data-testid="action-survey">{draft ? "Continue survey" : "Survey"}</button>{/if}
          <div class="menu">
            <button type="button" class="act" class:on={choosing} aria-expanded={choosing} disabled={busy} onclick={() => (choosing = !choosing)} data-testid="action-change">Record change ▾</button>
            {#if choosing}
              <ul class="f-choices pop" data-testid="change-choices">
                {#each changes as kind (kind)}
                  {@const [label, line] = changeWords(kind, s?.status)}
                  <li><button type="button" onclick={() => ((choosing = false), ontask(`change:${kind}`))} data-testid="action-{kind}"><strong>{label}</strong>{#if line}<span>{line}</span>{/if}</button></li>
                {/each}
              </ul>
            {/if}
          </div>
          <button type="button" class="act" disabled={busy} onclick={() => ontask("request")} data-testid="action-request">Request work</button>
          {#if s}<button type="button" class="act" disabled={busy} onclick={() => ontask("report")} data-testid="action-report">Report issue</button>{/if}
        </div>
      {/if}
    </div>
  </header>

  <div class="body">
    {#if change}<div class="change">{@render change()}</div>{/if}
    <div class="grid">
      <section class="card loc">
        <h2>Location</h2>
        <div class="mini">{@render map()}</div>
        {#if s}<p class="coords" data-testid="stop-location">{s.lat}, {s.lon}</p>{/if}
      </section>

      {#if s}
        <section class="card">
          <h2>Details</h2>
          <dl>
            <dt>Wheelchair boarding</dt><dd>{yesNo(s.wheelchair)}</dd>
            {#if s.fare_zone}<dt>Fare zone</dt><dd>{s.fare_zone}</dd>{/if}
            {#if s.description}<dt>Description</dt><dd>{s.description}</dd>{/if}
            {#if refs.length}{#each refs as [k, v] (k)}<dt>{refWords(k)}</dt><dd>{v}</dd>{/each}{/if}
          </dl>
          <h3>Amenities</h3>
          {#if amenities.length}
            <ul class="amenities" data-testid="stop-amenities">{#each amenities as [k, v] (k)}<li class:absent={!v}>{v ? "✓" : "✗"} {amenityWords(k)}</li>{/each}</ul>
          {:else}
            <p class="muted">None recorded.</p>
          {/if}
        </section>
      {/if}

      <section class="card">
        <h2>Routes</h2>
        {#if served?.routes.length}
          <p data-testid="stop-routes"><strong>{served.routes.map((r) => r.name).join(", ")}</strong></p>
          {#if visits.length}
            <table class="visits" data-testid="stop-visits">
              <thead><tr><th>Route</th><th>Previous</th><th>Next</th></tr></thead>
              <tbody>
                {#each visits as v (v.key)}
                  <tr data-testid="stop-visit">
                    <td><strong>{v.route}</strong> {v.direction}{v.timepoint ? " · time point" : ""}</td>
                    <td>{v.previous ? `after ${v.previous.name} (${metres(v.previous)})` : "first stop"}</td>
                    <td>{v.next ? `before ${v.next.name} (${metres(v.next)})` : "last stop"}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
          <p class="muted small">Schedule in effect {date}.</p>
        {:else}
          <p class="muted" data-testid="stop-routes">No routes in the schedule in effect {date}.</p>
        {/if}
      </section>

      <section class="card" data-testid="stop-requests">
        <h2>Work <span class="count">{open.length} open</span></h2>
        {#if requests.length}
          <table class="requests">
            <thead><tr><th>Request</th><th>Job</th><th>Status</th><th>Work order</th></tr></thead>
            <tbody>
              {#each [...open, ...closed] as r (r.id)}
                <tr class="link-row" onclick={() => onrequest(r.id)} data-testid="request-row">
                  <td><button type="button" class="f-link" onclick={(e) => (e.stopPropagation(), onrequest(r.id))}>{r.id}</button></td>
                  <td>{jobWords(r.request.job)}</td>
                  <td><span class="req-status {r.request.status}">{requestStatusWords(r.request.status)}</span></td>
                  <td>{r.request.work_order?.id ?? "—"}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {:else}
          <p class="muted">No requests.</p>
        {/if}
      </section>

      <section class="card">
        <h2>Last survey</h2>
        <p class="muted" data-testid="stop-surveyed">
          {s?.surveyed_by ? `Last surveyed ${s.surveyed_on ? `on ${s.surveyed_on} ` : ""}by ${s.surveyed_by}.` : "Never surveyed."}
          {#if draft}<strong class="draft-note">Unsaved survey draft on this device.</strong>{/if}
        </p>
        {#each siteGroups as g (g.title)}
          <h3>{g.title}</h3>
          <dl class="site" data-testid="stop-site">{#each g.rows as [k, v] (k)}<dt>{siteWords(k)}</dt><dd>{answerOf(k, v)}</dd>{/each}</dl>
        {/each}
        {#if refs.length}
          <h3>External IDs</h3>
          <dl class="site" data-testid="stop-refs">{#each refs as [k, v] (k)}<dt>{refWords(k)}</dt><dd>{v}</dd>{/each}</dl>
        {/if}
      </section>

      <section class="card wide">
        <h2>History <span class="count">{card.events.length}</span></h2>
        <table class="events" data-testid="stop-history">
          <thead><tr><th>Date</th><th>Event</th><th>Detail</th><th></th></tr></thead>
          <tbody>
            {#each allHistory ? history : history.slice(0, 8) as i (i)}
              {@const e = card.events[i]}
              <tr data-testid="stop-event">
                <td class="when">{e.on ?? "Initial"}</td>
                <td><strong>{e.surveyed_by && e.event === "changed" ? "Surveyed" : (EVENT_WORDS[e.event] ?? e.event)}</strong></td>
                <td>
                  {what(e)}{#if e.note}<span class="muted"> — {e.note}</span>{/if}
                  {#if attachments && e.attachments?.length}<AttachmentThumbs items={e.attachments} src={attachments.src} />{/if}
                </td>
                <td class="fix"><button type="button" class="f-link" disabled={busy} onclick={() => ontask(`fix:${i}`)} data-testid="fix-{i}">Correct</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if history.length > 8 && !allHistory}<button type="button" class="f-link" onclick={() => (allHistory = true)}>Show all {history.length}</button>{/if}
      </section>
    </div>
  </div>
</article>

<style>
  .stop-page { display: grid; grid-template-rows: auto minmax(0, 1fr); height: 100%; min-height: 0; }
  .top { padding: 0.8rem 1.6rem 1rem; border-bottom: 1px solid var(--line); }
  .back { font-size: 0.88rem; }
  .title-row { display: flex; flex-wrap: wrap; align-items: flex-end; gap: 0.8rem 2rem; margin-top: 0.4rem; }
  .titles { flex: 1; min-width: 16rem; }
  h1 { margin: 0; font-size: 1.45rem; }
  .standing { margin: 0.3rem 0 0; display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; }
  .badge { display: inline-flex; align-items: center; gap: 0.35rem; padding: 0.05rem 0.6rem; border-radius: 999px; border: 1.5px solid var(--edge); font-weight: 600; font-size: 0.85rem; }
  .badge::before { content: ""; width: 8px; height: 8px; border-radius: 50%; background: var(--fill); border: 1.5px solid var(--edge); }
  .actions { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: flex-start; }
  .act { min-height: 2.5rem; padding: 0 1rem; border-radius: 6px; font-weight: 600; }
  .act.primary { background: var(--accent); color: var(--surface); border: 1px solid var(--accent); }
  .act.on { outline: 2px solid var(--accent); }
  .menu { position: relative; }
  .pop { position: absolute; z-index: 5; right: 0; top: calc(100% + 4px); width: 22rem; padding: 0.4rem; background: var(--surface); border: 1px solid var(--line); border-radius: 8px; box-shadow: 0 6px 20px rgb(0 0 0 / 0.3); }
  .body { overflow: auto; padding: 1.2rem 1.6rem 3rem; }
  .change { max-width: 48rem; margin: 0 0 1.2rem; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(28rem, 1fr)); gap: 1.2rem; align-items: start; }
  .card { border: 1px solid var(--line); border-radius: 8px; padding: 0.9rem 1.1rem; min-width: 0; }
  .card.wide { grid-column: 1 / -1; }
  h2 { margin: 0 0 0.7rem; font-size: 1rem; display: flex; align-items: baseline; gap: 0.6rem; }
  h3 { font-size: 0.85rem; color: var(--muted); margin: 0.9rem 0 0.35rem; }
  .count { font-size: 0.82rem; font-weight: 400; color: var(--muted); }
  .mini { height: 15rem; border-radius: 6px; overflow: hidden; }
  .coords { margin: 0.5rem 0 0; font-variant-numeric: tabular-nums; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 0.3rem 1rem; margin: 0; font-size: 0.92rem; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  p { margin: 0.2rem 0; }
  .small { font-size: 0.8rem; }
  .amenities { list-style: none; padding: 0; margin: 0; display: flex; flex-wrap: wrap; gap: 0.35rem; }
  .amenities li { padding: 0.15rem 0.6rem; border-radius: 999px; background: var(--hover); font-size: 0.85rem; }
  .amenities .absent { color: var(--muted); background: transparent; border: 1px dashed var(--line); }
  table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
  th { text-align: left; font-weight: 600; color: var(--muted); font-size: 0.8rem; padding: 0.3rem 0.5rem; border-bottom: 1px solid var(--line); }
  td { padding: 0.4rem 0.5rem; border-bottom: 1px solid var(--line); vertical-align: top; }
  .link-row { cursor: pointer; }
  .link-row:hover { background: var(--hover); }
  .req-status { font-size: 0.82rem; padding: 0 0.45rem; border-radius: 999px; background: var(--hover); white-space: nowrap; }
  .req-status.submitted, .req-status.ordered { background: var(--accent-soft); }
  .req-status.completed { color: var(--ok); }
  .req-status.cancelled { color: var(--muted); text-decoration: line-through; }
  .when { white-space: nowrap; font-variant-numeric: tabular-nums; color: var(--muted); }
  .fix { text-align: right; }
  .draft-note { display: block; margin-top: 0.3rem; color: var(--warn); }
  @media (max-width: 900px) {
    .top { padding: 0.6rem 0.9rem; }
    .body { padding: 0.9rem; }
    .grid { grid-template-columns: 1fr; }
    .actions { width: 100%; }
    .act { flex: 1; min-height: 2.9rem; }
    .menu { flex: 1; display: flex; }
    .pop { left: 0; right: auto; width: min(22rem, 90vw); }
    .visits thead { display: none; }
    .visits tr { display: grid; }
    .visits td { border: 0; padding: 0.1rem 0; }
    .visits tr { border-bottom: 1px solid var(--line); padding: 0.4rem 0; }
  }
</style>
