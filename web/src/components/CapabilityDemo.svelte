<script lang="ts">
  /// The capability-bus demo panel: renders the host's demo state
  /// (capdemo.svelte.ts runs the composition and the document flow).
  import {
    demo, capabilityRows, watchlistState, assembleDemo,
    runDemo, simulateDegraded, seedV1, migrateForward, rollback,
  } from "../capdemo.svelte";

  $effect(() => {
    assembleDemo();
  });
  const rows = $derived(capabilityRows());
  const watchlist = $derived(watchlistState());
</script>

<section class="caps-panel" data-testid="caps-panel">
  <h3>Capability assembly <span class="tag">the bus</span></h3>
  <p class="hint">
    Horizontal capabilities assembled from a manifest: hash-pinned modules, loaded on
    demand, composed through one document.
  </p>

  {#if rows}
    <ul class="cap-list">
      {#each rows as cap (cap.id)}
        <li class="cap" data-testid={`cap-${cap.id}`}>
          <span class="cap-id">{cap.id}</span>
          <span class="cap-hash">{cap.hash.slice(0, 19)}…</span>
          <span class={`cap-status ${cap.status}`}>{cap.status}</span>
          <span class="cap-bytes">{cap.kib} KiB</span>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="hint">loading manifest…</p>
  {/if}

  <div class="actions">
    <button onclick={runDemo} disabled={demo.running} data-testid="caps-run">
      {demo.running ? "running…" : "Run composition demo"}
    </button>
    <button onclick={simulateDegraded} disabled={demo.running} data-testid="caps-degrade">
      Simulate missing router
    </button>
  </div>

  {#if demo.steps.length > 0}
    <ul class="steps">
      {#each demo.steps as step, i}
        <li class={step.ok ? "ok" : "bad"} data-testid={`caps-step-${i}`}>
          <span>{step.ok ? "✓" : "✗"}</span> {step.label}: {step.detail}
        </li>
      {/each}
    </ul>
  {/if}
  {#if demo.result}
    <p class="result" data-testid="caps-result">{demo.result}</p>
  {/if}

  <section class="doc-section">
    <h4>Versioned documents: migrate forward, roll back</h4>
    <p class="hint">
      Migrations are pure ops over document bytes; the store keeps every version
      (hash-chained by <code>produced_by</code>) — rollback is a pointer move.
    </p>

    {#if watchlist}
      <p class="doc-state" data-testid="caps-doc-version">
        watchlist v{watchlist.version} ·
        {watchlist.hash.slice(0, 19)}…
      </p>
      <p class="doc-chain hint">
        history: {watchlist.history}
      </p>
    {/if}

    <div class="actions">
      <button onclick={seedV1} data-testid="caps-seed">Seed v1 watchlist</button>
      <button onclick={migrateForward} data-testid="caps-migrate">Migrate → v2</button>
      <button onclick={rollback} data-testid="caps-rollback">Roll back</button>
    </div>

    {#if demo.docSteps.length > 0}
      <ul class="steps">
        {#each demo.docSteps as step, i}
          <li class={step.ok ? "ok" : "note"} data-testid={`caps-doc-step-${i}`}>
            <span>{step.ok ? "✓" : "!"}</span> {step.label}: {step.detail}
          </li>
        {/each}
      </ul>
    {/if}
    {#if demo.board.length > 0}
      <ul class="board" data-testid="caps-board">
        {#each demo.board as stop}
          <li>{stop.id} "<span class="label">{stop.label}</span>" — {stop.count} departures</li>
        {/each}
      </ul>
    {/if}
    {#if demo.docMessage}
      <p class="result" data-testid="caps-doc-message">{demo.docMessage}</p>
    {/if}
  </section>
</section>

<style>
  .caps-panel {
    border-top: 1px solid var(--line, #ddd);
    margin-top: 1rem;
    padding-top: 0.75rem;
  }
  h3 {
    margin: 0 0 0.25rem;
    font-size: 1rem;
  }
  .tag {
    font-size: 0.7rem;
    border: 1px solid var(--line, #ddd);
    border-radius: 4px;
    padding: 0 0.3rem;
    vertical-align: middle;
  }
  .hint {
    margin: 0.25rem 0 0.5rem;
    font-size: 0.8rem;
    opacity: 0.75;
  }
  .cap-list {
    list-style: none;
    margin: 0.5rem 0;
    padding: 0;
    font-size: 0.8rem;
  }
  .cap {
    display: grid;
    grid-template-columns: 5rem 1fr 6rem 4rem;
    gap: 0.5rem;
    padding: 0.15rem 0;
  }
  .cap-status {
    text-align: right;
  }
  .cap-status.ready {
    color: #2a7;
  }
  .cap-status.unavailable {
    color: #c43;
  }
  .cap-bytes {
    text-align: right;
    opacity: 0.7;
  }
  .actions {
    display: flex;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }
  .steps {
    list-style: none;
    margin: 0.5rem 0;
    padding: 0;
    font-size: 0.8rem;
    font-family: ui-monospace, monospace;
  }
  .steps li.ok {
    color: #2a7;
  }
  .steps li.bad {
    color: #c43;
  }
  .result {
    font-size: 0.85rem;
  }
  .doc-section {
    border-top: 1px solid var(--line, #ddd);
    margin-top: 0.75rem;
    padding-top: 0.5rem;
  }
  .doc-section h4 {
    margin: 0 0 0.25rem;
    font-size: 0.9rem;
  }
  .doc-state {
    font-family: ui-monospace, monospace;
    font-size: 0.8rem;
    margin: 0.25rem 0;
  }
  .steps li.note {
    color: #b80;
  }
  .board {
    list-style: none;
    margin: 0.25rem 0;
    padding: 0;
    font-size: 0.85rem;
  }
  .board .label {
    color: #2a7;
  }
</style>
