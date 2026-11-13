<script lang="ts">
  /// A new stop: its id, name, and code, its point (clicked on the map,
  /// or typed as measured), and where its life starts — planned, or in
  /// service for a stop that is there but was never recorded. The host
  /// writes it as the stop's first event. A RENDER component.
  import type { Event } from "../types/Event";
  import PointFields from "./PointFields.svelte";

  let {
    date,
    point,
    busy,
    onpoint,
    onsave,
    oncancel,
  }: {
    date: string;
    point: { lat: number; lon: number } | null;
    busy: boolean;
    /// A point typed in the fields (a map click comes by the map).
    onpoint: (lat: number, lon: number) => void;
    onsave: (stopId: string, event: Event) => void;
    oncancel: () => void;
  } = $props();

  let id = $state("");
  let name = $state("");
  let code = $state("");
  let start = $state<"planned" | "in_service">("planned");
  let on = $state("");
  let note = $state("");
  $effect(() => {
    on = date;
  });
</script>

<form
  class="new-stop"
  data-testid="new-stop"
  onsubmit={(e) => {
    e.preventDefault();
    if (!point) return;
    onsave(id.trim(), {
      on,
      event: start,
      lat: point.lat,
      lon: point.lon,
      ...(name.trim() ? { name: name.trim() } : {}),
      ...(code.trim() ? { code: code.trim() } : {}),
      ...(note.trim() ? { note: note.trim() } : {}),
    });
  }}
>
  <h2>New stop</h2>
  <PointFields {point} {onpoint} testid="new-point" />
  <label>Id <input type="text" bind:value={id} required placeholder="A new id" data-testid="new-id" /></label>
  <label>Name <input type="text" bind:value={name} data-testid="new-name" /></label>
  <label>Code <input type="text" bind:value={code} placeholder="The number on the sign" data-testid="new-code" /></label>
  <label>Starts
    <select bind:value={start} data-testid="new-start">
      <option value="planned">planned</option>
      <option value="in_service">in service (already there)</option>
    </select>
  </label>
  <label>From <input type="date" bind:value={on} required data-testid="new-on" /></label>
  <label>Note <input type="text" bind:value={note} data-testid="new-note" /></label>
  <div class="row">
    <button type="submit" disabled={busy || !point || !id.trim() || !on} data-testid="new-save">Save</button>
    <button type="button" onclick={oncancel}>Cancel</button>
  </div>
</form>

<style>
  h2 { margin: 0 0 0.3rem; font-size: 1.05rem; }
  form { display: flex; flex-direction: column; gap: 0.35rem; }
  label { display: flex; justify-content: space-between; gap: 0.6rem; align-items: center; }
  input[type="text"] { flex: 1; min-width: 0; }
  .row { display: flex; gap: 0.4rem; }
</style>
