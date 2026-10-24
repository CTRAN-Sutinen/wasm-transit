<script>
  import { app } from "../state.svelte.js";
  import { selectStop, showTrip, refreshNow, serviceDate, secondsSinceMidnight } from "../actions.svelte.js";

  const stops = $derived(app.gtfs ? app.gtfs.stops().slice(0, 2000) : []);
  const services = $derived(app.gtfs ? app.gtfs.active_services(serviceDate()) : []);

  /// trip_id → { byStop: Map(stop_id → delay), worst: delay }
  /// Joined from the GTFS-rt trip updates, when a feed is loaded.
  const rtTrips = $derived.by(() => {
    const m = new Map();
    for (const t of app.rtOverview?.trips ?? []) {
      const byStop = new Map();
      let worst = null;
      for (const s of t.stop_updates ?? []) {
        const d = s.departure_delay ?? s.arrival_delay;
        if (d == null) continue;
        if (s.stop_id) byStop.set(s.stop_id, d);
        worst = worst == null ? d : Math.max(worst, d);
      }
      if (t.trip_id) m.set(t.trip_id, { byStop, worst });
    }
    return m;
  });

  const deps = $derived.by(() => {
    void app.clock; // time moved
    void app.feedEpoch; // the effective feed changed (edits, reload)
    if (!app.gtfs || !app.selectedStop) return [];
    const raw = app.gtfs.departures(app.selectedStop, serviceDate(), secondsSinceMidnight(), 15);
    return raw.map((d) => {
      const rt = rtTrips.get(d.trip_id);
      const delay = rt ? (rt.byStop.get(d.stop_id) ?? rt.worst) : null;
      return {
        ...d,
        delay,
        expected: delay != null && d.departure_time != null ? d.departure_time + delay : null,
      };
    });
  });

  function fmtClock(secs) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
  }

  function delayChip(delay) {
    if (delay == null) return null;
    if (delay <= 60) return { label: "on time", cls: "ok" };
    const min = Math.round(delay / 60);
    return { label: `+${min} min`, cls: min > 5 ? "bad" : "warn" };
  }
</script>

<section>
  <h2>Departures</h2>
  <p>
    <select aria-label="Stop" value={app.selectedStop ?? ""} onchange={(e) => selectStop(e.currentTarget.value)}>
      {#each stops as s (s.stop_id)}
        <option value={s.stop_id}>{s.name}</option>
      {/each}
    </select>
    <button onclick={refreshNow}>Refresh</button>
    {#each services as s (s)}
      <span class="chip svc" title="service key running today">{s}</span>
    {/each}
  </p>

  {#if deps.length}
    <table>
      <tbody>
        {#each deps as d (d.trip_id + d.stop_sequence)}
          {@const chip = delayChip(d.delay)}
          <tr class="clickable-row" class:selected={app.selectedTripId === d.trip_id} onclick={() => showTrip(d.trip_id)}>
            <td class="time">
              {fmtClock(d.departure_time)}
              {#if d.expected != null && d.expected !== d.departure_time}
                <span class="expected" class:late={d.delay > 60} class:early={d.delay < 0}>
                  → {fmtClock(d.expected)}
                </span>
              {/if}
            </td>
            <td><span class="route-badge">{d.route_name}</span></td>
            <td>{d.headsign ?? ""}</td>
            <td>{#if chip}<span class="chip {chip.cls}">{chip.label}</span>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="muted">Click a departure to draw its path. {#if app.rtOverview}Times include live delays.{/if}</p>
  {:else if app.selectedStop}
    <p class="muted">Nothing upcoming at this stop right now.</p>
  {/if}
</section>
