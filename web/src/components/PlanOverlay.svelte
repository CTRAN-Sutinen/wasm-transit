<script>
  import { app } from "../state.svelte.js";

  const plan = $derived(app.plan);
  let dismissed = $state(false);

  // A new plan re-opens the overlay.
  $effect(() => {
    if (plan) dismissed = false;
  });

  function fmtClock(secs) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
  }
  function fmtWalk(secs) {
    const min = Math.round(secs / 60);
    return min >= 1 ? `${min} min` : `${secs} s`;
  }

  // Before/after when diversions are in force: how the active edits
  // changed this plan vs the base schedule.
  const compare = $derived.by(() => {
    const base = app.planBaseline;
    if (!base || !plan) return null;
    const delta = plan.arrival_secs - base.arrival_secs;
    if (delta === 0 && plan.legs.length === base.legs.length) return null;
    const min = Math.round(Math.abs(delta) / 60);
    const label = delta > 0 ? `${min} min later` : delta < 0 ? `${min} min earlier` : "different legs";
    return { base, label };
  });
</script>

{#if plan && !dismissed && plan.legs.length}
  <div class="plan-overlay" role="complementary" aria-label="Trip plan">
    <header>
      <strong>{plan.from_stop_name} → {plan.to_stop_name}</strong>
      <button class="link" onclick={() => (dismissed = true)}>Close</button>
    </header>
    <p>
      Leave {fmtClock(plan.depart_secs)} ·
      arrive <strong>{fmtClock(plan.arrival_secs)}</strong> ·
      {plan.legs.filter((l) => l.type === "transit").length} legs
    </p>
    {#if compare}
      <p class="plan-compare">
        Without the active diversion: arrive {fmtClock(compare.base.arrival_secs)}
        ({compare.label}) — toggle it off in the Diversions section to compare.
      </p>
    {/if}
    <ol class="timeline">
      {#each plan.legs as leg, i (i)}
        {#if leg.type === "transit"}
          <li class="leg-transit">
            <span class="t">{fmtClock(leg.board_secs)}</span>
            <span class="route-badge">{leg.route_name}</span>
            <span>
              {leg.board_stop_name} → <strong>{leg.alight_stop_name}</strong>
              <span class="muted">arr {fmtClock(leg.alight_secs)}</span>
            </span>
          </li>
        {:else}
          <li class="leg-walk">
            <span class="t"></span>
            <span class="muted">walk {fmtWalk(leg.walk_secs)} → {leg.to_stop_name}</span>
          </li>
        {/if}
      {/each}
    </ol>
  </div>
{/if}
