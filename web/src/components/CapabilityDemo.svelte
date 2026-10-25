<script lang="ts">
  /// The capability-bus demo: load the manifest, assemble gtfs +
  /// router, and run the composition — the same feed bytes into two
  /// independent modules, hashes asserted equal at the host, then a
  /// plan through one and departures through the other. Partial
  /// assemblies degrade per capability: with router unavailable the
  /// gtfs half still runs, and the statuses say so.
  import {
    caps,
    ensure,
    gtfsCap,
    routerCap,
    assertSameDocument,
    forgetCapability,
    restoreDocuments,
    putDocument,
    currentDocument,
    documentHistory,
    rollbackDocument,
    migrateWatchlistForward,
    type GtfsLoadFeed,
  } from "../capabilities.svelte";
  import type { Watchlist } from "../types/cap_gtfs/Watchlist";

  const DATE = "2026-10-23"; // the fixture's loaded Friday
  const DEPART = 7 * 3600;

  let running = $state(false);
  let result = $state("");
  let steps = $state<{ label: string; detail: string; ok: boolean }[]>([]);
  let degraded = $state(false);

  // The versioned-document demo: seed a v1 watchlist, query it
  // (labels are v1 notes), migrate forward to v2 (colors dropped and
  // REPORTED), roll back — the old version still queries, nothing
  // was destroyed.
  const SEED_V1 = JSON.stringify({
    version: "v1",
    stops: [
      { id: "A", note: "home", color: "blue" },
      { id: "B", note: "work" },
    ],
  });
  let docSteps = $state<{ label: string; detail: string; ok: boolean }[]>([]);
  let board = $state<{ id: string; label: string; count: number }[]>([]);
  let docMessage = $state("");

  $effect(() => {
    // The panel DECLARES its needs; the resolver assembles
    // demand-driven. Statuses render from resolver state alone.
    if (!caps.manifest) {
      ensure(["gtfs", "router"]).catch((error: unknown) => {
        steps.push({
          label: "manifest",
          detail: error instanceof Error ? error.message : String(error),
          ok: false,
        });
      });
    }
    restoreDocuments();
  });

  function fmtTime(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
  }

  /// The degraded-state simulation: point the router capability at a
  /// hash nobody serves. The resolver marks it unavailable; the
  /// composition runs its gtfs half only — exactly what a user whose
  /// bundle stopped at "know when the bus comes" would ship.
  async function simulateDegraded(): Promise<void> {
    degraded = true;
    if (caps.manifest) {
      const router = caps.manifest.capabilities.find((c) => c.id === "router");
      if (router) {
        router.url = "caps/router-0000000000000000000000000000000000000000000000000000000000000000.wasm";
        forgetCapability("router");
      }
    }
    await run();
  }

  async function gtfsModule() {
    const modules = await ensure(["gtfs"]);
    if (!modules.gtfs) throw new Error("gtfs capability unavailable");
    return gtfsCap(modules.gtfs);
  }

  async function feedHash(): Promise<string> {
    const gtfs = await gtfsModule();
    const bytes = new Uint8Array(await (await fetch("caps/sample-gtfs.zip")).arrayBuffer());
    return gtfs.loadFeed(bytes).feedHash;
  }

  async function seedV1(): Promise<void> {
    docSteps = [];
    board = [];
    docMessage = "";
    const record = await putDocument("watchlist", SEED_V1, 1);
    docSteps.push({
      label: "seed v1",
      detail: `${record.hash.slice(0, 19)}… (2 stops, one with a color)`,
      ok: true,
    });
    await queryBoard();
  }

  async function queryBoard(): Promise<void> {
    try {
      const current = currentDocument("watchlist");
      if (!current) {
        docMessage = "seed the watchlist first";
        return;
      }
      const gtfs = await gtfsModule();
      const feed = await feedHash();
      const document = JSON.parse(current.bytes) as Watchlist;
      const { watchlistHash } = gtfs.loadWatchlist(document);
      const stops = gtfs.watchlistBoard(watchlistHash, feed, DATE, DEPART, 3);
      board = stops.map((s) => ({ id: s.id, label: s.label, count: s.departures.length }));
      docSteps.push({
        label: `board @ v${current.version}`,
        detail: board.map((s) => `${s.id} "${s.label}" (${s.count})`).join(", "),
        ok: true,
      });
    } catch (error) {
      docMessage = error instanceof Error ? error.message : String(error);
    }
  }

  async function migrateForward(): Promise<void> {
    docSteps = [];
    const outcome = await migrateWatchlistForward("gtfs");
    if ("error" in outcome) {
      docMessage = outcome.error;
      return;
    }
    docSteps.push({
      label: `migrate → v${outcome.record.version}`,
      detail: `${outcome.record.hash.slice(0, 19)}… (from ${outcome.record.produced_by?.from.slice(0, 19)}…)`,
      ok: true,
    });
    for (const note of outcome.notes) {
      docSteps.push({
        label: "migration note",
        detail: `${note.kind}: ${note.field} ×${note.count} — no silent loss`,
        ok: false,
      });
    }
    await queryBoard();
  }

  async function rollback(): Promise<void> {
    docSteps = [];
    const record = await rollbackDocument("watchlist");
    if (!record) {
      docMessage = "nothing to roll back to";
      return;
    }
    docSteps.push({
      label: `rollback → v${record.version}`,
      detail: `${record.hash.slice(0, 19)}… still addressable, still queries`,
      ok: true,
    });
    await queryBoard();
  }

  async function run(): Promise<void> {
    running = true;
    steps = [];
    result = "";
    try {
      const bytes = new Uint8Array(await (await fetch("caps/sample-gtfs.zip")).arrayBuffer());

      const modules = await ensure(["gtfs", "router"]);
      const gtfs = modules.gtfs ? gtfsCap(modules.gtfs) : null;
      const router = modules.router ? routerCap(modules.router) : null;

      if (!gtfs) {
        steps.push({ label: "gtfs capability", detail: "unavailable", ok: false });
        return;
      }
      const loadedGtfs: GtfsLoadFeed = gtfs.loadFeed(bytes);
      steps.push({
        label: "gtfs.load_feed",
        detail: `${loadedGtfs.feedHash.slice(0, 19)}… (${loadedGtfs.summary.trips} trips)`,
        ok: true,
      });

      const departures = gtfs.departures(loadedGtfs.feedHash, "A", DATE, DEPART, 5);
      steps.push({
        label: "gtfs.departures",
        detail: `${departures.length} departures at A from ${fmtTime(DEPART)}`,
        ok: true,
      });

      if (!router) {
        steps.push({
          label: "router capability",
          detail: "unavailable — the assembly degraded to its gtfs half",
          ok: false,
        });
        result = `${departures.length} departures served; planning skipped (capability unavailable).`;
        return;
      }
      const loadedRouter = router.loadFeed(bytes);
      // The composition moment: two modules, one document, identity
      // asserted — never assumed.
      assertSameDocument(loadedGtfs, loadedRouter);
      steps.push({
        label: "hash handoff",
        detail: `${loadedGtfs.feedHash.slice(0, 19)}… == ${loadedRouter.feedHash.slice(0, 19)}…`,
        ok: true,
      });

      const plan = router.plan(loadedRouter.feedHash, "A", "B", DATE, DEPART, 4);
      steps.push({
        label: "router.plan",
        detail: plan
          ? `A → B: depart ${fmtTime(plan.depart_secs)}, arrive ${plan.arrival_hhmmss} (${plan.legs.length} legs)`
          : "unreachable",
        ok: true,
      });
      result = plan
        ? `Composition ok: ${departures.length} departures via gtfs, arrival ${plan.arrival_hhmmss} via router — one document, two capabilities.`
        : "Composition ok; the pair is unreachable on this date.";
    } catch (error) {
      steps.push({
        label: "failed",
        detail: error instanceof Error ? error.message : String(error),
        ok: false,
      });
    } finally {
      running = false;
    }
  }
</script>

<section class="caps-panel" data-testid="caps-panel">
  <h3>Capability assembly <span class="tag">PoC</span></h3>
  <p class="hint">
    Horizontal capabilities assembled from a manifest: hash-pinned modules, loaded on
    demand, composed through one document.
  </p>

  {#if caps.manifest}
    <ul class="cap-list">
      {#each caps.manifest.capabilities as cap (cap.id)}
        <li class="cap" data-testid={`cap-${cap.id}`}>
          <span class="cap-id">{cap.id}</span>
          <span class="cap-hash">{cap.hash.slice(0, 19)}…</span>
          <span class={`cap-status ${caps.statuses[cap.id]}`}>{caps.statuses[cap.id]}</span>
          <span class="cap-bytes">{(Number(cap.bytes) / 1024).toFixed(0)} KiB</span>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="hint">loading manifest…</p>
  {/if}

  <div class="actions">
    <button onclick={run} disabled={running} data-testid="caps-run">
      {running ? "running…" : "Run composition demo"}
    </button>
    <button onclick={simulateDegraded} disabled={running} data-testid="caps-degrade">
      Simulate missing router
    </button>
  </div>

  {#if steps.length > 0}
    <ul class="steps">
      {#each steps as step, i}
        <li class={step.ok ? "ok" : "bad"} data-testid={`caps-step-${i}`}>
          <span>{step.ok ? "✓" : "✗"}</span> {step.label}: {step.detail}
        </li>
      {/each}
    </ul>
  {/if}
  {#if result}
    <p class="result" data-testid="caps-result">{result}</p>
  {/if}

  <section class="doc-section">
    <h4>Versioned documents: migrate forward, roll back</h4>
    <p class="hint">
      Migrations are pure ops over document bytes; the store keeps every version
      (hash-chained by <code>produced_by</code>) — rollback is a pointer move.
    </p>

    {#if currentDocument("watchlist")}
      <p class="doc-state" data-testid="caps-doc-version">
        watchlist v{currentDocument("watchlist")?.version} ·
        {currentDocument("watchlist")?.hash.slice(0, 19)}…
      </p>
      <p class="doc-chain hint">
        history: {documentHistory("watchlist").map((r) => `v${r.version}`).join(" ← ")}
      </p>
    {/if}

    <div class="actions">
      <button onclick={seedV1} data-testid="caps-seed">Seed v1 watchlist</button>
      <button onclick={migrateForward} data-testid="caps-migrate">Migrate → v2</button>
      <button onclick={rollback} data-testid="caps-rollback">Roll back</button>
    </div>

    {#if docSteps.length > 0}
      <ul class="steps">
        {#each docSteps as step, i}
          <li class={step.ok ? "ok" : "note"} data-testid={`caps-doc-step-${i}`}>
            <span>{step.ok ? "✓" : "!"}</span> {step.label}: {step.detail}
          </li>
        {/each}
      </ul>
    {/if}
    {#if board.length > 0}
      <ul class="board" data-testid="caps-board">
        {#each board as stop}
          <li>{stop.id} "<span class="label">{stop.label}</span>" — {stop.count} departures</li>
        {/each}
      </ul>
    {/if}
    {#if docMessage}
      <p class="result" data-testid="caps-doc-message">{docMessage}</p>
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
