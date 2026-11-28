<script lang="ts">
  let {
    on = false,
    onclick,
    testid = "",
    disabled = false,
    class: klass = "",
    children,
  }: {
    on?: boolean;
    onclick?: () => void;
    testid?: string;
    disabled?: boolean;
    class?: string;
    children?: import("svelte").Snippet;
  } = $props();
</script>

<button
  type="button"
  class={klass}
  class:on
  aria-pressed={on}
  {disabled}
  {onclick}
  data-testid={testid}
>
  {@render children?.()}
</button>
