<script lang="ts">
  /// The ride-now UI — a map with a panel over it. The map is the
  /// network as a rider sees it: every stop (tap one), every live bus
  /// colored by how it is running and pointing where it heads (tap
  /// one), the route in play, the planned trip. The panel follows what
  /// the rider is doing: home (search, what leaves near you, saved and
  /// recent stops), one stop (its live board, alerts, detours), getting
  /// there (a trip, now or at a time, or the last bus), the routes and
  /// one route (its directions and stops), one bus (where it is going,
  /// stop by stop), and the alerts in force. A wide
  /// screen floats the panel at the left; a phone makes it a sheet
  /// that slides up over the map.
  /// Every panel is a render component fed from the host's `ride`;
  /// this file only places them and routes their events to the host.
  import {
    ride, loadFeedBytes, setLiveUrls, typed, clearFound, show, openStop, pickDeparture, directions, setEnd, swapEnds,
    setWhen, toggleSaved, locate, nearPoint, showRoutes, filterRoutes, openRoute, openVehicle, setFollow, directionsToSpot,
    fromHere, chooseOnMap, type Place,
  } from "./host.svelte";
  import RideMap from "../../components/RideMap.svelte";
  import LiveFeeds from "../../components/LiveFeeds.svelte";
  import PlaceSearch from "../../components/PlaceSearch.svelte";
  import StopCard from "../../components/StopCard.svelte";
  import RiderBoard from "../../components/RiderBoard.svelte";
  import TripPlan from "../../components/TripPlan.svelte";
  import AlertList from "../../components/AlertList.svelte";
  import RouteList from "../../components/RouteList.svelte";
  import RouteView from "../../components/RouteView.svelte";
  import BusView from "../../components/BusView.svelte";

  const FOUND = 25;
  const alerts = $derived(ride.live?.live.alerts ?? []);
  const saved = $derived(!!ride.stop && ride.saved.some((s) => s.id === ride.stop?.id));
  const routeColor = $derived(ride.focusRoute ? (ride.routes.find((r) => r.route_id === ride.focusRoute)?.color ?? null) : null);
  const bus = $derived(ride.vehicle ? (ride.live?.live.vehicles.find((v) => v.key === ride.vehicle?.key) ?? null) : null);
  const following = $derived(ride.view === "bus" && ride.follow);
  /// The picked departure's bus: the live document's vehicle on that
  /// trip instance, when one is reporting.
  const departureBus = $derived(
    ride.departure
      ? (ride.live?.live.vehicles.find((v) => v.trip === ride.departure?.trip && v.start_secs === ride.departure?.start_secs) ?? null)
      : null,
  );
  const searchFound = $derived(ride.found?.field === "search" ? ride.found : null);
  const searchAddresses = $derived(ride.addresses?.field === "search" ? ride.addresses : null);

  // The sheet: up (the panel) or down (the map) on a phone.
  let sheetUp = $state(true);
  let narrow = $state(false);
  let height = $state(800);
  $effect(() => {
    const mq = matchMedia("(max-width: 760px)");
    const sync = () => {
      narrow = mq.matches;
      height = innerHeight;
    };
    sync();
    mq.addEventListener("change", sync);
    addEventListener("resize", sync);
    return () => {
      mq.removeEventListener("change", sync);
      removeEventListener("resize", sync);
    };
  });
  const padding = $derived(
    narrow
      ? { top: 64, right: 24, bottom: sheetUp ? Math.round(height * 0.55) : 120, left: 24 }
      : { top: 72, right: 72, bottom: 48, left: 456 },
  );

  async function stopFromMap(place: Place) {
    sheetUp = true;
    await openStop(place);
  }

  async function spotFromMap(lat: number, lon: number) {
    // A spot that cannot be an end leaves the map up to tap a stop.
    if (!ride.picking || ride.walking) sheetUp = true;
    await nearPoint(lat, lon);
  }
</script>

<div class="app">
  <div class="map">
    <RideMap
      stops={ride.stops}
      marked={ride.view === "home" ? (ride.nearby?.map((p) => p.id) ?? []) : []}
      selected={ride.view === "stop" ? ride.stop : null}
      vehicles={ride.live?.live.vehicles ?? []}
      trip={ride.view === "stop" ? (ride.departure?.trip ?? null) : null}
      picked={ride.view === "bus" ? (ride.vehicle?.key ?? null) : null}
      follow={following}
      route={ride.routeFocus}
      routeId={ride.view === "route" || ride.view === "bus" ? ride.focusRoute : null}
      {routeColor}
      frameRoute={!following}
      legs={ride.view === "trip" ? ride.paths : null}
      from={ride.view === "trip" ? ride.from : null}
      to={ride.view === "trip" ? ride.to : null}
      here={ride.here}
      {padding}
      onstop={stopFromMap}
      onpoint={spotFromMap}
      onvehicle={(v) => { if (ride.picking) return; sheetUp = true; openVehicle(v); }}
    />
  </div>

  {#if ride.picking}
    <div class="pick-banner" data-testid="pick-banner">
      Tap the map to set your {ride.picking === "from" ? "start" : "destination"}
      <button onclick={() => { sheetUp = true; chooseOnMap(null); }}>Cancel</button>
    </div>
  {/if}

  <div class="corner">
    {#if alerts.length}
      <button class="alerts-btn" onclick={() => { sheetUp = true; show("alerts"); }} data-testid="alerts-button">
        <span aria-hidden="true">⚠</span> {alerts.length} alert{alerts.length === 1 ? "" : "s"}
      </button>
    {/if}
    <LiveFeeds urls={ride.urls} summary={ride.live?.summary ?? null} at={ride.liveAt} error={ride.liveError} off={ride.off.rt} onurls={setLiveUrls} />
  </div>

  <aside class="panel" class:down={narrow && !sheetUp}>
    <button class="handle" onclick={() => (sheetUp = !sheetUp)} aria-label={sheetUp ? "show the map" : "show the panel"}><span></span></button>
    <div class="body">
      {#if ride.view === "home"}
        <header class="brand">
          <h1>{ride.feed || "Ride now"}</h1>
          <p>Where are you headed?</p>
        </header>

        {#if !ride.feed && !ride.busy}
          <label class="feed">
            <span>Load the agency’s GTFS feed to begin</span>
            <input type="file" accept=".zip" aria-label="Agency GTFS zip" onchange={async (e) => {
              const f = e.currentTarget.files?.[0];
              if (f) await loadFeedBytes(new Uint8Array(await f.arrayBuffer()), f.name);
            }} />
          </label>
        {:else if ride.busy && !ride.feed}
          <div class="loading"><span></span> Loading the network…</div>
        {:else}
          <PlaceSearch
            label="stop name"
            placeholder={ride.geocoding ? "Search for a stop or address" : "Search for a stop"}
            found={searchFound}
            addresses={searchAddresses}
            limit={FOUND}
            off={ride.off.directory}
            ontype={(t) => typed("search", t)}
            onpick={openStop}
            onclose={clearFound}
          />
          <div class="actions">
            <button class="cta" onclick={() => show("trip")}>
              <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><path d="M12 3l8.5 9-8.5 9-8.5-9z" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" /><path d="M9.5 13.5v-2a1 1 0 011-1H15m-2-2l2 2-2 2" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
              Get directions
            </button>
            <button class="cta second" onclick={showRoutes}>
              <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"><circle cx="6" cy="6" r="2.2" fill="none" stroke="currentColor" stroke-width="1.8" /><circle cx="18" cy="18" r="2.2" fill="none" stroke="currentColor" stroke-width="1.8" /><path d="M6 8.2V10a4 4 0 004 4h4a4 4 0 014 4v-2.2" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" /></svg>
              Routes
            </button>
          </div>
          <p class="tip">Tap any bus on the map to see where it’s going.</p>

          <section class="group" data-testid="nearby">
            <div class="group-head">
              <h2>{ride.near && !ride.near.mine ? `Near ${ride.near.label ?? "this spot"}` : "Near you"}</h2>
              <span class="links">
                {#if ride.walking && ride.near && !ride.near.mine}
                  <button class="link" onclick={directionsToSpot} data-testid="directions-here">Directions here</button>
                {/if}
                <button class="link" onclick={locate} disabled={ride.locating}>
                  {ride.locating ? "Locating…" : ride.near?.mine ? "Update" : "Use my location"}
                </button>
              </span>
            </div>
            {#if ride.nearby}
              {#each ride.nearby as p (p.id)}
                {@const b = ride.nearbyBoards[p.id]}
                <StopCard
                  name={p.name}
                  distance={p.distance_m ?? null}
                  walk={p.walk_secs ?? null}
                  rows={b === undefined ? null : (b?.rows ?? [])}
                  pooled={b?.pooled ?? false}
                  routes={ride.routes}
                  note={b === null ? "Departures for this stop are unavailable right now." : ""}
                  onopen={() => openStop(p)}
                />
              {:else}
                <p class="quiet">No stops near here.</p>
              {/each}
            {:else}
              <p class="quiet">Share your location to see what leaves near you — or tap anywhere on the map.</p>
            {/if}
          </section>

          {#if ride.saved.length}
            <section class="group" data-testid="saved">
              <div class="group-head"><h2>Saved</h2></div>
              {#each ride.saved as s (s.id)}
                {@const b = ride.savedBoards?.find((w) => w.id === s.id)}
                <StopCard
                  name={s.label}
                  rows={b ? b.departures : ride.off.board ? [] : null}
                  routes={ride.routes}
                  note={ride.off.board ? "Departures are unavailable right now." : "Scheduled times — live predictions aren’t available for saved stops yet."}
                  onopen={() => openStop({ id: s.id, name: s.label })}
                />
              {/each}
            </section>
          {/if}

          {#if ride.recent.length}
            <section class="group" data-testid="recent">
              <div class="group-head"><h2>Recent</h2></div>
              <div class="chips">
                {#each ride.recent as p (p.id)}
                  <button class="chip-btn" onclick={() => openStop(p)}>{p.name}</button>
                {/each}
              </div>
            </section>
          {/if}

        {/if}
      {:else if ride.view === "stop" && ride.stop}
        <RiderBoard
          name={ride.stop.name}
          stopId={ride.stop.id}
          board={ride.board}
          error={ride.boardError}
          routes={ride.routes}
          off={ride.off.board}
          liveOff={ride.off.rt}
          {saved}
          picked={ride.departure}
          tracked={departureBus ? { label: departureBus.label ?? departureBus.vehicle_id ?? departureBus.key } : null}
          ontrack={() => departureBus && openVehicle({ key: departureBus.key, trip: departureBus.trip, route: departureBus.route })}
          onpick={(r) => pickDeparture({ trip: r.trip, start_secs: r.start_secs, route: r.route })}
          onsave={() => ride.stop && toggleSaved(ride.stop)}
          onfrom={() => ride.stop && directions("from", ride.stop)}
          onto={() => ride.stop && directions("to", ride.stop)}
          onback={() => show("home")}
        />
      {:else if ride.view === "trip"}
        <TripPlan
          from={ride.from}
          to={ride.to}
          when={ride.when}
          found={ride.found}
          addresses={ride.addresses}
          limit={FOUND}
          plan={ride.plan}
          routes={ride.routes}
          here={ride.walking && !!ride.here}
          onhere={fromHere}
          picking={ride.picking}
          spots={ride.walking}
          addressable={ride.walking && ride.geocoding}
          onmap={(end) => {
            // On a phone the sheet steps aside so the map can be tapped.
            if (ride.picking !== end) sheetUp = false;
            chooseOnMap(end);
          }}
          off={ride.off.router}
          searchOff={ride.off.directory}
          ontype={typed}
          onpick={setEnd}
          onclose={clearFound}
          onswap={swapEnds}
          onwhen={setWhen}
          onback={() => show(ride.stop ? "stop" : "home")}
        />
      {:else if ride.view === "routes"}
        <RouteList rows={ride.routeRows} routes={ride.routes} query={ride.routeQuery} off={ride.off.directory}
          onfilter={filterRoutes} onopen={openRoute} onback={() => show("home")} />
      {:else if ride.view === "route"}
        <RouteView route={ride.routes.find((r) => r.route_id === ride.route) ?? null} info={ride.routeInfo} off={ride.off.directory}
          onstop={openStop} onback={showRoutes} />
      {:else if ride.view === "bus"}
        <BusView vehicle={bus} progress={ride.progress} routes={ride.routes} follow={ride.follow} liveOff={ride.off.rt}
          onfollow={setFollow} onstop={openStop} onroute={openRoute} onback={() => show("home")} />
      {:else if ride.view === "alerts"}
        <header class="sub-head">
          <button class="back" onclick={() => show("home")} aria-label="back">‹</button>
          <h2>Service alerts</h2>
        </header>
        {#if alerts.length}
          <AlertList {alerts} routes={ride.routes} />
        {:else}
          <p class="quiet">No alerts in force.</p>
        {/if}
      {/if}

      {#if ride.status}
        <p class="status" data-testid="ride-status">{ride.status}</p>
      {/if}
    </div>
  </aside>
</div>

<style>
  .app { position: relative; height: 100dvh; overflow: hidden; background: var(--hover); }
  .map { position: absolute; inset: 0; }

  .pick-banner {
    position: absolute; z-index: 3; top: 0.9rem; left: 50%; transform: translateX(-50%); display: flex; align-items: center; gap: 0.75rem;
    background: var(--text); color: var(--surface); font-size: 0.85rem; font-weight: 600; padding: 0.45rem 0.6rem 0.45rem 0.9rem;
    border-radius: 999px; box-shadow: 0 4px 16px rgb(0 0 0 / 0.2); white-space: nowrap;
  }
  .pick-banner button { border: 0; background: none; color: inherit; font: inherit; text-decoration: underline; cursor: pointer; }
  .corner { position: absolute; top: 0.9rem; right: 0.9rem; display: flex; gap: 0.5rem; align-items: flex-start; z-index: 3; }
  .alerts-btn {
    border: 0; cursor: pointer; font: inherit; font-size: 0.8rem; font-weight: 700; padding: 0.4rem 0.75rem; border-radius: 999px;
    background: var(--warn); color: #fff; box-shadow: 0 2px 10px rgb(0 0 0 / 0.15);
  }

  .panel {
    position: absolute; z-index: 2; top: 0.9rem; left: 0.9rem; bottom: 0.9rem; width: 26rem;
    background: var(--surface); border-radius: 18px; box-shadow: 0 10px 40px rgb(0 0 0 / 0.18);
    display: flex; flex-direction: column; overflow: hidden;
  }
  .handle { display: none; }
  .body { flex: 1; overflow-y: auto; padding: 1.1rem 1.1rem 1.4rem; display: flex; flex-direction: column; gap: 1rem; }

  .brand h1 { margin: 0; font-size: 1.35rem; letter-spacing: -0.01em; }
  .brand p { margin: 0.1rem 0 0; color: var(--muted); }
  .group { display: grid; gap: 0.5rem; }
  .group-head { display: flex; align-items: baseline; justify-content: space-between; }
  .group h2 { font-size: 0.78rem; text-transform: uppercase; letter-spacing: 0.08em; color: var(--muted); margin: 0; }
  .links { display: flex; gap: 0.9rem; }
  .link { border: 0; background: none; color: var(--accent); font: inherit; font-size: 0.85rem; font-weight: 600; cursor: pointer; padding: 0; }
  .quiet { color: var(--muted); font-size: 0.88rem; margin: 0; line-height: 1.45; }
  .chips { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .chip-btn {
    border: 1px solid var(--line); background: var(--surface); color: var(--text); font: inherit; font-size: 0.82rem;
    padding: 0.3rem 0.7rem; border-radius: 999px; cursor: pointer; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .chip-btn:hover { border-color: var(--accent); color: var(--accent); }
  .cta {
    display: flex; align-items: center; gap: 0.5rem; border: 0; background: var(--accent); color: #fff;
    font: inherit; font-weight: 700; padding: 0.65rem 0.9rem; border-radius: 12px; cursor: pointer; margin-top: -0.4rem;
  }
  .cta:hover { filter: brightness(1.08); }
  .actions { display: grid; grid-template-columns: 1fr 1fr; gap: 0.5rem; margin-top: -0.4rem; }
  .actions .cta { margin-top: 0; justify-content: center; }
  .cta.second { background: var(--surface); color: var(--text); border: 1px solid var(--line); }
  .cta.second:hover { border-color: var(--accent); color: var(--accent); filter: none; }
  .tip { margin: -0.4rem 0 0; font-size: 0.8rem; color: var(--muted); }
  .feed { display: grid; gap: 0.4rem; padding: 1rem; border: 2px dashed var(--line); border-radius: 14px; color: var(--muted); }
  .loading { display: flex; align-items: center; gap: 0.6rem; color: var(--muted); }
  .loading span { width: 1rem; height: 1rem; border-radius: 50%; border: 2px solid var(--line); border-top-color: var(--accent); animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .sub-head { display: flex; align-items: center; gap: 0.4rem; }
  .sub-head h2 { margin: 0; font-size: 1.2rem; }
  .back { border: 0; background: none; color: var(--text); cursor: pointer; border-radius: 10px; width: 2.25rem; height: 2.25rem; font-size: 1.6rem; line-height: 1; }
  .back:hover { background: var(--hover); }
  .status { margin: 0; font-size: 0.82rem; color: var(--muted); padding: 0.5rem 0.75rem; background: var(--hover); border-radius: 10px; }

  /* A phone: the panel is a sheet over the bottom of the map. */
  @media (max-width: 760px) {
    .panel {
      top: auto; left: 0; right: 0; bottom: 0; width: auto; height: 62dvh;
      border-radius: 18px 18px 0 0; transition: transform 0.25s ease;
    }
    .panel.down { transform: translateY(calc(62dvh - 7.5rem)); }
    .handle { display: grid; place-items: center; border: 0; background: none; padding: 0.5rem 0 0.1rem; cursor: pointer; }
    .handle span { width: 2.6rem; height: 0.3rem; border-radius: 999px; background: var(--line); }
    .body { padding-top: 0.4rem; }
    .corner { top: 0.6rem; right: 0.6rem; }
  }
</style>
