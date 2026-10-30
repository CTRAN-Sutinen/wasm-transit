<script lang="ts">
  /// The view rail: one button per view, numbered for its key (1–6).
  /// Keys are the shell's to bind; this only renders and emits.
  let {
    views,
    active,
    onselect,
  }: {
    views: { id: string; label: string }[];
    active: string;
    onselect: (id: string) => void;
  } = $props();
</script>

<nav class="rail" aria-label="Views" data-testid="design-rail">
  {#each views as v, i (v.id)}
    <button
      type="button"
      class:on={v.id === active}
      aria-current={v.id === active ? "page" : undefined}
      aria-keyshortcuts={String(i + 1)}
      onclick={() => onselect(v.id)}
    >
      <b>{i + 1}</b>{v.label}
    </button>
  {/each}
</nav>

<style>
  .rail {
    display: flex; flex-direction: column; gap: 2px; padding: 0.6rem 0;
    background: #1c1917; min-height: 100%;
  }
  button {
    display: flex; flex-direction: column; align-items: center; gap: 0.2rem;
    padding: 0.55rem 0.25rem; border: 0; border-left: 3px solid transparent;
    background: transparent; color: #d6d3d1; font: inherit; font-size: 0.72rem; cursor: pointer;
  }
  button:hover { color: #fff; background: #292524; }
  b {
    display: flex; width: 1.5rem; height: 1.5rem; border-radius: 6px; align-items: center; justify-content: center;
    border: 1px solid #57534e; font-size: 0.72rem; font-weight: 500; font-family: ui-monospace, monospace;
  }
  .on { color: #fff; border-left-color: #fb923c; background: #292524; }
  .on b { background: #fb923c; border-color: #fb923c; color: #1c1917; }
  @media (max-width: 720px) {
    .rail { flex-direction: row; min-height: 0; padding: 0; overflow-x: auto; }
    button { flex: 1; border-left: 0; border-bottom: 3px solid transparent; }
    .on { border-bottom-color: #fb923c; }
  }
</style>
