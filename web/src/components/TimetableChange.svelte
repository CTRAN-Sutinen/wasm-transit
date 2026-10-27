<script lang="ts">
  /// The timetable's change gestures, as a planner states them —
  /// frequency over a window, the span, running time over a window,
  /// and a stop on every pattern of the direction. RENDER only: each
  /// form parses its times and emits one event; the host turns it into
  /// the cap-author edit (`set_headway`, `set_span`,
  /// `adjust_running_time`, `insert_direction_stop`,
  /// `remove_direction_stop`), which owns every rule.
  import type { TimetableStop } from "../types/TimetableStop";

  let {
    framed,
    stops,
    selectedStop,
    onheadway,
    onspan,
    onrunning,
    oninsertstop,
    onremovestop,
  }: {
    framed: boolean;
    stops: TimetableStop[];
    selectedStop: string;
    onheadway: (from_secs: number, to_secs: number, headway_secs: number) => void;
    onspan: (first_secs: number | null, last_secs: number | null) => void;
    onrunning: (from_secs: number, to_secs: number, delta_secs: number) => void;
    oninsertstop: (stop: string, after: string) => void;
    onremovestop: (stop: string) => void;
  } = $props();

  let hwFrom = $state("06:30");
  let hwTo = $state("09:00");
  let hwEvery = $state(15);
  let spanFirst = $state("");
  let spanLast = $state("");
  let rtFrom = $state("15:00");
  let rtTo = $state("18:00");
  let rtDelta = $state(3);
  let newStop = $state("");
  let error = $state("");

  /// "HH:MM" (hours may pass 24) → seconds, or null.
  function secs(text: string): number | null {
    const m = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
    return m && Number(m[2]) < 60 ? Number(m[1]) * 3600 + Number(m[2]) * 60 : null;
  }
  function need(...values: (number | null)[]): boolean {
    if (values.some((v) => v === null)) {
      error = "Times are HH:MM (hours may pass 24).";
      return false;
    }
    error = "";
    return true;
  }
  const selectedName = $derived(stops.find((s) => s.stop === selectedStop)?.name ?? "");
</script>

<section class="change" data-testid="timetable-change">
  <h3>Change this timetable</h3>
  {#if !framed}<p class="note">Fork the import into a plan to change it.</p>{/if}
  <fieldset disabled={!framed}>
    <form data-testid="change-headway" onsubmit={(e) => { e.preventDefault(); const [f, t] = [secs(hwFrom), secs(hwTo)]; if (need(f, t)) onheadway(f!, t!, Math.round(hwEvery * 60)); }}>
      <b>Frequency</b> every <input type="number" min="1" bind:value={hwEvery} aria-label="headway minutes" /> min
      from <input bind:value={hwFrom} aria-label="headway from" /> until <input bind:value={hwTo} aria-label="headway until" />
      <button>Apply</button>
    </form>
    <form data-testid="change-span" onsubmit={(e) => { e.preventDefault(); const [f, l] = [spanFirst ? secs(spanFirst) : null, spanLast ? secs(spanLast) : null]; if (need(spanFirst ? f : 0, spanLast ? l : 0)) onspan(f, l); }}>
      <b>Span</b> first <input bind:value={spanFirst} placeholder="HH:MM" aria-label="first departure" />
      last <input bind:value={spanLast} placeholder="HH:MM" aria-label="last departure" />
      <button>Apply</button>
    </form>
    <form data-testid="change-running" onsubmit={(e) => { e.preventDefault(); const [f, t] = [secs(rtFrom), secs(rtTo)]; if (need(f, t)) onrunning(f!, t!, Math.round(rtDelta * 60)); }}>
      <b>Running time</b> <input type="number" step="1" bind:value={rtDelta} aria-label="running time change minutes" /> min
      for trips leaving <input bind:value={rtFrom} aria-label="running time from" />–<input bind:value={rtTo} aria-label="running time until" />
      <button>Apply</button>
    </form>
    <div class="stops" data-testid="change-stops">
      <b>Stops</b>
      {#if selectedStop}
        <span>at <i>{selectedName}</i>:</span>
        <button onclick={() => onremovestop(selectedStop)}>Drop it</button>
        <span>or add stop id</span>
        <input bind:value={newStop} placeholder="stop id" aria-label="stop to add" />
        <button disabled={!newStop.trim()} onclick={() => oninsertstop(newStop.trim(), selectedStop)}>after it</button>
      {:else}
        <span class="muted">click a stop column in the timetable</span>
      {/if}
    </div>
  </fieldset>
  {#if error}<p class="err">{error}</p>{/if}
  <p class="muted note-foot">Applies to every pattern of this direction; running time changes every service type that runs them.</p>
</section>

<style>
  h3 { margin: 0 0 0.15rem; font-size: 0.95rem; }
  fieldset { border: 0; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.2rem; font-size: 0.85rem; }
  form, .stops { display: flex; flex-wrap: wrap; align-items: center; gap: 0.3rem; }
  input { width: 4.2rem; font: inherit; }
  input[type="number"] { width: 3.4rem; }
  .muted { color: var(--muted); font-size: 0.8rem; }
  .note { font-size: 0.85rem; margin: 0 0 0.25rem; }
  .err { color: var(--bad); font-size: 0.85rem; margin: 0.25rem 0 0; }
  .note-foot { margin: 0.2rem 0 0; }
</style>
