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
