<script lang="ts">
  /// The front door: every generated UI (each `src/uis/<id>/ui.json`,
  /// discovered at build time) with the questions it answers, and the
  /// original app demoted to a link. RENDER only — the list is data
  /// the build carries; nothing here calls the bus.
  type Ui = {
    id: string;
    title: string;
    summary: string;
    questions: string[];
    components: string[];
    session?: string;
    generated?: string;
  };

  const found = import.meta.glob<Ui>("../uis/*/ui.json", { eager: true, import: "default" });
  const uis = Object.values(found).sort((a, b) => a.title.localeCompare(b.title));
</script>

<main class="picker">
  <header>
    <h1>Transit UIs</h1>
    <p class="muted">
      Each one was generated for a set of questions. They are composed from the same render
      components over the same capability bus, so they give the same answers.
    </p>
  </header>

  <ul class="uis" data-testid="ui-list">
    {#each uis as ui (ui.id)}
      <li>
        <a class="card" href={`${ui.id}.html`} data-testid={`ui-${ui.id}`}>
          <span class="title">{ui.title}</span>
          <span class="summary">{ui.summary}</span>
          <ul class="questions">
            {#each ui.questions as q}<li>{q}</li>{/each}
          </ul>
          <span class="meta">
            {ui.components.length} components{#if ui.generated} · generated {ui.generated}{/if}
          </span>
        </a>
      </li>
    {:else}
      <li class="muted">No generated UIs yet: add one under <code>src/uis/&lt;id&gt;/</code>.</li>
    {/each}
  </ul>

  <footer>
    <a href="app.html" data-testid="original-app">The original app</a>
    <span class="muted">(persona tabs: rider, dispatcher, planner)</span>
  </footer>
</main>

<style>
  .picker { max-width: 56rem; margin: 0 auto; padding: 2rem 16px; }
  :global(body) { overflow: auto; }
  h1 { margin: 0 0 0.25rem; font-size: 1.5rem; }
  header p { margin: 0 0 1.5rem; max-width: 40rem; }
  .uis { list-style: none; margin: 0; padding: 0; display: grid; gap: 1rem; grid-template-columns: repeat(auto-fill, minmax(18rem, 1fr)); }
  .card {
    display: flex; flex-direction: column; gap: 0.5rem; height: 100%;
    padding: 1rem; border: 1px solid var(--line); border-radius: 8px;
    color: inherit; text-decoration: none;
  }
  .card:hover { border-color: var(--accent); background: var(--hover); }
  .title { font-weight: 600; font-size: 1.05rem; color: var(--accent); }
  .summary { font-size: 0.9rem; }
  .questions { margin: 0; padding-left: 1.1rem; font-size: 0.82rem; color: var(--muted); }
  .meta { margin-top: auto; font-size: 0.75rem; color: var(--muted); }
  footer { margin-top: 2rem; font-size: 0.9rem; display: flex; gap: 0.5rem; align-items: baseline; }
  footer a { color: var(--accent); }
  code { font-size: 0.85em; }
</style>
