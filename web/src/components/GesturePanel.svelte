<script lang="ts">
  /// The route's changes, one form per gesture (cap-author owns every
  /// rule): span, running time over a band, one trip moved on its
  /// own, a stop inserted or dropped. A frequency target the trips
  /// flex around is not offered: what it keeps of the trips already
  /// there waits on a decision. On the unframed import every form is
  /// off until it is forked.
  import Unavailable from "./Unavailable.svelte";
  import type { TimetableStop } from "../types/TimetableStop";
  import type { TimetableTrip } from "../types/TimetableTrip";
  import { clock, parseClock } from "./designFormat";

  let {
    framed,
    stops,
    selectedStop,
    trips,
    selectedTrip: pickedKey,
    window,
    onspan,
    onrunning,
    onretime,
    oninsertstop,
    onremovestop,
  }: {
    framed: boolean;
    stops: TimetableStop[];
    selectedStop: string;
    trips: TimetableTrip[];
    selectedTrip: string;
    window: { from: number; to: number } | null;
    onspan: (first: number | null, last: number | null) => void;
    onrunning: (from: number, to: number, delta: number) => void;
    onretime: (trip: string, toSecs: number) => void;
    oninsertstop: (stop: string, after: string) => void;
    onremovestop: (stop: string) => void;
  } = $props();

  let first = $state("");
  let last = $state("");
  let bandFrom = $state("");
  let bandTo = $state("");
  let delta = $state(2);
  let tripTo = $state("");
  let newStop = $state("");

  $effect(() => {
    if (window) {
      bandFrom = `${window.from}:00`;
      bandTo = `${window.to}:00`;
    }
  });
  $effect(() => {
    tripTo = selectedTrip ? clock(selectedTrip.start_secs) : "";
  });
  const selectedTrip = $derived(trips.find((t) => `${t.trip}@${t.start_secs}` === pickedKey) ?? null);
  const stopName = $derived(stops.find((s) => s.stop === selectedStop)?.name ?? "");
</script>

<section class="panel" aria-label="Change this direction" data-testid="gestures">
  <header>
    <h2>Change this direction</h2>
    {#if !framed}<span class="warn">Fork the import into a plan to change it.</span>{/if}
  </header>
  <fieldset disabled={!framed}>
    <div class="forms">
      <div class="g">
        <Unavailable
          title="Frequency target"
          reason="A target headway with a tolerance, keeping every trip already inside it. What a target keeps of the trips there is undecided, and today's op snaps them to a rigid grid."
          blockers={["D-54", "D-3", "QAF-90"]}
        />
      </div>
      <form class="g" onsubmit={(e) => { e.preventDefault(); onspan(parseClock(first), parseClock(last)); }} data-testid="gesture-span">
        <h3>Span</h3>
        <label>First trip <input bind:value={first} placeholder="5:30" aria-label="first trip" /></label>
        <label>Last trip <input bind:value={last} placeholder="23:00" aria-label="last trip" /></label>
        <button type="submit" disabled={!parseClock(first) && !parseClock(last)}>Apply</button>
      </form>
      <form class="g" onsubmit={(e) => { e.preventDefault(); const [a, b] = [parseClock(bandFrom), parseClock(bandTo)]; if (a != null && b != null) onrunning(a, b, delta * 60); }} data-testid="gesture-running">
        <h3>Running time</h3>
        <label>From <input bind:value={bandFrom} placeholder="15:00" aria-label="band from" /></label>
        <label>Until <input bind:value={bandTo} placeholder="18:00" aria-label="band until" /></label>
        <label>Change (min) <input type="number" bind:value={delta} aria-label="running time change" /></label>
        <p class="muted">Trips departing in the band run longer (or shorter), spread along each trip.</p>
        <button type="submit" disabled={parseClock(bandFrom) == null || parseClock(bandTo) == null || !delta}>Apply</button>
      </form>
      <form class="g" onsubmit={(e) => { e.preventDefault(); const to = parseClock(tripTo); if (selectedTrip && to != null) onretime(selectedTrip.trip, to); }} data-testid="gesture-trip">
        <h3>One trip</h3>
        {#if selectedTrip}
          <p class="muted">Trip {selectedTrip.trip}, now {clock(selectedTrip.start_secs)}. It keeps its id, its running times, and its block.</p>
          <label>Leaves at <input bind:value={tripTo} aria-label="trip leaves at" /></label>
          <button type="submit" disabled={parseClock(tripTo) == null}>Move</button>
        {:else}
          <p class="muted">Pick a line on the string-line — or drag it — to move that trip alone.</p>
        {/if}
      </form>
      <div class="g" data-testid="gesture-stops">
        <h3>Stops</h3>
        {#if selectedStop}
          <p class="muted">{stopName}</p>
          <button type="button" onclick={() => onremovestop(selectedStop)}>Drop it from the direction</button>
          <form onsubmit={(e) => { e.preventDefault(); if (newStop.trim()) oninsertstop(newStop.trim(), selectedStop); newStop = ""; }}>
            <label>Insert stop id after it <input bind:value={newStop} aria-label="stop to insert" /></label>
            <button type="submit" disabled={!newStop.trim()}>Insert</button>
          </form>
        {:else}
          <p class="muted">Pick a stop in the timetable to drop it or insert one after it.</p>
        {/if}
      </div>
    </div>
  </fieldset>
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--accent); border-radius: 8px; }
  header { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: baseline; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  h3 { margin: 0; font-size: 0.8rem; }
  .warn { color: var(--warn); font-size: 0.8rem; }
  fieldset { border: 0; margin: 0; padding: 0; min-width: 0; }
  .forms { display: flex; flex-wrap: wrap; gap: 0.6rem; padding: 0.7rem 0.9rem; }
  .g { flex: 1 1 13rem; min-width: 0; display: flex; flex-direction: column; gap: 0.35rem; border: 1px solid var(--line); border-radius: 8px; padding: 0.55rem 0.7rem; font-size: 0.8rem; }
  .g:first-child { border: 0; padding: 0; }
  label { display: flex; justify-content: space-between; align-items: center; gap: 0.4rem; }
  input { width: 5.5rem; font: inherit; padding: 0.15rem 0.35rem; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: var(--text); }
  .muted { color: var(--muted); margin: 0; }
  button { align-self: flex-start; font: inherit; padding: 0.2rem 0.65rem; border-radius: 6px; border: 1px solid var(--text); background: var(--text); color: var(--surface); cursor: pointer; }
  button:disabled { opacity: 0.4; cursor: default; }
  form { display: flex; flex-direction: column; gap: 0.35rem; }
</style>
