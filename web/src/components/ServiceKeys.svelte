<script lang="ts">
  /// The plan's service keys (directory `service_keys`) and the
  /// combinations they form (analysis `day_types`): pick a key to read
  /// and change its calendar — weekdays, date ranges, exceptions — and
  /// what it carries, a route's trips moved onto another key; add a
  /// key, or remove one that carries nothing. Events out; every change
  /// is the host's `apply`.
  import type { ServiceKey } from "../types/ServiceKey";
  import type { DayType } from "../types/DayType";
  import { weekdaysText } from "./designFormat";

  type Day = "mon" | "tue" | "wed" | "thu" | "fri" | "sat" | "sun";
  let {
    keys,
    dayTypes,
    framed,
    ontoggleday,
    onaddrange,
    onsetrange,
    onremoverange,
    onsetexception,
    onremoveexception,
    onrename,
    onmove,
    onadd,
    onremove,
    onroute,
  }: {
    keys: ServiceKey[];
    dayTypes: DayType[];
    framed: boolean;
    ontoggleday: (id: string, day: Day) => void;
    onaddrange: (id: string) => void;
    onsetrange: (id: string, index: number, start: string, end: string) => void;
    onremoverange: (id: string, index: number) => void;
    onsetexception: (id: string, date: string, state: "added" | "removed") => void;
    onremoveexception: (id: string, date: string) => void;
    onrename: (id: string, name: string) => void;
    onmove: (patternTrips: string[], to: string) => void;
    onadd: () => void;
    onremove: (id: string) => void;
    onroute: (route: string) => void;
  } = $props();

  const DAYS: Day[] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
  let picked = $state("");
  const key = $derived(keys.find((k) => k.id === picked) ?? keys.find((k) => k.trips > 0) ?? keys[0] ?? null);
  // The combinations this key is a member of, as day_types names them.
  const inGroups = $derived(key ? dayTypes.flatMap((t) => t.groups).filter((g) => g.members.includes(key.id)) : []);
  let exDate = $state("");
  let exState = $state<"added" | "removed">("added");
  let name = $state("");
  $effect(() => { name = key?.name ?? ""; });
  // A key just added opens: the id that was not there before the add.
  let before: string[] | null = null;
  $effect(() => {
    const ids = keys.map((k) => k.id);
    if (!before) return;
    const added = ids.find((id) => !before!.includes(id));
    if (added) { picked = added; before = null; }
  });
  function add() { before = keys.map((k) => k.id); onadd(); }
</script>

<section class="keys" aria-label="Service keys" data-testid="service-keys">
  <div class="list">
    <h2>Service keys</h2>
    <button type="button" class="new" disabled={!framed} onclick={add} data-testid="add-key">Add a key</button>
    <ul>
      {#each keys as k (k.id)}
        <li>
          <button type="button" class:on={k === key} aria-pressed={k === key} onclick={() => (picked = k.id)}>
            <strong>{k.id}{#if k.name && k.name !== k.id}<span class="muted"> · {k.name}</span>{/if}</strong>
            <span class="muted">{weekdaysText(DAYS.map((d) => k.rule.days[d]))} · {k.trips} trips · {k.routes.length} {k.routes.length === 1 ? "route" : "routes"}</span>
          </button>
        </li>
      {/each}
    </ul>
  </div>

  {#if key}
    <div class="detail" data-testid="service-key">
      <header>
        <h2>{key.id}</h2>
        <form onsubmit={(e) => { e.preventDefault(); if (name.trim() && name !== key.name) onrename(key.id, name.trim()); }}>
          <label>Name <input bind:value={name} disabled={!framed} /></label>
        </form>
        <button type="button" class="remove" disabled={!framed || key.trips > 0} data-testid="remove-key"
          title={key.trips > 0 ? "It carries trips: move its routes to another key first" : "Remove this key from the plan"}
          onclick={() => { const id = key.id; if (confirm(`Remove service key ${id}?`)) { picked = ""; onremove(id); } }}>Remove key</button>
      </header>
      {#if !framed}<p class="note">This is the base network. Start a plan (above) to change its calendar.</p>{/if}

      <h3>Weekdays</h3>
      <div class="days" role="group" aria-label="Weekdays">
        {#each DAYS as d (d)}
          <button type="button" class:on={key.rule.days[d]} aria-pressed={key.rule.days[d]} disabled={!framed} onclick={() => ontoggleday(key.id, d)}>{d}</button>
        {/each}
      </div>

      <h3>Date ranges <span class="muted">a date in a range, on a weekday above, runs</span></h3>
      <ul class="rows">
        {#each key.rule.ranges as r, i (i)}
          <li>
            <input type="date" value={r.start} disabled={!framed} aria-label="range start" onchange={(e) => onsetrange(key.id, i, e.currentTarget.value, r.end)} />
            –
            <input type="date" value={r.end} disabled={!framed} aria-label="range end" onchange={(e) => onsetrange(key.id, i, r.start, e.currentTarget.value)} />
            <button type="button" class="x" disabled={!framed} aria-label="Remove range" onclick={() => onremoverange(key.id, i)}>×</button>
          </li>
        {/each}
      </ul>
      <button type="button" disabled={!framed} onclick={() => onaddrange(key.id)}>Add a range</button>

      <h3>Exceptions <span class="muted">a date added or removed, whatever the weekday</span></h3>
      <ul class="rows" data-testid="exceptions">
        {#each key.rule.exceptions as x (x.date)}
          <li><span class="date">{x.date}</span><span class:added={x.state === "added"}>{x.state}</span>
            <button type="button" class="x" disabled={!framed} aria-label="Remove exception" onclick={() => onremoveexception(key.id, x.date)}>×</button></li>
        {/each}
      </ul>
      <form class="add" onsubmit={(e) => { e.preventDefault(); if (exDate) { onsetexception(key.id, exDate, exState); exDate = ""; } }}>
        <input type="date" bind:value={exDate} disabled={!framed} aria-label="exception date" />
        <select bind:value={exState} disabled={!framed} aria-label="exception state"><option value="added">runs</option><option value="removed">does not run</option></select>
        <button type="submit" disabled={!framed || !exDate}>Set</button>
      </form>

      <h3>What it carries</h3>
      <table>
        <thead><tr><th>Route</th><th>Trips</th><th>Move to another key</th></tr></thead>
        <tbody>
          {#each key.routes as r (r.route_id)}
            <tr>
              <td><button type="button" class="link" onclick={() => onroute(r.route_id)}>{r.name || r.route_id}</button></td>
              <td>{r.trips}</td>
              <td>
                <select disabled={!framed} aria-label="Move {r.route_id} to" value="" onchange={(e) => { const to = e.currentTarget.value; if (to) onmove(r.pattern_trips, to); }}>
                  <option value="">—</option>
                  {#each keys.filter((k) => k.id !== key.id) as k (k.id)}<option value={k.id}>{k.id}</option>{/each}
                </select>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>

      <h3>Runs with <span class="muted">the combinations it forms in the plan's period</span></h3>
      {#if inGroups.length === 0}<p class="muted">None in the plan's period.</p>{/if}
      <ul class="rows">
        {#each inGroups as g (g.id)}<li><span>{g.id}</span><span class="muted">{g.dates.length} days, {weekdaysText(g.weekdays)}</span></li>{/each}
      </ul>
    </div>
  {/if}
</section>

<style>
  .keys { display: grid; grid-template-columns: minmax(14rem, 18rem) 1fr; gap: 1rem; font-size: 0.82rem; }
  h2 { margin: 0 0 0.4rem; font-size: 0.95rem; }
  h3 { margin: 1rem 0 0.3rem; font-size: 0.8rem; }
  h3 .muted { font-weight: 400; }
  ul { list-style: none; margin: 0; padding: 0; }
  .list button { width: 100%; text-align: left; display: flex; flex-direction: column; gap: 0.1rem; padding: 0.4rem 0.6rem; border: 1px solid transparent; border-radius: 6px; background: none; color: var(--text); font: inherit; cursor: pointer; }
  .list button.on { border-color: var(--accent); background: var(--accent-soft); }
  .list .new, .remove { font: inherit; font-size: 0.78rem; padding: 0.2rem 0.6rem; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: var(--text); cursor: pointer; margin-bottom: 0.4rem; }
  .list .new { width: auto; display: inline-block; }
  .remove { margin-left: auto; }
  .list .new:disabled, .remove:disabled { opacity: 0.5; cursor: default; }
  .detail { border-left: 1px solid var(--line); padding-left: 1rem; min-width: 0; }
  header { display: flex; gap: 1rem; align-items: baseline; flex-wrap: wrap; }
  .days { display: flex; gap: 0.25rem; }
  .days button { font: inherit; text-transform: capitalize; padding: 0.25rem 0.5rem; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: var(--muted); cursor: pointer; }
  .days button.on { background: var(--accent); border-color: var(--accent); color: var(--surface); }
  .rows li { display: flex; gap: 0.6rem; align-items: center; padding: 0.15rem 0; }
  .date { font-variant-numeric: tabular-nums; }
  .added { color: var(--ok); }
  .x { background: none; border: 0; cursor: pointer; color: var(--muted); font-size: 1rem; }
  .add { display: flex; gap: 0.4rem; margin-top: 0.3rem; }
  table { border-collapse: collapse; }
  th, td { text-align: left; padding: 0.2rem 0.8rem 0.2rem 0; border-bottom: 1px solid var(--line); }
  th { color: var(--muted); font-weight: 500; font-size: 0.72rem; }
  .link { background: none; border: 0; padding: 0; font: inherit; color: var(--accent); cursor: pointer; }
  .note { padding: 0.4rem 0.6rem; border: 1px solid var(--warn); border-radius: 6px; }
  .muted { color: var(--muted); }
  input, select { font: inherit; padding: 0.15rem 0.3rem; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: var(--text); }
  @media (max-width: 900px) { .keys { grid-template-columns: 1fr; } .detail { border-left: 0; padding-left: 0; } }
</style>
