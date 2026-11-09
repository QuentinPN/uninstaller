<script lang="ts">
import { convertFileSrc } from "@tauri-apps/api/core";
import type { Program } from "../ipc/bindings";

interface Props {
  program: Program;
}

let { program }: Props = $props();

/** The URL that answered 404 or could not be decoded; rows are reused while scrolling. */
let failed = $state<string | null>(null);

// The id, never a path: the backend reads `DisplayIcon` itself (ADR 0004).
const src = $derived(
  program.displayIcon ? convertFileSrc(program.id, "icon") : null,
);
</script>

{#if src && src !== failed}
  <img class="app-icon" {src} alt="" loading="lazy" onerror={() => (failed = src)} />
{:else}
  <span class="app-icon glyph" aria-hidden="true">&#xE71D;</span>
{/if}

<style>
  .app-icon {
    width: var(--icon-size);
    height: var(--icon-size);
  }
  .glyph {
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    background: var(--fill-glass);
    color: var(--color-text-muted);
    font-family: var(--font-icons);
  }
</style>
