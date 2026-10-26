<script lang="ts">
  import { app } from "../state.svelte";
  import { selectStop, showTrip, refreshNow, serviceDate, secondsSinceMidnight, feedBytes } from "../actions";
  import { ensureGtfsFeed, gtfsCap, gtfsModule } from "../capabilities.svelte";
  import type { Departure } from "../types/Departure";
  import Combobox from "./Combobox.svelte";

  const stops = $derived(app.gtfs ? app.gtfs.stops().slice(0, 2000) : []);
  const stopOptions = $derived(stops.map((s) => ({ value: s.stop_id, label: s.name, sub: s.stop_id })));

  // Service keys running today — through the BUS (the monolith's op
  // is deleted). Decoration-grade: when the capability cannot
  // assemble the chips simply do not render (the board's rows are
  // unaffected).
  let services = $state<string[]>([]);
  let servicesSeq = 0;
  $effect(() => {
    void app.clock;
    const date = serviceDate();
    const seq = ++servicesSeq;
    const bytes = feedBytes();
    if (!app.gtfs || !bytes) {
      services = [];
      return;
    }
    void (async () => {
      try {
        const feedHash = await ensureGtfsFeed(bytes);
        if (seq !== servicesSeq || !feedHash) return;
        const module = await gtfsModule();
        if (!module || seq !== servicesSeq) return;
        services = gtfsCap(module).activeServices(feedHash, date);
      } catch {
        if (seq === servicesSeq) services = [];
      }
    })();
  });

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

  function joinRt(raw: Departure[]): (Departure & { delay: number | null; expected: number | null })[] {
    return raw.map((d) => {
      const rt = rtTrips.get(d.trip_id);
      const delay = rt ? (rt.byStop.get(d.stop_id) ?? rt.worst) : null;
      return {
        ...d,
        delay,
        expected: delay != null && d.departure_time != null ? d.departure_time + delay : null,
      };
    });
  }

  const deps = $derived.by(() => {
    void app.clock; // time moved
    void app.feedEpoch; // the effective feed changed (edits, reload)
    if (!app.gtfs || !app.selectedStop) return [];
    const raw = app.gtfs.departures(app.selectedStop, serviceDate(), secondsSinceMidnight(), 15);
    return joinRt(raw);
  });

  // -----------------------------------------------------------------
  // The STATION BOARD: the monolith has no pooled op (the bus owns
  // it), so a stop that answers nothing per-stop (a station pin —
  // its bays carry the trips) falls back to cap-gtfs's
  // station_departures, the walking group pooled, every row tagged
  // with the concrete bay. Async through the resolver, races
  // guarded; when the capability cannot assemble the board stays
  // empty and says so. (The board's destination is realtime-first —
  // see CAPS-PLAN; this is the schedule-side fallback landing first.)
  // -----------------------------------------------------------------
  /// Station-ness, from the feed's own stop structure: the selected
  /// stop is station-like when it HAS a parent (a bay) or HAS
  /// children (a station pin). Plain stops never pool — their
  /// per-stop board is exact (diversion-patched), and a diverted-away
  /// stop must show its emptiness, not its base schedule.
  const stationLike = $derived.by(() => {
    const sel = stops.find((s) => s.stop_id === app.selectedStop);
    if (!sel) return false;
    return sel.parent_station != null || stops.some((s) => s.parent_station === app.selectedStop);
  });

  let pooled = $state<{ rows: (Departure & { delay: number | null; expected: number | null })[] } | null>(null);
  let pooledSeq = 0;
  $effect(() => {
    void app.clock;
    void app.feedEpoch;
    void app.rtOverview;
    const stop = app.selectedStop;
    const perStop = deps;
    const seq = ++pooledSeq;
    if (!stop || !app.gtfs || perStop.length > 0 || !stationLike) {
      pooled = null;
      return;
    }
    const bytes = feedBytes();
    if (!bytes) {
      pooled = null;
      return;
    }
    void (async () => {
      try {
        const feedHash = await ensureGtfsFeed(bytes);
        if (seq !== pooledSeq) return;
        if (!feedHash) {
          pooled = null;
          app.status =
            "gtfs capability unavailable — station boards are off until it loads (the capability bus did not assemble).";
          return;
        }
        const module = await gtfsModule();
        if (!module || seq !== pooledSeq) return;
        const rows = gtfsCap(module).stationDepartures(
          feedHash,
          stop,
          serviceDate(),
          secondsSinceMidnight(),
          15,
        );
        if (seq !== pooledSeq) return;
        pooled = rows.length ? { rows: joinRt(rows) } : null;
      } catch {
        if (seq === pooledSeq) pooled = null;
      }
    })();
  });

  const board = $derived(pooled ? pooled.rows : deps);

  function fmtClock(secs: number) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
  }

  function delayChip(delay: number | null) {
    if (delay == null) return null;
    if (delay <= 60) return { label: "on time", cls: "ok" };
    const min = Math.round(delay / 60);
    return { label: `+${min} min`, cls: min > 5 ? "bad" : "warn" };
  }
</script>

<section>
  <h2>Departures</h2>
  <p class="picker-row">
    <Combobox
      label="Stop"
      options={stopOptions}
      value={app.selectedStop ?? ""}
      onchange={(v) => selectStop(v)}
    />
    <button onclick={refreshNow}>Refresh</button>
    {#each services as s (s)}
      <span class="chip svc" title="service key running today">{s}</span>
    {/each}
  </p>

  {#if board.length}
    <table>
      <tbody>
        {#each board as d (d.trip_id + d.stop_id + d.stop_sequence)}
          {@const chip = delayChip(d.delay)}
          <tr class="clickable-row" class:selected={app.selectedTripId === d.trip_id} onclick={() => showTrip(d.trip_id)}>
            <td class="time">
              {fmtClock(d.departure_time ?? 0)}
              {#if d.expected != null && d.expected !== d.departure_time}
                <span class="expected" class:late={(d.delay ?? 0) > 60} class:early={(d.delay ?? 0) < 0}>
                  → {fmtClock(d.expected)}
                </span>
              {/if}
            </td>
            <td><span class="route-badge">{d.route_name}</span></td>
            <td>{d.headsign ?? ""}</td>
            {#if pooled}
              <td class="muted" title="the concrete bay this leaves from">{d.stop_id}</td>
            {/if}
            <td>{#if chip}<span class="chip {chip.cls}">{chip.label}</span>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if pooled}
      <p class="muted">
        Station board — every bay at this stop's group, tagged by bay.
        {#if app.rtOverview}Times include live delays.{/if}
      </p>
    {:else}
      <p class="muted">Click a departure to draw its path. {#if app.rtOverview}Times include live delays.{/if}</p>
    {/if}
  {:else if app.selectedStop}
    <p class="muted">Nothing upcoming at this stop right now.</p>
  {/if}
</section>
