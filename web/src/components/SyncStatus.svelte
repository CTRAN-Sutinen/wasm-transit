<script lang="ts">
  /// What a staff page holds that the server has not confirmed (D-189):
  /// the writes that wait to be sent, the writes the server refused
  /// with its reasons, the writes it kept with a warning (a broken
  /// reference, D-233), whether the server answers and the time of the
  /// data the page works from, and a new version that waits. A RENDER
  /// component: the state and the actions are the page's (its host,
  /// over src/outbox.svelte.ts and kit's build lifecycle). It shows
  /// nothing while all is sent and the server answers.
  type Held = { key: string; label: string; reason: string };
  type Warned = { key: string; label: string; warning: string };
  let {
    waiting,
    held,
    offline,
    asOf = "",
    needsSignIn = false,
    updateReady = false,
    canUpdate = true,
    warned = [],
    onsend,
    onretry,
    ondiscard,
    ondismiss = () => {},
    onupdate,
  }: {
    waiting: number;
    held: Held[];
    offline: boolean;
    /// The time the server last confirmed the page's data ("4:05 PM").
    asOf?: string;
    needsSignIn?: boolean;
    updateReady?: boolean;
    /// False while the page holds unsaved work: the update waits.
    canUpdate?: boolean;
    /// The writes the server kept with a warning (a broken reference).
    warned?: Warned[];
    onsend: () => void;
    onretry: (key: string) => void;
    ondiscard: (key: string) => void;
    ondismiss?: (key: string) => void;
    onupdate: () => void;
  } = $props();
  const changes = (n: number) => `${n} change${n === 1 ? "" : "s"}`;
</script>

{#if offline || waiting || held.length || warned.length || updateReady}
  <div class="sync" data-testid="sync-status" role="status">
    {#if offline}
      <p class="line" data-testid="sync-offline">
        <strong>Offline</strong>{asOf ? ` · data as of ${asOf}` : ""}
      </p>
    {/if}
    {#if waiting}
      <p class="line" data-testid="sync-waiting">
        {needsSignIn ? `Sign in to send ${changes(waiting)}` : `${changes(waiting)} waiting to send`}
        {#if !needsSignIn}<button type="button" class="quiet" onclick={onsend} data-testid="sync-send">Send now</button>{/if}
      </p>
    {/if}
    {#each held as h (h.key)}
      <div class="held" data-testid="sync-held">
        <p><strong>Not saved:</strong> {h.label}</p>
        <p class="reason">{h.reason}</p>
        <p class="actions">
          <button type="button" onclick={() => onretry(h.key)} data-testid="sync-retry">Send again</button>
          <button type="button" class="quiet" onclick={() => ondiscard(h.key)} data-testid="sync-discard">Discard</button>
        </p>
      </div>
    {/each}
    {#each warned as w (w.key)}
      <div class="held" data-testid="sync-warned">
        <p><strong>Saved with a warning:</strong> {w.label}</p>
        <p class="reason">{w.warning}</p>
        <p class="actions">
          <button type="button" class="quiet" onclick={() => ondismiss(w.key)} data-testid="sync-dismiss">Dismiss</button>
        </p>
      </div>
    {/each}
    {#if updateReady}
      <p class="line" data-testid="update-ready">
        A new version is ready
        <button type="button" onclick={onupdate} disabled={!canUpdate} title={canUpdate ? "" : "Save or close your work first"}>Reload</button>
      </p>
    {/if}
  </div>
{/if}

<style>
  .sync {
    display: grid;
    gap: 0.35rem;
    padding: 0.45rem 0.7rem;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--surface);
    font-size: 0.85rem;
  }
  p { margin: 0; }
  .line { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .held { border-left: 3px solid var(--bad); padding-left: 0.5rem; display: grid; gap: 0.2rem; }
  .reason { color: var(--muted); }
  .actions { display: flex; gap: 0.6rem; }
  .quiet { background: none; border: none; color: var(--accent); cursor: pointer; text-decoration: underline; font: inherit; padding: 0; }
</style>
