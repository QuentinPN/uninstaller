<script lang="ts">
import { fr } from "../i18n/fr";

interface Props {
  title: string;
  text: string;
  /** For an error: shows "Réessayer" (or `action`) under the text. */
  onretry?: (() => void) | undefined;
  action?: string;
}

let { title, text, onretry, action = fr.common.retry }: Props = $props();
</script>

<div class="empty">
  <p class="title">{title}</p>
  <p class="text">{text}</p>
  {#if onretry}
    <button class="retry" onclick={onretry}>{action}</button>
  {/if}
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
  .retry {
    padding: var(--space-2) var(--space-4);
    border: none;
    border-radius: var(--radius-pill);
    background: var(--color-primary);
    color: var(--color-on-primary);
    cursor: pointer;
  }
</style>
