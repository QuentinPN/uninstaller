<script lang="ts">
import { fr } from "../i18n/fr";

interface Props {
  /** Index of the step under way: the ones before it are done. */
  current: number;
}

let { current }: Props = $props();

const t = fr.uninstall;
const STEPS = Object.keys(t.steps) as (keyof typeof t.steps)[];
</script>

<ol class="steps" aria-label={t.stepsLabel}>
  {#each STEPS as key, i (key)}
    <li class:done={i < current} aria-current={i === current ? "step" : undefined}>
      <span class="mark num" aria-hidden="true">
        {#if i < current}<span class="glyph">&#xE73E;</span>{:else}{i + 1}{/if}
      </span>
      {t.steps[key]}
    </li>
  {/each}
</ol>

<style>
  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
  li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .done {
    color: var(--color-text-secondary);
  }
  [aria-current="step"] {
    color: var(--color-text);
  }
  .mark {
    display: grid;
    place-items: center;
    width: var(--space-6);
    height: var(--space-6);
    border-radius: var(--radius-pill);
    background: var(--fill-glass);
  }
  [aria-current="step"] .mark {
    background: var(--color-primary);
    color: var(--color-on-primary);
  }
  .glyph {
    font-family: var(--font-icons);
  }
</style>
