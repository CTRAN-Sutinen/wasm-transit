<script lang="ts">
  /// The stops the filters show on a date (cap-stops `stops_on`) as a
  /// table: each stop's name, id and code, status, since and until, the
  /// note, and when it was last surveyed; a column heading sorts by it, a
  /// row opens the stop. A RENDER component.
  import type { StopOn } from "../types/StopOn";
  import { STATUS_COLORS, STATUS_WORDS } from "./stopWords";

  let { stops, onstop }: { stops: StopOn[]; onstop: (stop: string) => void } = $props();

  type Col = "name" | "id" | "status" | "since" | "until" | "surveyed";
  let by = $state<Col>("name");
  let down = $state(false);
  let limit = $state(300);
  const key = (s: StopOn, c: Col): string =>
    c === "name" ? (s.name ?? s.stop_id) : c === "id" ? s.stop_id : c === "status" ? s.status : c === "since" ? (s.since ?? "") : c === "until" ? (s.until ?? "") : (s.surveyed_on ?? "");
  const rows = $derived([...stops].sort((a, b) => (down ? -1 : 1) * key(a, by).localeCompare(key(b, by), undefined, { numeric: true }) || a.stop_id.localeCompare(b.stop_id)));
  const sort = (c: Col) => (by === c ? (down = !down) : ((by = c), (down = false)));
  const COLS: [Col, string][] = [["name", "Stop"], ["id", "ID"], ["status", "Status"], ["since", "Since"], ["until", "Until"], ["surveyed", "Last surveyed"]];
</script>

<div class="scroll" data-testid="stops-table">
  <table>
    <thead>
      <tr>
        {#each COLS as [c, label] (c)}<th><button type="button" onclick={() => sort(c)}>{label}{by === c ? (down ? " ↓" : " ↑") : ""}</button></th>{/each}
        <th>Notes</th>
      </tr>
    </thead>
    <tbody>
      {#each rows.slice(0, limit) as s (s.stop_id)}
        <tr onclick={() => onstop(s.stop_id)} data-testid="stop-row">
          <td><strong>{s.name || s.stop_id}</strong></td>
          <td class="muted">{s.stop_id}{s.code && s.code !== s.stop_id ? ` · ${s.code}` : ""}</td>
          <td><i class="dot" style="--fill:{STATUS_COLORS[s.status][0]};--edge:{STATUS_COLORS[s.status][1]}"></i>{STATUS_WORDS[s.status]}</td>
          <td class="num">{s.since ?? ""}</td>
          <td class="num">{s.until ?? ""}</td>
          <td class="num">{s.surveyed_on ?? ""}</td>
          <td class="muted">{s.note ?? ""}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  {#if rows.length > limit}<button type="button" class="more" onclick={() => (limit += 300)}>Show more ({rows.length - limit} left)</button>{/if}
</div>

<style>
  .scroll { overflow: auto; height: 100%; padding: 0 1.6rem 2rem; }
  table { width: 100%; border-collapse: collapse; font-size: 0.92rem; }
  thead th { position: sticky; top: 0; background: var(--surface); text-align: left; border-bottom: 1px solid var(--line); padding: 0; }
  th button { width: 100%; text-align: left; padding: 0.6rem; border: 0; background: none; color: var(--muted); font: inherit; font-size: 0.82rem; font-weight: 600; cursor: pointer; }
  thead th:last-child { padding: 0.6rem; color: var(--muted); font-size: 0.82rem; }
  td { padding: 0.5rem 0.6rem; border-bottom: 1px solid var(--line); }
  tbody tr { cursor: pointer; }
  tbody tr:hover { background: var(--hover); }
  .num { font-variant-numeric: tabular-nums; white-space: nowrap; }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 50%; border: 1.5px solid var(--edge); background: var(--fill); margin-right: 0.4rem; }
  .more { margin: 0.8rem 0; }
  @media (max-width: 900px) { .scroll { padding: 0 0.9rem 2rem; } td:nth-child(n + 4) { display: none; } thead th:nth-child(n + 4) { display: none; } }
</style>
