<script lang="ts">
import type { Snippet } from "svelte";

interface Props {
  title: string;
  text: string;
  /** Optional action under the text, such as "Réessayer". */
  children?: Snippet;
}

let { title, text, children }: Props = $props();
</script>

<div class="empty">
  <p class="title">{title}</p>
  <p class="text">{text}</p>
  {@render children?.()}
</div>

<style>
  .empty {
    max-width: 44ch;
    margin: var(--space-8) auto;
    text-align: center;
  }
  .title {
    margin: 0 0 var(--space-2);
    color: var(--color-text);
  }
  .text {
    margin: 0 0 var(--space-4);
    color: var(--color-text-secondary);
    line-height: 1.5;
  }
</style>
