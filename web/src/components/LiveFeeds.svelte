<script lang="ts">
  /// Is this live? — a RENDER component: a pill that says whether the
  /// times on screen are live (when the agency's feeds were last read,
  /// how many buses cap-rt's live document saw) or the schedule, and
  /// behind it the feed URLs the host reads.
  import type { LiveSummary } from "../types/LiveSummary";

  type Urls = { trips: string; vehicles: string; alerts: string };
  let {
    urls,
    summary,
    at,
    error,
    off,
    onurls,
  }: {
    urls: Urls;
    /// cap-rt's live summary; null with no live document.
    summary: LiveSummary | null;
    /// When the feeds were last read (the host's clock).
    at: string;
    error: string;
    /// The realtime capability did not assemble.
    off: boolean;
    onurls: (urls: Urls) => void;
  } = $props();

  let open = $state(false);
  let draft = $state({ trips: "", vehicles: "", alerts: "" });
  $effect(() => {
    draft = { ...urls };
  });

  const mode = $derived(off ? "off" : error && !summary ? "error" : summary ? "live" : "schedule");
</script>

<div class="live" data-testid="live-feeds">
  <button class="pill {mode}" onclick={() => (open = !open)} aria-expanded={open}>
    <span class="dot" aria-hidden="true"></span>
    {#if mode === "live" && summary}
      Live {at} · {summary.vehicles} buses
    {:else if mode === "off"}
      Live data unavailable
    {:else if mode === "error"}
      Live feed down — schedule
    {:else}
      Schedule only
    {/if}
  </button>

  {#if open}
    <div class="pop" role="dialog" aria-label="Live feeds">
      {#if off}
        <p class="bad">The realtime capability didn’t load — every time shown is the schedule.</p>
      {:else if summary}
        <p>Times are live as of {at}, with {summary.vehicles} buses reporting.</p>
      {:else}
        <p>No live data — every time shown is the schedule.</p>
      {/if}
      {#if error}<p class="bad">Last read failed ({error}); showing the last good data.</p>{/if}
      <form onsubmit={(e) => { e.preventDefault(); onurls(draft); open = false; }}>
        <label>Trip updates <input type="url" aria-label="trip updates url" bind:value={draft.trips} placeholder="https://…/TripUpdates" /></label>
        <label>Vehicle positions <input type="url" aria-label="vehicle positions url" bind:value={draft.vehicles} placeholder="https://…/VehiclePositions" /></label>
        <label>Service alerts <input type="url" aria-label="service alerts url" bind:value={draft.alerts} placeholder="https://…/Alerts" /></label>
        <div class="row">
          <button type="button" onclick={() => (open = false)}>Close</button>
          <button type="submit" class="primary">Use these feeds</button>
        </div>
      </form>
    </div>
  {/if}
</div>

<style>
  .live { position: relative; }
  .pill {
    display: inline-flex; align-items: center; gap: 0.45rem; border: 0; cursor: pointer; font: inherit; font-size: 0.8rem; font-weight: 600;
    padding: 0.4rem 0.75rem; border-radius: 999px; background: var(--surface); color: var(--text);
    box-shadow: 0 2px 10px rgb(0 0 0 / 0.15); white-space: nowrap;
  }
  .dot { width: 0.55rem; height: 0.55rem; border-radius: 50%; background: var(--muted); }
  .live .pill.live .dot { background: var(--ok); animation: pulse 2s infinite; }
  .pill.error .dot, .pill.off .dot { background: var(--bad); }
  @keyframes pulse { 0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--ok) 60%, transparent); } 70% { box-shadow: 0 0 0 7px transparent; } 100% { box-shadow: 0 0 0 0 transparent; } }
  .pop {
    position: absolute; right: 0; top: calc(100% + 0.4rem); width: min(22rem, calc(100vw - 2rem)); z-index: 10;
    background: var(--surface); border: 1px solid var(--line); border-radius: 14px; padding: 0.75rem;
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.2); font-size: 0.85rem;
  }
  .pop p { margin: 0 0 0.5rem; }
  .bad { color: var(--bad); }
  form { display: grid; gap: 0.45rem; }
  label { display: grid; gap: 0.15rem; color: var(--muted); font-size: 0.78rem; }
  input { font: inherit; font-size: 0.82rem; padding: 0.35rem 0.5rem; border: 1px solid var(--line); border-radius: 8px; background: var(--surface); color: var(--text); }
  .row { display: flex; justify-content: flex-end; gap: 0.4rem; margin-top: 0.2rem; }
  .row button { font: inherit; padding: 0.35rem 0.8rem; border-radius: 8px; border: 1px solid var(--line); background: var(--surface); color: var(--text); cursor: pointer; }
  .row .primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
