<script lang="ts">
  /// A frame with no answer, said plainly: what it would show, why it
  /// cannot (an op the bus does not have yet, a decision it waits on,
  /// a capability that did not load, a document that is not there, or
  /// an op that refused), and — for a failure — a retry. The rest of
  /// the view keeps answering around it. `state` is the frame's state
  /// as the answer graph keeps it (src/uis/answers.ts).
  type FrameState =
    | { kind: "unassembled"; cap: string }
    | { kind: "starved"; missing: string }
    | { kind: "failed"; message: string };
  let {
    title,
    reason,
    blockers = [],
    error = "",
    state,
    onretry,
  }: {
    title: string;
    reason?: string;
    blockers?: string[];
    error?: string;
    state?: FrameState;
    onretry?: () => void;
  } = $props();
  const failure = $derived(state?.kind === "failed" ? state.message : error);
  const said = $derived(
    state?.kind === "unassembled" ? "This part of the app did not load." : state?.kind === "starved" ? `Waiting for ${state.missing}.` : reason,
  );
  const retry = $derived(state && state.kind !== "failed" ? undefined : onretry);
</script>

<div class="unavailable" class:failed={!!failure} role="status" data-testid="unavailable" data-state={state?.kind ?? (error ? "failed" : "")}>
  <div class="head">
    <span class="icon" aria-hidden="true">{failure ? "!" : "–"}</span>
    <strong>{title}</strong>
  </div>
  {#if failure}
    <p>Couldn't answer this: {failure}</p>
  {:else if said}
    <p>{said}</p>
  {/if}
  {#if blockers.length || retry}
    <div class="foot">
      {#each blockers as b (b)}<span class="blocker">{b}</span>{/each}
      {#if retry}<button type="button" onclick={retry}>Try again</button>{/if}
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
