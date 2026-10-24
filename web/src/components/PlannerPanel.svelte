<script>
  import { app, getNetworkDoc } from "../state.svelte.js";
  import {
    deriveFromFeed, loadNetworkText, loadNetworkFile, materializeNetwork,
    validateNetwork, exportNetworkText, downloadNetwork,
  } from "../planner.svelte.js";
  import StopsEditor from "./StopsEditor.svelte";
  import RoutesEditor from "./RoutesEditor.svelte";
  import PatternEditor from "./PatternEditor.svelte";
  import CalendarEditor from "./CalendarEditor.svelte";
  import ServiceEditor from "./ServiceEditor.svelte";
  import BlockEditor from "./BlockEditor.svelte";

  // The document: transit-network `Network` (plain data owned by
  // state.svelte.js, inferred from the loaded feed or imported from a
  // file). Everything below is derived or a whole-doc replacement.
  const doc = $derived(getNetworkDoc());
  const issues = $derived(app.networkIssues);

  const counts = $derived.by(() => {
    if (!doc) return null;
    const n = (o) => Object.keys(o ?? {}).length;
    return {
      stops: n(doc.stops), routes: n(doc.routes), patterns: n(doc.patterns),
      calendars: n(doc.calendars), services: n(doc.services), blocks: n(doc.blocks),
    };
  });

  // One line per issue: the artifact returns snake_case variants with
  // their fields (see transit_network::NetworkError). Capped: a big
  // broken document can report hundreds.
  const issueLines = $derived(
    issues.slice(0, 50).map((issue) => {
      const [[variant, fields]] = Object.entries(issue);
      const detail = Object.entries(fields ?? {})
        .map(([k, v]) => `${k}: ${v}`)
        .join(", ");
      return { variant, detail };
    }),
  );

  let pasteText = $state("");
  let networkFile; // bind:this — no reactivity needed

  async function copyDocument() {
    await navigator.clipboard.writeText(exportNetworkText());
    app.status = "Document copied to the clipboard as JSON.";
  }
</script>

<section>
  <h2>Network document</h2>

  {#if !doc}
    <p class="muted">
      A planner works off an existing network: load a GTFS feed in the Rider
      or Dispatcher view, then derive the document from it here.
    </p>
  {/if}

  <p>
    <button class="primary" onclick={deriveFromFeed} disabled={!app.gtfs}>
      Derive from loaded feed
    </button>
  </p>

  {#if doc}
    {#if counts}
      <p class="chips">
        <span class="chip">{app.networkLabel}</span>
        <span class="chip svc">{counts.stops} stops</span>
        <span class="chip svc">{counts.routes} routes</span>
        <span class="chip svc">{counts.patterns} patterns</span>
        <span class="chip svc">{counts.services} services</span>
        <span class="chip svc">{counts.blocks} blocks</span>
      </p>
    {/if}

    <h3>Validation</h3>
    {#if issues.length === 0}
      <p><span class="chip ok">valid</span> <button class="primary" onclick={materializeNetwork}>Load into app</button></p>
    {:else}
      <p><span class="chip bad">{issues.length} issue{issues.length > 1 ? "s" : ""}</span>
         <button onclick={validateNetwork}>Revalidate</button></p>
      <ul class="issues">
        {#each issueLines as issue (issue.variant + issue.detail)}
          <li><code>{issue.variant}</code> {issue.detail}</li>
        {/each}
      </ul>
    {/if}
    <p class="muted">Materializing registers a derived feed — the Rider and Dispatcher views, the map, planning, all of it runs on the authored network.</p>
  {/if}

  <details class="io-drawer">
    <summary>Import / export</summary>
    <p>
      <button onclick={() => networkFile.click()}>Open .json…</button>
      <button onclick={downloadNetwork} disabled={!doc}>Download .json</button>
      <button onclick={copyDocument} disabled={!doc}>Copy JSON</button>
      <input type="file" accept=".json,application/json" hidden bind:this={networkFile}
             onchange={(e) => { if (e.currentTarget.files[0]) loadNetworkFile(e.currentTarget.files[0]); e.currentTarget.value = ""; }}>
    </p>
    <textarea rows="4" cols="60" placeholder="…or paste a network document (JSON)"
              aria-label="Network JSON" bind:value={pasteText}></textarea>
    <p><button onclick={() => { if (loadNetworkText(pasteText, "pasted")) pasteText = ""; }}>Use pasted JSON</button></p>
  </details>
</section>

{#if doc}
  <StopsEditor />
  <RoutesEditor />
  <PatternEditor />
  <CalendarEditor />
  <ServiceEditor />
  <BlockEditor />
{/if}
