<script lang="ts">
  import { app } from "../state.svelte";

  const rt = $derived(app.rtOverview);

  /// Worst known delay for a trip update (max over its stops).
  function worstDelay(t: any) {
    let worst = null;
    for (const s of t.stop_updates ?? []) {
      const d = s.departure_delay ?? s.arrival_delay;
      if (d != null) worst = worst == null ? d : Math.max(worst, d);
    }
    return worst;
  }

  /// The bus's delay: its trip's worst known stop delay. The same
  /// join the map dots use — a bus without a prediction is grey.
  const delayByTrip = $derived.by(() => {
    const m = new Map<string, number>();
    for (const t of rt?.trips ?? []) {
      if (!t.trip_id) continue;
      const d = worstDelay(t);
      if (d != null) m.set(t.trip_id, d);
    }
    return m;
  });

  // The chips count BUSES (vehicle positions), not trip updates —
  // a trip update without a running vehicle is a prediction, not a
  // bus on the street.
  const buckets = $derived.by(() => {
    const b = { ok: 0, warn: 0, bad: 0, none: 0 };
    for (const v of app.vehicles) {
      const d = v.trip_id ? delayByTrip.get(v.trip_id) : undefined;
      if (d == null) b.none++;
      else if (d <= 60) b.ok++;
      else if (d <= 300) b.warn++;
      else b.bad++;
    }
    return b;
  });

  /// Late buses, worst first. Falls back to late trips (predictions)
  /// when the feed carries no vehicle positions.
  const late = $derived.by(() => {
    type Late = { key: string; route: string | null; trip: string | null; delay: number; busNo: string };
    const buses: Late[] = [];
    for (const v of app.vehicles) {
      const d = v.trip_id ? delayByTrip.get(v.trip_id) : undefined;
      if (d != null && d > 60) {
        buses.push({ key: v.entity_id, route: v.route_id ?? null, trip: v.trip_id ?? null, delay: d, busNo: v.label ?? v.vehicle_id ?? "" });
      }
    }
    buses.sort((a, b) => b.delay - a.delay);
    if (buses.length || app.vehicles.length) return buses.slice(0, 15);
    const preds: Late[] = [];
    for (const t of rt?.trips ?? []) {
      const d = worstDelay(t);
      if (d != null && d > 60) {
        preds.push({ key: t.trip_id ?? "?", route: t.route_id ?? null, trip: t.trip_id ?? null, delay: d, busNo: "" });
      }
    }
    preds.sort((a, b) => b.delay - a.delay);
    return preds.slice(0, 15);
  });

  const lateArePredictions = $derived(!app.vehicles.length);

  function fmtDelay(d: number) {
    const min = Math.round(d / 60);
    return min >= 1 ? `+${min} min` : `+${d} s`;
  }
</script>

<section>
  <h2>Realtime</h2>
  {#if app.vehicles.length}
    <p class="chips">
      {#if buckets.ok}<span class="chip ok">{buckets.ok} on time</span>{/if}
      {#if buckets.warn}<span class="chip warn">{buckets.warn} late 1–5 min</span>{/if}
      {#if buckets.bad}<span class="chip bad">{buckets.bad} late 5+ min</span>{/if}
      {#if buckets.none}<span class="chip">{buckets.none} no prediction</span>{/if}
      <span class="chip svc">{app.vehicles.length} buses</span>
    </p>
  {:else}
    <p class="muted">
      Trip updates only — predictions show on the boards. Bus status
      chips need vehicle positions (Data sources).
    </p>
  {/if}
  {#if rt?.alerts}<span class="chip warn">{rt.alerts} alerts</span>{/if}

  {#if late.length}
    <table>
      <tbody>
        {#each late as x (x.key)}
          <tr>
            <td><span class="route-badge">{x.route ?? "?"}</span></td>
            {#if x.busNo}<td><span class="chip svc">{x.busNo}</span></td>{/if}
            <td class="muted">{x.trip ?? "?"}</td>
            <td><span class="chip {x.delay > 300 ? "bad" : "warn"}">{fmtDelay(x.delay)}</span></td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if lateArePredictions}
      <p class="muted">Late trips (predictions — no vehicle positions in this feed).</p>
    {/if}
  {:else if rt?.trips.length && app.vehicles.length}
    <p class="muted">Every bus with data is on time.</p>
  {/if}

  <p class="muted legend">
    Buses on the map:
    <span class="dot ok"></span> on time
    <span class="dot warn"></span> 1–5 min late
    <span class="dot bad"></span> 5+ min late
    <span class="dot"></span> no data
  </p>
</section>
