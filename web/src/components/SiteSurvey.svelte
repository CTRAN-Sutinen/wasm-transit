<script lang="ts" module>
  /// A survey half filled (answers as typed, "" not recorded). The host
  /// keeps it on the device until it is saved or thrown away (D-158).
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
  /// A stop's site survey (D-158), filled at the stop on a phone or a
  /// tablet: when, the site questions grouped as the
  /// Service Stop Worksheet groups them, what is at the stop, and the
  /// stop's ids in other systems (D-160). The questions and the answers
  /// each takes are cap-stops `questions`'; the words are stopWords'.
  /// It starts from what the stop already records (or the draft this
  /// device kept), reports every keystroke as the draft, and saves one
  /// `changed` event carrying every answer recorded — a survey says what
  /// was seen that day, the unchanged answers too. A RENDER component.
  import type { Event } from "../types/Event";
  import type { StopOn } from "../types/StopOn";
  import type { SiteQuestion } from "../types/cap_stops/SiteQuestion";
  import Attachments from "./Attachments.svelte";
  import { REFS, SITE_GROUPS, amenityWords, answerWords, refWords, siteWords } from "./stopWords";

  let {
    stop,
    questions,
    date,
    busy,
    draft,
    ondraft,
    onsave,
    oncancel,
    attachments = null,
  }: {
    stop: StopOn | null;
    questions: { site: SiteQuestion[]; amenities: string[] };
    date: string;
    busy: boolean;
    /// The draft this device kept for the stop, if any.
    draft: SurveyDraft | null;
    ondraft: (draft: SurveyDraft | null) => void;
    onsave: (event: Event, done: string) => void;
    oncancel: () => void;
    /// Adding and showing photos, where the deployment keeps them (D-162).
    attachments?: { onadd: (file: File) => Promise<{ hash: string; type: string; name?: string; caption?: string; taken_on?: string } | null>; src: (hash: string) => Promise<string | null> } | null;
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

  // The form's values: the device's draft, else what the stop records.
  // What the stop records is the baseline: a form left at it keeps no
  // draft, and every other state of it is the draft.
  let baseline = JSON.stringify(fromStop());
  // svelte-ignore state_referenced_locally
  let d = $state<SurveyDraft>(draft ? copy(draft) : fromStop());
  // svelte-ignore state_referenced_locally
  let restored = $state(!!draft);
  let extraRef = $state("");

  $effect(() => {
    const now = JSON.stringify(d);
    ondraft(now === baseline ? null : (JSON.parse(now) as SurveyDraft));
  });

  const byKey = $derived(new Map(questions.site.map((q) => [q.key, q])));
  const grouped = $derived.by(() => {
    const named = new Set(SITE_GROUPS.flatMap((g) => g.keys));
    const groups = SITE_GROUPS.map((g) => ({ title: g.title, qs: g.keys.map((k) => byKey.get(k)).filter((q): q is SiteQuestion => !!q) }));
    const other = questions.site.filter((q) => !named.has(q.key));
    return other.length ? [...groups, { title: "Other", qs: other }] : groups;
  });
  // the agency's own questions the stop already has
  const ownSite = $derived(Object.keys(d.site).filter((k) => !byKey.has(k)));

  function startOver() {
    d = fromStop();
    baseline = JSON.stringify(d);
    restored = false;
    ondraft(null);
  }

  function addRef() {
    const k = extraRef.trim().toLowerCase().replaceAll(/\s+/g, "_");
    if (k && !(k in d.refs)) d.refs[k] = "";
    extraRef = "";
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
    onsave(event, `Survey of ${stop?.stop_id ?? "the stop"} on ${d.on} saved.`);
  }
</script>

<form
  class="survey"
  data-testid="survey-form"
  onsubmit={(e) => {
    e.preventDefault();
    save();
  }}
>
  <h3>Site survey</h3>
  {#if restored}
    <p class="restored" data-testid="survey-restored">
      Unsaved draft restored.
      <button type="button" class="link" onclick={startOver}>Discard draft</button>
    </p>
  {/if}
  <div class="grid">
    <label>On <input type="date" bind:value={d.on} required data-testid="survey-on" /></label>
  </div>
  <p class="muted small">Recorded as surveyed by you, the person signed in.</p>

  {#each grouped as g (g.title)}
    <fieldset>
      <legend>{g.title}</legend>
      <div class="grid">
        {#each g.qs as q (q.key)}
          <label class:wide={q.key === "remarks"}>
            {siteWords(q.key)}
            {#if q.answer.kind === "one_of"}
              <select bind:value={d.site[q.key]} data-testid="survey-{q.key}">
                <option value="">not recorded</option>
                {#each q.answer.answers as a (a)}<option value={a}>{answerWords(a)}</option>{/each}
              </select>
            {:else if q.answer.kind === "number"}
              <input type="text" inputmode="numeric" pattern="[0-9]*" bind:value={d.site[q.key]} data-testid="survey-{q.key}" />
            {:else if q.key === "remarks"}
              <textarea rows="3" bind:value={d.site[q.key]} data-testid="survey-{q.key}"></textarea>
            {:else}
              <input type="text" bind:value={d.site[q.key]} data-testid="survey-{q.key}" />
            {/if}
          </label>
        {/each}
        {#each ownSite as k (k)}
          <label>{siteWords(k)} <input type="text" bind:value={d.site[k]} /></label>
        {/each}
      </div>
    </fieldset>
  {/each}

  <fieldset>
    <legend>At the stop</legend>
    <div class="grid">
      {#each Object.keys(d.amenities) as a (a)}
        <label>
          {amenityWords(a)}
          <select bind:value={d.amenities[a]} data-testid="survey-amenity-{a}">
            <option value="">not recorded</option><option value="yes">there</option><option value="no">not there</option>
          </select>
        </label>
      {/each}
    </div>
  </fieldset>

  <fieldset>
    <legend>Ids in other systems</legend>
    <div class="grid">
      {#each Object.keys(d.refs) as k (k)}
        <label>{refWords(k)} <input type="text" bind:value={d.refs[k]} data-testid="survey-ref-{k}" /></label>
      {/each}
    </div>
    <div class="add">
      <input type="text" bind:value={extraRef} placeholder="Another system" aria-label="Another system's name" />
      <button type="button" onclick={addRef} disabled={!extraRef.trim()}>Add</button>
    </div>
  </fieldset>

  {#if attachments}
    <Attachments bind:items={() => d.attachments ?? [], (v) => (d.attachments = v)} onadd={attachments.onadd} src={attachments.src} testid="survey-attachments" />
  {/if}

  <label class="note">Note <input type="text" bind:value={d.note} placeholder="Anything else, in a few words" data-testid="survey-note" /></label>
  <div class="row">
    <button type="submit" disabled={busy || !d.on} data-testid="survey-save">Save survey</button>
    <button type="button" onclick={() => (startOver(), oncancel())} data-testid="survey-discard">Discard</button>
    <button type="button" onclick={oncancel} data-testid="survey-keep">Save draft</button>
  </div>
</form>

<style>
  .survey { display: flex; flex-direction: column; gap: 0.5rem; margin-top: 0.5rem; padding: 0.6rem; border: 1px solid var(--line); border-radius: 6px; }
  h3 { margin: 0; font-size: 0.95rem; }
  fieldset { border: 1px solid var(--line); border-radius: 6px; margin: 0; padding: 0.3rem 0.6rem 0.5rem; min-width: 0; }
  legend { padding: 0 0.25rem; font-size: 0.85rem; color: var(--muted); }
  /* Each question a row — its words beside its answer — in as many
     columns as the width holds. */
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(17rem, 1fr)); gap: 0.3rem 1.2rem; }
  label { display: grid; grid-template-columns: 8.5rem minmax(0, 1fr); align-items: center; gap: 0.5rem; font-size: 0.85rem; min-width: 0; }
  label.wide { grid-column: 1 / -1; align-items: start; }
  label.note { font-size: 0.85rem; }
  input, select, textarea { font: inherit; font-size: 0.9rem; width: 100%; min-width: 0; }
  textarea { min-height: 3.5rem; resize: vertical; }
  .add { display: flex; gap: 0.4rem; margin-top: 0.4rem; max-width: 24rem; }
  .add input { flex: 1; }
  .row { display: flex; flex-wrap: wrap; gap: 0.4rem; }
  .muted.small { margin: 0; font-size: 0.8rem; }
  .restored { margin: 0; font-size: 0.85rem; color: var(--warn); }
  .link { background: none; border: none; padding: 0; color: var(--accent); cursor: pointer; text-decoration: underline; font: inherit; }
  /* A finger at the stop: larger targets, the text a phone will not zoom. */
  @media (pointer: coarse) {
    input, select, textarea { font-size: 1rem; min-height: 2.5rem; }
    .row button { min-height: 2.75rem; padding: 0 0.9rem; }
  }
</style>
