<script lang="ts">
  /// Getting there — a RENDER component: the trip's two ends (stop
  /// type-aheads, the rider's location, or chosen on the map), when (leave now, leave at, arrive by, or the last
  /// bus), and cap-router's answer: the itinerary, and the rider's
  /// notice when a detour changes the trip (`plan_compare`).
  import type { Itinerary as Plan } from "../types/Itinerary";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import PlaceSearch from "./PlaceSearch.svelte";
  import Itinerary from "./Itinerary.svelte";

  type Place = { id: string; name: string; lat?: number; lon?: number; detail?: string };
  type Field = "search" | "from" | "to";
  type When = { mode: "now" | "depart" | "arrive" | "last"; secs: number | null };

  let {
    from,
    to,
    when,
    found,
    addresses = null,
    limit,
    plan,
    routes,
    here = false,
    onhere,
    picking = null,
    spots = false,
    addressable = false,
    onmap,
    off,
    searchOff,
    ontype,
    onpick,
    onclose,
    onswap,
    onwhen,
    onback,
  }: {
    from: Place | null;
    to: Place | null;
    when: When;
    found: { field: Field; text: string; places: Place[] } | null;
    /// The geocoder's addresses for the field being typed, if any.
    addresses?: { field: Field; text: string; places: Place[] } | null;
    limit: number;
    plan: { itinerary: Plan | null; notice: string | null; error?: string } | null;
    routes: DirectoryRoute[];
    /// The rider's location can be an end (walking the streets).
    here?: boolean;
    onhere?: (field: "from" | "to") => void;
    /// The end the next tap on the map sets, if the rider is choosing.
    picking?: "from" | "to" | null;
    /// Any spot can be an end (the streets walk it); else only a stop.
    spots?: boolean;
    /// The fields take addresses (a geocoder, and streets to walk).
    addressable?: boolean;
    /// Choose an end on the map (asked again: cancel).
    onmap?: (field: "from" | "to") => void;
    /// The router capability did not assemble.
    off: boolean;
    searchOff: boolean;
    ontype: (field: Field, text: string) => void;
    onpick: (field: "from" | "to", place: Place) => void;
    onclose: () => void;
    onswap: () => void;
    onwhen: (when: When) => void;
    onback: () => void;
  } = $props();

  const MODES = [
    { mode: "now", label: "Leave now" },
    { mode: "depart", label: "Leave at" },
    { mode: "arrive", label: "Arrive by" },
    { mode: "last", label: "Last bus" },
  ] as const;

  /// The time field speaks "HH:MM"; the op speaks seconds.
  function hhmm(secs: number | null): string {
    if (secs == null) return "";
    return `${String(Math.floor(secs / 3600) % 24).padStart(2, "0")}:${String(Math.floor((secs % 3600) / 60)).padStart(2, "0")}`;
  }

  function secsOf(v: string): number | null {
    const m = /^(\d{1,2}):(\d{2})$/.exec(v);
    return m ? Number(m[1]) * 3600 + Number(m[2]) * 60 : null;
  }

  function mode(m: When["mode"]) {
    if (m === "now" || m === "last") onwhen({ mode: m, secs: null });
    else onwhen({ mode: m, secs: when.secs ?? secsOf(new Date().toTimeString().slice(0, 5)) });
  }

  const forField = (f: Field) => (found && found.field === f ? found : null);
  const addressesFor = (f: Field) => (addresses && addresses.field === f ? addresses : null);
</script>

<section class="trip" data-testid="trip-plan">
  <header>
    <button class="back" onclick={onback} aria-label="back">‹</button>
    <h2>Get there</h2>
  </header>

  <div class="ends">
    <div class="fields">
      <PlaceSearch label="From" placeholder={addressable ? "From — a stop or address" : "From — a stop"} icon="from" value={from?.name ?? ""} found={forField("from")} addresses={addressesFor("from")} {limit} off={searchOff}
        ontype={(t) => ontype("from", t)} onpick={(p) => onpick("from", p)} {onclose} />
      <PlaceSearch label="To" placeholder={addressable ? "To — a stop or address" : "To — a stop"} icon="to" value={to?.name ?? ""} found={forField("to")} addresses={addressesFor("to")} {limit} off={searchOff}
        ontype={(t) => ontype("to", t)} onpick={(p) => onpick("to", p)} {onclose} />
    </div>
    <button class="swap" onclick={onswap} aria-label="swap from and to" title="Swap">
      <svg viewBox="0 0 24 24" width="20" height="20"><path d="M8 4v15m0 0l-3.5-3.5M8 19l3.5-3.5M16 20V5m0 0l-3.5 3.5M16 5l3.5 3.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg>
    </button>
  </div>

  {#if onmap || (here && onhere)}
    <div class="choose" data-testid="choose-ends">
      {#each [["from", "Start", from], ["to", "Destination", to]] as const as [field, label, end] (field)}
        <div class="row">
          <span class="label">{label}</span>
          {#if here && onhere && end?.id !== "@here"}
            <button class="link" onclick={() => onhere(field)} aria-label={`${label}: my location`}>My location</button>
          {/if}
          {#if onmap}
            <button class="link" class:on={picking === field} aria-pressed={picking === field}
              aria-label={`${label}: choose on map`} onclick={() => onmap(field)}>
              {picking === field ? "Cancel" : "Choose on map"}
            </button>
          {/if}
        </div>
      {/each}
    </div>
    {#if picking}
      <p class="picking" data-testid="picking">Tap the map to set your {picking === "from" ? "start" : "destination"} — {spots ? "a stop, or any spot" : "a stop"}.</p>
    {/if}
  {/if}

  <div class="when" role="radiogroup" aria-label="when">
    {#each MODES as m (m.mode)}
      <button role="radio" aria-checked={when.mode === m.mode} class:on={when.mode === m.mode} onclick={() => mode(m.mode)}>{m.label}</button>
    {/each}
  </div>
  {#if when.mode === "depart" || when.mode === "arrive"}
    <label class="time">
      {when.mode === "depart" ? "Leave at" : "Arrive by"}
      <input type="time" value={hhmm(when.secs)} onchange={(e) => onwhen({ mode: when.mode, secs: secsOf(e.currentTarget.value) })} />
    </label>
  {/if}

  {#if off}
    <p class="off">Trip planning is unavailable right now — the router capability did not load.</p>
  {:else if !from || !to}
    <p class="hint">Choose where you’re starting and where you’re going — type a stop, or tap one on the map and choose “Directions”.</p>
  {:else if from.id === to.id}
    <p class="hint">Those are the same place.</p>
  {:else if !plan}
    <div class="skeleton" aria-label="planning"><span></span><span></span></div>
  {:else if plan.error}
    <p class="off">Couldn’t plan this trip right now ({plan.error}).</p>
  {:else}
    <div class="answer" data-testid="plan-now">
      {#if plan.notice}
        <p class="notice" data-testid="plan-notice"><span aria-hidden="true">⤳</span> {plan.notice}</p>
      {/if}
      {#if plan.itinerary}
        {#if when.mode === "last"}<p class="label" data-testid="last-bus">The last trip today</p>{/if}
        <Itinerary itinerary={plan.itinerary} {routes} />
      {:else if when.mode === "last" || when.mode === "arrive"}
        <p class="hint">No bus gets you to {to.name} {when.mode === "arrive" ? "by then" : "today"}.</p>
      {:else}
        <p class="hint">No more trips from {from.name} to {to.name} today.</p>
      {/if}
    </div>
  {/if}
</section>

<style>
  header { display: flex; align-items: center; gap: 0.4rem; margin-bottom: 0.5rem; }
  h2 { font-size: 1.2rem; margin: 0; }
  .back, .swap {
    border: 0; background: none; color: var(--text); cursor: pointer; border-radius: 10px;
    width: 2.25rem; height: 2.25rem; display: grid; place-items: center; font-size: 1.6rem; line-height: 1; flex: none;
  }
  .back:hover, .swap:hover { background: var(--hover); }
  .ends { display: flex; align-items: center; gap: 0.3rem; }
  .fields { flex: 1; display: grid; gap: 0.4rem; min-width: 0; }
  .choose { display: grid; gap: 0.25rem; margin-top: 0.5rem; }
  .row { display: flex; align-items: baseline; gap: 0.9rem; }
  .label { font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); width: 6.5rem; }
  .link.on { text-decoration: underline; }
  .picking { margin: 0.4rem 0 0; padding: 0.45rem 0.65rem; border-radius: 10px; font-size: 0.85rem; background: color-mix(in srgb, var(--accent) 12%, transparent); }
  .link { border: 0; background: none; color: var(--accent); font: inherit; font-size: 0.85rem; font-weight: 600; cursor: pointer; padding: 0; }
  .when { display: flex; gap: 0.25rem; margin: 0.75rem 0 0.4rem; background: var(--hover); padding: 0.2rem; border-radius: 10px; }
  .when button {
    flex: 1; border: 0; background: none; color: var(--muted); font: inherit; font-size: 0.82rem; font-weight: 600;
    padding: 0.4rem 0.2rem; border-radius: 8px; cursor: pointer; white-space: nowrap;
  }
  .when button.on { background: var(--surface); color: var(--text); box-shadow: 0 1px 3px rgb(0 0 0 / 0.12); }
  .time { display: flex; align-items: center; gap: 0.5rem; font-size: 0.88rem; color: var(--muted); margin: 0.2rem 0 0.4rem; }
  .time input { font: inherit; padding: 0.3rem 0.5rem; border: 1px solid var(--line); border-radius: 8px; background: var(--surface); color: var(--text); }
  .answer { margin-top: 0.75rem; }
  .notice { margin: 0 0 0.75rem; padding: 0.6rem 0.75rem; border-radius: 12px; font-size: 0.88rem; background: color-mix(in srgb, var(--warn) 14%, transparent); border-left: 4px solid var(--warn); }
  .label { margin: 0 0 0.3rem; font-size: 0.75rem; font-weight: 750; letter-spacing: 0.06em; text-transform: uppercase; color: var(--accent); }
  .hint { color: var(--muted); font-size: 0.9rem; line-height: 1.45; }
  .off { color: var(--bad); font-size: 0.9rem; }
  .skeleton { display: grid; gap: 0.5rem; padding: 0.75rem 0; }
  .skeleton span { height: 2.8rem; border-radius: 10px; background: linear-gradient(90deg, var(--hover), var(--line), var(--hover)); background-size: 200% 100%; animation: shimmer 1.2s infinite; }
  @keyframes shimmer { to { background-position: -200% 0; } }
</style>
