<script lang="ts">
  /// One change recorded on a stop (D-157), as a task: a step in its
  /// life (the stop's `next` — only those are offered), a move, a change
  /// of details, or the correction of an event entered wrong. It asks
  /// only what that change needs, says in one sentence what will be
  /// saved, and shows why it was refused beside the Save button. A move's
  /// point is clicked on the map, typed, or the device's fix. Each is
  /// one event the host writes. A RENDER component.
  import "./forms.css";
  import type { Details } from "../types/Details";
  import type { Event } from "../types/Event";
  import type { Refusal } from "../types/Refusal";
  import type { StopOn } from "../types/StopOn";
  import TaskFrame from "./TaskFrame.svelte";
  import PointFields from "./PointFields.svelte";
  import { EVENT_WORDS, amenityWords, changeWords, changedWords, problemWords } from "./stopWords";

  let {
    stopId,
    stop,
    events,
    kind,
    choices = [],
    date,
    busy,
    routes,
    requests,
    fromRequest = "",
    amenityNames,
    point,
    onpoint,
    onlocate,
    refusals,
    error,
    onsave,
    oncorrect,
    onclose,
    card = false,
  }: {
    stopId: string;
    stop: StopOn | null;
    events: Event[];
    /// A lifecycle event, "move", "details", "fix:<index>", or "" — the
    /// change chosen first, among `choices`.
    kind: string;
    /// The changes the stop may take that day (its `next`, a move, new
    /// details), offered when no kind is given.
    choices?: string[];
    date: string;
    busy: boolean;
    /// The routes that stop here in the schedule running on the date.
    routes: string[];
    /// The stop's requests: a change may name the one it carries out.
    requests: { id: string; job: string }[];
    fromRequest?: string;
    /// The amenity names a stop's details offer (cap-stops `questions`).
    amenityNames: string[];
    point: { lat: number; lon: number } | null;
    onpoint: (lat: number, lon: number) => void;
    onlocate?: () => Promise<{ lat: number; lon: number; accuracy: number }>;
    refusals: Refusal[];
    error: string;
    onsave: (event: Event, done: string) => void;
    oncorrect: (index: number, event: Event | null, done: string) => void;
    onclose: () => void;
    /// In a page, as a card (the stop page); else a panel (beside the map,
    /// for a move).
    card?: boolean;
  } = $props();

  const name = $derived(stop?.name || stopId);
  // svelte-ignore state_referenced_locally
  let chosen = $state(kind);
  const fix = $derived(chosen.startsWith("fix:") ? Number(chosen.slice(4)) : -1);
  const fixing = $derived(fix >= 0 ? events[fix] : null);
  const [title, line] = $derived(chosen === "" ? ["Record change", ""] : fixing ? [`Correct event: ${EVENT_WORDS[fixing.event] ?? fixing.event}`, "Corrects an event entered by mistake."] : changeWords(chosen, stop?.status));

  // svelte-ignore state_referenced_locally
  let on = $state(fixing ? (fixing.on ?? "") : date);
  // svelte-ignore state_referenced_locally
  let until = $state(fixing?.until ?? "");
  // svelte-ignore state_referenced_locally
  let temporary = $state(!!fixing?.until);
  // svelte-ignore state_referenced_locally
  let note = $state(fixing?.note ?? "");
  // svelte-ignore state_referenced_locally
  let request = $state(fromRequest);
  let removing = $state(false);
  let unchanged = $state(false);

  const yn = (v: boolean | undefined) => (v == null ? "" : v ? "yes" : "no");
  // the details form, starting from what the stop records
  // svelte-ignore state_referenced_locally
  let dName = $state(stop?.name ?? "");
  // svelte-ignore state_referenced_locally
  let dCode = $state(stop?.code ?? "");
  // svelte-ignore state_referenced_locally
  let dWheelchair = $state(yn(stop?.wheelchair));
  // svelte-ignore state_referenced_locally
  let dAmenities = $state<Record<string, string>>(Object.fromEntries(amenityNames.map((k) => [k, yn(stop?.amenities?.[k])])));

  const canUntil = $derived(chosen === "suspended" || chosen === "move" || fixing?.event === "suspended" || fixing?.event === "moved");

  function details(): Details {
    // Only what changed is set (D-153: every detail changes by an event).
    const out: Details = {};
    if (dName.trim() && dName.trim() !== (stop?.name ?? "")) out.name = dName.trim();
    if (dCode.trim() && dCode.trim() !== (stop?.code ?? "")) out.code = dCode.trim();
    if (dWheelchair && dWheelchair !== yn(stop?.wheelchair)) out.wheelchair = dWheelchair === "yes";
    const amen: Record<string, boolean> = {};
    for (const k of amenityNames) if (dAmenities[k] && dAmenities[k] !== yn(stop?.amenities?.[k])) amen[k] = dAmenities[k] === "yes";
    if (Object.keys(amen).length) out.amenities = amen;
    return out;
  }

  /// What will be saved, in a line.
  const summary = $derived.by(() => {
    const span = `${on || "undated"}${canUntil && temporary && until ? ` – ${until}` : canUntil ? (chosen === "suspended" || fixing?.event === "suspended" ? " (open-ended)" : " (permanent)") : ""}`;
    if (fixing) return `${EVENT_WORDS[fixing.event] ?? fixing.event} · ${span}`;
    switch (chosen) {
      case "move":
        return point ? `Move to ${point.lat}, ${point.lon} · ${span}` : "Select a location on the map.";
      case "details": {
        const { amenities, ...rest } = details();
        const n = Object.keys(rest).length + Object.keys(amenities ?? {}).length;
        return n ? `${n} ${n === 1 ? "change" : "changes"} · ${span}` : "No changes.";
      }
      default:
        return `${changeWords(chosen, stop?.status)[0]} · ${span}`;
    }
  });
  /// What the change does not do: the schedule still visits the stop.
  const scheduleNote = $derived(
    (chosen === "suspended" || chosen === "retired" || chosen === "move") && routes.length
      ? `Scheduled routes: ${routes.join(", ")}. The schedule is not changed; use a service change for that.`
      : "",
  );

  const ready = $derived(!busy && (!!on || (fixing && !fixing.on)) && (chosen !== "move" || !!point) && (!canUntil || !temporary || !!until));

  function withExtras(e: Event): Event {
    return { ...e, ...(note.trim() ? { note: note.trim() } : {}), ...(request ? { request } : {}) };
  }

  function save() {
    unchanged = false;
    if (fixing) {
      const fixed: Event = { ...fixing, ...(fixing.on || on ? { on: on || undefined } : {}) };
      if (canUntil && temporary && until) fixed.until = until;
      else delete fixed.until;
      if (note.trim()) fixed.note = note.trim();
      else delete fixed.note;
      return oncorrect(fix, fixed, `Corrected the ${(EVENT_WORDS[fixing.event] ?? fixing.event).toLowerCase()} event on ${name}.`);
    }
    if (chosen === "move") {
      if (!point) return;
      return onsave(withExtras({ on, event: "moved", lat: point.lat, lon: point.lon, ...(temporary && until ? { until } : {}) }), changedWords("move", stop?.status, name, on, temporary ? until : ""));
    }
    if (chosen === "details") {
      const d = details();
      if (!Object.keys(d).length) return void (unchanged = true);
      return onsave(withExtras({ on, event: "changed", ...d }), changedWords("details", stop?.status, name, on, ""));
    }
    const e: Event = withExtras({ on, event: chosen as Event["event"], ...(chosen === "suspended" && temporary && until ? { until } : {}) });
    onsave(e, changedWords(chosen, stop?.status, name, on, chosen === "suspended" && temporary ? until : ""));
  }
</script>

<TaskFrame {title} subtitle="{name} · {stopId}{request ? ` · Request ${request}` : ""}" {onclose} {card} closeLabel="Cancel" testid="change"
  problems={[...refusals.map((r) => `Not saved: ${problemWords(r.problem.kind)}.`), ...(error ? [error] : [])]}>
  {#if line}<p class="f-hint lead">{line}</p>{/if}
  {#if chosen === ""}
    <ul class="f-choices" role="list" data-testid="change-choices">
      {#each choices as c (c)}
        {@const [label, sub] = changeWords(c, stop?.status)}
        <li><button type="button" onclick={() => (chosen = c)} data-testid="choose-{c}"><strong>{label}</strong><span>{sub}</span></button></li>
      {/each}
    </ul>
  {:else}

  {#if chosen === "move"}
    <div class="f-field"><PointFields {point} {onpoint} {onlocate} testid="move-point" /></div>
  {/if}

  <div class="f-pair">
    {#if !fixing || fixing.on || fix > 0}
      <label class="f-field">
        <span class="f-label">{chosen === "suspended" ? "Start date" : "Effective date"}</span>
        <input type="date" bind:value={on} required data-testid="form-on" />
      </label>
    {/if}
    {#if canUntil}
      <div class="f-field" role="group" aria-label="How long">
        <span class="f-label">Duration</span>
        <div class="f-chips">
          <button type="button" aria-pressed={!temporary} onclick={() => (temporary = false)} data-testid="form-open-ended">{chosen === "suspended" || fixing?.event === "suspended" ? "Open-ended" : "Permanent"}</button>
          <button type="button" aria-pressed={temporary} onclick={() => (temporary = true)} data-testid="form-temporary">Temporary</button>
        </div>
      </div>
    {/if}
  </div>
  {#if canUntil && temporary}
    <label class="f-field">
      <span class="f-label">End date</span>
      <input type="date" bind:value={until} data-testid="form-until" />
    </label>
  {/if}

  {#if chosen === "details"}
    <div class="f-pair">
      <label class="f-field"><span class="f-label">Name</span><input type="text" bind:value={dName} data-testid="form-name" /></label>
      <label class="f-field"><span class="f-label">Code</span><input type="text" bind:value={dCode} data-testid="form-code" /></label>
    </div>
    <div class="f-field" role="group" aria-label="Wheelchair boarding">
      <span class="f-label">Wheelchair boarding</span>
      <div class="f-chips">
        <button type="button" aria-pressed={dWheelchair === "yes"} onclick={() => (dWheelchair = dWheelchair === "yes" ? "" : "yes")} data-testid="form-wheelchair-yes">Yes</button>
        <button type="button" aria-pressed={dWheelchair === "no"} onclick={() => (dWheelchair = dWheelchair === "no" ? "" : "no")} data-testid="form-wheelchair-no">No</button>
      </div>
    </div>
    <div class="f-section">
      <h3>Amenities</h3>
      {#each amenityNames as k (k)}
        <div class="amenity" data-testid="form-amenity-{k}">
          <span>{amenityWords(k)}</span>
          <div class="f-chips">
            <button type="button" aria-pressed={dAmenities[k] === "yes"} onclick={() => (dAmenities[k] = dAmenities[k] === "yes" ? "" : "yes")} data-testid="form-amenity-{k}-yes">There</button>
            <button type="button" aria-pressed={dAmenities[k] === "no"} onclick={() => (dAmenities[k] = dAmenities[k] === "no" ? "" : "no")} data-testid="form-amenity-{k}-no">Not there</button>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  {#if requests.length && !fixing}
    <label class="f-field">
      <span class="f-label">Related request</span>
      <select bind:value={request} data-testid="form-request">
        <option value="">None</option>
        {#each requests as r (r.id)}<option value={r.id}>{r.id} · {r.job}</option>{/each}
      </select>
    </label>
  {/if}
  <label class="f-field">
    <span class="f-label">Notes</span>
    <input type="text" bind:value={note} placeholder="Optional" data-testid="form-note" />
  </label>

  <div class="f-summary" data-testid="change-summary">
    <p><strong>{summary}</strong></p>
    {#if scheduleNote}<p class="muted">{scheduleNote}</p>{/if}
  </div>
  {#if unchanged}<p class="f-hint" data-testid="form-unchanged">No changes to save.</p>{/if}
  {#if fixing && removing}
    <div class="f-summary confirm" data-testid="fix-confirm">
      <p><strong>Delete this event?</strong> Only for an event entered by mistake.</p>
      <div class="row">
        <button type="button" class="danger" disabled={busy} onclick={() => oncorrect(fix, null, `Removed an event entered by mistake on ${name}.`)} data-testid="fix-remove-yes">Delete</button>
        <button type="button" onclick={() => (removing = false)}>Back</button>
      </div>
    </div>
  {/if}

  {/if}

  {#snippet actions()}
    {#if fixing}
      <button type="button" disabled={busy || removing} onclick={() => (removing = true)} data-testid="fix-remove">Delete event</button>
    {/if}
    <span class="grow"></span>
    <button type="button" onclick={onclose}>Cancel</button>
    {#if chosen !== ""}<button type="button" class="primary" disabled={!ready} onclick={save} data-testid="form-save">Save</button>{/if}
  {/snippet}
</TaskFrame>

<style>
  .lead { margin-bottom: 1rem; }
  .amenity { display: flex; justify-content: space-between; align-items: center; gap: 0.6rem; flex-wrap: wrap; padding: 0.25rem 0; border-bottom: 1px solid var(--line); }
  .confirm { border: 1px solid var(--bad); }
  .row { display: flex; gap: 0.4rem; margin-top: 0.4rem; }
  .danger { color: var(--bad); border-color: var(--bad); }
</style>
