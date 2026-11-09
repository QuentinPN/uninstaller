<script lang="ts">
import { fr } from "../i18n/fr";
import { SECTIONS, type SectionId } from "./sections";

interface Props {
  current: SectionId;
  onselect: (id: SectionId) => void;
}

let { current, onselect }: Props = $props();
</script>

<nav class="pill" aria-label={fr.nav.label}>
  {#each SECTIONS as section (section.id)}
    <button
      type="button"
      class="tab"
      aria-current={current === section.id ? "page" : undefined}
      title={`${fr.nav[section.id]} (Ctrl+${section.digit})`}
      onclick={() => onselect(section.id)}
    >
      {fr.nav[section.id]}
    </button>
  {/each}
  <span class="separator" aria-hidden="true"></span>
  <button
    type="button"
    class="tab icon"
    aria-label={fr.nav.settings}
    title={fr.nav.settings}
    aria-current={current === "settings" ? "page" : undefined}
    onclick={() => onselect("settings")}
  >
    <span aria-hidden="true">&#xE713;</span>
  </button>
</nav>

<style>
  .pill {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: var(--space-1);
    border: 1px solid var(--separator);
    border-radius: var(--radius-pill);
    background: var(--material);
    backdrop-filter: var(--material-blur);
  }
  .tab {
    border: 0;
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    white-space: nowrap;
    transition:
      background-color var(--motion-fast) ease,
      color var(--motion-fast) ease;
  }
  .tab:hover {
    background: var(--fill-glass);
    color: var(--color-text);
  }
  .tab[aria-current="page"] {
    background: var(--color-primary);
    color: var(--color-on-primary);
  }
  .icon {
    padding: var(--space-2) var(--space-3);
    font-family: var(--font-icons);
  }
  .separator {
    width: 1px;
    align-self: stretch;
    margin: var(--space-2) var(--space-1);
    background: var(--separator);
  }
</style>
