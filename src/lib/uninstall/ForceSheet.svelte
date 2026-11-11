<script lang="ts">
import { onMount } from "svelte";
import { fr } from "../i18n/fr";
import { commands, type ForceFailure, type Program } from "../ipc/bindings";
import { failedCall } from "../ipc/failed";

interface Props {
  onclose: () => void;
  /** The program named, kept by the app: its leftovers screen. */
  onleftovers: (program: Program) => void;
}

let { onclose, onleftovers }: Props = $props();

const t = fr.unlisted;

let name = $state("");
let folder = $state("");
let checking = $state(false);
let failure = $state<ForceFailure | null>(null);
let dialog: HTMLDialogElement;

async function submit(event: SubmitEvent) {
  event.preventDefault();
  checking = true;
  const result = await commands.forceUnlisted(name, folder).catch(failedCall);
  checking = false;
  if (result.status === "ok") onleftovers(result.data);
  else failure = result.error;
}

function close() {
  if (!checking) onclose();
}

/** Échap closes the sheet, never while the name is checked. */
function oncancel(event: Event) {
  event.preventDefault();
  close();
}

onMount(() => {
  dialog.showModal();
  return () => dialog.close();
});
</script>

<dialog class="sheet" bind:this={dialog} {oncancel} aria-labelledby="unlisted-title">
  <h2 id="unlisted-title">{t.title}</h2>
  <form onsubmit={submit}>
    <p class="hint">{t.intro}</p>
    <label>
      {t.name}
      <input class="field" bind:value={name} oninput={() => (failure = null)} />
    </label>
    <label>
      {t.folder}
      <input
        class="field"
        bind:value={folder}
        placeholder={t.folderPlaceholder}
        spellcheck="false"
        oninput={() => (failure = null)}
      />
    </label>
    <p class="failed" role="status">{failure ? t.failure[failure] : ""}</p>
    <div class="buttons">
      <button type="button" class="action" onclick={close}>{t.cancel}</button>
      <button class="action primary" disabled={checking}>
        {checking ? t.checking : t.search}
      </button>
    </div>
  </form>
</dialog>

<style>
  form {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  p {
    margin: 0;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  .field {
    color: var(--color-text);
    font-size: var(--font-size-body);
  }
  .failed {
    min-height: 1lh;
  }
</style>
