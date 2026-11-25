<script lang="ts">
  import { datesText, kindText, originText, stampText, timeText, titleText, whoText } from "./disruptionsFormat";
  import type { Disruption } from "../types/Disruption";
  import type { NoteRow } from "../types/NoteRow";
  import type { TripStopView } from "../types/TripStopView";

  let {
    disruption,
    route,
    tripStops,
    notes,
    zone,
    date,
    notesOff,
    busy,
    error,
    onclose,
    onnote,
    onstartalert,
  }: {
    disruption: Disruption | null;
    route: string;
    tripStops: TripStopView[];
    notes: NoteRow[];
    zone: string | undefined;
    date: string;
    notesOff: string;
    busy: boolean;
    error: string;
    onclose: () => void;
    onnote: (text: string, id: string | null) => Promise<boolean>;
    onstartalert: () => void;
  } = $props();

  let draft = $state("");
  let editing = $state<string | null>(null);
  let edited = $state("");
  async function add() {
    if (draft.trim() && (await onnote(draft.trim(), null))) draft = "";
  }
  async function saveEdit(id: string) {
    if (edited.trim() && (await onnote(edited.trim(), id))) editing = null;
  }
</script>

<section class="detail" data-testid="disruption-detail">
  <button type="button" class="back" onclick={onclose}>‹ Back</button>
  {#if !disruption}
    <p class="muted">Not in today's disruptions</p>
  {:else}
    {@const d = disruption}
    <header>
      {#if route}<span class="badge">{route}</span>{/if}
      <div>
        <h2>{kindText(d)}</h2>
        <span class="sub">{originText(d.from)}</span>
      </div>
    </header>
    <p class="title">{titleText(d)}</p>
    {#if d.headsign}<p class="sub">To {d.headsign}</p>{/if}
    {#if d.next_start_secs != null}<p class="sub">Next trip {timeText(d.next_start_secs)}</p>{/if}
    {#if datesText(d, date)}<p class="sub">{datesText(d, date)}</p>{/if}
    <p class="told" class:none={!d.explained_by.length} data-testid="detail-alert">
      {d.explained_by.length ? `Alert ${d.explained_by.join(", ")}` : "No alert"}
      {#if !d.explained_by.length}<button type="button" class="link" onclick={onstartalert} data-testid="detail-start-alert">Start alert</button>{/if}
    </p>

    {#if d.trip && tripStops.length}
      <h3>Stops not served <span class="count">{tripStops.length}</span></h3>
      <ol class="stops">
        {#each tripStops as s, i (i)}
          <li><span>{s.name}</span><span class="time">{timeText(s.departure_secs)}</span></li>
        {/each}
      </ol>
    {:else if !d.trip && d.stops?.length}
      <h3>{d.from === "stops" ? "Stop" : "Stops not served"} <span class="count">{d.stops.length}</span></h3>
      <ol class="stops">
        {#each d.stops as s (s.id)}
          <li><span>{s.name ?? s.id}</span><span class="time">{s.id}</span></li>
        {/each}
      </ol>
    {/if}

    <h3>Notes <span class="count">{notes.length}</span></h3>
    {#if notesOff}<p class="muted">{notesOff}</p>{:else if notes.length === 0}<p class="muted">No notes</p>{/if}
    <ul class="notes" data-testid="detail-notes">
      {#each notes as n (n.id)}
        <li>
          <span class="sub">{whoText(n.note.written.by)} · {stampText(n.note.written.at, zone, date)}</span>
          {#if editing === n.id}
            <textarea rows="3" bind:value={edited} aria-label="Note"></textarea>
            <span class="row">
              <button type="button" onclick={() => (editing = null)}>Cancel</button>
              <button type="button" class="primary" disabled={busy || !edited.trim()} onclick={() => saveEdit(n.id)}>Save</button>
            </span>
          {:else}
            <span>{n.note.text}</span>
            <span class="row">
              {#if n.note.edited}<span class="sub">Edited {stampText(n.note.edited.at, zone, date)} by {whoText(n.note.edited.by)}</span>{/if}
              <button type="button" class="link" onclick={() => ((editing = n.id), (edited = n.note.text))}>Edit</button>
            </span>
          {/if}
        </li>
      {/each}
    </ul>
    <div class="write" data-testid="note-box">
      <textarea rows="3" placeholder="Note" aria-label="Note" bind:value={draft}></textarea>
      <span class="row">
        <span class="sub">Internal · Can't be deleted</span>
        <button type="button" class="primary" disabled={busy || !draft.trim()} onclick={add} data-testid="add-note">Add note</button>
      </span>
      {#if error}<span class="bad" role="alert">{error}</span>{/if}
    </div>
  {/if}
</section>

<style>
  .detail { display: grid; gap: 0.35rem; }
  .back { justify-self: start; border: 0; background: none; color: var(--accent); cursor: pointer; font: inherit; padding: 0; margin-bottom: 0.4rem; }
  header { display: flex; gap: 0.7rem; align-items: center; }
  .badge { min-width: 3rem; height: 2.2rem; display: inline-grid; place-items: center; padding: 0 0.4rem; border-radius: 6px; background: var(--text); color: var(--surface); font-weight: 700; }
  h2 { font-size: 1.05rem; margin: 0; }
  h3 { font-size: 0.9rem; margin: 0.9rem 0 0.3rem; display: flex; gap: 0.5rem; align-items: center; }
  .count { font-weight: 600; background: var(--hover); border-radius: 999px; padding: 0 0.5rem; font-size: 0.8rem; }
  .title { font-weight: 600; margin: 0.4rem 0 0; }
  p { margin: 0; }
  .sub { display: block; color: var(--muted); font-size: 0.85rem; }
  .told { font-size: 0.85rem; font-weight: 600; color: #174a94; margin-top: 0.3rem; }
  .told.none { color: #8a3f06; }
  .stops, .notes { list-style: none; margin: 0; padding: 0; }
  .stops li { display: flex; justify-content: space-between; gap: 1rem; padding: 0.25rem 0; border-top: 1px solid var(--line); font-size: 0.9rem; }
  .time { color: var(--muted); white-space: nowrap; }
  .write { display: grid; gap: 0.4rem; margin-top: 0.4rem; }
  textarea { font: inherit; padding: 0.45rem 0.55rem; border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--text); resize: vertical; }
  .row { display: flex; justify-content: space-between; align-items: center; gap: 0.5rem; }
  button { border: 1px solid var(--line); background: var(--surface); color: var(--text); border-radius: 6px; padding: 0.3rem 0.8rem; cursor: pointer; font: inherit; font-size: 0.85rem; }
  button.primary { background: var(--accent); color: var(--on-accent); border-color: var(--accent); }
  button:disabled { opacity: 0.5; cursor: default; }
  button.link { border: 0; background: none; color: var(--accent); padding: 0 0.3rem; }
  .bad { color: var(--bad); font-size: 0.85rem; }
  .notes li { display: grid; gap: 0.1rem; padding: 0.45rem 0; border-top: 1px solid var(--line); }
  @media (prefers-color-scheme: dark) {
    .told { color: #93c5fd; }
    .told.none { color: #fdba74; }
  }
</style>
