<script lang="ts">
  /// A form's photos and drawings (D-162): "Take photo" opens the
  /// phone's camera, "Add file" a photo or a PDF from the device; each is
  /// added to the deployment at once (the page's `onadd`) and named here
  /// by its hash, with a caption to type and a way to take it off the
  /// record. Shown only where the deployment keeps attachments.
  /// A RENDER component.
  import AttachmentThumbs from "./AttachmentThumbs.svelte";

  type Item = { hash: string; type: string; name?: string; caption?: string; taken_on?: string };
  let {
    items = $bindable(),
    onadd,
    src,
    testid,
  }: {
    items: Item[];
    onadd: (file: File) => Promise<Item | null>;
    src: (hash: string) => Promise<string | null>;
    testid: string;
  } = $props();

  let adding = $state(false);

  async function add(files: FileList | null) {
    if (!files?.length) return;
    adding = true;
    try {
      for (const file of Array.from(files)) {
        const a = await onadd(file);
        if (a && !items.some((x) => x.hash === a.hash)) items = [...items, a];
      }
    } finally {
      adding = false;
    }
  }
</script>

<fieldset class="attachments" data-testid={testid}>
  <legend>Photos and drawings</legend>
  <AttachmentThumbs {items} {src} />
  {#each items as a, i (a.hash)}
    <div class="row">
      <span class="name">{a.name || a.hash.slice(7, 15)}</span>
      <input type="text" placeholder="What it shows" aria-label="Caption for {a.name || 'photo'}" value={a.caption ?? ""}
        oninput={(e) => (items = items.map((x, j) => (j === i ? { ...x, caption: e.currentTarget.value || undefined } : x)))} />
      <button type="button" class="link" onclick={() => (items = items.filter((_, j) => j !== i))}>Remove</button>
    </div>
  {/each}
  <div class="add">
    <label class="button">Take photo
      <input type="file" accept="image/*" capture="environment" disabled={adding} onchange={(e) => add(e.currentTarget.files)} data-testid="{testid}-camera" />
    </label>
    <label class="button">Add file
      <input type="file" accept="image/*,application/pdf" multiple disabled={adding} onchange={(e) => add(e.currentTarget.files)} data-testid="{testid}-file" />
    </label>
    {#if adding}<span class="muted">Adding…</span>{/if}
  </div>
</fieldset>

<style>
  .attachments { border: 1px solid var(--line); border-radius: 6px; margin: 0; padding: 0.3rem 0.6rem 0.5rem; min-width: 0; display: flex; flex-direction: column; gap: 0.3rem; }
  legend { padding: 0 0.25rem; font-size: 0.85rem; color: var(--muted); }
  .row { display: grid; grid-template-columns: minmax(0, 10rem) minmax(0, 1fr) auto; gap: 0.5rem; align-items: center; font-size: 0.85rem; }
  .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--muted); }
  .add { display: flex; flex-wrap: wrap; gap: 0.4rem; align-items: center; }
  .button { position: relative; display: inline-flex; align-items: center; padding: 0.2rem 0.7rem; border: 1px solid var(--line); border-radius: 6px; cursor: pointer; font-size: 0.85rem; }
  .button input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
  .button:focus-within { outline: 2px solid var(--accent); }
  .link { background: none; border: none; padding: 0; color: var(--accent); cursor: pointer; text-decoration: underline; font: inherit; }
  @media (pointer: coarse) { .button { min-height: 2.75rem; padding: 0 1rem; } }
</style>
