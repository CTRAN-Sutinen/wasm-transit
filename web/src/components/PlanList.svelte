<script lang="ts">
  /// The plan set: each plan with its pick date, which is open and
  /// which is the baseline; open, set as baseline, fork the open plan
  /// into a dated pick, or import a feed as a new base.
  let {
    plans,
    activeKey,
    compareKey,
    feed,
    onopen,
    oncompare,
    onfork,
    onfeed,
  }: {
    plans: { key: string; id: string; name: string; effectiveFrom: string | null }[];
    activeKey: string;
    compareKey: string;
    feed: { label: string; info: { feed_version?: string | null } | null } | null;
    onopen: (key: string) => void;
    oncompare: (key: string) => void;
    onfork: (name: string, effectiveFrom: string) => void;
    onfeed: (file: File) => void;
  } = $props();

  let name = $state("");
  let date = $state("");
</script>

<section class="panel" aria-label="Plans" data-testid="plan-list">
  <header>
    <h2>Plans</h2>
    {#if feed}<span class="muted">base: {feed.label}{feed.info?.feed_version ? ` · ${feed.info.feed_version}` : ""}</span>{/if}
  </header>
  <table>
    <thead><tr><th>Plan</th><th>Picks</th><th></th></tr></thead>
    <tbody>
      {#each plans as p (p.key)}
        <tr class:active={p.key === activeKey}>
          <td>
            <strong>{p.name}</strong>
            {#if p.key === activeKey}<span class="tag open">open</span>{/if}
            {#if p.key === compareKey}<span class="tag">baseline</span>{/if}
          </td>
          <td class="muted">{p.effectiveFrom ?? (p.id ? "no date" : "unframed — the base")}</td>
          <td class="actions">
            {#if p.key !== activeKey}<button type="button" onclick={() => onopen(p.key)}>Open</button>{/if}
            {#if p.key !== activeKey && p.key !== compareKey}<button type="button" onclick={() => oncompare(p.key)}>Set baseline</button>{/if}
          </td>
        </tr>
      {:else}
        <tr><td colspan="3" class="muted">No plans yet — import a GTFS feed.</td></tr>
      {/each}
    </tbody>
  </table>
  <form class="fork" onsubmit={(e) => { e.preventDefault(); onfork(name, date); name = ""; }}>
    <strong>Fork the open plan</strong>
    <label>Name <input bind:value={name} placeholder="Winter 2027" data-testid="fork-name" /></label>
    <label>Picks on <input type="date" bind:value={date} data-testid="fork-date" /></label>
    <button type="submit" class="pri" disabled={!name.trim() || !date}>Fork</button>
  </form>
  <label class="feed">
    Import a GTFS feed as a new base
    <input type="file" accept=".zip" onchange={(e) => { const f = e.currentTarget.files?.[0]; if (f) onfeed(f); }} data-testid="feed-input" />
  </label>
</section>

<style>
  .panel { background: var(--surface); border: 1px solid var(--line); border-radius: 8px; }
  header { display: flex; gap: 0.6rem; align-items: baseline; padding: 0.7rem 1rem; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 0.9rem; }
  .muted { color: var(--muted); font-size: 0.8rem; }
  table { width: 100%; border-collapse: collapse; }
  th { text-align: left; font-size: 0.68rem; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); font-weight: 500; padding: 0.5rem 1rem; border-bottom: 1px solid var(--line); }
  td { padding: 0.5rem 1rem; border-bottom: 1px solid var(--line); font-size: 0.85rem; }
  tr.active { background: var(--accent-soft); }
  .tag { font-size: 0.7rem; padding: 0.05rem 0.35rem; border-radius: 4px; border: 1px solid var(--line); margin-left: 0.3rem; color: var(--muted); }
  .tag.open { border-color: var(--accent); color: var(--accent); }
  .actions { text-align: right; white-space: nowrap; }
  button { font: inherit; font-size: 0.8rem; padding: 0.25rem 0.6rem; border-radius: 6px; border: 1px solid var(--line); background: var(--surface); color: var(--text); cursor: pointer; margin-left: 0.3rem; }
  button.pri { background: var(--text); color: var(--surface); border-color: var(--text); }
  button:disabled { opacity: 0.5; cursor: default; }
  .fork { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: center; padding: 0.7rem 1rem; border-bottom: 1px solid var(--line); font-size: 0.85rem; }
  .fork input { font: inherit; padding: 0.2rem 0.4rem; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: var(--text); }
  .feed { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: center; padding: 0.7rem 1rem; font-size: 0.8rem; color: var(--muted); }
</style>
