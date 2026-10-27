<script lang="ts">
  import { app, getNetworkDoc } from "../state.svelte";
  import {
    updateServiceType, removeServiceType, toggleServiceTypeDay, setServiceTypeRange,
    addServiceTypeRange, removeServiceTypeRange, addServiceTypeException,
    removeServiceTypeException, selectEntity, plannerBack, dayNames,
    fmtHeadwaySafe, fmtHHMMSS,
  } from "../planner";

  const doc = $derived(getNetworkDoc());
  const id = $derived(app.plannerSelection?.kind === "service_type" ? app.plannerSelection.id : "");
  const type = $derived(id ? doc?.service_types?.[id] ?? null : null);
  const detail = $derived(app.serviceTypeDetail?.service_type === id ? app.serviceTypeDetail : null);
  const coverage = $derived(app.typeCoverage?.find((tc) => tc.service_type === id) ?? null);

  function fmtSpan(first: number | null | undefined, last: number | null | undefined): string {
    if (first == null || last == null) return "—";
    return `${fmtHHMMSS(first).slice(0, 5)}–${fmtHHMMSS(last).slice(0, 5)}`;
  }

  let exceptionDate = $state("");
</script>

{#if type}
  <section>
    <h2>Service type <code>{id}</code></h2>
    <p class="ent-row">
      <label>name
        <input aria-label="Service type name" value={type.name}
               onchange={(e) => updateServiceType(id, { name: e.currentTarget.value })}>
      </label>
    </p>

    {#if detail}
      <p class="chips">
        <span class="chip svc">{detail.trips} trips</span>
        <span class="chip svc">{detail.routes.length} routes</span>
        <span class="chip svc">{detail.stops.length} stops served</span>
        <span class="chip svc">{detail.revenue_hours.toFixed(1)} rev hrs</span>
        <span class="chip svc">{detail.transfer_stops} transfer points</span>
        <span class="muted">on its representative day, {detail.date}</span>
      </p>
      {#if coverage}
        <p class="chips">
          <span class="chip ok" title={`within 400 m of a stop, measured on ${coverage.date}`}>
            serves {coverage.served_pct.toFixed(1)}% of population
          </span>
          <span class="chip svc" title="stops whose combined headway is ≤15 min">
            {coverage.frequent_pct.toFixed(1)}% with frequent service
          </span>
        </p>
      {/if}

      <h4>Routes running</h4>
      <div class="table-scroll">
        <table class="stat-table">
          <thead>
            <tr><th>route</th><th>trips</th><th>headway</th><th>span</th></tr>
          </thead>
          <tbody>
            {#each detail.routes as row (row.route_id)}
              <tr>
                <td>
                  <button class="link" aria-label={`Open route ${row.route_id}`}
                          onclick={() => selectEntity("route", row.route_id)}>
                    <strong>{row.name}</strong>
                  </button>
                </td>
                <td>{row.trips}</td>
                <td>{fmtHeadwaySafe(row.typical_headway_secs).replace(" headway", "")}</td>
                <td>{fmtSpan(row.first_start_secs, row.last_arrival_secs)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <h4>Schedules ({detail.pattern_trips.length})</h4>
      <ul class="rail-items">
        {#each detail.pattern_trips as s (s.pattern_trips)}
          <li>
            <button class="rail-item" aria-label={`Open schedule ${s.pattern_trips}`}
                    onclick={() => selectEntity("pattern_trips", s.pattern_trips)}>
              {s.pattern_name} <span class="muted">· {s.trips} trips</span>
            </button>
          </li>
        {/each}
      </ul>

      <h4>Stops served ({detail.stops.length})</h4>
      <div class="table-scroll">
        <table class="stat-table">
          <thead>
            <tr><th>stop</th><th>routes</th><th>deps</th><th>headway</th><th></th></tr>
          </thead>
          <tbody>
            {#each detail.stops as row (row.stop_id)}
              <tr>
                <td>
                  <button class="link" aria-label={`Open stop ${row.stop_id}`}
                          onclick={() => selectEntity("stop", row.stop_id)}>
                    <strong>{row.name}</strong>
                  </button>
                </td>
                <td>{row.routes}</td>
                <td>{row.departures}</td>
                <td>{fmtHeadwaySafe(row.typical_headway_secs).replace(" headway", "")}</td>
                <td>
                  {#if row.transfer}
                    <span class="chip svc" title={`${row.transfer_pairs ?? 0} timed route-pair connections across its walking group`}>
                      transfer
                    </span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {:else}
      <p class="muted">Computing the day view…</p>
    {/if}
  </section>

  <details class="io-drawer">
    <summary>Days it runs (the date mapping)</summary>
    <p class="chips">
      {#each dayNames() as day (day)}
        <button class="chip" class:sched={type.rule.days[day]} class:off={!type.rule.days[day]}
                aria-label={`${day} ${type.rule.days[day] ? "runs" : "off"}`}
                onclick={() => toggleServiceTypeDay(id, day)}>{day}</button>
      {/each}
    </p>
    <h4>Date ranges</h4>
    <ul class="svc-rows">
      {#each type.rule.ranges as range, i (i)}
        <li>
          <input aria-label={`Range ${i + 1} start`} type="date" value={range.start}
                 onchange={(e) => setServiceTypeRange(id, i, { start: e.currentTarget.value })}>
          <span aria-hidden="true">→</span>
          <input aria-label={`Range ${i + 1} end`} type="date" value={range.end}
                 onchange={(e) => setServiceTypeRange(id, i, { end: e.currentTarget.value })}>
          <button aria-label={`Remove range ${i + 1}`} onclick={() => removeServiceTypeRange(id, i)}>remove</button>
        </li>
      {/each}
    </ul>
    <p><button onclick={() => addServiceTypeRange(id)}>Add range</button></p>
    <h4>Exceptions</h4>
    <ul class="svc-rows">
      {#each type.rule.exceptions as exc (exc.date)}
        <li>
          <code>{exc.date}</code>
          <select aria-label={`Exception state for ${exc.date}`} value={exc.state}
                  onchange={(e) => addServiceTypeException(id, exc.date, e.currentTarget.value as "added" | "removed")}>
            <option value="added">added</option>
            <option value="removed">removed</option>
          </select>
          <button aria-label={`Remove exception ${exc.date}`}
                  onclick={() => removeServiceTypeException(id, exc.date)}>remove</button>
        </li>
      {/each}
    </ul>
    <p class="ent-row">
      <input aria-label="Exception date" type="date" bind:value={exceptionDate}>
      <button disabled={!exceptionDate}
              onclick={() => { addServiceTypeException(id, exceptionDate, "removed"); exceptionDate = ""; }}>
        Add removed date
      </button>
      <button disabled={!exceptionDate}
              onclick={() => { addServiceTypeException(id, exceptionDate, "added"); exceptionDate = ""; }}>
        Add added date
      </button>
    </p>
    <p>
      <button onclick={() => { removeServiceType(id); plannerBack(); }}>Remove service type</button>
    </p>
  </details>
{:else}
  <p class="muted">Service type not found (removed?).</p>
{/if}
