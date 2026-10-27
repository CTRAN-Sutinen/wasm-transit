<script lang="ts">
  /// The change gesture on one schedule: its departures as the
  /// document holds them, one trip added or removed per click. RENDER
  /// only — each click is an event the host turns into a NetworkEdit
  /// (cap-author `add_start` / `remove_start`, which own the +10 min
  /// placement and the one-start-stays rule).
  let {
    scheduleId,
    starts,
    framed,
    onadd,
    onremove,
  }: {
    scheduleId: string;
    starts: number[] | null;
    framed: boolean;
    onadd: () => void;
    onremove: (index: number) => void;
  } = $props();

  function hm(secs: number): string {
    return `${String(Math.floor(secs / 3600)).padStart(2, "0")}:${String(Math.floor((secs % 3600) / 60)).padStart(2, "0")}`;
  }
</script>

<section class="schedule-change" data-testid="schedule-change">
  <h3>Schedule <span class="muted">{scheduleId}</span></h3>
  {#if starts === null}
    <p class="muted">Headway-band schedule — its trips are its bands.</p>
  {:else}
    {#if !framed}<p class="note">Fork the import into a plan to change it.</p>{/if}
    <div class="starts">
      {#each starts as s, i (i)}
        <span class="start">{hm(s)}{#if framed}<button title="remove this trip" onclick={() => onremove(i)}>×</button>{/if}</span>
      {/each}
      {#if framed}<button class="add" onclick={onadd}>+ trip</button>{/if}
    </div>
  {/if}
</section>

<style>
  h3 { margin: 0 0 0.25rem; font-size: 0.95rem; }
  .muted { color: var(--muted); font-weight: normal; font-size: 0.85rem; }
  .note { font-size: 0.85rem; margin: 0 0 0.25rem; }
  .starts { display: flex; flex-wrap: wrap; gap: 4px; font-size: 0.8rem; font-variant-numeric: tabular-nums; }
  .start { border: 1px solid var(--line); border-radius: 4px; padding: 0 4px; }
  .start button { border: 0; background: none; color: inherit; cursor: pointer; padding: 0 0 0 3px; }
  .add { font-size: 0.8rem; }
</style>
