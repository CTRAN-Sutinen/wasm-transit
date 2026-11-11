<script lang="ts">
  /// The hours every read narrows to: from and to as service-day
  /// hours, or the whole day. Events out; the host owns the window.
  let {
    window,
    onwindow,
  }: { window: { from: number; to: number } | null; onwindow: (w: { from: number; to: number } | null) => void } = $props();

  const hours = Array.from({ length: 29 }, (_, i) => i);
  const label = (h: number) => `${h % 24}:00${h >= 24 ? " (+1)" : ""}`;
</script>

<div class="hours" role="group" aria-label="Hours" data-testid="hour-window">
  <span class="muted">Hours</span>
  <select aria-label="from hour" value={window?.from ?? ""} onchange={(e) => {
    const from = e.currentTarget.value === "" ? null : Number(e.currentTarget.value);
    onwindow(from == null ? null : { from, to: Math.max(from + 1, window?.to ?? from + 3) });
  }}>
    <option value="">all day</option>
    {#each hours.slice(0, 28) as h (h)}<option value={h}>{label(h)}</option>{/each}
  </select>
  {#if window}
    <span class="muted">to</span>
    <select aria-label="to hour" value={window.to} onchange={(e) => onwindow({ from: window.from, to: Number(e.currentTarget.value) })}>
      {#each hours.filter((h) => h > window.from) as h (h)}<option value={h}>{label(h)}</option>{/each}
    </select>
  {/if}
</div>

<style>
  .hours { display: flex; gap: 0.3rem; align-items: center; font-size: 0.8rem; }
  select { font: inherit; padding: 0.2rem; border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--text); }
  .muted { color: var(--muted); }
</style>
