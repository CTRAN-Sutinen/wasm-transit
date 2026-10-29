<script lang="ts">
  /// A stop display's setup — a RENDER component: the three layouts to
  /// pick from and the stop type-ahead (PlaceSearch over the
  /// directory's `stops`). Picking a stop is an event.
  import PlaceSearch from "./PlaceSearch.svelte";

  type Layout = "center" | "route" | "routes";
  type Place = { id: string; name: string };

  let {
    layout,
    found,
    limit,
    off,
    current,
    ontype,
    onlayout,
    onpick,
    onclose,
  }: {
    layout: Layout;
    found: { text: string; places: Place[] } | null;
    limit: number;
    off: boolean;
    /// The stop on screen, when one is.
    current: Place | null;
    ontype: (text: string) => void;
    onlayout: (layout: Layout) => void;
    onpick: (place: Place) => void;
    onclose: () => void;
  } = $props();

  const LAYOUTS: { id: Layout; title: string; blurb: string }[] = [
    { id: "center", title: "Transit center", blurb: "Every bay's routes in one list, each time with its bay." },
    { id: "route", title: "Single route", blurb: "One route, large — the stop most stops are." },
    { id: "routes", title: "Several routes", blurb: "A street stop's two or three routes, each a band across the screen." },
  ];
</script>

<section class="setup" data-testid="display-setup">
  <h1>Set up this display</h1>
  <fieldset>
    <legend>Layout</legend>
    {#each LAYOUTS as l (l.id)}
      <button class:on={layout === l.id} aria-pressed={layout === l.id} onclick={() => onlayout(l.id)} data-testid="layout-{l.id}">
        <strong>{l.title}</strong>
        <span>{l.blurb}</span>
      </button>
    {/each}
  </fieldset>
  <div class="stop">
    <PlaceSearch
      label="Stop"
      placeholder="Search a stop by name or number"
      value={current?.name ?? ""}
      {found}
      {limit}
      {off}
      {ontype}
      {onpick}
      onclose={() => {}}
    />
  </div>
  {#if current}
    <button class="back" onclick={onclose}>Show {current.name}</button>
  {/if}
</section>

<style>
  .setup { max-width: 44rem; margin: 0 auto; padding: 2rem 1rem; display: grid; gap: 1.2rem; }
  h1 { margin: 0; font-size: 1.5rem; }
  fieldset { border: 0; padding: 0; margin: 0; display: grid; grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr)); gap: 0.6rem; }
  legend { font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); margin-bottom: 0.4rem; }
  fieldset button {
    display: grid; gap: 0.2rem; text-align: left; font: inherit; color: var(--text); cursor: pointer;
    background: var(--surface); border: 2px solid var(--line); border-radius: 10px; padding: 0.7rem 0.8rem;
  }
  fieldset button span { color: var(--muted); font-size: 0.85rem; }
  fieldset button.on { border-color: var(--accent); background: var(--accent-soft); }
  .stop { display: grid; }
  .back { justify-self: start; font: inherit; border: 0; border-radius: 8px; padding: 0.6rem 1rem; background: var(--accent); color: #fff; cursor: pointer; }
</style>
