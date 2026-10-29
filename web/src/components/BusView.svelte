<script lang="ts">
  /// One bus — a RENDER component over the live document's vehicle
  /// (its route, its status under the document's window, the stop it
  /// is at or heading to) and rt's `trip_progress` for its trip (every
  /// stop with the scheduled and predicted time), its stops named by
  /// the directory's `stops_by_ids`. The stop it heads to is marked;
  /// the stops before it read as passed.
  import type { LiveVehicle } from "../types/LiveVehicle";
  import type { TripProgress } from "../types/TripProgress";
  import type { DirectoryStop } from "../types/cap_directory/DirectoryStop";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import { clock, routeColor, statusText, statusTone } from "./rideFormat";

  let {
    vehicle,
    progress,
    routes,
    follow,
    liveOff,
    onfollow,
    onstop,
    onroute,
    onback,
  }: {
    /// The bus in the current live document; null when it stopped
    /// reporting.
    vehicle: LiveVehicle | null;
    progress: { progress: TripProgress | null; names: DirectoryStop[] } | null;
    routes: DirectoryRoute[];
    follow: boolean;
    liveOff: boolean;
    onfollow: (on: boolean) => void;
    onstop: (stop: { id: string; name: string; lat?: number; lon?: number }) => void;
    onroute: (routeId: string) => void;
    onback: () => void;
  } = $props();

  const route = $derived(vehicle?.route ? routes.find((r) => r.route_id === vehicle.route) : undefined);
  const names = $derived(new Map((progress?.names ?? []).map((s) => [s.stop_id, s])));
  const next = $derived(progress?.progress?.stops.findIndex((s) => s.stop_id === vehicle?.next_stop) ?? -1);
  const tone = $derived(vehicle ? statusTone(vehicle.status) : "sched");
</script>

<section class="bus" data-testid="bus-view" style:--route={routeColor(route?.color)}>
  <header>
    <button class="back" onclick={onback} aria-label="back">‹</button>
    {#if vehicle?.route_name}<span class="badge">{vehicle.route_name}</span>{/if}
    <div class="title">
      <h2>{route?.long_name ?? (vehicle?.route_name ? `Route ${vehicle.route_name}` : "Bus")}</h2>
      {#if vehicle}<span class="sub">Bus {vehicle.label ?? vehicle.vehicle_id ?? vehicle.key}</span>{/if}
    </div>
  </header>

  {#if liveOff}
    <p class="off">Live data is unavailable right now.</p>
  {:else if !vehicle}
    <p class="quiet">This bus has stopped reporting its position.</p>
  {:else}
    <div class="state">
      <span class="status {tone}" data-status={vehicle.status}>{statusText(vehicle.status, vehicle.delay_secs)}</span>
      <label class="follow"><input type="checkbox" checked={follow} onchange={(e) => onfollow(e.currentTarget.checked)} /> Follow on the map</label>
    </div>
    {#if vehicle.route}
      <button class="link" onclick={() => vehicle?.route && onroute(vehicle.route)}>See this route’s stops ›</button>
    {/if}

    {#if !vehicle.trip}
      <p class="quiet">This bus isn’t on a scheduled trip right now.</p>
    {:else if !progress}
      <div class="skeleton"><span></span><span></span><span></span></div>
    {:else if !progress.progress}
      <p class="quiet">No predictions for this bus’s trip right now.</p>
    {:else}
      <ol class="line" data-testid="bus-stops">
        {#each progress.progress.stops as s, i (s.stop_id + ":" + s.stop_sequence)}
          {@const stop = names.get(s.stop_id)}
          {@const sched = s.scheduled_departure ?? s.scheduled_arrival}
          {@const pred = s.predicted_departure ?? s.predicted_arrival}
          <li class:passed={next >= 0 && i < next} class:next={i === next}>
            <button onclick={() => onstop({ id: s.stop_id, name: stop?.name ?? s.stop_id, lat: stop?.lat, lon: stop?.lon })}>
              <span class="name">{stop?.name ?? s.stop_id}</span>
              {#if pred != null}
                <span class="t">{clock(pred).hm}<small>{clock(pred).ap}</small>
                  {#if sched != null && sched !== pred}<s>{clock(sched).hm}</s>{/if}</span>
              {:else if sched != null}
                <span class="t sched">{clock(sched).hm}<small>{clock(sched).ap}</small></span>
              {/if}
            </button>
          </li>
        {/each}
      </ol>
    {/if}
  {/if}
</section>

<style>
  header { display: flex; align-items: center; gap: 0.5rem; }
  .back { border: 0; background: none; color: var(--text); cursor: pointer; border-radius: 10px; width: 2.25rem; height: 2.25rem; font-size: 1.6rem; line-height: 1; flex: none; }
  .back:hover { background: var(--hover); }
  .badge { min-width: 2.8rem; padding: 0.2rem 0.45rem; border-radius: 8px; text-align: center; font-weight: 750; background: var(--route, var(--accent)); color: #fff; flex: none; }
  .title { min-width: 0; }
  h2 { font-size: 1.1rem; margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { color: var(--muted); font-size: 0.78rem; }
  .state { display: flex; align-items: center; justify-content: space-between; gap: 0.5rem; margin: 0.7rem 0 0.2rem; }
  .status { font-weight: 750; font-size: 1.05rem; }
  .ok { color: var(--ok); }
  .warn { color: var(--warn); }
  .bad { color: var(--bad); }
  .sched { color: var(--muted); }
  .follow { font-size: 0.82rem; color: var(--muted); display: flex; gap: 0.3rem; align-items: center; cursor: pointer; }
  .link { border: 0; background: none; color: var(--accent); font: inherit; font-size: 0.85rem; font-weight: 600; padding: 0; cursor: pointer; }
  .line { list-style: none; margin: 0.8rem 0 0; padding: 0; position: relative; }
  .line::before { content: ""; position: absolute; left: 0.55rem; top: 0.9rem; bottom: 0.9rem; width: 4px; border-radius: 2px; background: var(--route, var(--accent)); }
  .line li { position: relative; padding-left: 1.6rem; }
  .line li::before {
    content: ""; position: absolute; left: 0.3rem; top: 50%; translate: 0 -50%; width: 8px; height: 8px; border-radius: 50%;
    background: var(--surface); border: 2.5px solid var(--route, var(--accent));
  }
  .line li.passed { opacity: 0.45; }
  .line li.next::before { width: 14px; height: 14px; left: 0.15rem; background: var(--route, var(--accent)); box-shadow: 0 0 0 4px color-mix(in srgb, var(--route, var(--accent)) 30%, transparent); }
  .line li.next .name { font-weight: 750; }
  .line button {
    width: 100%; display: flex; justify-content: space-between; gap: 0.5rem; border: 0; background: none; color: var(--text);
    font: inherit; font-size: 0.88rem; padding: 0.32rem 0.3rem; cursor: pointer; text-align: left; border-radius: 6px;
  }
  .line button:hover { background: var(--hover); }
  .name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t { font-variant-numeric: tabular-nums; font-weight: 650; white-space: nowrap; }
  .t small { color: var(--muted); margin-left: 0.1em; font-weight: 500; }
  .t s { color: var(--muted); font-weight: 400; margin-left: 0.35rem; font-size: 0.8rem; }
  .quiet { color: var(--muted); margin: 0.8rem 0; }
  .off { color: var(--bad); font-size: 0.9rem; }
  .skeleton { display: grid; gap: 0.5rem; padding: 0.75rem 0; }
  .skeleton span { height: 2rem; border-radius: 10px; background: linear-gradient(90deg, var(--hover), var(--line), var(--hover)); background-size: 200% 100%; animation: shimmer 1.2s infinite; }
  @keyframes shimmer { to { background-position: -200% 0; } }
</style>
