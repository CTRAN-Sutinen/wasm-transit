<script lang="ts">
  let {
    point,
    onpoint,
    onlocate,
    testid,
  }: {
    point: { lat: number; lon: number } | null;
    onpoint: (lat: number, lon: number) => void;
    onlocate?: () => Promise<{ lat: number; lon: number; accuracy: number }>;
    testid: string;
  } = $props();

  let fix = $state("");
  let locating = $state(false);

  async function here() {
    if (!onlocate) return;
    locating = true;
    fix = "Finding this device's location…";
    try {
      const p = await onlocate();
      // Seven decimals is about 1 cm, finer than any device's fix.
      lat = String(+p.lat.toFixed(7));
      lon = String(+p.lon.toFixed(7));
      onpoint(+lat, +lon);
      fix = `This device's location, good to about ${Math.round(p.accuracy)} m. Type the measured point if you have one.`;
    } catch (e) {
      fix = `No location: ${(e as Error).message}.`;
    } finally {
      locating = false;
    }
  }

  let lat = $state("");
  let lon = $state("");

  // A point from the map shows in the fields. A point typed in the fields stays
  // as typed, so the cursor does not see a reformat.
  $effect(() => {
    if (!point) return;
    if (Number(lat) !== point.lat || lat.trim() === "") lat = String(point.lat);
    if (Number(lon) !== point.lon || lon.trim() === "") lon = String(point.lon);
  });

  function typed() {
    if (lat.trim() === "" || lon.trim() === "") return;
    const a = Number(lat);
    const o = Number(lon);
    if (Number.isFinite(a) && Number.isFinite(o)) onpoint(a, o);
  }
</script>

<fieldset class="point" data-testid={testid}>
  <legend>Point</legend>
  <label>Latitude
    <input type="text" inputmode="decimal" autocomplete="off" spellcheck="false" placeholder="decimal degrees" value={lat} oninput={(e) => { lat = e.currentTarget.value; typed(); }} data-testid="{testid}-lat" />
  </label>
  <label>Longitude
    <input type="text" inputmode="decimal" autocomplete="off" spellcheck="false" placeholder="decimal degrees" value={lon} oninput={(e) => { lon = e.currentTarget.value; typed(); }} data-testid="{testid}-lon" />
  </label>
  {#if onlocate}
    <button type="button" class="here" disabled={locating} onclick={here} data-testid="{testid}-here">Use my location</button>
  {/if}
  <p class="muted small" data-testid="{testid}-fix">{fix || (point ? "Click the map, or type the measured point." : "Click the map where the stop goes, or type its measured point.")}</p>
</fieldset>

<style>
  .point { display: flex; flex-direction: column; gap: 0.35rem; border: 1px solid var(--line); border-radius: 6px; padding: 0.4rem 0.6rem 0.5rem; margin: 0; }
  legend { padding: 0 0.25rem; font-size: 0.85rem; color: var(--muted); }
  label { display: flex; justify-content: space-between; gap: 0.6rem; align-items: center; }
  input { flex: 1; min-width: 0; max-width: 12rem; font-variant-numeric: tabular-nums; }
  p { margin: 0; }
  .here { align-self: flex-start; min-height: 2.25rem; padding: 0 0.8rem; }
  .small { font-size: 0.8rem; }
</style>
