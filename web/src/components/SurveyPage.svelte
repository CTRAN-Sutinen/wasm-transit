<script lang="ts" module>
  /// A survey half filled (answers as typed, "" not recorded). The host
  /// keeps it on the device until it is saved or discarded (D-158).
  export type SurveyDraft = {
    on: string;
    site: Record<string, string>;
    amenities: Record<string, string>;
    refs: Record<string, string>;
    note: string;
    /// Photos added while surveying (their hashes; the files are kept
    /// by the deployment as each is added).
    attachments?: { hash: string; type: string; name?: string; caption?: string; taken_on?: string }[];
  };
</script>

<script lang="ts">
  /// A stop's site survey (D-158) as the worksheet it replaces: one page,
  /// every section under its own heading — the Service Stop Worksheet's
  /// sections, then amenities, external IDs, and photos and notes — with
  /// a section index beside it (each section's answered count and its
  /// changes since the record; a click goes there) and a summary column
  /// with the survey date, what changed, and Save. Every answer starts
  /// from what the stop records, shown beside it, so a resurvey is a
  /// pass of confirmations. On a phone the index is a bar across the
  /// top and the summary follows the form. The questions and the answers
  /// each takes are cap-stops `questions`'; the words are stopWords'.
  /// Every change is reported as the draft (the host keeps it on the
  /// device); the survey saves as one `changed` event carrying every
  /// answer recorded. A RENDER component.
  import "./forms.css";
  import type { Event } from "../types/Event";
  import type { Refusal } from "../types/Refusal";
  import type { StopOn } from "../types/StopOn";
  import type { SiteQuestion } from "../types/cap_stops/SiteQuestion";
  import Attachments from "./Attachments.svelte";
  import { REFS, SITE_GROUPS, amenityWords, answerWords, problemWords, refWords, siteWords } from "./stopWords";

  type Item = { hash: string; type: string; name?: string; caption?: string; taken_on?: string };
  let {
    stopId,
    stop,
    questions,
    date,
    busy,
    draft,
    ondraft,
    onsave,
    onclose,
    refusals,
    error,
    attachments = null,
  }: {
    stopId: string;
    stop: StopOn | null;
    questions: { site: SiteQuestion[]; amenities: string[] };
    date: string;
    busy: boolean;
    draft: SurveyDraft | null;
    ondraft: (draft: SurveyDraft | null) => void;
    onsave: (event: Event, done: string) => void;
    /// Left unsaved: the draft stays on the device.
    onclose: () => void;
    refusals: Refusal[];
    error: string;
    attachments?: { onadd: (file: File) => Promise<Item | null>; src: (hash: string) => Promise<string | null> } | null;
  } = $props();

  const yn = (v: boolean | undefined) => (v == null ? "" : v ? "yes" : "no");

  function fromStop(): SurveyDraft {
    const refs: Record<string, string> = Object.fromEntries(REFS.map(([k]) => [k, ""]));
    Object.assign(refs, stop?.refs ?? {});
    return {
      on: date,
      site: { ...Object.fromEntries(questions.site.map((q) => [q.key, ""])), ...(stop?.site ?? {}) },
      amenities: Object.fromEntries(questions.amenities.map((a) => [a, yn(stop?.amenities?.[a])])),
      refs,
      note: "",
      attachments: [],
    };
  }

  const copy = (x: SurveyDraft): SurveyDraft => JSON.parse(JSON.stringify(x)) as SurveyDraft;
  // svelte-ignore state_referenced_locally
  const recorded = fromStop();
  let baseline = $state(JSON.stringify(recorded));
  // svelte-ignore state_referenced_locally
  let d = $state<SurveyDraft>(draft ? { ...copy(recorded), ...copy(draft) } : copy(recorded));
  // svelte-ignore state_referenced_locally
  let restored = $state(!!draft);
  let extraRef = $state("");

  $effect(() => {
    const now = JSON.stringify(d);
    ondraft(now === baseline ? null : (JSON.parse(now) as SurveyDraft));
  });

  const byKey = $derived(new Map(questions.site.map((q) => [q.key, q])));
  const ownSite = $derived(Object.keys(recorded.site).filter((k) => !byKey.has(k)));
  const siteSections = $derived.by(() => {
    const named = new Set(SITE_GROUPS.flatMap((g) => g.keys));
    const groups = SITE_GROUPS.map((g) => ({ title: g.title, keys: g.keys.filter((k) => byKey.has(k)) })).filter((g) => g.keys.length);
    const other = [...questions.site.filter((q) => !named.has(q.key)).map((q) => q.key), ...ownSite];
    return other.length ? [...groups, { title: "Other", keys: other }] : groups;
  });

  const words = (key: string, v: string) => (v ? (byKey.get(key)?.answer.kind === "one_of" ? answerWords(v) : v) : "");
  const amenityAnswer = (v: string) => (v === "yes" ? "There" : v === "no" ? "Not there" : "");
  const slug = (t: string) => `s-${t.toLowerCase().replaceAll(/[^a-z]+/g, "-")}`;

  /// Each section of the index: answered of asked, and changed.
  const index = $derived.by(() => {
    const site = siteSections.map((g) => ({
      title: g.title,
      answered: g.keys.filter((k) => (d.site[k] ?? "").trim()).length,
      total: g.keys.length,
      changed: g.keys.filter((k) => (d.site[k] ?? "").trim() !== (recorded.site[k] ?? "").trim()).length,
    }));
    const am = Object.keys(d.amenities);
    const rf = Object.keys(d.refs);
    return [
      ...site,
      { title: "Amenities", answered: am.filter((k) => d.amenities[k]).length, total: am.length, changed: am.filter((k) => d.amenities[k] !== (recorded.amenities[k] ?? "")).length },
      { title: "External IDs", answered: rf.filter((k) => d.refs[k]?.trim()).length, total: rf.length, changed: rf.filter((k) => (d.refs[k] ?? "").trim() !== (recorded.refs[k] ?? "").trim()).length },
      { title: "Photos & notes", answered: (d.attachments?.length ?? 0) + (d.note.trim() ? 1 : 0), total: 0, changed: 0 },
    ];
  });

  const changes = $derived.by(() => {
    const out: { what: string; was: string; now: string }[] = [];
    for (const k of Object.keys(d.site)) {
      const was = (recorded.site[k] ?? "").trim();
      const now = (d.site[k] ?? "").trim();
      if (was !== now) out.push({ what: siteWords(k), was: words(k, was), now: words(k, now) });
    }
    for (const k of Object.keys(d.amenities))
      if ((recorded.amenities[k] ?? "") !== d.amenities[k]) out.push({ what: amenityWords(k), was: amenityAnswer(recorded.amenities[k] ?? ""), now: amenityAnswer(d.amenities[k]) });
    for (const k of Object.keys(d.refs)) {
      const was = (recorded.refs[k] ?? "").trim();
      const now = (d.refs[k] ?? "").trim();
      if (was !== now) out.push({ what: refWords(k), was, now });
    }
    return out;
  });

  let active = $state("");
  let form: HTMLDivElement;
  $effect(() => {
    if (!form) return;
    const seen = new IntersectionObserver(
      (entries) => {
        const top = entries.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
        if (top) active = top.target.id;
      },
      { root: form, rootMargin: "0px 0px -70% 0px" },
    );
    for (const el of form.querySelectorAll("section[id]")) seen.observe(el);
    return () => seen.disconnect();
  });
  const go = (title: string) => form?.querySelector(`#${slug(title)}`)?.scrollIntoView({ behavior: "smooth", block: "start" });

  function discard() {
    d = copy(recorded);
    baseline = JSON.stringify(d);
    restored = false;
    ondraft(null);
  }

  function addRef() {
    const k = extraRef.trim().toLowerCase().replaceAll(/\s+/g, "_");
    if (k && !(k in d.refs)) d.refs[k] = "";
    extraRef = "";
  }

  /// Every answer in a section put back to what the stop records.
  function revert(title: string) {
    const g = siteSections.find((x) => x.title === title);
    if (g) for (const k of g.keys) d.site[k] = recorded.site[k] ?? "";
    if (title === "Amenities") d.amenities = { ...recorded.amenities };
    if (title === "External IDs") d.refs = { ...recorded.refs };
  }

  function bump(key: string, by: number) {
    const n = Number(d.site[key] || 0);
    d.site[key] = String(Math.max(0, (Number.isFinite(n) ? Math.trunc(n) : 0) + by));
  }

  function save() {
    const pick = (m: Record<string, string>) => Object.fromEntries(Object.entries(m).map(([k, v]) => [k, v.trim()]).filter(([, v]) => v));
    const site = pick(d.site);
    const refs = pick(d.refs);
    const amenities = Object.fromEntries(Object.entries(d.amenities).filter(([, v]) => v).map(([k, v]) => [k, v === "yes"]));
    const event: Event = {
      on: d.on,
      event: "changed",
      // the surveyor is the signed-in writer: the server names them
      // (cap-stops attribute_surveys), whatever this says
      surveyed_by: "",
      ...(Object.keys(site).length ? { site } : {}),
      ...(Object.keys(refs).length ? { refs } : {}),
      ...(Object.keys(amenities).length ? { amenities } : {}),
      ...(d.note.trim() ? { note: d.note.trim() } : {}),
      ...(d.attachments?.length ? { attachments: d.attachments } : {}),
    };
    onsave(event, `Survey of ${stop?.name || stopId} on ${d.on} saved.`);
  }

  const problems = $derived([...refusals.map((r) => `Not saved: ${problemWords(r.problem.kind)}.`), ...(error ? [`${error} Draft kept on this device.`] : [])]);
</script>

{#snippet was(before: string, now: string)}
  {#if before && before !== now}<span class="was" data-testid="was">was {before}</span>{:else if !before && now}<span class="was new">new</span>{/if}
{/snippet}

{#snippet question(key: string)}
  {@const q = byKey.get(key)}
  {@const before = words(key, (recorded.site[key] ?? "").trim())}
  <div class="q" class:changed={(d.site[key] ?? "").trim() !== (recorded.site[key] ?? "").trim()} role="group" aria-labelledby="q-{key}" data-testid="survey-q-{key}">
    <div class="q-head"><span class="f-label" id="q-{key}">{siteWords(key)}</span>{@render was(before, words(key, (d.site[key] ?? "").trim()))}</div>
    {#if q?.answer.kind === "one_of"}
      <div class="f-chips">
        {#each q.answer.answers as a (a)}
          <button type="button" aria-pressed={d.site[key] === a} onclick={() => (d.site[key] = d.site[key] === a ? "" : a)} data-testid="survey-{key}-{a}">{answerWords(a)}</button>
        {/each}
      </div>
    {:else if q?.answer.kind === "number"}
      <div class="number">
        <button type="button" aria-label="One fewer" onclick={() => bump(key, -1)}>−</button>
        <input type="text" inputmode="numeric" bind:value={d.site[key]} aria-labelledby="q-{key}" data-testid="survey-{key}" />
        <button type="button" aria-label="One more" onclick={() => bump(key, 1)}>+</button>
      </div>
    {:else if key === "remarks" || key === "landmark"}
      <textarea rows="2" bind:value={d.site[key]} aria-labelledby="q-{key}" data-testid="survey-{key}"></textarea>
    {:else}
      <input type="text" bind:value={d.site[key]} aria-labelledby="q-{key}" data-testid="survey-{key}" />
    {/if}
  </div>
{/snippet}

{#snippet sectionHead(title: string)}
  {@const ix = index.find((x) => x.title === title)}
  <header class="sec-head">
    <h2>{title}</h2>
    {#if ix && ix.total}<span class="count">{ix.answered}/{ix.total}</span>{/if}
    {#if ix?.changed}<button type="button" class="f-link revert" onclick={() => revert(title)} data-testid="survey-revert-{slug(title)}">Undo {ix.changed} {ix.changed === 1 ? "change" : "changes"}</button>{/if}
  </header>
{/snippet}

<div class="survey-page" data-testid="survey">
  <header class="top">
    <div class="titles">
      <h1>Site survey</h1>
      <p class="muted">{stop?.name || stopId} · Stop {stopId}{stop?.surveyed_on ? ` · last surveyed ${stop.surveyed_on}${stop.surveyed_by ? ` by ${stop.surveyed_by}` : ""}` : " · never surveyed"}</p>
    </div>
    <button type="button" onclick={onclose} data-testid="survey-close">{JSON.stringify(d) === baseline ? "Close" : "Close, keep draft"}</button>
  </header>

  <nav class="index" aria-label="Sections">
    {#each index as ix (ix.title)}
      <button type="button" class:on={active === slug(ix.title)} onclick={() => go(ix.title)} data-testid="survey-nav-{slug(ix.title)}">
        <span class="name">{ix.title}</span>
        <span class="meta">{ix.total ? `${ix.answered}/${ix.total}` : ix.answered ? ix.answered : ""}{#if ix.changed}<i class="dot" title="{ix.changed} changed"></i>{/if}</span>
      </button>
    {/each}
  </nav>

  <div class="form" bind:this={form}>
    {#if restored}
      <p class="restored" data-testid="survey-restored">Draft restored. <button type="button" class="f-link" onclick={discard} data-testid="survey-discard">Discard draft</button></p>
    {/if}
    {#each siteSections as g (g.title)}
      <section id={slug(g.title)}>
        {@render sectionHead(g.title)}
        <div class="qs">{#each g.keys as key (key)}{@render question(key)}{/each}</div>
      </section>
    {/each}
    <section id={slug("Amenities")}>
      {@render sectionHead("Amenities")}
      <div class="qs">
        {#each Object.keys(d.amenities) as a (a)}
          <div class="q" class:changed={d.amenities[a] !== (recorded.amenities[a] ?? "")} role="group" aria-labelledby="a-{a}" data-testid="survey-amenity-{a}">
            <div class="q-head"><span class="f-label" id="a-{a}">{amenityWords(a)}</span>{@render was(amenityAnswer(recorded.amenities[a] ?? ""), amenityAnswer(d.amenities[a]))}</div>
            <div class="f-chips">
              <button type="button" aria-pressed={d.amenities[a] === "yes"} onclick={() => (d.amenities[a] = d.amenities[a] === "yes" ? "" : "yes")} data-testid="survey-amenity-{a}-yes">There</button>
              <button type="button" aria-pressed={d.amenities[a] === "no"} onclick={() => (d.amenities[a] = d.amenities[a] === "no" ? "" : "no")} data-testid="survey-amenity-{a}-no">Not there</button>
            </div>
          </div>
        {/each}
      </div>
    </section>
    <section id={slug("External IDs")}>
      {@render sectionHead("External IDs")}
      <div class="qs">
        {#each Object.keys(d.refs) as k (k)}
          <label class="q" class:changed={(d.refs[k] ?? "").trim() !== (recorded.refs[k] ?? "").trim()}>
            <div class="q-head"><span class="f-label">{refWords(k)}</span>{@render was((recorded.refs[k] ?? "").trim(), (d.refs[k] ?? "").trim())}</div>
            <input type="text" bind:value={d.refs[k]} data-testid="survey-ref-{k}" />
          </label>
        {/each}
        <div class="q">
          <span class="f-label">Other system</span>
          <div class="inline">
            <input type="text" bind:value={extraRef} placeholder="System name" aria-label="Other system name" />
            <button type="button" onclick={addRef} disabled={!extraRef.trim()}>Add</button>
          </div>
        </div>
      </div>
    </section>
    <section id={slug("Photos & notes")}>
      {@render sectionHead("Photos & notes")}
      {#if attachments}
        <Attachments bind:items={() => d.attachments ?? [], (v) => (d.attachments = v)} onadd={attachments.onadd} src={attachments.src} testid="survey-attachments" />
      {/if}
      <label class="q wide">
        <span class="f-label">Notes</span>
        <textarea rows="3" bind:value={d.note} data-testid="survey-note"></textarea>
      </label>
    </section>
  </div>

  <aside class="summary" data-testid="survey-review">
    <label class="f-field">
      <span class="f-label">Survey date</span>
      <input type="date" bind:value={d.on} required data-testid="survey-on" />
      <span class="f-hint">Surveyor: the signed-in user.</span>
    </label>
    <h2>{changes.length ? `${changes.length} ${changes.length === 1 ? "change" : "changes"}` : "No changes"}</h2>
    {#if changes.length}
      <ul class="changes" data-testid="survey-changes">
        {#each changes as c (c.what)}<li><span class="what">{c.what}</span> <span class="muted">{c.was || "—"} →</span> <strong>{c.now || "—"}</strong></li>{/each}
      </ul>
    {:else}
      <p class="muted">Every answer matches the record.</p>
    {/if}
    {#if (d.attachments?.length ?? 0) > 0}<p class="muted">{d.attachments?.length} {d.attachments?.length === 1 ? "photo" : "photos"}</p>{/if}
    {#if problems.length}<ul class="problems" role="alert" data-testid="survey-problems">{#each problems as p, i (i)}<li>{p}</li>{/each}</ul>{/if}
    <div class="save">
      <button type="button" class="primary" disabled={busy || !d.on} onclick={save} data-testid="survey-save">Save survey</button>
      {#if restored || JSON.stringify(d) !== baseline}<button type="button" class="f-link" onclick={discard} data-testid="survey-discard-all">Discard draft</button>{/if}
    </div>
  </aside>
</div>

<style>
  .survey-page {
    display: grid; height: 100%; min-height: 0; background: var(--surface);
    grid-template-columns: 15rem minmax(0, 1fr) 20rem; grid-template-rows: auto minmax(0, 1fr);
    grid-template-areas: "top top top" "index form summary";
  }
  .top { grid-area: top; display: flex; align-items: center; gap: 1rem; padding: 0.8rem 1.4rem; border-bottom: 1px solid var(--line); }
  .titles { flex: 1; min-width: 0; }
  h1 { margin: 0; font-size: 1.2rem; }
  .top p { margin: 0.1rem 0 0; }
  .index { grid-area: index; overflow: auto; padding: 0.8rem 0.6rem; border-right: 1px solid var(--line); display: flex; flex-direction: column; gap: 0.1rem; }
  .index button {
    display: flex; justify-content: space-between; align-items: center; gap: 0.5rem; width: 100%; text-align: left;
    padding: 0.45rem 0.7rem; border: 0; border-radius: 6px; background: none; color: var(--text); font: inherit; font-size: 0.92rem; cursor: pointer;
  }
  .index button:hover { background: var(--hover); }
  .index button.on { background: var(--accent-soft); font-weight: 600; }
  .meta { color: var(--muted); font-size: 0.8rem; font-variant-numeric: tabular-nums; display: inline-flex; align-items: center; gap: 0.3rem; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--warn); display: inline-block; }
  .form { grid-area: form; overflow: auto; padding: 1rem 1.6rem 40vh; scroll-padding-top: 0.5rem; }
  section { margin: 0 0 1.8rem; }
  .sec-head { display: flex; align-items: baseline; gap: 0.7rem; border-bottom: 1px solid var(--line); padding-bottom: 0.35rem; margin-bottom: 0.9rem; }
  .sec-head h2 { margin: 0; font-size: 1.05rem; }
  .count { color: var(--muted); font-size: 0.85rem; font-variant-numeric: tabular-nums; }
  .revert { margin-left: auto; font-size: 0.85rem; }
  .qs { display: grid; grid-template-columns: repeat(auto-fill, minmax(19rem, 1fr)); gap: 1rem 1.6rem; }
  .q { display: flex; flex-direction: column; gap: 0.35rem; min-width: 0; padding-left: 0.6rem; border-left: 3px solid transparent; }
  .q.changed { border-left-color: var(--warn); }
  .q.wide { grid-column: 1 / -1; margin-top: 0.8rem; }
  .q-head { display: flex; align-items: baseline; gap: 0.5rem; }
  .was { font-size: 0.78rem; color: var(--warn); }
  .q input:not([type="checkbox"]), .q textarea {
    font: inherit; font-size: 0.95rem; padding: 0.45rem 0.55rem; border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--text); width: 100%;
  }
  .number { display: flex; gap: 0.35rem; align-items: center; }
  .number input { max-width: 5rem; text-align: center; }
  .number button { width: 2.4rem; min-height: 2.4rem; font-size: 1.1rem; }
  .inline { display: flex; gap: 0.4rem; }
  .restored { margin: 0 0 1rem; padding: 0.5rem 0.7rem; border-radius: 6px; background: color-mix(in srgb, var(--warn) 15%, transparent); }
  .summary { grid-area: summary; overflow: auto; padding: 1rem 1.2rem; border-left: 1px solid var(--line); display: flex; flex-direction: column; }
  .summary h2 { font-size: 1rem; margin: 0.4rem 0 0.5rem; }
  .changes { list-style: none; margin: 0 0 0.8rem; padding: 0; display: flex; flex-direction: column; gap: 0.35rem; font-size: 0.88rem; }
  .what { color: var(--muted); }
  .problems { color: var(--bad); padding-left: 1.1rem; margin: 0.4rem 0; font-size: 0.9rem; }
  .save { margin-top: auto; display: flex; flex-direction: column; gap: 0.5rem; padding-top: 1rem; }
  .primary { min-height: 2.8rem; background: var(--accent); color: var(--surface); border: 1px solid var(--accent); border-radius: 6px; font-weight: 600; font-size: 1rem; cursor: pointer; }
  .primary:disabled { opacity: 0.5; }
  /* A tablet: the summary goes under the form. A phone: one column, the
     index a bar of sections across the top, Save held at the bottom. */
  @media (max-width: 1100px) {
    .survey-page { grid-template-columns: 13rem minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr) auto; grid-template-areas: "top top" "index form" "summary summary"; }
    .summary { border-left: 0; border-top: 1px solid var(--line); max-height: 40dvh; }
  }
  @media (max-width: 700px) {
    .survey-page { grid-template-columns: 1fr; grid-template-rows: auto auto minmax(0, 1fr) auto; grid-template-areas: "top" "index" "form" "summary"; }
    .top { padding: 0.6rem 0.9rem; }
    .index { flex-direction: row; overflow-x: auto; border-right: 0; border-bottom: 1px solid var(--line); padding: 0.4rem; scrollbar-width: none; }
    .index button { flex: none; width: auto; white-space: nowrap; }
    .form { padding: 0.8rem 0.9rem 30vh; }
    .qs { grid-template-columns: 1fr; }
    .summary { max-height: none; padding: 0.6rem 0.9rem; }
    .summary h2, .summary .changes, .summary > p, .summary .f-hint { display: none; }
    .summary > .f-field { flex-direction: row; align-items: center; margin: 0 0 0.4rem; }
    .save { flex-direction: row; padding-top: 0; }
    .save .primary { flex: 1; }
  }
  @media (pointer: coarse) { .number button { width: 2.9rem; min-height: 2.9rem; } }
</style>
