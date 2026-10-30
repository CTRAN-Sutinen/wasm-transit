<script lang="ts">
  /// Is the pick ready to publish: the document's issues (network
  /// `validate`, or cap-author's answer for a document it authored),
  /// the last change's warnings, and the export — the network's
  /// publish path, which refuses an invalid document itself.
  import Unavailable from "./Unavailable.svelte";
  import type { NetworkError } from "../types/NetworkError";
  import type { NetworkWarning } from "../types/NetworkWarning";

  let {
    issues,
    warnings,
    framed,
    exported,
    exportError,
    onexport,
  }: {
    issues: NetworkError[];
    warnings: NetworkWarning[];
    framed: boolean;
    exported: string;
    exportError: string;
    onexport: () => void;
  } = $props();

  /// An issue or warning as words: its kind, then its fields.
  function words(item: object): { kind: string; detail: string } {
    const [kind, body] = Object.entries(item)[0] ?? ["", {}];
    const detail = Object.entries(body as Record<string, unknown>)
      .map(([k, v]) => `${k.replace(/_/g, " ")} ${v}`)
      .join(", ");
    return { kind: kind.replace(/_/g, " "), detail };
  }
</script>

<section class="panel" aria-label="Ready to publish?" data-testid="readiness">
  <header>
    <h2>Ready to publish?</h2>
    <button type="button" onclick={onexport} disabled={!framed} data-testid="export">Export GTFS</button>
  </header>
  <ul>
    {#each issues as issue, i (i)}
      {@const w = words(issue)}
      <li class="bad"><span class="dot">!</span><div><strong>{w.kind}</strong><div class="muted">{w.detail}</div></div></li>
    {/each}
    {#each warnings as warning, i (i)}
      {@const w = words(warning)}
      <li class="warn"><span class="dot">!</span><div><strong>{w.kind}</strong><div class="muted">{w.detail}</div></div></li>
    {/each}
    {#if !issues.length && !warnings.length}
      <li class="ok"><span class="dot">✓</span><div><strong>No issues</strong><div class="muted">the document validates</div></div></li>
    {/if}
  </ul>
  {#if exportError}
    <div class="pad"><Unavailable title="Export" error={exportError} onretry={onexport} /></div>
  {:else if exported}
    <p class="muted pad">Exported — feed {exported.slice(0, 19)}…</p>
  {:else if !framed}
    <p class="muted pad">The unframed import is the base; export a forked plan.</p>
  {/if}
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; gap: 0.6rem; align-items: center; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  header button { margin-left: auto; font: inherit; font-weight: 600; padding: 0.3rem 0.8rem; border-radius: 6px; border: 1px solid var(--text); background: var(--text); color: var(--surface); cursor: pointer; }
  header button:disabled { opacity: 0.4; cursor: default; }
  ul { list-style: none; margin: 0; padding: 0; max-height: 50vh; overflow-y: auto; }
  li { display: grid; grid-template-columns: 1.4rem 1fr; gap: 0.6rem; padding: 0.55rem 0.9rem; border-bottom: 1px solid var(--line); font-size: 0.85rem; }
  li strong { display: inline-block; }
  li strong::first-letter { text-transform: uppercase; }
  .dot { width: 1.2rem; height: 1.2rem; border-radius: 50%; display: flex; align-items: center; justify-content: center; font-size: 0.7rem; font-weight: 700; color: #fff; }
  .bad .dot { background: var(--bad); }
  .warn .dot { background: var(--warn); }
  .ok .dot { background: var(--ok); }
  .muted { color: var(--muted); font-size: 0.78rem; }
  .pad { margin: 0; padding: 0.6rem 0.9rem; }
</style>
