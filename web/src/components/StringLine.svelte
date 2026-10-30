<script lang="ts">
  /// The string-line (directory `timetable`): time across, the
  /// direction's stops down, one line per trip — so a line's slope is
  /// its running time, a slower band reads at a glance, and so does a
  /// trip out of step with its neighbours. Against a baseline, a
  /// moved trip is drawn in the accent with its baseline time dashed
  /// beside it, and an added trip in blue. Editable, a trip drags
  /// along the time axis to nudge it on its own (whole minutes); the
  /// drop is an event, the change is the host's.
  import type { Timetable } from "../types/Timetable";
  import type { TimetableTrip } from "../types/TimetableTrip";
  import { clock } from "./designFormat";

  let {
    timetable,
    window,
    editable,
    selected,
    onselect,
    onnudge,
  }: {
    timetable: Timetable;
    window: { from: number; to: number } | null;
    editable: boolean;
    selected: string;
    onselect: (key: string) => void;
    onnudge: (trip: string, toSecs: number) => void;
  } = $props();

  let allStops = $state(false);
  const columns = $derived(
    timetable.stops.map((s, i) => ({ ...s, i })).filter((s, _, all) => allStops || s.timepoint || all.every((x) => !x.timepoint)),
  );
  const key = (t: TimetableTrip) => `${t.trip}@${t.start_secs}`;

  const W = 1000;
  const LEFT = 0;
  const rowH = $derived(columns.length > 8 ? 26 : 44);
  const H = $derived(Math.max(1, columns.length - 1) * rowH + 24);
  const span = $derived.by(() => {
    const times = timetable.trips.flatMap((t) => t.times.filter((x): x is number => x != null));
    const lo = Math.floor(Math.min(...times, 6 * 3600) / 3600) * 3600;
    const hi = Math.ceil(Math.max(...times, lo + 3600) / 3600) * 3600;
    return { lo, hi };
  });
  const x = (secs: number) => LEFT + ((secs - span.lo) / (span.hi - span.lo)) * (W - LEFT);
  const y = (row: number) => 12 + row * rowH;
  function points(t: TimetableTrip, shift = 0): string {
    return columns
      .map((c, row) => (t.times[c.i] != null ? `${x(t.times[c.i]! + shift).toFixed(1)},${y(row)}` : null))
      .filter(Boolean)
      .join(" ");
  }
  const hours = $derived(Array.from({ length: (span.hi - span.lo) / 3600 + 1 }, (_, i) => span.lo + i * 3600));

  // Dragging: the pixel offset becomes whole minutes.
  let svg: SVGSVGElement;
  let drag = $state<{ key: string; trip: string; start: number; startX: number; delta: number } | null>(null);
  function secsPerPixel() {
    const width = svg?.getBoundingClientRect().width || W;
    return ((span.hi - span.lo) / (W - LEFT)) * (W / width);
  }
  function down(e: PointerEvent, t: TimetableTrip) {
    onselect(key(t));
    if (!editable) return;
    try {
      (e.currentTarget as Element).setPointerCapture(e.pointerId);
    } catch {
      // A pointer the browser no longer tracks drags without capture.
    }
    drag = { key: key(t), trip: t.trip, start: t.start_secs, startX: e.clientX, delta: 0 };
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    drag.delta = Math.round(((e.clientX - drag.startX) * secsPerPixel()) / 60) * 60;
  }
  function up() {
    if (drag && drag.delta !== 0) onnudge(drag.trip, drag.start + drag.delta);
    drag = null;
  }
  const kind = (t: TimetableTrip) => (t.added ? "added" : t.retimed_from_secs != null ? "moved" : "kept");
</script>

<section class="panel" aria-label="String-line" data-testid="string-line">
  <header>
    <h2>{timetable.route_name} · to {timetable.direction_name}</h2>
    <span class="muted">{timetable.trips.length} trips · time across, stops down; a line's slope is its running time</span>
    <label class="toggle"><input type="checkbox" bind:checked={allStops} /> every stop</label>
  </header>
  <div class="frame">
    <div class="names" style:height="{H}px">
      {#each columns as c, row (c.stop)}
        <span style:top="{y(row) - 7}px" class:tp={c.timepoint}>{c.name}</span>
      {/each}
    </div>
    <div class="plot">
      <svg
        bind:this={svg}
        viewBox="0 0 {W} {H}"
        preserveAspectRatio="none"
        style:height="{H}px"
        role="img"
        aria-label="{timetable.trips.length} trips by time and stop"
        onpointermove={move}
        onpointerup={up}
      >
        {#if window}
          <rect class="win" x={x(window.from * 3600)} width={x(window.to * 3600) - x(window.from * 3600)} y="0" height={H}></rect>
        {/if}
        {#each hours as h (h)}<line class="grid" x1={x(h)} x2={x(h)} y1="0" y2={H}></line>{/each}
        {#each columns as c, row (c.stop)}<line class="grid" x1="0" x2={W} y1={y(row)} y2={y(row)}></line>{/each}
        {#each timetable.trips as t (key(t))}
          {#if t.retimed_from_secs != null}
            <polyline class="ghost" points={points(t, t.retimed_from_secs - t.start_secs)}></polyline>
          {/if}
        {/each}
        {#each timetable.trips as t (key(t))}
          <g class="trip {kind(t)}" class:sel={selected === key(t)}>
            <polyline points={points(t, drag?.key === key(t) ? drag.delta : 0)}></polyline>
            <polyline
              class="hit"
              class:grab={editable}
              points={points(t)}
              role="button"
              tabindex="-1"
              aria-label="Trip {t.trip} at {clock(t.start_secs)}"
              onpointerdown={(e) => down(e, t)}
            ><title>{t.trip} · {clock(t.start_secs)} · runs {Math.round(t.run_secs / 60)} min{t.retimed_from_secs != null ? ` · was ${clock(t.retimed_from_secs)}` : ""}{t.added ? " · added" : ""}</title></polyline>
          </g>
        {/each}
      </svg>
      <div class="ticks">
        {#each hours as h, i (h)}{#if i % 2 === 0}<span style:left="{(x(h) / W) * 100}%">{clock(h)}</span>{/if}{/each}
      </div>
    </div>
  </div>
  <footer>
    <span><i class="k kept"></i>unchanged</span>
    <span><i class="k moved"></i>moved</span>
    <span><i class="k ghost"></i>its baseline time</span>
    <span><i class="k added"></i>added</span>
    {#if timetable.removed.length}<span class="muted">{timetable.removed.length} baseline trips no longer run</span>{/if}
    {#if drag}<span class="drag">{drag.trip}: {clock(drag.start)} → {clock(drag.start + drag.delta)}</span>
    {:else if editable}<span class="muted">drag a line to move that trip alone</span>{/if}
  </footer>
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: baseline; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.78rem; }
  .toggle { margin-left: auto; font-size: 0.78rem; color: var(--muted); }
  .frame { display: flex; padding: 0.5rem 0.9rem 0; }
  .names { position: relative; flex: 0 0 11rem; }
  .names span { position: absolute; right: 0.5rem; left: 0; font-size: 0.7rem; color: var(--muted); text-align: right; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; line-height: 14px; }
  .names span.tp { color: var(--text); font-weight: 500; }
  .plot { flex: 1; min-width: 0; }
  svg { display: block; width: 100%; touch-action: none; }
  .win { fill: var(--accent-soft); }
  .grid { stroke: var(--line); stroke-width: 1; vector-effect: non-scaling-stroke; }
  polyline { fill: none; vector-effect: non-scaling-stroke; }
  .trip polyline:first-child { stroke: var(--text); stroke-width: 1.1; opacity: 0.75; }
  .trip.moved polyline:first-child { stroke: var(--accent); stroke-width: 2; opacity: 1; }
  .trip.added polyline:first-child { stroke: #2563eb; stroke-width: 2.2; opacity: 1; }
  .trip.sel polyline:first-child { stroke-width: 3.2; opacity: 1; }
  .ghost { stroke: var(--muted); stroke-width: 1.2; stroke-dasharray: 3 3; }
  .hit { stroke: transparent; stroke-width: 10; cursor: pointer; pointer-events: stroke; }
  .hit.grab { cursor: ew-resize; }
  .ticks { position: relative; height: 1.2rem; }
  .ticks span { position: absolute; transform: translateX(-50%); font-size: 0.68rem; color: var(--muted); }
  footer { display: flex; flex-wrap: wrap; gap: 0.9rem; align-items: center; padding: 0.5rem 0.9rem; border-top: 1px solid var(--line); font-size: 0.78rem; }
  .k { display: inline-block; width: 1.1rem; height: 3px; vertical-align: middle; margin-right: 0.3rem; border-radius: 2px; }
  .k.kept { background: var(--text); }
  .k.moved { background: var(--accent); }
  .k.ghost { background: repeating-linear-gradient(90deg, var(--muted) 0 3px, transparent 3px 6px); }
  .k.added { background: #2563eb; }
  .drag { font-weight: 600; color: var(--accent); }
</style>
