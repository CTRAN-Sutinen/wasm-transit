<script lang="ts">
  /// Which plan, against which baseline, and where it sits in the
  /// chain — cap-plans' answers (in force, governing periods, chain
  /// issues) beside the document's own issues and cap-author's
  /// warnings. RENDER only: opening, comparing, forking, and loading
  /// a feed are events.
  import type { FeedInfo } from "../types/FeedInfo";
  import type { NetworkError } from "../types/NetworkError";
  import type { NetworkWarning } from "../types/NetworkWarning";
  import type { PlanIssue } from "../types/cap_plans/PlanIssue";
  import type { PlanPeriod } from "../types/cap_plans/PlanPeriod";

  type PlanRow = { key: string; id: string; name: string; effectiveFrom: string | null };

  let {
    feed,
    plans,
    activeKey,
    compareKey,
    chain,
    issues,
    warnings,
    status,
    busy,
    onopen,
    oncompare,
    onfork,
    onfeed,
  }: {
    feed: { label: string; info: FeedInfo | null } | null;
    plans: PlanRow[];
    activeKey: string;
    compareKey: string;
    chain: { inForce: string | null; periods: PlanPeriod[]; issues: PlanIssue[] } | null;
    issues: NetworkError[];
    warnings: NetworkWarning[];
    status: string;
    busy: boolean;
    onopen: (key: string) => void;
    oncompare: (key: string) => void;
    onfork: (name: string, effectiveFrom: string) => void;
    onfeed: (file: File) => void;
  } = $props();

  let forkName = $state("");
  let forkDate = $state("");

  const activeId = $derived(plans.find((p) => p.key === activeKey)?.id ?? "");
  const period = $derived(chain?.periods.find((p) => p.id === activeId) ?? null);

  /// A tagged-union row as words: its tag, then its fields.
  function said(tagged: object): string {
    const [[tag, body]] = Object.entries(tagged);
    return `${tag.replaceAll("_", " ")}: ${Object.values(body as object).join(", ")}`;
  }
</script>

<header class="plan-bar" data-testid="plan-bar">
  <div class="feed">
    {#if feed?.info}
      <b>{feed.info.feed_publisher_name}</b> <span class="muted">{feed.info.feed_version ?? ""}</span>
    {:else if feed}
      <b>{feed.label}</b>
    {/if}
    <label class="file">feed… <input type="file" accept=".zip" onchange={(e) => {
      const f = (e.currentTarget as HTMLInputElement).files?.[0];
      if (f) onfeed(f);
    }} /></label>
  </div>

  {#if plans.length}
    <label>Plan
      <select value={activeKey} onchange={(e) => onopen((e.currentTarget as HTMLSelectElement).value)} data-testid="plan-select">
        {#each plans as p (p.key)}<option value={p.key}>{p.name}{p.effectiveFrom ? ` · ${p.effectiveFrom}` : ""}</option>{/each}
      </select>
    </label>
    <label>vs
      <select value={compareKey} onchange={(e) => oncompare((e.currentTarget as HTMLSelectElement).value)} data-testid="compare-select">
        <option value="">—</option>
        {#each plans.filter((p) => p.key !== activeKey) as p (p.key)}<option value={p.key}>{p.name}</option>{/each}
      </select>
    </label>
    <span class="chain">
      {#if period}
        governs {period.effective_from} → {period.until ?? "open"}
        {#if chain?.inForce === activeId}<b>· in force</b>{/if}
      {:else if !activeId}
        <span class="muted">unframed import</span>
      {/if}
      {#each chain?.issues ?? [] as i}<span class="bad">{i.kind.replaceAll("_", " ")}</span>{/each}
    </span>
    <form class="fork" onsubmit={(e) => { e.preventDefault(); onfork(forkName, forkDate); forkName = ""; }}>
      <input placeholder="new plan name" bind:value={forkName} data-testid="fork-name" />
      <input type="date" bind:value={forkDate} data-testid="fork-date" />
      <button disabled={busy || !forkName || !forkDate}>Fork</button>
    </form>
  {/if}

  <div class="health" data-testid="plan-health">
    <span class:busy>{status}</span>
    {#if issues.length}<span class="bad" title={issues.map(said).join("\n")}>{issues.length} issue{issues.length === 1 ? "" : "s"}</span>{/if}
    {#each warnings as w}<span class="warn">{said(w)}</span>{/each}
  </div>
</header>

<style>
  .plan-bar {
    display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem 1rem;
    padding: 0.4rem 0.75rem; font-size: 0.85rem;
    border-bottom: 1px solid var(--line);
  }
  .feed { display: flex; gap: 0.5rem; align-items: baseline; }
  .file input { width: 0; height: 0; opacity: 0; position: absolute; }
  .file { cursor: pointer; text-decoration: underline; opacity: 0.75; }
  .fork { display: flex; gap: 0.25rem; }
  .fork input:first-child { width: 9rem; }
  .health { flex-basis: 100%; display: flex; flex-wrap: wrap; gap: 0.75rem; }
  .muted { color: var(--muted); }
  .bad { color: var(--bad); }
  .warn { color: var(--warn); }
  .busy { opacity: 0.65; font-style: italic; }
</style>
