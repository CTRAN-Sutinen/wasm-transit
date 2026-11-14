<script lang="ts">
  /// A new route, as a task in three steps: the route (its id
  /// and names), each way it runs (a name, its stops picked in order —
  /// the plan's, or ones the stops record holds — and the time end to
  /// end; the way back starts as the way out reversed), and its trips
  /// (per way and service key: the first and last departures and the
  /// minutes between). Create hands the whole route to the host as one
  /// change; what refuses it is said beside the actions. A RENDER
  /// component.
  import "./forms.css";
  import TaskFrame from "./TaskFrame.svelte";
  import Combobox from "./Combobox.svelte";

  type StopOpt = { value: string; label: string; sub?: string; fromStops: boolean };
  type Spec = {
    id: string;
    name: string;
    longName: string;
    directions: { id: string; name: string; stops: string[]; runSecs: number }[];
    trips: { direction: string; serviceType: string; firstSecs: number; lastSecs: number; everySecs: number }[];
  };

  let {
    stops,
    keys,
    routeIds,
    busy,
    problems,
    oncreate,
    onclose,
  }: {
    stops: StopOpt[];
    keys: { id: string; name: string }[];
    routeIds: string[];
    busy: boolean;
    problems: string[];
    oncreate: (spec: Spec) => void;
    onclose: () => void;
  } = $props();

  let step = $state(0);
  let id = $state("");
  let name = $state("");
  let longName = $state("");
  type Way = { name: string; stops: string[]; runMin: string; on: boolean; reversed: boolean };
  let ways = $state<Way[]>([
    { name: "", stops: [], runMin: "", on: true, reversed: false },
    { name: "", stops: [], runMin: "", on: true, reversed: true },
  ]);
  type Row = { key: string; first: string; last: string; every: string };
  let rows = $state<Row[][]>([[], []]);
  /// What is typed in each direction's stop box, cleared once a stop is added.
  let adding = $state(["", ""]);

  const byId = $derived(new Map(stops.map((s) => [s.value, s])));
  const stopName = (s: string) => byId.get(s)?.label ?? s;
  /// The way back, while it follows the way out: its stops reversed.
  const wayStops = (i: number) => (i === 1 && ways[1].reversed ? [...ways[0].stops].reverse() : ways[i].stops);
  const live = $derived([0, 1].filter((i) => ways[i].on));
  const idTaken = $derived(routeIds.includes(id.trim()));
  const routeReady = $derived(!!id.trim() && !idTaken && !!(name.trim() || longName.trim()));
  const minutes = (v: string) => (/^\d+$/.test(v.trim()) && Number(v) > 0 ? Number(v) : null);
  const waysReady = $derived(live.length > 0 && live.every((i) => wayStops(i).length >= 2 && minutes(ways[i].runMin) !== null && !!ways[i].name.trim()));
  const secs = (t: string) => {
    const m = t.match(/^(\d{1,2}):(\d{2})$/);
    return m ? Number(m[1]) * 3600 + Number(m[2]) * 60 : null;
  };
  const rowOk = (r: Row) => !!r.key && secs(r.first) !== null && secs(r.last) !== null && secs(r.last)! >= secs(r.first)! && minutes(r.every) !== null;
  const tripRows = $derived(live.flatMap((i) => rows[i].map((r) => ({ i, r }))));
  const tripsReady = $derived(tripRows.length > 0 && tripRows.every(({ r }) => rowOk(r)));

  function addStop(i: number, stop: string) {
    if (!stop) return;
    const list = ways[i].stops;
    if (list[list.length - 1] === stop) return;
    ways[i].stops = [...list, stop];
  }
  function moveStop(i: number, at: number, by: number) {
    const list = [...ways[i].stops];
    const to = at + by;
    if (to < 0 || to >= list.length) return;
    [list[at], list[to]] = [list[to], list[at]];
    ways[i].stops = list;
  }
  function dropStop(i: number, at: number) {
    ways[i].stops = ways[i].stops.filter((_, j) => j !== at);
  }
  function addRow(i: number) {
    const copy = rows[i].at(-1) ?? rows[0].at(-1);
    rows[i] = [...rows[i], copy ? { ...copy } : { key: keys[0]?.id ?? "", first: "06:00", last: "20:00", every: "30" }];
  }
  function toStep(n: number) {
    if (n === 2) {
      for (const i of live) if (!rows[i].length) addRow(i);
    }
    step = n;
  }
  function create() {
    const dirs = live.map((i) => ({ id: String(i), name: ways[i].name.trim(), stops: wayStops(i), runSecs: minutes(ways[i].runMin)! * 60 }));
    const trips = tripRows.map(({ i, r }) => ({
      direction: String(i),
      serviceType: r.key,
      firstSecs: secs(r.first)!,
      lastSecs: secs(r.last)!,
      everySecs: minutes(r.every)! * 60,
    }));
    oncreate({ id: id.trim(), name: name.trim(), longName: longName.trim(), directions: dirs, trips });
  }
</script>

<TaskFrame title="New route" subtitle={id.trim() ? `Route ${name.trim() || id.trim()}` : "A route the plan does not run yet"} steps={["Route", "Directions", "Trips"]} {step}
  onstep={(i) => i < step && (step = i)} {onclose} closeLabel="Cancel" {problems} testid="new-route">
  {#if step === 0}
    <div class="f-pair">
      <label class="f-field"><span class="f-label">Route ID</span><input type="text" bind:value={id} placeholder="e.g. 099" data-testid="new-route-id" /></label>
      <label class="f-field"><span class="f-label">Short name</span><input type="text" bind:value={name} placeholder="e.g. 99" data-testid="new-route-name" /></label>
    </div>
    <label class="f-field"><span class="f-label">Long name</span><input type="text" bind:value={longName} placeholder="e.g. Washougal/Camas/East Fourth Plain" data-testid="new-route-long-name" /></label>
    {#if idTaken}<p class="f-hint warn" data-testid="new-route-taken">The plan already has route {id.trim()}.</p>{/if}
  {:else if step === 1}
    {#each [0, 1] as i (i)}
      <fieldset class="f-field way" data-testid="new-route-way-{i}">
        <legend class="f-label">
          {#if i === 0}Direction 1{:else}<label class="check"><input type="checkbox" bind:checked={ways[1].on} data-testid="new-route-way-1-on" /> Direction 2</label>{/if}
        </legend>
        {#if ways[i].on}
          <div class="f-pair">
            <label class="f-field"><span class="f-label">Name</span><input type="text" bind:value={ways[i].name} placeholder={i === 0 ? "e.g. To East Fourth Plain" : "e.g. To Washougal"} data-testid="new-route-way-{i}-name" /></label>
            <label class="f-field"><span class="f-label">End to end (min)</span><input type="text" inputmode="numeric" bind:value={ways[i].runMin} placeholder="e.g. 40" data-testid="new-route-way-{i}-run" /></label>
          </div>
          {#if i === 1}
            <label class="check"><input type="checkbox" bind:checked={ways[1].reversed} onchange={() => { if (!ways[1].reversed && !ways[1].stops.length) ways[1].stops = [...ways[0].stops].reverse(); }} data-testid="new-route-way-1-reversed" /> Same stops as direction 1, reversed</label>
          {/if}
          <ol class="stops" data-testid="new-route-way-{i}-stops">
            {#each wayStops(i) as s, j (`${s}-${j}`)}
              <li>
                <span class="n">{j + 1}</span>
                <span class="name">{stopName(s)}<small>{s}{byId.get(s)?.fromStops ? " · from the stops record" : ""}</small></span>
                {#if !(i === 1 && ways[1].reversed)}
                  <span class="acts">
                    <button type="button" class="f-link" disabled={j === 0} onclick={() => moveStop(i, j, -1)} aria-label="Move {stopName(s)} up">↑</button>
                    <button type="button" class="f-link" disabled={j === wayStops(i).length - 1} onclick={() => moveStop(i, j, 1)} aria-label="Move {stopName(s)} down">↓</button>
                    <button type="button" class="f-link" onclick={() => dropStop(i, j)} aria-label="Remove {stopName(s)}">Remove</button>
                  </span>
                {/if}
              </li>
            {:else}
              <li class="muted">No stops yet.</li>
            {/each}
          </ol>
          {#if !(i === 1 && ways[1].reversed)}
            <div class="find" data-testid="new-route-way-{i}-add">
              <Combobox label="Add a stop" bind:value={adding[i]} options={stops} placeholder="Add a stop by name or ID…" onchange={(v) => { addStop(i, v); adding[i] = ""; }} />
            </div>
          {/if}
        {/if}
      </fieldset>
    {/each}
  {:else}
    {#each live as i (i)}
      <fieldset class="f-field" data-testid="new-route-trips-{i}">
        <legend class="f-label">{ways[i].name || `Direction ${i + 1}`}</legend>
        <table class="trips">
          <thead><tr><th>Service key</th><th>First</th><th>Last</th><th>Every (min)</th><th></th></tr></thead>
          <tbody>
            {#each rows[i] as r, j (j)}
              <tr data-testid="new-route-trip-row">
                <td><select bind:value={r.key} aria-label="Service key">{#each keys as k (k.id)}<option value={k.id}>{k.name || k.id}</option>{/each}</select></td>
                <td><input type="time" bind:value={r.first} aria-label="First departure" /></td>
                <td><input type="time" bind:value={r.last} aria-label="Last departure" /></td>
                <td><input type="text" inputmode="numeric" bind:value={r.every} aria-label="Minutes between trips" /></td>
                <td><button type="button" class="f-link" onclick={() => (rows[i] = rows[i].filter((_, k) => k !== j))}>Remove</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
        <button type="button" class="f-link" onclick={() => addRow(i)} data-testid="new-route-add-row-{i}">Add a service key</button>
      </fieldset>
    {/each}
  {/if}

  {#snippet actions()}
    <button type="button" disabled={step === 0} onclick={() => (step -= 1)}>Back</button>
    {#if step < 2}
      <button type="button" class="primary" disabled={step === 0 ? !routeReady : !waysReady} onclick={() => toStep(step + 1)} data-testid="new-route-next">Next</button>
    {:else}
      <button type="button" class="primary" disabled={busy || !tripsReady} onclick={create} data-testid="new-route-create">Create route</button>
    {/if}
  {/snippet}
</TaskFrame>

<style>
  .way { border-top: 1px solid var(--line); padding-top: 0.8rem; }
  .check { display: flex; gap: 0.4rem; align-items: center; font-size: 0.9rem; margin-bottom: 0.6rem; }
  .stops { list-style: none; padding: 0; margin: 0 0 0.6rem; display: flex; flex-direction: column; gap: 0.2rem; max-width: 36rem; }
  .stops li { display: flex; gap: 0.6rem; align-items: center; padding: 0.3rem 0.4rem; border: 1px solid var(--line); border-radius: 6px; }
  .stops li.muted { border-style: dashed; color: var(--muted); }
  .n { color: var(--muted); font-variant-numeric: tabular-nums; min-width: 1.5rem; text-align: right; }
  .name { flex: 1; display: flex; flex-direction: column; }
  .name small { color: var(--muted); font-size: 0.75rem; }
  .acts { display: flex; gap: 0.5rem; }
  .find { max-width: 36rem; }
  .trips { border-collapse: collapse; margin-bottom: 0.4rem; }
  .trips th { text-align: left; font-weight: 600; font-size: 0.8rem; color: var(--muted); padding: 0.2rem 0.4rem; }
  .trips td { padding: 0.2rem 0.4rem; }
  .trips input, .trips select { font: inherit; padding: 0.3rem; border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--text); }
  .trips input[inputmode="numeric"] { width: 4.5rem; }
  .warn { color: var(--warn); }
  fieldset > .f-link { align-self: flex-start; }
</style>
