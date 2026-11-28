<script lang="ts">
  import {
    minutesText,
    originText,
    stampText,
    timeText,
    titleText,
    whoText,
  } from "./disruptionsFormat";
  import type { AlertSource } from "../types/AlertSource";
  import type { Disruption } from "../types/Disruption";
  import type { LateTrip } from "../types/LateTrip";
  import type { LiveDay } from "../types/LiveDay";
  import type { NoteRow } from "../types/NoteRow";
  import type { NoteTarget } from "../types/NoteTarget";
  import type { Today } from "../types/Today";

  let {
    late,
    live,
    day,
    notes,
    zone,
    date,
    lateOff,
    notesOff,
    onbus,
    onopen,
  }: {
    late: LateTrip[];
    live: LiveDay | null;
    day: Today | null;
    notes: NoteRow[];
    zone: string | undefined;
    date: string;
    lateOff: string;
    notesOff: string;
    onbus: (at: { lat: number; lon: number }) => void;
    onopen: (source: AlertSource) => void;
  } = $props();

  type Row = { route: string; d: Disruption };
  const rows = (pick: (d: Disruption) => boolean): Row[] =>
    (day?.routes ?? []).flatMap((r) =>
      r.disruptions.filter(pick).map((d) => ({ route: r.name, d })),
    );
  const detours = $derived(rows((d) => d.from !== "stops" && !d.trip));
  const cancels = $derived(rows((d) => !!d.trip));

  function about(target: NoteTarget): string {
    switch (target.kind) {
      case "trip":
        return `Trip ${target.trip} at ${timeText(target.start_secs)}`;
      case "edit":
        return `Record ${target.id}`;
      case "alert":
        return `Alert ${target.id}`;
      default:
        return `Stop ${target.stop}`;
    }
  }
</script>

<div class="board" data-testid="dispatch-board">
  <section data-testid="late-now">
    <h2>Late now <span class="count">{late.length}</span></h2>
    {#if lateOff}
      <p class="muted">{lateOff}</p>
    {:else if !live}
      <p class="muted">No live feed</p>
    {:else if late.length === 0}
      <p class="muted">Nothing late</p>
    {:else}
      <table class="rows">
        <tbody>
          {#each late as t, i (i)}
            <tr
              class:dark={!t.has_position}
              onclick={() =>
                t.lat != null &&
                t.lon != null &&
                onbus({ lat: t.lat, lon: t.lon })}
              data-testid="late-row"
            >
              <td class="route">{t.route_name ?? t.route_id ?? ""}</td>
              <td>
                {[
                  t.start_secs != null ? timeText(t.start_secs) : "",
                  t.headsign ? `to ${t.headsign}` : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                <span class="sub">
                  {t.near_stop ? `Near ${t.near_stop.name}` : "No position"}
                </span>
              </td>
              <td class="bus">
                {#if t.vehicle_label ?? t.vehicle_id}Bus {t.vehicle_label ??
                    t.vehicle_id}{/if}
              </td>
              <td class="delay">{minutesText(t.worst_delay_secs)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>

  <section data-testid="detours">
    <h2>Detours <span class="count">{detours.length}</span></h2>
    {#if detours.length === 0}<p class="muted">None today</p>{/if}
    {#each detours as { route, d } (JSON.stringify(d.source))}
      <button type="button" class="item" onclick={() => onopen(d.source)}>
        <span class="route">{route}</span>
        <span>{titleText(d)}<span class="sub">{originText(d.from)}</span></span>
        <span class="told" class:none={!d.explained_by.length}>
          {d.explained_by.length ? "Alert" : "No alert"}
        </span>
      </button>
    {/each}
  </section>

  <section data-testid="cancels">
    <h2>Cancelled trips <span class="count">{cancels.length}</span></h2>
    {#if cancels.length === 0}<p class="muted">None today</p>{/if}
    {#each cancels as { route, d } (JSON.stringify(d.source))}
      <button
        type="button"
        class="item"
        onclick={() => onopen(d.source)}
        data-testid="cancel-row"
      >
        <span class="route">{route}</span>
        <span>
          {titleText(d)}<span class="sub">
            {originText(d.from)}{#if d.next_start_secs != null}
              · Next trip {timeText(d.next_start_secs)}{/if}
          </span>
        </span>
        <span class="told" class:none={!d.explained_by.length}>
          {d.explained_by.length ? "Alert" : "No alert"}
        </span>
      </button>
    {/each}
  </section>

  <section data-testid="day-notes">
    <h2>Notes <span class="count">{notes.length}</span></h2>
    {#if notesOff}
      <p class="muted">{notesOff}</p>
    {:else if notes.length === 0}
      <p class="muted">No notes today</p>
    {/if}
    {#each notes as n (n.id)}
      <button
        type="button"
        class="note"
        onclick={() =>
          n.note.target.kind !== "alert" &&
          onopen(n.note.target as AlertSource)}
      >
        <span class="sub">
          {whoText(n.note.written.by)} · {stampText(
            n.note.written.at,
            zone,
            date,
          )} · {about(n.note.target)}
        </span>
        <span>{n.note.text}</span>
      </button>
    {/each}
  </section>
</div>

<style>
  .board {
    display: grid;
    gap: 1.2rem;
  }
  h2 {
    font-size: 0.95rem;
    margin: 0 0 0.4rem;
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }
  .count {
    font-weight: 600;
    background: var(--hover);
    border-radius: 999px;
    padding: 0 0.5rem;
    font-size: 0.8rem;
  }
  .rows {
    width: 100%;
    border-collapse: collapse;
  }
  .rows tr {
    cursor: pointer;
    border-top: 1px solid var(--line);
  }
  .rows tr:hover {
    background: var(--hover);
  }
  .rows td {
    padding: 0.35rem 0.3rem;
    vertical-align: top;
  }
  .rows tr.dark td {
    color: var(--muted);
  }
  .route {
    font-weight: 700;
    min-width: 2.6rem;
  }
  .bus {
    color: var(--muted);
    white-space: nowrap;
    font-size: 0.85rem;
  }
  .delay {
    text-align: right;
    font-weight: 600;
    color: var(--warn);
    white-space: nowrap;
  }
  .sub {
    display: block;
    color: var(--muted);
    font-size: 0.82rem;
  }
  .item,
  .note {
    display: grid;
    width: 100%;
    text-align: left;
    border: 0;
    border-top: 1px solid var(--line);
    background: none;
    padding: 0.4rem 0.3rem;
    cursor: pointer;
    font: inherit;
    color: inherit;
  }
  .item {
    grid-template-columns: 2.8rem 1fr auto;
    gap: 0.5rem;
    align-items: start;
  }
  .item:hover,
  .note:hover {
    background: var(--hover);
  }
  .told {
    font-size: 0.78rem;
    font-weight: 600;
    color: #174a94;
  }
  .told.none {
    color: #8a3f06;
  }
  @media (prefers-color-scheme: dark) {
    .told {
      color: #93c5fd;
    }
    .told.none {
      color: #fdba74;
    }
  }
</style>
