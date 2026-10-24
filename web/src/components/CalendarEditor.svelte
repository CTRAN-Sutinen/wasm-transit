<script>
  import { getNetworkDoc } from "../state.svelte.js";
  import {
    addCalendar, toggleCalendarDay, setCalendarRange, addCalendarRange,
    removeCalendarRange, addCalendarException, removeCalendarException,
    removeCalendar, dayNames,
  } from "../planner.svelte.js";

  const doc = $derived(getNetworkDoc());
  const calendars = $derived(doc ? Object.entries(doc.calendars ?? {}) : []);

  let selected = $state("");
  const calendar = $derived(doc?.calendars?.[selected] ?? null);

  let exceptionDate = $state("");
</script>

<section>
  <h2>Calendars</h2>
  <p>
    <select aria-label="Calendar" bind:value={selected}>
      <option value="" disabled>Calendar…</option>
      {#each calendars as [id] (id)}
        <option value={id}>{id}</option>
      {/each}
    </select>
    <button onclick={addCalendar}>Add calendar</button>
    {#if calendar}
      <button aria-label="Remove calendar" onclick={() => { removeCalendar(selected); selected = ""; }}>remove</button>
    {/if}
  </p>

  {#if calendar}
    <p class="chips">
      {#each dayNames() as day (day)}
        <button class="chip" class:sched={calendar.days[day]} class:off={!calendar.days[day]}
                aria-label={`${day} ${calendar.days[day] ? "runs" : "off"}`}
                onclick={() => toggleCalendarDay(selected, day)}>{day}</button>
      {/each}
    </p>

    <h3>Date ranges</h3>
    <ul class="svc-rows">
      {#each calendar.ranges as range, i (i)}
        <li>
          <input aria-label={`Range ${i + 1} start`} type="date" value={range.start}
                 onchange={(e) => setCalendarRange(selected, i, { start: e.currentTarget.value })}>
          <span aria-hidden="true">→</span>
          <input aria-label={`Range ${i + 1} end`} type="date" value={range.end}
                 onchange={(e) => setCalendarRange(selected, i, { end: e.currentTarget.value })}>
          <button aria-label={`Remove range ${i + 1}`} onclick={() => removeCalendarRange(selected, i)}>remove</button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addCalendarRange(selected)}>Add range</button></p>

    <h3>Exceptions</h3>
    <ul class="svc-rows">
      {#each calendar.exceptions as exc (exc.date)}
        <li>
          <code>{exc.date}</code>
          <select aria-label={`Exception state for ${exc.date}`} value={exc.state}
                  onchange={(e) => addCalendarException(selected, exc.date, e.currentTarget.value)}>
            <option value="added">added</option>
            <option value="removed">removed</option>
          </select>
          <button aria-label={`Remove exception ${exc.date}`}
                  onclick={() => removeCalendarException(selected, exc.date)}>remove</button>
        </li>
      {/each}
    </ul>
    <p>
      <input aria-label="Exception date" type="date" bind:value={exceptionDate}>
      <button disabled={!exceptionDate} onclick={() => { addCalendarException(selected, exceptionDate, "removed"); exceptionDate = ""; }}>
        Add removed date
      </button>
      <button disabled={!exceptionDate} onclick={() => { addCalendarException(selected, exceptionDate, "added"); exceptionDate = ""; }}>
        Add added date
      </button>
    </p>
  {/if}
</section>
