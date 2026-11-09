<script lang="ts">
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onMount } from "svelte";
import { fr } from "../i18n/fr";

interface Props {
  /** The process runs as administrator (ADR 0002: it should not). */
  elevated: boolean;
}

let { elevated }: Props = $props();

// Null in a plain browser (UI previews): the buttons then do nothing.
const appWindow = isTauri() ? getCurrentWindow() : null;
let maximized = $state(false);

onMount(() => {
  if (!appWindow) return;
  const sync = () =>
    void appWindow.isMaximized().then(
      (m) => (maximized = m),
      () => {},
    );
  sync();
  const unlisten = appWindow.onResized(sync);
  return () => void unlisten.then((stop) => stop());
});
</script>

<!-- Replaces the Windows title bar. The empty area drags the window and
     double-clicking it maximizes; the buttons keep Windows' order and glyphs. -->
<div class="titlebar" data-tauri-drag-region>
  {#if elevated}
    <span class="elevated" title={fr.window.elevatedHint}>{fr.window.elevated}</span>
  {/if}
  <div class="controls">
    <button
      type="button"
      aria-label={fr.window.minimize}
      title={fr.window.minimize}
      tabindex="-1"
      onclick={() => void appWindow?.minimize()}
    >
      <span aria-hidden="true">&#xE921;</span>
    </button>
    <button
      type="button"
      aria-label={maximized ? fr.window.restore : fr.window.maximize}
      title={maximized ? fr.window.restore : fr.window.maximize}
      tabindex="-1"
      onclick={() => void appWindow?.toggleMaximize()}
    >
      <span aria-hidden="true">{maximized ? "" : ""}</span>
    </button>
    <button
      type="button"
      class="close"
      aria-label={fr.window.close}
      title={fr.window.close}
      tabindex="-1"
      onclick={() => void appWindow?.close()}
    >
      <span aria-hidden="true">&#xE8BB;</span>
    </button>
  </div>
</div>

<style>
  .titlebar {
    position: fixed;
    inset: 0 0 auto 0;
    z-index: 1000;
    height: var(--titlebar-height);
    display: flex;
    align-items: center;
    background: var(--color-bg);
    user-select: none;
  }
  .elevated {
    margin-left: var(--space-3);
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--fill-glass);
    color: var(--color-danger);
    font-size: var(--font-size-sm);
  }
  .controls {
    display: flex;
    height: 100%;
    margin-left: auto;
  }
  button {
    width: var(--caption-button-width);
    height: 100%;
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--color-text);
    font-family: var(--font-icons);
    font-size: var(--caption-glyph-size);
    transition: background-color var(--motion-fast) ease;
  }
  button:hover {
    background: var(--fill-hover);
  }
  button:active {
    background: var(--fill-pressed);
  }
  button.close:hover {
    background: var(--color-caption-close);
    color: var(--color-on-caption-close);
  }
  button.close:active {
    background: var(--color-caption-close-pressed);
  }
</style>
