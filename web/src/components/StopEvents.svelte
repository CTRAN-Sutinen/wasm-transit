<script lang="ts">
  /// One stop in the stops document (cap-stops `stop`, D-153): where it
  /// stands on the date — open, closed, proposed, retired — what is at
  /// it, and every event it has, oldest first. Closing it for dates and
  /// reopening it are its edits (D-144); the host writes them. A RENDER
  /// component: the stop and its events come in as props; formatting
  /// only.
  import type { Event } from "../types/Event";
  import type { Refusal } from "../types/Refusal";
  import type { StopOn } from "../types/StopOn";

  let {
    date,
    held,
    history,
    refusals,
    busy,
    onclose,
    onreopen,
  }: {
    /// The date the stop is read on.
    date: string;
    /// The deployment keeps a stops document.
    held: boolean;
    /// The stop on the date and its events; null when the document does
    /// not hold the stop.
    history: { stop: StopOn | null; events: Event[] } | null;
    /// Why the last change was refused.
    refusals: Refusal[];
    busy: boolean;
    onclose: (from: string, until: string, note: string) => void;
    onreopen: (on: string, note: string) => void;
  } = $props();

  const status = $derived(history?.stop?.status ?? null);
  const STATUS: Record<string, string> = { open: "Open", closed: "Closed", proposed: "Proposed", retired: "Retired" };
  const KIND: Record<string, string> = {
    opened: "Opened",
    closed: "Closed",
    reopened: "Reopened",
    moved: "Moved",
    changed: "Changed",
    proposed: "Proposed",
    retired: "Retired",
  };
  const words = (name: string) => name.replaceAll("_", " ");
  const amenities = $derived(Object.entries(history?.stop?.amenities ?? {}));
  const there = $derived(amenities.filter(([, v]) => v).map(([k]) => words(k)));
  const notThere = $derived(amenities.filter(([, v]) => !v).map(([k]) => words(k)));

  /// What an event changed, in a few words.
  function what(e: Event): string {
    const parts: string[] = [];
    if (e.until) parts.push(`until ${e.until}`);
    if (e.plan) parts.push(`for plan ${e.plan}`);
    if (e.event === "moved" && e.lat != null && e.lon != null) parts.push(`to ${e.lat.toFixed(5)}, ${e.lon.toFixed(5)}`);
    if (e.name && e.event !== "opened") parts.push(`name "${e.name}"`);
    return parts.join(", ");
  }

  const PROBLEM: Record<string, string> = {
    nothing_closed: "the stop is not closed on that date",
    until_not_after: "the day it opens again must come after the first day closed",
    out_of_order: "the events would be out of date order",
    after_retired: "the stop is retired by then",
    bad_date: "that is not a date",
    no_date: "the event needs a date",
  };
  const problem = (r: Refusal) => PROBLEM[r.problem.kind] ?? words(r.problem.kind);

  // The edit form: its dates start at the date read.
  let from = $state("");
  let until = $state("");
  let note = $state("");
  $effect(() => {
    from = date;
    until = "";
    note = "";
  });
</script>

<section class="stop-events" data-testid="stop-events">
  <h3>On {date}</h3>
  {#if !held}
    <p class="muted" data-testid="stop-events-none">The deployment keeps no stop history yet. Closing a stop begins one.</p>
  {:else if !history}
    <p class="muted" data-testid="stop-events-none">The stop history does not hold this stop.</p>
  {:else}
    <p data-testid="stop-status">
      <strong class="status {status ?? 'none'}">{status ? STATUS[status] : "Not there yet"}</strong>
      {#if status === "proposed" && history.stop?.plan}<span class="muted">for plan {history.stop.plan}</span>{/if}
    </p>
    {#if amenities.length}
      <p data-testid="stop-amenities">
        {#if there.length}Has: {there.join(", ")}.{/if}
        {#if notThere.length}<span class="muted">Checked, not there: {notThere.join(", ")}.</span>{/if}
      </p>
    {/if}
    <h3>History</h3>
    <ol class="events">
      {#each history.events as e, i (i)}
        <li data-testid="stop-event">
          <span class="when">{e.on ?? "before records"}</span>
          <span>{KIND[e.event] ?? e.event}{what(e) ? ` ${what(e)}` : ""}</span>
          {#if e.note}<span class="muted">— {e.note}</span>{/if}
        </li>
      {/each}
    </ol>
  {/if}

  {#if !held || !history || status === "open" || status === "closed"}
    <form
      class="edit"
      onsubmit={(e) => {
        e.preventDefault();
        if (status === "closed") onreopen(from, note);
        else onclose(from, until, note);
      }}
    >
      <h3>{status === "closed" ? "Reopen the stop" : "Close the stop"}</h3>
      <label>{status === "closed" ? "Open again from" : "First day closed"} <input type="date" bind:value={from} required data-testid="stop-edit-from" /></label>
      {#if status !== "closed"}
        <label>Open again from <input type="date" bind:value={until} data-testid="stop-edit-until" /></label>
        <p class="muted hint">Leave it empty to keep the stop closed until it is reopened.</p>
      {/if}
      <label>Note <input type="text" bind:value={note} placeholder="Water main work" data-testid="stop-edit-note" /></label>
      <button type="submit" disabled={busy || !from} data-testid="stop-edit-submit">{status === "closed" ? "Reopen" : "Close"}</button>
    </form>
  {/if}
  {#if refusals.length}
    <ul class="refused" data-testid="stop-refused">
      {#each refusals as r, i (i)}
        <li>Refused: {problem(r)}.</li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  h3 { font-size: 0.95rem; margin: 0.6rem 0 0.3rem; }
  p { margin: 0.2rem 0; }
  .status.closed, .status.retired { color: var(--bad, #b91c1c); }
  .status.proposed { color: var(--muted); }
  .events { list-style: none; padding: 0; margin: 0; }
  .events li { display: flex; flex-wrap: wrap; gap: 0.2rem 0.6rem; align-items: baseline; padding: 0.1rem 0; }
  .when { font-variant-numeric: tabular-nums; min-width: 6.5rem; color: var(--muted); }
  .edit { display: flex; flex-direction: column; gap: 0.35rem; margin-top: 0.4rem; }
  .edit label { display: flex; justify-content: space-between; gap: 0.6rem; align-items: center; }
  .edit input[type="text"] { flex: 1; min-width: 0; }
  .edit button { align-self: flex-start; }
  .hint { font-size: 0.8rem; }
  .refused { color: var(--bad, #b91c1c); padding-left: 1rem; margin: 0.4rem 0 0; }
</style>
