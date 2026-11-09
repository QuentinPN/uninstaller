<script lang="ts">
interface Props {
  title: string;
  text: string;
}

let { title, text }: Props = $props();
</script>

<div class="empty">
  <p class="title">{title}</p>
  <p class="text">{text}</p>
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
    margin: 0;
    color: var(--color-text-secondary);
    line-height: 1.5;
  }
</style>
