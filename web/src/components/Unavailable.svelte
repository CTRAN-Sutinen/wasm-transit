<script lang="ts">
  /// A frame with no answer, said plainly: what it would show, why it
  /// cannot (an op the bus does not have yet, a decision it waits on,
  /// or a capability that failed), and — for a failure — a retry. The
  /// rest of the view keeps answering around it.
  let {
    title,
    reason,
    blockers = [],
    error = "",
    onretry,
  }: {
    title: string;
    reason?: string;
    blockers?: string[];
    error?: string;
    onretry?: () => void;
  } = $props();
</script>

<div class="unavailable" class:failed={!!error} role="status" data-testid="unavailable">
  <div class="head">
    <span class="icon" aria-hidden="true">{error ? "!" : "–"}</span>
    <strong>{title}</strong>
  </div>
  {#if error}
    <p>Couldn't answer this: {error}</p>
  {:else if reason}
    <p>{reason}</p>
  {/if}
  {#if blockers.length || onretry}
    <div class="foot">
      {#each blockers as b (b)}<span class="blocker">{b}</span>{/each}
      {#if onretry}<button type="button" onclick={onretry}>Try again</button>{/if}
    </div>
  {/if}
</div>

<style>
  .unavailable {
    border: 1px dashed var(--line);
    border-radius: 8px;
    padding: 0.75rem 0.9rem;
    color: var(--muted);
    background: var(--surface);
    font-size: 0.85rem;
  }
  .unavailable.failed { border-style: solid; border-color: var(--bad); }
  .head { display: flex; gap: 0.5rem; align-items: center; color: var(--text); }
  .icon {
    display: inline-flex; width: 1.2rem; height: 1.2rem; border-radius: 50%;
    align-items: center; justify-content: center; font-size: 0.75rem; font-weight: 700;
    border: 1px solid var(--line);
  }
  .failed .icon { background: var(--bad); color: var(--surface); border-color: var(--bad); }
  p { margin: 0.35rem 0 0; }
  .foot { display: flex; flex-wrap: wrap; gap: 0.4rem; margin-top: 0.5rem; align-items: center; }
  .blocker {
    font-size: 0.75rem; padding: 0.05rem 0.4rem; border-radius: 4px;
    border: 1px dashed var(--line); color: var(--muted);
  }
  button { font: inherit; padding: 0.2rem 0.6rem; border-radius: 6px; border: 1px solid var(--line); background: var(--surface); color: var(--text); cursor: pointer; }
</style>
