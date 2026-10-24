<script>
  import { app, getNetworkDoc } from "../state.svelte.js";
  import {
    toggleCalendarDay, setCalendarRange, addCalendarRange, removeCalendarRange,
    addCalendarException, removeCalendarException, clearSelection, dayNames,
  } from "../planner.svelte.js";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "calendar" ? app.plannerSelection.id : null);
  const calendar = $derived(id ? doc?.calendars?.[id] ?? null : null);

  let exceptionDate = $state("");
</script>

{#if calendar}
  <h3>Calendar <code>{id}</code></h3>
  <p class="chips">
    {#each dayNames() as day (day)}
      <button class="chip" class:sched={calendar.days[day]} class:off={!calendar.days[day]}
              aria-label={`${day} ${calendar.days[day] ? "runs" : "off"}`}
              onclick={() => toggleCalendarDay(id, day)}>{day}</button>
    {/each}
  </p>

  <h4>Date ranges</h4>
  <ul class="svc-rows">
    {#each calendar.ranges as range, i (i)}
      <li>
        <input aria-label={`Range ${i + 1} start`} type="date" value={range.start}
               onchange={(e) => setCalendarRange(id, i, { start: e.currentTarget.value })}>
        <span aria-hidden="true">→</span>
        <input aria-label={`Range ${i + 1} end`} type="date" value={range.end}
               onchange={(e) => setCalendarRange(id, i, { end: e.currentTarget.value })}>
        <button aria-label={`Remove range ${i + 1}`} onclick={() => removeCalendarRange(id, i)}>remove</button>
      </li>
    {/each}
  </ul>
  <p><button onclick={() => addCalendarRange(id)}>Add range</button></p>

  <h4>Exceptions</h4>
  <ul class="svc-rows">
    {#each calendar.exceptions as exc (exc.date)}
      <li>
        <code>{exc.date}</code>
        <select aria-label={`Exception state for ${exc.date}`} value={exc.state}
                onchange={(e) => addCalendarException(id, exc.date, e.currentTarget.value)}>
          <option value="added">added</option>
          <option value="removed">removed</option>
        </select>
        <button aria-label={`Remove exception ${exc.date}`}
                onclick={() => removeCalendarException(id, exc.date)}>remove</button>
      </li>
    {/each}
  </ul>
  <p class="ent-row">
    <input aria-label="Exception date" type="date" bind:value={exceptionDate}>
    <button disabled={!exceptionDate}
            onclick={() => { addCalendarException(id, exceptionDate, "removed"); exceptionDate = ""; }}>
      Add removed date
    </button>
    <button disabled={!exceptionDate}
            onclick={() => { addCalendarException(id, exceptionDate, "added"); exceptionDate = ""; }}>
      Add added date
    </button>
  </p>
  <p class="muted">Services referencing this calendar show under its chip in the overview.</p>
{:else}
  <p class="muted">Calendar not found (removed?).</p>
{/if}
