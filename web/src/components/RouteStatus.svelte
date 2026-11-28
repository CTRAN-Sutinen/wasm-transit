<script lang="ts">
  import {
    datesText,
    kindText,
    minutesText,
    stampText,
    timeText,
    titleText,
    whoText,
  } from "./disruptionsFormat";
  import type { AlertSource } from "../types/AlertSource";
  import type { Today } from "../types/Today";

  let {
    today,
    zone,
    date,
    focus,
    onopen,
    onfocus,
  }: {
    today: Today;
    zone: string | undefined;
    date: string;
    focus: string;
    onopen: (source: AlertSource) => void;
    onfocus: (route: { id: string; label: string } | null) => void;
  } = $props();
</script>

<section class="status" data-testid="route-status">
  <h2>Affected routes <span class="count">{today.routes.length}</span></h2>
  {#if today.routes.length === 0}
    <p class="muted">No disruptions</p>
  {/if}
  <ul class="cards">
    {#each today.routes as r (r.route)}
      <li
        class="card"
        class:focused={focus === r.route}
        data-testid="route-card"
        data-route={r.route}
      >
        <button
          type="button"
          class="badge"
          title={r.long_name ?? r.name}
          onclick={() =>
            onfocus(focus === r.route ? null : { id: r.route, label: r.name })}
        >
          {r.name}
        </button>
        <div class="lines">
          {#each r.disruptions as d (JSON.stringify(d.source))}
            <button
              type="button"
              class="line"
              onclick={() => onopen(d.source)}
              data-testid="disruption-line"
            >
              <span class="what">
                <strong>{kindText(d)}:</strong>
                {titleText(d)}
              </span>
              {#if d.next_start_secs != null}<span class="sub">
                  Next trip {timeText(d.next_start_secs)}
                </span>{/if}
              {#if datesText(d, date)}<span class="sub">
                  {datesText(d, date)}
                </span>{/if}
              <span class="told" class:none={d.explained_by.length === 0}>
                {d.explained_by.length ? "Alert posted" : "No alert"}
              </span>
              {#if d.latest_note}
                <span class="note">
                  {whoText(d.latest_note.note.written.by)}, {stampText(
                    d.latest_note.note.edited?.at ??
                      d.latest_note.note.written.at,
                    zone,
                    date,
                  )}: {d.latest_note.note.text}
                </span>
              {/if}
            </button>
          {/each}
          {#if r.late_trips > 0}
            <span class="late">
              {r.late_trips} late{#if r.worst_delay_secs != null}{` · worst ${minutesText(r.worst_delay_secs)}`}{/if}
            </span>
          {/if}
        </div>
      </li>
    {/each}
  </ul>
  <p class="others muted">
    Other routes ({today.unaffected_routes}): no changes
  </p>
</section>

<style>
  .status h2 {
    font-size: 0.95rem;
    margin: 0 0 0.6rem;
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
  .cards {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.6rem;
  }
  .card {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.75rem;
    padding: 0.7rem;
    border: 1px solid var(--line);
    border-radius: 8px;
  }
  .card.focused {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }
  .badge {
    min-width: 3rem;
    height: 2rem;
    padding: 0 0.4rem;
    border-radius: 6px;
    border: 0;
    cursor: pointer;
    background: var(--text);
    color: var(--surface);
    font-weight: 700;
    font-size: 0.9rem;
  }
  .lines {
    display: grid;
    gap: 0.4rem;
    min-width: 0;
  }
  .line {
    display: grid;
    gap: 0.1rem;
    text-align: left;
    border: 0;
    background: none;
    padding: 0.25rem 0.35rem;
    margin: -0.25rem -0.35rem;
    border-radius: 6px;
    cursor: pointer;
    color: inherit;
    font: inherit;
  }
  .line:hover {
    background: var(--hover);
  }
  .sub {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .told {
    font-size: 0.78rem;
    font-weight: 600;
    color: #174a94;
  }
  .told.none {
    color: #8a3f06;
  }
  .note {
    font-size: 0.85rem;
    border-left: 3px solid var(--line);
    padding-left: 0.5rem;
    color: var(--muted);
  }
  .late {
    font-size: 0.85rem;
    color: var(--warn);
  }
  .others {
    margin: 0.8rem 0 0;
    font-size: 0.85rem;
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
