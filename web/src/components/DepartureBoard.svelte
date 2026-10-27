<script lang="ts">
  import { app, getEditDocument } from "../state.svelte";
  import { selectStop, showTrip, refreshNow, serviceDate, secondsSinceMidnight, feedBytes } from "../actions";
  import {
    boardCap,
    boardModule,
    ensureBoardBoarding,
    ensureBoardEdits,
    calendarCap,
    calendarModule,
    ensureCalendarNetwork,
    type Boarding,
  } from "../capabilities.svelte";
  import type { BoardDeparture } from "../types/BoardDeparture";
  import Combobox from "./Combobox.svelte";

  const stops = $derived(app.gtfs ? app.gtfs.stops().slice(0, 2000) : []);
  const stopOptions = $derived(stops.map((s) => ({ value: s.stop_id, label: s.name, sub: s.stop_id })));

  // Service keys running today — through the BUS (the calendar
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
        const networkHash = await ensureCalendarNetwork(bytes);
        if (seq !== servicesSeq || !networkHash) return;
        const module = await calendarModule();
        if (!module || seq !== servicesSeq) return;
        services = calendarCap(module).activeServices(networkHash, date);
      } catch {
        if (seq === servicesSeq) services = [];
      }
    })();
  });

  /// trip_id → { byStop: Map(stop_id → delay), worst: delay }.
  /// Joined from the GTFS-rt trip updates, when a feed is loaded.
  /// Keyed by the AGENCY's trip ids — the join to the board's
  /// model-native rows goes through the import's trip-id map.
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

  type Row = BoardDeparture & { delay: number | null; expected: number | null; agencyTripIds: string[] };

  // -----------------------------------------------------------------
  // THE BOARD, through the capability bus: the loaded feed boards
  // into cap-board (network document + trip-id map), the edit
  // document registers against it, and the day's rows answer from
  // the module — diversion-patched, model-native ((schedule,
  // start) identity, concrete stop, final-stop destination). A
  // station-like stop that answers nothing per-stop falls back to
  // the module's own station board (the walking group pooled, rows
  // tagged with the concrete bay). When the assembly cannot form
  // the board is OFF and says so — never silently empty.
  // -----------------------------------------------------------------
  let board = $state<{ rows: Row[]; pooled: boolean } | null>(null);
  let boardOff = $state(false);
  let boardSeq = 0;
  $effect(() => {
    void app.clock; // time moved
    void app.feedEpoch; // the effective feed changed (edits, reload)
    void app.rtOverview;
    const stop = app.selectedStop;
    const seq = ++boardSeq;
    const bytes = feedBytes();
    if (!stop || !app.gtfs || !bytes) {
      board = null;
      boardOff = false;
      return;
    }
    void (async () => {
      let boarding: Boarding | null = null;
      try {
        boarding = await ensureBoardBoarding(bytes);
        if (seq !== boardSeq) return;
        if (!boarding) {
          board = null;
          boardOff = true;
          return;
        }
        // The derived (schedule, start) → agency ids join: rt
        // delays and click-through are keyed by the agency's ids.
        const agency = new Map(
          boarding.tripMap.entries.map((e) => [`${e.schedule}@${e.start_secs}`, e.agency_trip_ids]),
        );
        const join = (rows: BoardDeparture[]): Row[] =>
          rows.map((d) => {
            const ids = agency.get(`${d.schedule}@${d.start_secs}`) ?? [];
            let delay = null as number | null;
            for (const id of ids) {
              const rt = rtTrips.get(id);
              if (!rt) continue;
              delay = rt.byStop.get(d.stop) ?? rt.worst;
              break;
            }
            return {
              ...d,
              delay,
              expected: delay != null ? d.departure_secs + delay : null,
              agencyTripIds: ids,
            };
          });
        const editsHash = await ensureBoardEdits(
          boarding.networkHash,
          getEditDocument(),
        );
        if (seq !== boardSeq) return;
        const module = await boardModule();
        if (!module || seq !== boardSeq) return;
        const date = serviceDate();
        const cap = boardCap(module);
        let rows = cap.departures(
          boarding.networkHash,
          editsHash,
          stop,
          date,
          secondsSinceMidnight(),
          15,
        );
        let pooled = false;
        // Station-ness, from the feed's own stop structure: a stop
        // with a parent (a bay) or children (a station pin). Plain
        // stops never pool — a diverted-away stop must show its
        // emptiness, not its base schedule.
        const sel = stops.find((s) => s.stop_id === stop);
        const stationLike =
          !!sel &&
          (sel.parent_station != null || stops.some((s) => s.parent_station === stop));
        if (rows.length === 0 && stationLike) {
          rows = cap.stationDepartures(
            boarding.networkHash,
            editsHash,
            stop,
            date,
            secondsSinceMidnight(),
            15,
          );
          pooled = true;
        }
        boardOff = false;
        board = { rows: join(rows), pooled };
      } catch {
        if (seq === boardSeq) {
          board = null;
          boardOff = true;
        }
      }
    })();
  });

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

  {#if board && board.rows.length}
    <table>
      <tbody>
        {#each board.rows as d (d.schedule + d.start_secs + d.stop)}
          {@const chip = delayChip(d.delay)}
          <tr
            class="clickable-row"
            class:selected={board.pooled ? false : app.selectedTripId === d.agencyTripIds[0]}
            onclick={() => d.agencyTripIds[0] && showTrip(d.agencyTripIds[0])}
          >
            <td class="time">
              {fmtClock(d.departure_secs)}
              {#if d.expected != null && d.expected !== d.departure_secs}
                <span class="expected" class:late={(d.delay ?? 0) > 60} class:early={(d.delay ?? 0) < 0}>
                  → {fmtClock(d.expected)}
                </span>
              {/if}
            </td>
            <td><span class="route-badge">{d.route_name}</span></td>
            <td>{d.destination}</td>
            {#if board.pooled}
              <td class="muted" title="the concrete bay this leaves from">{d.stop}</td>
            {/if}
            <td>{#if chip}<span class="chip {chip.cls}">{chip.label}</span>{/if}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if board.pooled}
      <p class="muted">
        Station board — every bay at this stop's group, tagged by bay.
        {#if app.rtOverview}Times include live delays.{/if}
      </p>
    {:else}
      <p class="muted">Click a departure to draw its path. {#if app.rtOverview}Times include live delays.{/if}</p>
    {/if}
  {:else if boardOff && app.selectedStop}
    <p class="muted">
      The board needs the network and board capabilities; the bus
      did not assemble them, so the board is off. Other views are
      unaffected.
    </p>
  {:else if app.selectedStop}
    <p class="muted">Nothing upcoming at this stop right now.</p>
  {/if}
</section>
