<script>
  import { app } from "../state.svelte.js";

  const rt = $derived(app.rtOverview);

  /// Worst known delay for a trip update (max over its stops).
  function worstDelay(t) {
    let worst = null;
    for (const s of t.stop_updates ?? []) {
      const d = s.departure_delay ?? s.arrival_delay;
      if (d != null) worst = worst == null ? d : Math.max(worst, d);
    }
    return worst;
  }

  const buckets = $derived.by(() => {
    const b = { ok: 0, warn: 0, bad: 0 };
    for (const t of rt?.trips ?? []) {
      const d = worstDelay(t);
      if (d == null) continue;
      if (d <= 60) b.ok++;
      else if (d <= 300) b.warn++;
      else b.bad++;
    }
    return b;
  });

  const late = $derived.by(() =>
    (rt?.trips ?? [])
      .map((t) => ({ t, delay: worstDelay(t) }))
      .filter((x) => x.delay != null && x.delay > 60)
      .sort((a, b) => b.delay - a.delay)
      .slice(0, 15)
  );

  function fmtDelay(d) {
    const min = Math.round(d / 60);
    return min >= 1 ? `+${min} min` : `+${d} s`;
  }
</script>

<section>
  <h2>Realtime</h2>
  <p class="chips">
    {#if buckets.ok}<span class="chip ok">{buckets.ok} on time</span>{/if}
    {#if buckets.warn}<span class="chip warn">{buckets.warn} late 1–5 min</span>{/if}
    {#if buckets.bad}<span class="chip bad">{buckets.bad} late 5+ min</span>{/if}
    {#if rt.vehicles}<span class="chip svc">{rt.vehicles} vehicles</span>{/if}
    {#if rt.alerts}<span class="chip warn">{rt.alerts} alerts</span>{/if}
  </p>

  {#if late.length}
    <table>
      <tbody>
        {#each late as { t, delay } (t.trip_id)}
          <tr>
            <td><span class="route-badge">{t.route_id ?? "?"}</span></td>
            <td class="muted">{t.trip_id}</td>
            <td><span class="chip {delay > 300 ? "bad" : "warn"}">{fmtDelay(delay)}</span></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else if rt.trips.length}
    <p class="muted">Everything with data is on time.</p>
  {:else}
    <p class="muted">No trip updates in this feed.</p>
  {/if}

  <p class="muted legend">
    Buses on the map:
    <span class="dot ok"></span> on time
    <span class="dot warn"></span> 1–5 min late
    <span class="dot bad"></span> 5+ min late
    <span class="dot"></span> no data
  </p>
</section>
