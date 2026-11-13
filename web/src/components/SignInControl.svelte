<script lang="ts">
  /// Who is signed in to write, with the control to sign in or out —
  /// shown only where the deployment asks for sign-in (D-58: reads are
  /// public, a write carries the person's token). A RENDER component:
  /// the state and the actions are the page's (src/signin.svelte.ts).
  let {
    available,
    name,
    error,
    onsignin,
    onsignout,
  }: {
    available: boolean;
    name: string;
    error: string;
    onsignin: () => void;
    onsignout: () => void;
  } = $props();
</script>

{#if available}
  <span class="signin" data-testid="signin">
    {#if name}
      <span class="who" data-testid="signin-name">{name}</span>
      <button type="button" class="quiet" onclick={onsignout} data-testid="signout">Sign out</button>
    {:else}
      <button type="button" onclick={onsignin} data-testid="signin-button">Sign in</button>
    {/if}
    {#if error}<span class="error" role="alert">Sign-in failed: {error}</span>{/if}
  </span>
{/if}

<style>
  .signin { display: inline-flex; align-items: center; gap: 0.4rem; font-size: 0.85rem; }
  .who { color: var(--muted); }
  .error { color: var(--bad); }
  .quiet { background: none; border: none; color: var(--accent); cursor: pointer; text-decoration: underline; font: inherit; padding: 0; }
</style>
