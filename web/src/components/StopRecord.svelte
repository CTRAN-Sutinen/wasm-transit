<script lang="ts">
  /// One stop as the network holds it (cap-directory `stop_detail`):
  /// its record — name, code, point, station, transfer, access, fare
  /// zone — and every pattern that visits it, by route, with the trips
  /// each runs under the day type. A visit opens its route. A RENDER
  /// component: the detail comes in as a prop; formatting only.
  import type { StopDetail } from "../types/StopDetail";
  import type { StopVisit } from "../types/StopVisit";

  let {
    stopId,
    detail,
    stationName,
    groupName,
    onvisit,
  }: {
    stopId: string;
    detail: StopDetail;
    /// The parent station's name, when the stop is a bay of one.
    stationName: string | null;
    /// The day type the trips are counted under.
    groupName: string;
    onvisit: (route: string, direction: string | null) => void;
  } = $props();

  const s = $derived(detail.stop);
  const access = $derived(s.info?.wheelchair == null ? "not recorded" : s.info.wheelchair ? "yes" : "no");

  /// The visits by route, in the answer's order.
  const byRoute = $derived.by(() => {
    const out: { route: string; name: string; visits: StopVisit[] }[] = [];
    for (const v of detail.visits) {
      const last = out[out.length - 1];
      if (last?.route === v.route) last.visits.push(v);
      else out.push({ route: v.route, name: v.route_name, visits: [v] });
    }
    return out;
  });

  const where = (v: StopVisit) => (v.endpoint ? (v.index === 0 ? "first stop" : "last stop") : `stop ${v.index + 1}`);
</script>

<section class="stop-record" data-testid="stop-record">
  <h2>{s.name || stopId}</h2>
  <dl>
    <dt>Id</dt><dd>{stopId}</dd>
    {#if s.info?.code}<dt>Code</dt><dd>{s.info.code}</dd>{/if}
    <dt>Kind</dt><dd>{s.kind === "station" ? "Station" : "Stop"}</dd>
    {#if s.parent}<dt>Station</dt><dd>{stationName ?? s.parent}</dd>{/if}
    <dt>Location</dt><dd>{s.lat.toFixed(5)}, {s.lon.toFixed(5)}</dd>
    <dt>Transfer point</dt><dd>{s.transfer ? "yes" : "no"}</dd>
    <dt>Wheelchair boarding</dt><dd>{access}</dd>
    {#if s.info?.fare_zone}<dt>Fare zone</dt><dd>{s.info.fare_zone}</dd>{/if}
    {#if s.info?.description}<dt>Description</dt><dd>{s.info.description}</dd>{/if}
  </dl>

  <h3>What serves it</h3>
  {#if detail.visits.length === 0}
    <p class="muted" data-testid="stop-unserved">
      No pattern visits this {s.kind === "station" ? "station itself" : "stop"}.{#if detail.routes.length}{" "}{s.kind === "station" ? "Its stops are" : "Its station is"}
        served by {detail.routes.map((r) => r.name).join(", ")}.{/if}
    </p>
  {:else}
    <p class="muted">Trips are counted under {groupName || "every service key"}.</p>
    <ul class="visits">
      {#each byRoute as r (r.route)}
        <li>
          <strong>{r.name}</strong>
          <ul>
            {#each r.visits as v (`${v.pattern}#${v.index}`)}
              <li data-testid="stop-visit">
                <button type="button" class="link" onclick={() => onvisit(v.route, v.direction)}>
                  {v.direction_name ? `to ${v.direction_name}` : "no direction"} · {v.pattern_name || v.pattern}
                </button>
                <span class="muted">{where(v)}{v.timepoint ? ", timepoint" : ""} · {v.trips} {v.trips === 1 ? "trip" : "trips"}</span>
              </li>
            {/each}
          </ul>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  h2 { margin: 0; font-size: 1rem; }
  dl { display: grid; grid-template-columns: max-content 1fr; gap: 0.2rem 0.8rem; margin: 0.4rem 0 0.8rem; }
  dt { color: var(--muted); }
  dd { margin: 0; }
  h3 { font-size: 0.95rem; margin: 0.6rem 0 0.3rem; }
  .visits, .visits ul { list-style: none; padding: 0; margin: 0; }
  .visits > li { margin-bottom: 0.5rem; }
  .visits ul li { display: flex; flex-wrap: wrap; gap: 0.2rem 0.6rem; align-items: baseline; padding-left: 0.8rem; }
  .link { text-align: left; background: none; border: none; padding: 0; color: var(--accent); cursor: pointer; text-decoration: underline; font: inherit; }
</style>
