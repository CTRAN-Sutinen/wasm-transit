<script lang="ts">
  /// The realtime panel — a RENDER component over cap-rt's live
  /// summary (vehicles by status, the late list worst first, and
  /// whether that list is buses or predictions). It counts and ranks
  /// nothing itself.
  import type { LiveSummary } from "../types/LiveSummary";

  let { summary, alerts }: { summary: LiveSummary | null; alerts: number } = $props();

  function fmtDelay(d: number) {
    const min = Math.round(d / 60);
    return min >= 1 ? `+${min} min` : `+${d} s`;
  }
</script>

<section>
  <h2>Realtime</h2>
  {#if summary?.vehicles}
    <p class="chips">
      {#if summary.counts.early}<span class="chip warn">{summary.counts.early} early</span>{/if}
      {#if summary.counts.on_time}<span class="chip ok">{summary.counts.on_time} on time</span>{/if}
      {#if summary.counts.late}<span class="chip warn">{summary.counts.late} late 1–5 min</span>{/if}
      {#if summary.counts.very_late}<span class="chip bad">{summary.counts.very_late} late 5+ min</span>{/if}
      {#if summary.counts.no_prediction}<span class="chip">{summary.counts.no_prediction} no prediction</span>{/if}
      <span class="chip svc">{summary.vehicles} buses</span>
    </p>
  {:else}
    <p class="muted">
      Trip updates only — predictions show on the boards. Bus status
      chips need vehicle positions (Data sources).
    </p>
  {/if}
  {#if alerts}<span class="chip warn">{alerts} alerts</span>{/if}

  {#if summary?.late.length}
    <table>
      <tbody>
        {#each summary.late as x (x.key)}
          <tr>
            <td><span class="route-badge">{x.route ?? "?"}</span></td>
            {#if x.label}<td><span class="chip svc">{x.label}</span></td>{/if}
            <td class="muted">{x.agency_trip_id ?? "?"}</td>
            <td><span class="chip {x.status === 'very_late' ? 'bad' : 'warn'}">{fmtDelay(x.delay_secs)}</span></td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if summary.late_are_predictions}
      <p class="muted">Late trips (predictions — no vehicle positions in this feed).</p>
    {/if}
  {:else if summary?.vehicles}
    <p class="muted">Every bus with data is on time.</p>
  {/if}

  <p class="muted legend">
    Buses on the map:
    <span class="dot ok"></span> on time
    <span class="dot warn"></span> 1–5 min late
    <span class="dot bad"></span> 5+ min late
    <span class="dot"></span> no data
  </p>
</section>
