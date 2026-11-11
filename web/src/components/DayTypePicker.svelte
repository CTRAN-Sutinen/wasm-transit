<script lang="ts">
  /// The plan's day types (analysis `day_types`): one tab per type,
  /// named by its days of the week, its regular service first; the
  /// combinations that run on fewer of its days are a second choice
  /// under it, each named by its members and dates (D-55).
  import type { DayType } from "../types/DayType";
  import { weekdaysText } from "./designFormat";

  let { dayTypes, selected, onselect }: { dayTypes: DayType[]; selected: string; onselect: (group: string) => void } = $props();

  const current = $derived(dayTypes.find((t) => t.groups.some((g) => g.id === selected)) ?? null);
</script>

<div class="day-types" role="group" aria-label="Day type" data-testid="day-types">
  {#each dayTypes as t (t.groups[0].id)}
    <button type="button" class:on={t === current} aria-pressed={t === current} onclick={() => onselect(t.groups[0].id)}>
      {weekdaysText(t.weekdays)}
    </button>
  {/each}
  {#if current && current.groups.length > 1}
    <select aria-label="Which combination" value={selected} onchange={(e) => onselect(e.currentTarget.value)} data-testid="day-type-group">
      {#each current.groups as g, i (g.id)}
        <option value={g.id}>{i === 0 ? `Regular — ${g.dates.length} days` : `${g.id} — ${g.dates.length} ${g.dates.length === 1 ? "day" : "days"} (${weekdaysText(g.weekdays)})`}</option>
      {/each}
    </select>
  {/if}
</div>

<style>
  .day-types { display: flex; gap: 0.25rem; align-items: center; flex-wrap: wrap; }
  button { border: 1px solid var(--line); background: var(--surface); color: var(--text); border-radius: 6px; padding: 0.3rem 0.7rem; font: inherit; font-size: 0.82rem; cursor: pointer; }
  button.on { background: var(--accent); border-color: var(--accent); color: var(--surface); font-weight: 600; }
  select { font: inherit; font-size: 0.78rem; padding: 0.25rem; border: 1px solid var(--line); border-radius: 6px; background: var(--surface); color: var(--text); max-width: 18rem; }
</style>
