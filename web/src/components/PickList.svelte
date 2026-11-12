<script lang="ts">
  /// The plans as picks on a timeline (D-139): every pick the deployment
  /// publishes and every draft on this device, by pick date, the one in
  /// force today marked (plans `resolve`, from the host). A published
  /// pick is read-only — it opens and forks; a draft is also renamed,
  /// re-dated, and removed. Fork the open plan into a dated draft (its
  /// calendar becomes its period, D-138), or import a feed as a draft
  /// picking on its first service date (D-140). Events out; every change
  /// is the host's.
  type Row = { key: string; id: string; name: string; effectiveFrom: string | null; published: boolean; loaded: boolean };
  let {
    plans,
    activeKey,
    compareKey,
    inForceId,
    onopen,
    oncompare,
    onrename,
    ondate,
    ondelete,
    onfork,
    onfeed,
  }: {
    plans: Row[];
    activeKey: string;
    compareKey: string;
    inForceId: string | null;
    onopen: (key: string) => void;
    oncompare: (key: string) => void;
    onrename: (key: string, name: string) => void;
    ondate: (key: string, date: string) => void;
    ondelete: (key: string) => void;
    onfork: (name: string, effectiveFrom: string, through: string) => void;
    onfeed: (file: File) => void;
  } = $props();

  // By pick date; a plan with none last.
  const rows = $derived([...plans].sort((a, b) => (a.effectiveFrom ?? "9999").localeCompare(b.effectiveFrom ?? "9999") || a.name.localeCompare(b.name)));
  let renaming = $state("");
  let newName = $state("");
  let dating = $state("");
  let newDate = $state("");
  let name = $state("");
  let date = $state("");
  let through = $state("");
</script>

<section class="panel" aria-label="Picks" data-testid="plan-list">
  <header><h2>Picks</h2><span class="muted">published picks and drafts, by the day each takes effect</span></header>
  <table>
    <thead><tr><th>Plan</th><th>Picks on</th><th></th></tr></thead>
    <tbody>
      {#each rows as p (p.key)}
        <tr class:active={p.key === activeKey} data-testid="pick-row">
          <td>
            {#if renaming === p.key}
              <form class="inline" onsubmit={(e) => { e.preventDefault(); if (newName.trim()) onrename(p.key, newName.trim()); renaming = ""; }}>
                <!-- svelte-ignore a11y_autofocus -->
                <input bind:value={newName} aria-label="New name for {p.name}" autofocus onkeydown={(e) => { if (e.key === "Escape") renaming = ""; }} />
                <button type="submit" disabled={!newName.trim()}>Save</button>
                <button type="button" onclick={() => (renaming = "")}>Cancel</button>
              </form>
            {:else}
              <strong>{p.name}</strong>
              {#if p.key === activeKey}<span class="tag open">open</span>{/if}
              {#if p.key === compareKey}<span class="tag">compared</span>{/if}
              {#if p.id && p.id === inForceId}<span class="tag now">in force today</span>{/if}
              <span class="tag" class:pub={p.published}>{p.published ? "published" : "draft"}</span>
            {/if}
          </td>
          <td class="muted">
            {#if dating === p.key}
              <form class="inline" onsubmit={(e) => { e.preventDefault(); if (newDate) ondate(p.key, newDate); dating = ""; }}>
                <input type="date" bind:value={newDate} aria-label="New pick date for {p.name}" />
                <button type="submit" disabled={!newDate}>Save</button>
                <button type="button" onclick={() => (dating = "")}>Cancel</button>
              </form>
            {:else}
              {p.effectiveFrom ?? "no pick date"}
            {/if}
          </td>
          <td class="actions">
            {#if p.key !== activeKey}<button type="button" onclick={() => onopen(p.key)}>Open</button>{/if}
            {#if p.key !== activeKey && p.key !== compareKey}<button type="button" onclick={() => oncompare(p.key)}>Compare</button>{/if}
            {#if p.key === compareKey}<button type="button" onclick={() => oncompare("")}>Stop comparing</button>{/if}
            {#if !p.published && p.loaded}
              <button type="button" data-testid="rename-plan" onclick={() => { renaming = p.key; newName = p.name; }}>Rename</button>
              <button type="button" data-testid="date-plan" onclick={() => { dating = p.key; newDate = p.effectiveFrom ?? ""; }}>Change date</button>
            {/if}
            {#if p.loaded}
              <button type="button" class="del" data-testid="delete-plan"
                onclick={() => { if (confirm(p.published ? `Remove the copy of "${p.name}" from this device? It stays published.` : `Delete the draft "${p.name}"? Its changes and undo history go with it.`)) ondelete(p.key); }}>
                {p.published ? "Remove copy" : "Delete"}</button>
            {/if}
          </td>
        </tr>
      {:else}
        <tr><td colspan="3" class="muted">No picks yet — import a GTFS feed.</td></tr>
      {/each}
    </tbody>
  </table>
  <form class="fork" onsubmit={(e) => { e.preventDefault(); onfork(name, date, through); name = ""; }}>
    <strong>Fork the open plan</strong>
    <label>Name <input bind:value={name} placeholder="e.g. Winter 2027" data-testid="fork-name" /></label>
    <label>Picks on <input type="date" bind:value={date} data-testid="fork-date" /></label>
    <label>Runs through <input type="date" bind:value={through} min={date} data-testid="fork-through" /></label>
    <button type="submit" class="pri" disabled={!name.trim() || !date} title={!name.trim() || !date ? "Give the fork a name and the day it picks" : ""}>Fork</button>
    <p class="hint">A fork is a draft copy of the open plan that you can change; a published pick changes only this way. "Picks on" is the day it takes effect. "Runs through" is optional: the last day of its period, which carries the weekly service to that day. The fork keeps only the dates in its period.</p>
  </form>
  <label class="feed">
    Import a GTFS feed as a draft (it picks on its first service date)
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
  .tag { font-size: 0.7rem; padding: 0.05rem 0.35rem; border-radius: 4px; border: 1px solid var(--line); margin-left: 0.3rem; color: var(--muted); white-space: nowrap; }
  .tag.open { border-color: var(--accent); color: var(--accent); }
  .tag.now { border-color: var(--ok); color: var(--ok); }
  .tag.pub { color: var(--text); }
  .actions { text-align: right; white-space: nowrap; }
  button { font: inherit; font-size: 0.8rem; padding: 0.25rem 0.6rem; border-radius: 6px; border: 1px solid var(--line); background: var(--surface); color: var(--text); cursor: pointer; margin-left: 0.3rem; }
  button.pri { background: var(--text); color: var(--surface); border-color: var(--text); }
  button.del { color: var(--bad); }
  button:disabled { opacity: 0.5; cursor: default; }
  .inline { display: flex; gap: 0.3rem; align-items: center; }
  .inline button { margin-left: 0; }
  input { font: inherit; padding: 0.2rem 0.4rem; border: 1px solid var(--line); border-radius: 5px; background: var(--surface); color: var(--text); }
  .fork { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: center; padding: 0.7rem 1rem; border-bottom: 1px solid var(--line); font-size: 0.85rem; }
  .hint { flex-basis: 100%; margin: 0; font-size: 0.78rem; color: var(--muted); }
  .feed { display: flex; flex-wrap: wrap; gap: 0.6rem; align-items: center; padding: 0.7rem 1rem; font-size: 0.8rem; color: var(--muted); }
</style>
