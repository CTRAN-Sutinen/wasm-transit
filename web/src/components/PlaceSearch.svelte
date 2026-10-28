<script lang="ts">
  /// A stop type-ahead — a RENDER component over the directory's
  /// `stops` answer (every word typed in the name or the stop id). It
  /// emits what is typed, a beat after typing pauses, and renders the
  /// answer as it lands: arrow keys move, Enter picks, Escape closes.
  /// It matches nothing itself; the typed words are only marked bold.
  type Place = { id: string; name: string; lat?: number; lon?: number };

  let {
    label,
    placeholder,
    value = "",
    found,
    limit,
    off,
    icon = "search",
    ontype,
    onpick,
    onclose,
  }: {
    label: string;
    placeholder: string;
    /// The picked place's name (a trip end), shown until the rider types.
    value?: string;
    /// The directory's answer for this field; null when it asked
    /// nothing (or for another field).
    found: { text: string; places: Place[] } | null;
    /// The answer's cap: a full page means there may be more.
    limit: number;
    off: boolean;
    icon?: "search" | "from" | "to";
    ontype: (text: string) => void;
    onpick: (place: Place) => void;
    onclose: () => void;
  } = $props();

  let text = $state("");
  let editing = $state(false);
  let active = $state(0);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const listId = `ps-${crypto.randomUUID()}`;

  const open = $derived(editing && text.trim().length > 0 && found !== null);

  function input(e: Event) {
    text = (e.currentTarget as HTMLInputElement).value;
    editing = true;
    active = 0;
    clearTimeout(timer);
    timer = setTimeout(() => ontype(text), 120);
  }

  function pick(place: Place) {
    clearTimeout(timer);
    editing = false;
    text = "";
    onpick(place);
  }

  function close() {
    clearTimeout(timer);
    editing = false;
    text = "";
    onclose();
  }

  function key(e: KeyboardEvent) {
    const places = found?.places ?? [];
    if (e.key === "ArrowDown") {
      active = Math.min(active + 1, places.length - 1);
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      active = Math.max(active - 1, 0);
      e.preventDefault();
    } else if (e.key === "Enter" && places[active]) {
      pick(places[active]);
      e.preventDefault();
    } else if (e.key === "Escape") {
      close();
    }
  }

  /// The name with each typed word marked (case-insensitive).
  function marked(name: string, typedText: string): { s: string; hit: boolean }[] {
    const words = typedText.trim().split(/\s+/).filter(Boolean).map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
    if (words.length === 0) return [{ s: name, hit: false }];
    // A capturing split puts every match at an odd index.
    return name
      .split(new RegExp(`(${words.join("|")})`, "i"))
      .map((s, i) => ({ s, hit: i % 2 === 1 }))
      .filter((p) => p.s);
  }
</script>

<div class="ps" class:open data-testid="search-{icon}">
  <label class="field">
    <span class="icon {icon}" aria-hidden="true">
      {#if icon === "search"}
        <svg viewBox="0 0 24 24" width="18" height="18"><circle cx="11" cy="11" r="6.5" fill="none" stroke="currentColor" stroke-width="2" /><path d="M16 16l4.5 4.5" stroke="currentColor" stroke-width="2" stroke-linecap="round" /></svg>
      {/if}
    </span>
    <input
      type="search"
      aria-label={label}
      {placeholder}
      autocomplete="off"
      spellcheck="false"
      role="combobox"
      aria-expanded={open}
      aria-controls={listId}
      value={editing ? text : value}
      oninput={input}
      onkeydown={key}
      onfocus={(e) => e.currentTarget.select()}
      onblur={() => setTimeout(() => { if (editing && !text.trim()) close(); }, 150)}
      disabled={off}
    />
  </label>

  {#if off}
    <p class="off">Stop search is unavailable right now — tap a stop on the map instead.</p>
  {:else if open && found}
    <ul class="results" id={listId} role="listbox">
      {#each found.places as p, i (p.id)}
        <li role="option" aria-selected={i === active}>
          <button class:active={i === active} onmousedown={(e) => e.preventDefault()} onclick={() => pick(p)}>
            <span class="pin" aria-hidden="true"></span>
            <span class="name">{#each marked(p.name, found.text) as part, j (j)}{#if part.hit}<b>{part.s}</b>{:else}{part.s}{/if}{/each}</span>
            <span class="id">#{p.id}</span>
          </button>
        </li>
      {:else}
        <li class="none">
          No stop is named “{found.text}”. Stop names follow the sign (“Van Mall”, not “Vancouver Mall”) — try another word, the stop number, or tap the map.
        </li>
      {/each}
      {#if found.places.length === limit}
        <li class="more">Showing the first {limit} — keep typing to narrow it.</li>
      {/if}
    </ul>
  {/if}
</div>

<style>
  .ps { position: relative; }
  .field {
    display: flex; align-items: center; gap: 0.4rem;
    background: var(--hover); border: 1px solid var(--line); border-radius: 12px;
    padding: 0 0.7rem; transition: border-color 0.15s, box-shadow 0.15s;
  }
  .field:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-soft); background: var(--surface); }
  .icon { display: grid; place-items: center; color: var(--muted); width: 18px; flex: none; }
  .icon.from::before, .icon.to::before { content: ""; width: 10px; height: 10px; border-radius: 50%; border: 2.5px solid var(--accent); }
  .icon.to::before { background: var(--accent); }
  input {
    flex: 1; min-width: 0; border: 0; background: transparent; color: var(--text);
    font: inherit; font-size: 1rem; padding: 0.7rem 0; outline: none;
  }
  input::-webkit-search-cancel-button { cursor: pointer; }
  .results {
    list-style: none; margin: 0.35rem 0 0; padding: 0.25rem; position: absolute; left: 0; right: 0; z-index: 5;
    background: var(--surface); border: 1px solid var(--line); border-radius: 12px;
    box-shadow: 0 12px 32px rgb(0 0 0 / 0.16); max-height: min(22rem, 50dvh); overflow-y: auto;
  }
  .results button {
    width: 100%; display: flex; align-items: center; gap: 0.6rem; text-align: left;
    border: 0; background: none; color: var(--text); font: inherit; padding: 0.55rem 0.6rem; border-radius: 8px; cursor: pointer;
  }
  .results button.active, .results button:hover { background: var(--hover); }
  .pin { width: 8px; height: 8px; border-radius: 50%; border: 2px solid var(--muted); flex: none; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name b { color: var(--accent); font-weight: 650; }
  .id { color: var(--muted); font-size: 0.75rem; font-variant-numeric: tabular-nums; }
  .none, .more { padding: 0.6rem; color: var(--muted); font-size: 0.85rem; line-height: 1.45; }
  .more { font-size: 0.75rem; border-top: 1px solid var(--line); }
  .off { color: var(--bad); font-size: 0.85rem; margin: 0.4rem 0 0; }
</style>
