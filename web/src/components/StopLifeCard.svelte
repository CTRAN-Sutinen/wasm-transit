<script lang="ts">
  /// One stop in the stops document (cap-stops `stop`, D-157): where it
  /// stands on the date, its details and amenities, the routes that visit
  /// it in the schedule running that day, and its history. Its changes:
  /// what it may become that day (the op's `next` — only those are
  /// offered), a move, a change of details, and a correction of an event
  /// that was wrong. Each is one event the host writes. A RENDER
  /// component: the stop, its events, and its choices come in as props.
  import type { Details } from "../types/Details";
  import type { Event } from "../types/Event";
  import type { Refusal } from "../types/Refusal";
  import type { StopDetail } from "../types/StopDetail";
  import type { StopOn } from "../types/StopOn";
  import { ACTION_WORDS, AMENITIES, EVENT_WORDS, STATUS_WORDS, amenityWords, problemWords } from "./stopWords";

  let {
    stopId,
    card,
    served,
    date,
    busy,
    refusals,
    moving,
    point,
    onevent,
    oncorrect,
    onmove,
    onclose,
  }: {
    stopId: string;
    card: { stop: StopOn | null; events: Event[]; next: string[] };
    /// The routes visiting it in the schedule running on the date; null
    /// when that schedule does not have the stop.
    served: StopDetail | null;
    date: string;
    busy: boolean;
    refusals: Refusal[];
    /// The map is choosing the stop's new point, and the point chosen.
    moving: boolean;
    point: { lat: number; lon: number } | null;
    onevent: (event: Event, done: string) => void;
    oncorrect: (index: number, event: Event | null, done: string) => void;
    onmove: (start: boolean) => void;
    onclose: () => void;
  } = $props();

  const s = $derived(card.stop);
  const name = $derived(s?.name || card.events.find((e) => e.name)?.name || stopId);
  const amenities = $derived(Object.entries(s?.amenities ?? {}));
  const yesNo = (v: boolean | undefined) => (v == null ? "not recorded" : v ? "yes" : "no");

  /// The open form: a lifecycle event, "move", "details", or "fix-<i>".
  let form = $state("");
  let on = $state("");
  let until = $state("");
  let note = $state("");
  // the details form
  let dName = $state("");
  let dCode = $state("");
  let dWheelchair = $state("");
  let dAmenities = $state<Record<string, string>>({});

  function open(which: string) {
    form = which;
    on = date;
    until = "";
    note = "";
    if (which === "details") {
      dName = s?.name ?? "";
      dCode = s?.code ?? "";
      dWheelchair = s?.wheelchair == null ? "" : s.wheelchair ? "yes" : "no";
      dAmenities = Object.fromEntries(AMENITIES.map(([k]) => [k, s?.amenities?.[k] == null ? "" : s.amenities[k] ? "yes" : "no"]));
    }
    if (which.startsWith("fix-")) {
      const e = card.events[Number(which.slice(4))];
      on = e.on ?? "";
      until = e.until ?? "";
      note = e.note ?? "";
    }
    onmove(which === "move");
  }

  function cancel() {
    form = "";
    onmove(false);
  }

  const withNote = (e: Event): Event => (note.trim() ? { ...e, note: note.trim() } : e);

  function submitLifecycle(kind: string) {
    const e: Event = withNote({ on, event: kind as Event["event"], ...(kind === "suspended" && until ? { until } : {}) });
    onevent(e, `${ACTION_WORDS[kind]}: ${stopId} from ${on}${kind === "suspended" && until ? ` until ${until}` : ""}.`);
    form = "";
  }

  function submitMove() {
    if (!point) return;
    const e: Event = withNote({ on, event: "moved", lat: point.lat, lon: point.lon, ...(until ? { until } : {}) });
    onevent(e, `Moved ${stopId} from ${on}${until ? ` until ${until}` : ""}.`);
    form = "";
  }

  function submitDetails() {
    // Only what changed is set (D-153: every detail changes by an event).
    const details: Details = {};
    if (dName.trim() && dName.trim() !== (s?.name ?? "")) details.name = dName.trim();
    if (dCode.trim() && dCode.trim() !== (s?.code ?? "")) details.code = dCode.trim();
    const was = s?.wheelchair == null ? "" : s.wheelchair ? "yes" : "no";
    if (dWheelchair && dWheelchair !== was) details.wheelchair = dWheelchair === "yes";
    const amen: Record<string, boolean> = {};
    for (const [k] of AMENITIES) {
      const now = dAmenities[k];
      const before = s?.amenities?.[k] == null ? "" : s.amenities[k] ? "yes" : "no";
      if (now && now !== before) amen[k] = now === "yes";
    }
    if (Object.keys(amen).length) details.amenities = amen;
    if (!Object.keys(details).length) return void (form = "");
    onevent(withNote({ on, event: "changed", ...details }), `Changed ${stopId} from ${on}.`);
    form = "";
  }

  function submitFix(i: number) {
    const e = card.events[i];
    const fixed: Event = { ...e, ...(e.on || on ? { on: on || undefined } : {}) };
    if (until) fixed.until = until;
    else delete fixed.until;
    if (note.trim()) fixed.note = note.trim();
    else delete fixed.note;
    oncorrect(i, fixed, `Corrected ${EVENT_WORDS[e.event].toLowerCase()} on ${stopId}.`);
    form = "";
  }

  /// What an event set, in a few words.
  function what(e: Event): string {
    const parts: string[] = [];
    if (e.until) parts.push(`until ${e.until}`);
    if (e.event === "moved" && e.lat != null && e.lon != null) parts.push(`to ${e.lat.toFixed(5)}, ${e.lon.toFixed(5)}`);
    if (e.name && e.event === "changed") parts.push(`name "${e.name}"`);
    if (e.code && e.event === "changed") parts.push(`code ${e.code}`);
    if (e.wheelchair != null && e.event === "changed") parts.push(`wheelchair boarding ${yesNo(e.wheelchair)}`);
    for (const [k, v] of Object.entries(e.amenities ?? {})) parts.push(`${amenityWords(k).toLowerCase()} ${v ? "there" : "not there"}`);
    return parts.join(", ");
  }

  /// A suspension and a move may be temporary (the move form is
  /// "move", its event "moved").
  const canUntil = (kind: string) => kind === "suspended" || kind === "moved" || kind === "move";
</script>

<section class="stop-card" data-testid="stop-card">
  <div class="head">
    <h2>{name}</h2>
    <button type="button" class="link" onclick={onclose}>Close</button>
  </div>
  <p class="muted">Id {stopId}{s?.code ? ` · code ${s.code}` : ""}</p>

  {#if !s}
    <p data-testid="stop-status"><strong>Not there yet</strong> on {date}.</p>
  {:else}
    <p data-testid="stop-status">
      <strong class="status {s.status}">{STATUS_WORDS[s.status]}</strong>
      {#if s.since}<span>since {s.since}</span>{/if}{#if s.until}<span>, back {s.until}</span>{/if}
      {#if s.note}<span class="muted">— {s.note}</span>{/if}
    </p>
    <dl>
      <dt>Location</dt><dd>{s.lat.toFixed(5)}, {s.lon.toFixed(5)}</dd>
      <dt>Wheelchair boarding</dt><dd>{yesNo(s.wheelchair)}</dd>
      {#if s.fare_zone}<dt>Fare zone</dt><dd>{s.fare_zone}</dd>{/if}
      {#if s.description}<dt>Description</dt><dd>{s.description}</dd>{/if}
    </dl>
    <h3>At the stop</h3>
    {#if amenities.length}
      <ul class="amenities" data-testid="stop-amenities">
        {#each amenities as [k, v] (k)}<li class:absent={!v}>{amenityWords(k)}: {v ? "there" : "not there"}</li>{/each}
      </ul>
    {:else}
      <p class="muted">Nothing recorded.</p>
    {/if}
  {/if}

  <h3>Routes that stop here</h3>
  {#if served?.routes.length}
    <p data-testid="stop-routes">{served.routes.map((r) => r.name).join(", ")}</p>
    <p class="muted small">In the schedule running on {date}.</p>
  {:else}
    <p class="muted" data-testid="stop-routes">No route stops here in the schedule running on {date}.</p>
  {/if}

  {#if s?.status !== "retired"}
    <h3>Record a change on {date}</h3>
    <div class="actions">
      {#each card.next as kind (kind)}
        <button type="button" class:on={form === kind} disabled={busy} onclick={() => open(kind)} data-testid="action-{kind}">{ACTION_WORDS[kind]}</button>
      {/each}
      {#if s}
        <button type="button" class:on={form === "move"} disabled={busy} onclick={() => open("move")} data-testid="action-move">Move</button>
        <button type="button" class:on={form === "details"} disabled={busy} onclick={() => open("details")} data-testid="action-details">Change details</button>
      {/if}
    </div>
  {/if}

  {#if form && !form.startsWith("fix-")}
    <form
      class="edit"
      data-testid="stop-form"
      onsubmit={(e) => {
        e.preventDefault();
        if (form === "move") submitMove();
        else if (form === "details") submitDetails();
        else submitLifecycle(form);
      }}
    >
      {#if form === "move"}
        <p>{moving && !point ? "Click the map where the stop goes." : point ? `New point: ${point.lat.toFixed(5)}, ${point.lon.toFixed(5)}` : ""}</p>
      {/if}
      <label>From <input type="date" bind:value={on} required data-testid="form-on" /></label>
      {#if canUntil(form)}
        <label>Until <input type="date" bind:value={until} data-testid="form-until" /></label>
        <p class="muted small">{form === "suspended" ? "The first day back in service. Leave it empty if the stop's return is not known." : "For a temporary move: the first day back at the old point."}</p>
      {/if}
      {#if form === "details"}
        <label>Name <input type="text" bind:value={dName} data-testid="form-name" /></label>
        <label>Code <input type="text" bind:value={dCode} data-testid="form-code" /></label>
        <label>Wheelchair boarding
          <select bind:value={dWheelchair}><option value="">not recorded</option><option value="yes">yes</option><option value="no">no</option></select>
        </label>
        <fieldset>
          <legend>At the stop</legend>
          {#each AMENITIES as [k, label] (k)}
            <label>{label}
              <select bind:value={dAmenities[k]} data-testid="form-amenity-{k}"><option value="">not recorded</option><option value="yes">there</option><option value="no">not there</option></select>
            </label>
          {/each}
        </fieldset>
      {/if}
      <label>Note <input type="text" bind:value={note} placeholder="Why, in a few words" data-testid="form-note" /></label>
      <div class="row">
        <button type="submit" disabled={busy || !on || (form === "move" && !point)} data-testid="form-save">Save</button>
        <button type="button" onclick={cancel}>Cancel</button>
      </div>
    </form>
  {/if}

  {#if refusals.length}
    <ul class="refused" data-testid="stop-refused">
      {#each refusals as r, i (i)}<li>Not saved: {problemWords(r.problem.kind)}.</li>{/each}
    </ul>
  {/if}

  <h3>History</h3>
  <ol class="events" data-testid="stop-history">
    {#each card.events as e, i (i)}
      <li data-testid="stop-event">
        <span class="when">{e.on ?? "before records"}</span>
        <span>{EVENT_WORDS[e.event] ?? e.event}{what(e) ? ` ${what(e)}` : ""}</span>
        {#if e.note}<span class="muted">— {e.note}</span>{/if}
        <button type="button" class="link" disabled={busy} onclick={() => open(`fix-${i}`)} data-testid="fix-{i}">Fix</button>
        {#if form === `fix-${i}`}
          <form
            class="edit fix"
            onsubmit={(ev) => {
              ev.preventDefault();
              submitFix(i);
            }}
          >
            {#if e.on || i > 0}<label>Date <input type="date" bind:value={on} required data-testid="fix-on" /></label>{/if}
            {#if canUntil(e.event)}<label>Until <input type="date" bind:value={until} data-testid="fix-until" /></label>{/if}
            <label>Note <input type="text" bind:value={note} data-testid="fix-note" /></label>
            <div class="row">
              <button type="submit" disabled={busy} data-testid="fix-save">Save the correction</button>
              <button type="button" disabled={busy} onclick={() => ((form = ""), oncorrect(i, null, `Removed an event entered by mistake on ${stopId}.`))} data-testid="fix-remove">Remove it</button>
              <button type="button" onclick={cancel}>Cancel</button>
            </div>
            <p class="muted small">A correction fixes an event entered wrong. A real change on a later day is recorded as a new change instead.</p>
          </form>
        {/if}
      </li>
    {/each}
  </ol>
</section>

<style>
  .head { display: flex; justify-content: space-between; align-items: baseline; gap: 0.6rem; }
  h2 { margin: 0; font-size: 1.05rem; }
  h3 { font-size: 0.95rem; margin: 0.8rem 0 0.3rem; }
  p { margin: 0.2rem 0; }
  .small { font-size: 0.8rem; }
  .status.suspended, .status.retired { color: var(--bad); }
  .status.planned, .status.permitted, .status.built { color: var(--accent); }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 0.2rem 0.8rem; margin: 0.4rem 0; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  .amenities { list-style: none; padding: 0; margin: 0; display: flex; flex-wrap: wrap; gap: 0.2rem 0.8rem; }
  .amenities .absent { color: var(--muted); }
  .actions { display: flex; flex-wrap: wrap; gap: 0.35rem; }
  .actions button.on { outline: 2px solid var(--accent); }
  .edit { display: flex; flex-direction: column; gap: 0.35rem; margin-top: 0.5rem; padding: 0.5rem; border: 1px solid var(--line); border-radius: 6px; }
  .edit label { display: flex; justify-content: space-between; gap: 0.6rem; align-items: center; }
  .edit input[type="text"] { flex: 1; min-width: 0; }
  .edit fieldset { border: 1px solid var(--line); border-radius: 6px; display: flex; flex-direction: column; gap: 0.25rem; }
  .row { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .refused { color: var(--bad); padding-left: 1rem; margin: 0.4rem 0 0; }
  .events { list-style: none; padding: 0; margin: 0; }
  .events li { display: flex; flex-wrap: wrap; gap: 0.2rem 0.6rem; align-items: baseline; padding: 0.15rem 0; }
  .when { font-variant-numeric: tabular-nums; min-width: 6.5rem; color: var(--muted); }
  .fix { flex-basis: 100%; }
  .link { background: none; border: none; padding: 0; color: var(--accent); cursor: pointer; text-decoration: underline; font: inherit; }
</style>
