<script lang="ts">
  import RouteStopSearch from "./RouteStopSearch.svelte";
  import { timeText, zonedParts, zonedUnix } from "./disruptionsFormat";
  import type { AlertEntity } from "../types/AlertEntity";
  import type { AlertFlag } from "../types/AlertFlag";
  import type { AlertRefusal } from "../types/AlertRefusal";
  import type { AlertSource } from "../types/AlertSource";
  import type { AlertText } from "../types/AlertText";
  import type { DirectoryRoute } from "../types/cap_directory/DirectoryRoute";
  import type { DirectoryStop } from "../types/cap_directory/DirectoryStop";
  import type { EntityRow } from "../types/EntityRow";
  import type { ServiceAlert } from "../types/ServiceAlert";
  import type { TemplateRow } from "../types/TemplateRow";
  import type { Unfilled } from "../types/Unfilled";

  let {
    form,
    templates,
    routes,
    stops,
    refusals,
    flags,
    zone,
    now,
    busy,
    error,
    query,
    found,
    oninput,
    onchange,
    ontemplate,
    onsave,
    oncancel,
    onstartsnow,
    onpreview,
  }: {
    form: {
      id: string;
      fresh: boolean;
      alert: ServiceAlert;
      template: string;
      unfilled: Unfilled[];
      startsNow: boolean;
    };
    templates: TemplateRow[];
    routes: DirectoryRoute[];
    stops: { stop_id: string; name: string }[];
    refusals: AlertRefusal[];
    flags: AlertFlag[];
    zone: string | undefined;
    now: number;
    busy: boolean;
    error: string;
    query: string;
    found: { routes: EntityRow[]; stops: DirectoryStop[] } | null;
    oninput: (text: string) => void;
    onchange: (alert: ServiceAlert) => void;
    ontemplate: (id: string) => void;
    onsave: (publish: boolean) => void;
    oncancel: () => void;
    onstartsnow: (on: boolean) => void;
    onpreview: () => void;
  } = $props();

  const a = $derived(form.alert);
  const routeName = (id: string) =>
    routes.find((r) => r.route_id === id)?.name ?? id;
  const stopName = (id: string) =>
    stops.find((s) => s.stop_id === id)?.name ?? id;

  const EFFECTS: [ServiceAlert["effect"], string][] = [
    ["no_service", "No service"],
    ["reduced_service", "Reduced service"],
    ["significant_delays", "Delays"],
    ["detour", "Detour"],
    ["stop_moved", "Stop moved"],
    ["modified_service", "Service change"],
    ["additional_service", "Added service"],
    ["accessibility_issue", "Accessibility"],
    ["no_effect", "Notice"],
    ["other", "Other"],
    ["unknown", "Not set"],
  ];
  const CAUSES: [ServiceAlert["cause"], string][] = [
    ["construction", "Construction"],
    ["maintenance", "Maintenance"],
    ["police_activity", "Police activity"],
    ["accident", "Accident"],
    ["weather", "Weather"],
    ["technical_problem", "Technical problem"],
    ["medical_emergency", "Medical emergency"],
    ["special_event", "Special event"],
    ["demonstration", "Demonstration"],
    ["holiday", "Holiday"],
    ["strike", "Strike"],
    ["other", "Other"],
    ["unknown", "Not set"],
  ];
  const SEVERITIES: [ServiceAlert["severity"], string][] = [
    ["info", "Info"],
    ["warning", "Warning"],
    ["severe", "Severe"],
    ["unknown", "Not set"],
  ];

  const set = (patch: Partial<ServiceAlert>) => onchange({ ...a, ...patch });

  function entityText(e: AlertEntity): string {
    const route = e.route ? `Route ${routeName(e.route)}` : "";
    if (e.trip)
      return `${route ? `${route} · ` : ""}${e.start_secs != null ? `${timeText(e.start_secs)} trip` : `Trip ${e.trip}`}${e.date ? `, ${e.date}` : ""}`;
    if (e.stop)
      return route ? `${route} at ${stopName(e.stop)}` : stopName(e.stop);
    if (e.direction) return `${route}, ${e.direction}`;
    if (route) return route;
    if (e.route_type != null) return `Route type ${e.route_type}`;
    return "Whole agency";
  }
  const addEntity = (e: AlertEntity) => set({ informed: [...a.informed, e] });
  const dropEntity = (i: number) =>
    set({ informed: a.informed.filter((_, j) => j !== i) });

  const period = $derived(a.communication?.[0] ?? { start: null, end: null });
  const startParts = $derived(
    period.start != null ? zonedParts(period.start, zone) : null,
  );
  const endParts = $derived(
    period.end != null ? zonedParts(period.end, zone) : null,
  );
  function setPeriod(start: number | null, end: number | null) {
    set({
      communication: [{ start, end }, ...(a.communication ?? []).slice(1)],
    });
  }

  const langs = $derived([
    ...new Set(
      [...a.header, ...(a.description ?? [])].map((t) => t.lang ?? ""),
    ),
  ]);
  let lang = $state("");
  let newLang = $state("");
  const textIn = (texts: AlertText[] | undefined, l: string) =>
    texts?.find((t) => (t.lang ?? "") === l)?.text ?? "";
  function setText(
    field: "header" | "description" | "url",
    l: string,
    text: string,
  ) {
    const texts = (a[field] ?? []).filter((t) => (t.lang ?? "") !== l);
    const next =
      text || field === "header"
        ? [...texts, l ? { lang: l, text } : { text }]
        : texts;
    set({
      [field]: next.sort((x, y) => (x.lang ?? "").localeCompare(y.lang ?? "")),
    } as Partial<ServiceAlert>);
  }
  function addLang() {
    const l = newLang.trim().toLowerCase();
    if (!l || langs.includes(l)) return;
    setText("header", l, "");
    lang = l;
    newLang = "";
  }

  function sourceText(s: AlertSource): string {
    if (s.kind === "trip")
      return `Trip ${s.trip} at ${timeText(s.start_secs)}, cancelled (CAD feed)`;
    if (s.kind === "edit") return `Record ${s.id} (Dispatch)`;
    return `Stop ${stopName(s.stop)} closed`;
  }

  function refusalText(r: AlertRefusal): string {
    switch (r.problem.kind) {
      case "no_id":
        return "No id";
      case "no_entity":
        return "Add where it applies";
      case "no_header":
        return "Add a header";
      case "end_before_start":
        return "Ends before it starts";
      case "impact_outside_communication":
        return "Service window is outside when riders are told";
      case "direction_without_route":
        return "A direction needs its route";
      case "instance_without_trip":
        return "A trip time needs its trip";
      case "bad_date":
        return `Not a date: ${r.problem.date}`;
      case "repeated_language":
        return `Two ${r.problem.field} texts in ${r.problem.lang ?? "the default language"}`;
    }
  }
  function flagText(f: AlertFlag): string {
    const n = f.notice;
    switch (n.kind) {
      case "unknown_route":
        return `Route ${n.route} is not in the schedule`;
      case "unknown_direction":
        return `Route ${n.route} has no direction ${n.direction}`;
      case "unknown_stop":
        return `Stop ${n.stop} is not in the schedule`;
      case "unknown_trip":
        return `Trip ${n.trip} is not in the schedule`;
      case "missing_translation":
        return `No ${n.lang} ${n.field}`;
    }
  }
</script>

<section class="editor" data-testid="alert-editor">
  <header class="top">
    <button type="button" class="back" onclick={oncancel}>‹ Alerts</button>
    <h2>{form.fresh ? "New alert" : "Edit alert"}</h2>
    <span class="sub">{a.status === "published" ? "Published" : "Draft"}</span>
  </header>

  <label class="field">
    <span>Template</span>
    <select
      value={form.template}
      onchange={(e) =>
        e.currentTarget.value && ontemplate(e.currentTarget.value)}
      data-testid="template"
    >
      <option value="">No template</option>
      {#each templates as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
    </select>
  </label>

  <div class="three">
    <label class="field">
      <span>Effect</span>
      <select
        value={a.effect}
        onchange={(e) =>
          set({ effect: e.currentTarget.value as ServiceAlert["effect"] })}
        data-testid="effect"
      >
        {#each EFFECTS as [v, l] (v)}<option value={v}>{l}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span>Cause</span>
      <select
        value={a.cause}
        onchange={(e) =>
          set({ cause: e.currentTarget.value as ServiceAlert["cause"] })}
      >
        {#each CAUSES as [v, l] (v)}<option value={v}>{l}</option>{/each}
      </select>
    </label>
    <label class="field">
      <span>Severity</span>
      <select
        value={a.severity}
        onchange={(e) =>
          set({ severity: e.currentTarget.value as ServiceAlert["severity"] })}
      >
        {#each SEVERITIES as [v, l] (v)}<option value={v}>{l}</option>{/each}
      </select>
    </label>
  </div>

  <fieldset>
    <legend>Applies to</legend>
    <ul class="scopes" data-testid="applies-to">
      {#each a.informed as e, i (i)}
        <li>
          <span>{entityText(e)}</span><button
            type="button"
            class="x"
            aria-label="Remove"
            onclick={() => dropEntity(i)}
          >
            ×
          </button>
        </li>
      {/each}
    </ul>
    <div class="add">
      <RouteStopSearch
        {query}
        {found}
        pick={null}
        {oninput}
        onpick={(p) =>
          p &&
          (addEntity(p.kind === "route" ? { route: p.id } : { stop: p.id }),
          oninput(""))}
      />
      <button type="button" onclick={() => addEntity({})}>Whole agency</button>
    </div>
  </fieldset>

  <fieldset>
    <legend>When</legend>
    <div class="when">
      <span class="lbl">Starts</span>
      <input
        type="date"
        aria-label="Start date"
        value={form.startsNow ? "" : (startParts?.date ?? "")}
        disabled={form.startsNow}
        onchange={(e) =>
          setPeriod(
            zonedUnix(e.currentTarget.value, startParts?.time ?? "00:00", zone),
            period.end,
          )}
      />
      <input
        type="time"
        aria-label="Start time"
        value={form.startsNow ? "" : (startParts?.time ?? "")}
        disabled={form.startsNow}
        onchange={(e) =>
          setPeriod(
            zonedUnix(
              startParts?.date ?? zonedParts(now, zone).date,
              e.currentTarget.value,
              zone,
            ),
            period.end,
          )}
      />
      <label class="check">
        <input
          type="checkbox"
          checked={form.startsNow}
          onchange={(e) => onstartsnow(e.currentTarget.checked)}
          data-testid="starts-now"
        /> Starts now
      </label>
      <span class="lbl">Ends</span>
      <input
        type="date"
        aria-label="End date"
        value={endParts?.date ?? ""}
        disabled={period.end == null}
        onchange={(e) =>
          setPeriod(
            period.start,
            zonedUnix(e.currentTarget.value, endParts?.time ?? "23:59", zone),
          )}
      />
      <input
        type="time"
        aria-label="End time"
        value={endParts?.time ?? ""}
        disabled={period.end == null}
        onchange={(e) =>
          setPeriod(
            period.start,
            zonedUnix(
              endParts?.date ?? zonedParts(now, zone).date,
              e.currentTarget.value,
              zone,
            ),
          )}
      />
      <label class="check">
        <input
          type="checkbox"
          checked={period.end == null}
          onchange={(e) =>
            setPeriod(
              period.start,
              e.currentTarget.checked
                ? null
                : (form.startsNow ? now : (period.start ?? now)) + 3600,
            )}
          data-testid="unknown-end"
        /> Unknown end
      </label>
    </div>
  </fieldset>

  <fieldset>
    <legend>Text</legend>
    <div class="langs" role="tablist">
      {#each langs as l (l)}
        <button
          type="button"
          role="tab"
          class:on={lang === l}
          aria-selected={lang === l}
          onclick={() => (lang = l)}
        >
          {l ? l : "English"}
        </button>
      {/each}
      <input
        class="lang"
        placeholder="Language"
        aria-label="Add a language"
        bind:value={newLang}
        onkeydown={(e) => e.key === "Enter" && addLang()}
      />
      <button type="button" onclick={addLang}>Add</button>
    </div>
    <label class="field">
      <span>Header</span>
      <input
        value={textIn(a.header, lang)}
        oninput={(e) => setText("header", lang, e.currentTarget.value)}
        data-testid="header"
      />
    </label>
    <label class="field">
      <span>Description</span>
      <textarea
        rows="4"
        value={textIn(a.description, lang)}
        oninput={(e) => setText("description", lang, e.currentTarget.value)}
      ></textarea>
    </label>
    {#if form.unfilled.some((u) => (u.lang ?? "") === lang)}
      <p class="warn">
        Unfilled: {form.unfilled
          .filter((u) => (u.lang ?? "") === lang)
          .map((u) => `{${u.blank}}`)
          .join(", ")}
      </p>
    {/if}
    <label class="field">
      <span>Link</span>
      <input
        type="url"
        value={textIn(a.url, lang)}
        oninput={(e) => setText("url", lang, e.currentTarget.value)}
      />
    </label>
  </fieldset>

  {#if a.explains?.length}
    <fieldset>
      <legend>Explains</legend>
      <ul class="scopes">
        {#each a.explains as s, i (i)}
          <li>
            <span>{sourceText(s)}</span><button
              type="button"
              class="x"
              aria-label="Remove"
              onclick={() =>
                set({ explains: a.explains?.filter((_, j) => j !== i) })}
            >
              ×
            </button>
          </li>
        {/each}
      </ul>
    </fieldset>
  {/if}

  {#if refusals.length || flags.length || error}
    <ul class="problems" data-testid="alert-problems">
      {#if error}<li class="bad">{error}</li>{/if}
      {#each refusals as r, i (i)}<li class="bad">{refusalText(r)}</li>{/each}
      {#each flags as f, i (i)}<li class="warn">{flagText(f)}</li>{/each}
    </ul>
  {/if}

  <div class="save">
    <button
      type="button"
      class="left"
      onclick={onpreview}
      data-testid="open-preview"
    >
      Preview
    </button>
    <button type="button" onclick={oncancel}>Cancel</button>
    <button
      type="button"
      disabled={busy}
      onclick={() => onsave(false)}
      data-testid="save-draft"
    >
      Save draft
    </button>
    <button
      type="button"
      class="primary"
      disabled={busy || refusals.length > 0}
      onclick={() => onsave(true)}
      data-testid="publish"
    >
      Publish
    </button>
  </div>
</section>

<style>
  .editor {
    display: grid;
    gap: 0.8rem;
  }
  .top {
    display: grid;
    gap: 0.2rem;
  }
  .back {
    justify-self: start;
    border: 0;
    background: none;
    color: var(--accent);
    cursor: pointer;
    font: inherit;
    padding: 0;
  }
  h2 {
    font-size: 1.05rem;
    margin: 0;
  }
  .sub {
    color: var(--muted);
    font-size: 0.85rem;
  }
  .field {
    display: grid;
    gap: 0.2rem;
    font-size: 0.85rem;
  }
  .field > span,
  legend,
  .lbl {
    color: var(--muted);
    font-size: 0.8rem;
  }
  .three {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.5rem;
  }
  select,
  input,
  textarea {
    font: inherit;
    padding: 0.4rem 0.5rem;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--surface);
    color: var(--text);
    min-width: 0;
  }
  fieldset {
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0.5rem 0.7rem 0.7rem;
    margin: 0;
    display: grid;
    gap: 0.5rem;
  }
  .scopes {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.25rem;
  }
  .scopes li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    padding: 0.3rem 0.5rem;
    border-radius: 6px;
    background: var(--hover);
    font-size: 0.9rem;
  }
  .x {
    border: 0;
    background: none;
    cursor: pointer;
    font-size: 1.1rem;
    color: var(--muted);
  }
  .add {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.5rem;
  }
  .when {
    display: grid;
    grid-template-columns: auto 1fr 1fr auto;
    gap: 0.4rem 0.5rem;
    align-items: center;
  }
  .check {
    display: flex;
    gap: 0.3rem;
    align-items: center;
    font-size: 0.85rem;
    white-space: nowrap;
  }
  .langs {
    display: flex;
    gap: 0.3rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .langs button.on {
    background: var(--accent-soft);
    font-weight: 600;
  }
  .lang {
    width: 6rem;
  }
  button {
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    border-radius: 6px;
    padding: 0.35rem 0.8rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.88rem;
  }
  button.primary {
    background: var(--accent);
    color: var(--on-accent);
    border-color: var(--accent);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .problems {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0.25rem;
    font-size: 0.88rem;
  }
  .bad {
    color: var(--bad);
  }
  .warn {
    color: var(--warn);
    margin: 0;
    font-size: 0.85rem;
  }
  .save .left {
    margin-right: auto;
  }
  .save {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    position: sticky;
    bottom: 0;
    background: var(--surface);
    padding: 0.5rem 0;
    border-top: 1px solid var(--line);
  }
</style>
