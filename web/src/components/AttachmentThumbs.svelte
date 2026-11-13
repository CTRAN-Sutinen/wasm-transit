<script lang="ts">
  /// A record's photos and drawings as small pictures (D-162), each
  /// opening full size; a PDF as its name. The bytes come from the page
  /// (`src`: read with the signed-in person's token) — without sign-in, a
  /// picture says so. A RENDER component.
  let {
    items,
    src,
  }: {
    items: { hash: string; type: string; name?: string; caption?: string }[];
    src: (hash: string) => Promise<string | null>;
  } = $props();
</script>

{#if items.length}
  <ul class="thumbs" data-testid="attachments">
    {#each items as a (a.hash)}
      <li>
        {#await src(a.hash) then url}
          {#if !url}
            <span class="none" title={a.name}>{a.caption || a.name || "photo"} (sign in to see)</span>
          {:else if a.type.startsWith("image/")}
            <a href={url} target="_blank" rel="noopener"><img src={url} alt={a.caption || a.name || "photo"} data-testid="attachment-img" /></a>
          {:else}
            <a href={url} target="_blank" rel="noopener" download={a.name}>{a.name || "document"}</a>
          {/if}
          {#if a.caption}<span class="caption">{a.caption}</span>{/if}
        {/await}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .thumbs { list-style: none; padding: 0; margin: 0.2rem 0; display: flex; flex-wrap: wrap; gap: 0.4rem; }
  li { display: flex; flex-direction: column; gap: 0.1rem; max-width: 7rem; font-size: 0.75rem; }
  img { width: 7rem; height: 5rem; object-fit: cover; border-radius: 4px; border: 1px solid var(--line); display: block; }
  .caption, .none { color: var(--muted); overflow: hidden; text-overflow: ellipsis; }
</style>
