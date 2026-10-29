<script lang="ts">
  /// A stop display's header — a RENDER component: which stop this is
  /// (the directory's name and id), the time now (the host's clock),
  /// and whether the times are live.
  import { clockText } from "./rideFormat";

  let {
    name,
    stopId,
    nowSecs,
    live,
    onsetup,
  }: {
    name: string;
    stopId: string;
    nowSecs: number;
    /// A live document fed the board.
    live: boolean;
    onsetup: () => void;
  } = $props();
</script>

<header class="head">
  <div class="stop">
    <h1 data-testid="display-stop">{name}</h1>
    <span class="id">Stop {stopId}</span>
  </div>
  <div class="right">
    <span class="mode" class:on={live} data-testid="display-live">{live ? "● Live times" : "Scheduled times"}</span>
    <span class="clock" data-testid="display-clock">{clockText(nowSecs)}</span>
    <button class="setup" onclick={onsetup} aria-label="Change the stop or layout">⚙</button>
  </div>
</header>

<style>
  .head {
    display: flex; align-items: center; justify-content: space-between; gap: 1em;
    padding: 0.5em 0.8em; border-bottom: 3px solid var(--accent);
    font-size: clamp(1rem, 3vh, 2.4rem);
  }
  .stop { min-width: 0; display: grid; }
  h1 { margin: 0; font-size: 1.4em; line-height: 1.15; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .id { color: var(--muted); font-size: 0.55em; }
  .right { display: flex; align-items: center; gap: 0.8em; flex: none; }
  .mode { font-size: 0.5em; color: var(--muted); }
  .mode.on { color: var(--ok); }
  .clock { font-weight: 800; font-size: 1.3em; font-variant-numeric: tabular-nums; }
  .setup { border: 0; background: none; color: var(--muted); font-size: 0.7em; cursor: pointer; opacity: 0.5; }
  .setup:hover, .setup:focus-visible { opacity: 1; }
</style>
