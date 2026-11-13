<script lang="ts">
  /// A stop's point as two typed numbers, beside the map click: a point
  /// measured in the field is entered here at its own precision. The
  /// map and these fields edit one point (the host's); a click fills
  /// them, and a whole pair of numbers typed here moves the marker.
  /// Whether a point is on the earth is the receive path's to say
  /// (transit-stops refuses one that is not), not these fields'.
  /// A RENDER component.
  let {
    point,
    onpoint,
    testid,
  }: {
    point: { lat: number; lon: number } | null;
    onpoint: (lat: number, lon: number) => void;
    testid: string;
  } = $props();

  let lat = $state("");
  let lon = $state("");

  // A point from the map shows in the fields; a point that came from
  // the fields is left as typed (no reformatting under the cursor).
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
  <p class="muted small">{point ? "Click the map, or type the measured point." : "Click the map where the stop goes, or type its measured point."}</p>
</fieldset>

<style>
  .point { display: flex; flex-direction: column; gap: 0.35rem; border: 1px solid var(--line); border-radius: 6px; padding: 0.4rem 0.6rem 0.5rem; margin: 0; }
  legend { padding: 0 0.25rem; font-size: 0.85rem; color: var(--muted); }
  label { display: flex; justify-content: space-between; gap: 0.6rem; align-items: center; }
  input { flex: 1; min-width: 0; max-width: 12rem; font-variant-numeric: tabular-nums; }
  p { margin: 0; }
  .small { font-size: 0.8rem; }
</style>
