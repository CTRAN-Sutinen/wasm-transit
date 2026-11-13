<script lang="ts">
  /// Is the pick ready to publish: the document's issues and warnings
  /// (network `validate`, or cap-author's answer for a document it
  /// authored), the export — the network's GTFS
  /// path, which refuses an invalid document itself — and publishing
  /// the pick to the deployment, its answer as the server gave it.
  /// An issue reads as a sentence (designFormat `issueWords`). A block
  /// whose trip arrives after its next trip leaves offers the planner's
  /// usual answer as an event: hold that next trip until the bus is
  /// there — the issue's own trip and time, the host's `retime_trip`.
  import Unavailable from "./Unavailable.svelte";
  import type { NetworkError } from "../types/NetworkError";
  import type { NetworkWarning } from "../types/NetworkWarning";
  import { clock, issueWords } from "./designFormat";

  let {
    issues,
    warnings,
    framed,
    exported,
    exportError,
    onexport,
    published,
    publishRefused,
    publishError,
    onpublish,
    onhold,
    onblock,
    signin,
    locked = "This is the base network, not a pick. Fork it into a plan (Plans view) to export or publish.",
  }: {
    issues: NetworkError[];
    warnings: NetworkWarning[];
    framed: boolean;
    exported: string;
    exportError: string;
    onexport: () => void;
    published: { plans_hash: string } | null;
    publishRefused: object[];
    publishError: string;
    onpublish: () => void;
    /// Hold a trip until a time (a block's next trip, until its bus
    /// arrives); absent, no fix is offered.
    onhold?: (trip: string, toSecs: number) => void;
    /// Open a block an issue names; absent, the block is text.
    onblock?: (block: string) => void;
    /// Who is signed in to publish, beside the button (the page's
    /// SignInControl); absent where the deployment asks for no sign-in.
    signin?: import("svelte").Snippet;
    /// Why the plan cannot be exported or published (shown when it
    /// cannot): the host's words for its plan set.
    locked?: string;
  } = $props();

  /// The block an issue is about (every block issue names it), else null.
  function blockOf(issue: object): string | null {
    const body = Object.values(issue)[0] as { block?: unknown } | undefined;
    return typeof body?.block === "string" ? body.block : null;
  }

  /// The trip to hold and until when, as a block_infeasible issue
  /// names them; null for every other issue.
  function hold(issue: object): { trip: string; until: number } | null {
    const f = (issue as { block_infeasible?: { next: string; arrival_secs: number } }).block_infeasible;
    return f ? { trip: f.next, until: f.arrival_secs } : null;
  }

  const words = issueWords;
</script>

<section class="panel" aria-label="Ready to publish?" data-testid="readiness">
  <header>
    <h2>Ready to publish?</h2>
    <button type="button" class="quiet" onclick={onexport} disabled={!framed} data-testid="export">Export GTFS</button>
    <button type="button" onclick={onpublish} disabled={!framed} data-testid="publish">Publish</button>
    {@render signin?.()}
  </header>
  {#if issues.length}
    <p class="gate" data-testid="readiness-gate">
      Not ready: {issues.length} {issues.length === 1 ? "issue" : "issues"} to fix. The export refuses a plan with issues, and so does the deployment. Fix {issues.length === 1 ? "it" : "them"} on the route, or undo the change that made {issues.length === 1 ? "it" : "them"}.
    </p>
  {:else if warnings.length}
    <p class="gate soft" data-testid="readiness-gate">Ready, with {warnings.length} {warnings.length === 1 ? "warning" : "warnings"} to read first. A warning never blocks the export or the pick.</p>
  {/if}
  <ul>
    {#each issues as issue, i (i)}
      {@const w = words(issue)}
      {@const h = framed && onhold ? hold(issue) : null}
      <li class="bad">
        <span class="dot">!</span>
        <div>
          <strong>{w.kind}</strong>
          <div class="muted">{w.detail}</div>
          {#if onblock && blockOf(issue)}
            <button type="button" class="fix" onclick={() => onblock?.(blockOf(issue)!)}>Open block {blockOf(issue)}</button>
          {/if}
          {#if h}
            <button type="button" class="fix" onclick={() => onhold?.(h.trip, h.until)} title="Moves trip {h.trip} alone; it keeps its running time and its block. Later trips of the block may then need the same.">Hold trip {h.trip} until {clock(h.until)}</button>
          {/if}
        </div>
      </li>
    {/each}
    {#each warnings as warning, i (i)}
      {@const w = words(warning)}
      <li class="warn"><span class="dot">!</span><div><strong>{w.kind}</strong><div class="muted">{w.detail}</div></div></li>
    {/each}
    {#if !issues.length && !warnings.length}
      <li class="ok"><span class="dot">✓</span><div><strong>No issues</strong><div class="muted">the document validates</div></div></li>
    {/if}
  </ul>
  {#if publishError}
    <div class="pad"><Unavailable title="Publish" error={publishError} onretry={onpublish} /></div>
  {:else if publishRefused.length}
    <ul data-testid="publish-refused">
      {#each publishRefused as refusal, i (i)}
        {@const w = words(refusal)}
        <li class="bad"><span class="dot">!</span><div><strong>Not published: {w.kind}</strong><div class="muted">{w.detail}</div></div></li>
      {/each}
    </ul>
  {:else if published}
    <p class="muted pad" data-testid="published">Published — plan set {published.plans_hash.slice(0, 19)}…</p>
  {/if}
  {#if exportError && issues.length}
    <!-- The export refused the plan for the issues listed above: say
         so once, in words; the op's own message rides as the tooltip. -->
    <p class="pad refused" data-testid="export-refused" title={exportError}>Not exported: the export refused this plan for the {issues.length === 1 ? "issue" : `${issues.length} issues`} above.</p>
  {:else if exportError}
    <div class="pad"><Unavailable title="Export" error={exportError} onretry={onexport} /></div>
  {:else if exported}
    <p class="muted pad">Exported — feed {exported.slice(0, 19)}…</p>
  {:else if !framed}
    <p class="muted pad">{locked}</p>
  {/if}
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; min-width: 0; }
  header { display: flex; gap: 0.6rem; align-items: center; padding: 0.6rem 0.9rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  header button:first-of-type { margin-left: auto; }
  header button.quiet { background: transparent; color: var(--text); }
  header button { font: inherit; font-weight: 600; padding: 0.3rem 0.8rem; border-radius: 6px; border: 1px solid var(--text); background: var(--text); color: var(--surface); cursor: pointer; }
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
  .gate { margin: 0; padding: 0.55rem 0.9rem; font-size: 0.82rem; border-bottom: 1px solid var(--line); color: var(--bad); background: color-mix(in srgb, var(--bad) 7%, var(--surface)); }
  .fix { margin-top: 0.35rem; font: inherit; font-size: 0.78rem; padding: 0.15rem 0.55rem; border-radius: 6px; border: 1px solid var(--line); background: var(--surface); color: var(--text); cursor: pointer; }
  .fix:hover { border-color: var(--accent); color: var(--accent); }
  .refused { color: var(--bad); font-size: 0.82rem; }
  .gate.soft { color: var(--warn); background: color-mix(in srgb, var(--warn) 8%, var(--surface)); }
</style>
