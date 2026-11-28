<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    subtitle = "",
    steps = [],
    step = 0,
    onstep,
    onclose,
    closeLabel = "Close",
    problems = [],
    card = false,
    tools,
    children,
    actions,
    testid,
  }: {
    title: string;
    subtitle?: string;
    steps?: string[];
    step?: number;
    onstep?: (i: number) => void;
    onclose: () => void;
    closeLabel?: string;
    /// The last save's refusal shows beside the actions, so it is in view
    /// wherever the body is scrolled.
    problems?: string[];
    card?: boolean;
    tools?: Snippet;
    children: Snippet;
    actions: Snippet;
    testid: string;
  } = $props();

  let body: HTMLDivElement;
  $effect(() => {
    void step;
    body?.scrollTo({ top: 0 });
  });
</script>

<section class="task" class:card data-testid={testid}>
  <header>
    <div class="titles">
      <h2>{title}</h2>
      {#if subtitle}<p class="sub">{subtitle}</p>{/if}
    </div>
    {#if tools}<div class="tools">{@render tools()}</div>{/if}
    <button type="button" class="close" onclick={onclose} data-testid="{testid}-close">{closeLabel}</button>
  </header>
  {#if steps.length > 1}
    <ol class="steps" class:many={steps.length > 4} aria-label="Steps">
      {#each steps as s, i (s)}
        <li class:done={i < step} class:now={i === step}>
          <button type="button" disabled={!onstep || i === step} onclick={() => onstep?.(i)} title={s} aria-current={i === step ? "step" : undefined} data-testid="{testid}-step-{i}">
            <span class="n">{i + 1}</span><span class="label">{s}</span>
          </button>
        </li>
      {/each}
    </ol>
  {/if}
  <div class="body" bind:this={body}>
    {@render children()}
  </div>
  <footer>
    {#if problems.length}
      <ul class="problems" role="alert" data-testid="{testid}-problems">{#each problems as p, i (i)}<li>{p}</li>{/each}</ul>
    {/if}
    {@render actions()}
  </footer>
</section>

<style>
  .task { display: grid; grid-template-rows: auto auto 1fr auto; height: 100%; min-height: 0; background: var(--surface); }
  .task.card { height: auto; border: 1px solid var(--accent); border-radius: 8px; overflow: hidden; }
  .task.card .body { overflow: visible; }
  header { display: flex; align-items: flex-start; gap: 0.6rem; padding: 0.8rem 0.9rem 0.5rem; border-bottom: 1px solid var(--line); }
  .titles { flex: 1; min-width: 0; }
  h2 { margin: 0; font-size: 1.05rem; }
  .sub { margin: 0.15rem 0 0; color: var(--muted); font-size: 0.85rem; }
  .tools { display: flex; gap: 0.4rem; }
  .close { flex: none; }
  .steps { list-style: none; margin: 0; padding: 0.45rem 0.9rem; display: flex; gap: 0.25rem; overflow-x: auto; border-bottom: 1px solid var(--line); scrollbar-width: none; }
  .steps li { flex: 1 0 auto; }
  .steps button {
    display: flex; align-items: center; gap: 0.35rem; width: 100%; padding: 0.25rem 0.4rem; border: 0; border-bottom: 3px solid var(--line);
    background: none; color: var(--muted); font: inherit; font-size: 0.8rem; cursor: pointer; white-space: nowrap; border-radius: 0;
  }
  .steps button:disabled { cursor: default; }
  .steps .done button { border-bottom-color: color-mix(in srgb, var(--accent) 55%, var(--line)); color: var(--text); }
  .steps .now button { border-bottom-color: var(--accent); color: var(--text); font-weight: 600; }
  .n { display: inline-grid; place-items: center; width: 1.25rem; height: 1.25rem; border-radius: 50%; background: var(--hover); font-size: 0.72rem; }
  .now .n { background: var(--accent); color: var(--surface); }
  .body { overflow: auto; padding: 0.9rem; min-height: 0; }
  footer { display: flex; flex-wrap: wrap; gap: 0.5rem; align-items: center; padding: 0.6rem 0.9rem; border-top: 1px solid var(--line); background: var(--surface); }
  footer :global(.grow) { flex: 1; }
  .problems { flex-basis: 100%; margin: 0 0 0.2rem; padding-left: 1.1rem; color: var(--bad); font-size: 0.88rem; }
  footer :global(button) { min-height: 2.4rem; padding: 0 1rem; }
  footer :global(button.primary) { background: var(--accent); color: var(--surface); border-color: var(--accent); font-weight: 600; }
  footer :global(button.primary:disabled) { opacity: 0.5; }
  .steps.many li:not(.now) .label { display: none; }
  .steps.many li { flex: 0 0 auto; }
  .steps.many li.now { flex: 1 0 auto; }
  @media (max-width: 800px) {
    .steps li:not(.now) .label { display: none; }
    .steps li { flex: 0 0 auto; }
    footer :global(button) { min-height: 2.9rem; flex: 1; }
  }
</style>
