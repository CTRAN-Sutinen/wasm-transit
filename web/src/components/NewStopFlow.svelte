<script lang="ts">
  /// A new stop, as a task in two steps: where it goes (a click on the
  /// map, a measured point typed, or this device's location), then what
  /// it is — its id, name, and code, and where its life starts: planned,
  /// or in service for a stop that is there but was never recorded. It
  /// may go on to request its installation. The host writes it as the
  /// stop's first event; a taken id is refused beside the form.
  /// A RENDER component.
  import "./forms.css";
  import type { Event } from "../types/Event";
  import type { Refusal } from "../types/Refusal";
  import TaskFrame from "./TaskFrame.svelte";
  import PointFields from "./PointFields.svelte";
  import { problemWords } from "./stopWords";

  let {
    date,
    point,
    busy,
    refusals,
    error,
    onpoint,
    onlocate,
    onsave,
    onclose,
  }: {
    date: string;
    point: { lat: number; lon: number } | null;
    busy: boolean;
    refusals: Refusal[];
    error: string;
    onpoint: (lat: number, lon: number) => void;
    onlocate?: () => Promise<{ lat: number; lon: number; accuracy: number }>;
    /// The stop's first event, and whether to request its installation next.
    onsave: (stopId: string, event: Event, request: boolean) => void;
    onclose: () => void;
  } = $props();

  let step = $state(0);
  let id = $state("");
  let name = $state("");
  let code = $state("");
  let start = $state<"planned" | "in_service">("planned");
  // svelte-ignore state_referenced_locally
  let on = $state(date);
  let note = $state("");
  let request = $state(true);

  function save() {
    if (!point) return;
    onsave(
      id.trim(),
      {
        on,
        event: start,
        lat: point.lat,
        lon: point.lon,
        ...(name.trim() ? { name: name.trim() } : {}),
        ...(code.trim() ? { code: code.trim() } : {}),
        ...(note.trim() ? { note: note.trim() } : {}),
      },
      start === "planned" && request,
    );
  }
</script>

<TaskFrame title="New stop" subtitle={point ? `At ${point.lat}, ${point.lon}` : "Place on map"} steps={["Location", "Details"]} {step} onstep={(i) => (step = i)} {onclose} closeLabel="Cancel" testid="new-stop"
  problems={[...refusals.map((r) => `Not saved: ${problemWords(r.problem.kind)}.`), ...(error ? [error] : [])]}>
  {#if step === 0}
    <p class="f-hint lead">Click the map, enter coordinates, or use this device's location.</p>
    <PointFields {point} {onpoint} {onlocate} testid="new-point" />
  {:else}
    <label class="f-field">
      <span class="f-label">Stop ID</span>
      <input type="text" bind:value={id} required data-testid="new-id" />
      
    </label>
    <div class="f-pair">
      <label class="f-field"><span class="f-label">Name</span><input type="text" bind:value={name} placeholder="e.g. NE 99th St & Hwy 99" data-testid="new-name" /></label>
      <label class="f-field"><span class="f-label">Stop code</span><input type="text" bind:value={code} data-testid="new-code" /></label>
    </div>
    <ul class="f-choices start" role="list" aria-label="Initial status">
      <li><button type="button" aria-pressed={start === "planned"} onclick={() => (start = "planned")} data-testid="new-start-planned"><strong>Planned</strong><span>Not built yet.</span></button></li>
      <li><button type="button" aria-pressed={start === "in_service"} onclick={() => (start = "in_service")} data-testid="new-start-in_service"><strong>In service</strong><span>Existing stop, not yet recorded.</span></button></li>
    </ul>
    <label class="f-field"><span class="f-label">Effective date</span><input type="date" bind:value={on} required data-testid="new-on" /></label>
    <label class="f-field"><span class="f-label">Notes</span><input type="text" bind:value={note} placeholder="Optional" data-testid="new-note" /></label>
    {#if start === "planned"}
      <label class="check"><input type="checkbox" bind:checked={request} data-testid="new-request" /> Create installation request</label>
    {/if}
  {/if}

  {#snippet actions()}
    <button type="button" disabled={step === 0} onclick={() => (step = 0)}>Back</button>
    <span class="grow"></span>
    {#if step === 0}
      <button type="button" class="primary" disabled={!point} onclick={() => (step = 1)} data-testid="new-next">Next</button>
    {:else}
      <button type="button" class="primary" disabled={busy || !point || !id.trim() || !on} onclick={save} data-testid="new-save">Create stop</button>
    {/if}
  {/snippet}
</TaskFrame>

<style>
  .lead { margin-bottom: 0.9rem; }
  .start { margin: 0 0 1rem; }
  .check { display: flex; gap: 0.5rem; align-items: center; margin: 0 0 1rem; font-size: 0.92rem; }
  .check input { width: 1.2rem; height: 1.2rem; }
</style>
