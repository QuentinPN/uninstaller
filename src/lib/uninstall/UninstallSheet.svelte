<script lang="ts">
import { onMount } from "svelte";
import { fr } from "../i18n/fr";
import { commands, type Program, type UninstallFailure } from "../ipc/bindings";
import { failedCall } from "../ipc/failed";
import { autofocus } from "../ui/autofocus";
import Steps from "./Steps.svelte";

interface Props {
  program: Program;
  /** `ran`: the uninstaller was started, so the list has to be read again. */
  onclose: (ran: boolean) => void;
  /** Steps 3 and 4: the leftovers screen, for the program as listed before. */
  onleftovers: () => void;
}

let { program, onclose, onleftovers }: Props = $props();

const t = fr.uninstall;
type PointOutcome = keyof typeof fr.backups.point;

/** STATE_MACHINES.md 1, up to the uninstaller's end. */
type Stage =
  | { state: "confirm" }
  | { state: "point" }
  | { state: "pointFailed"; outcome: PointOutcome }
  | { state: "running" }
  | { state: "failed"; failure: UninstallFailure }
  | { state: "stillListed" };

let stage = $state<Stage>({ state: "confirm" });
let withPoint = $state(true);
let ran = false;
let dialog: HTMLDialogElement;

const busy = $derived(stage.state === "point" || stage.state === "running");
const step = $derived(
  stage.state === "confirm" ||
    stage.state === "point" ||
    stage.state === "pointFailed"
    ? 0
    : 1,
);

async function start() {
  if (!withPoint) return runUninstaller();
  stage = { state: "point" };
  const result = await commands.createRestorePoint().catch(failedCall);
  const outcome = result.status === "ok" ? result.data : result.error;
  // « skipped »: Windows keeps the recent point, which still covers this.
  if (outcome === "created" || outcome === "skipped") runUninstaller();
  else stage = { state: "pointFailed", outcome };
}

async function runUninstaller() {
  stage = { state: "running" };
  const result = await commands.runUninstaller(program.id).catch(failedCall);
  if (result.status === "error") {
    stage = { state: "failed", failure: result.error };
    return;
  }
  ran = true;
  const { finished, stillListed } = result.data;
  if (finished && stillListed) stage = { state: "stillListed" };
  else onleftovers();
}

function stopWaiting() {
  commands.stopUninstaller().catch(() => {});
}

function close() {
  if (!busy) onclose(ran);
}

/** Échap closes the sheet, never while a step is under way. */
function oncancel(event: Event) {
  event.preventDefault();
  close();
}

onMount(() => {
  // showModal focuses the first control, the checkbox: give it back to « Annuler ».
  const safe = document.activeElement as HTMLElement | null;
  dialog.showModal();
  safe?.focus();
});
</script>

<dialog bind:this={dialog} {oncancel} aria-labelledby="uninstall-title">
  <h2 id="uninstall-title">{t.title(program.name)}</h2>
  {#if program.publisher}<p class="publisher">{program.publisher}</p>{/if}
  <Steps current={step} />

  <div class="body" role="status" aria-busy={busy}>
    {#if stage.state === "confirm"}
      <p>{t.intro}</p>
      <label class="check">
        <input type="checkbox" bind:checked={withPoint} />
        {t.withPoint}
      </label>
    {:else if stage.state === "point"}
      <p>{fr.backups.creatingPoint}</p>
      <p class="hint">{t.pointHint}</p>
    {:else if stage.state === "pointFailed"}
      <p class="failed">{fr.backups.point[stage.outcome]}</p>
      <p class="hint">{t.withoutPointHint}</p>
    {:else if stage.state === "running"}
      <p>{t.running}</p>
      <p class="hint">{t.runningHint}</p>
    {:else if stage.state === "failed"}
      <p class="failed">{t.failure[stage.failure]}</p>
    {:else}
      <p>{t.stillListed(program.name)}</p>
    {/if}
  </div>

  <div class="buttons">
    <!-- Each stage focuses its safe choice, so the keyboard never lands on the page. -->
    {#if stage.state === "confirm"}
      <button class="action" use:autofocus onclick={close}>{t.cancel}</button>
      <button class="action primary" onclick={start}>{t.start}</button>
    {:else if stage.state === "pointFailed"}
      <button class="action" use:autofocus onclick={close}>{t.cancel}</button>
      <button class="action primary" onclick={runUninstaller}>{t.withoutPoint}</button>
    {:else if stage.state === "running"}
      <button class="action" use:autofocus onclick={stopWaiting}>{t.stopWaiting}</button>
    {:else if stage.state === "failed"}
      <button class="action" use:autofocus onclick={close}>{t.close}</button>
      {#if stage.failure === "refused"}
        <button class="action primary" onclick={runUninstaller}>{fr.common.retry}</button>
      {/if}
    {:else if stage.state === "stillListed"}
      <button class="action" use:autofocus onclick={close}>{t.close}</button>
      <button class="action primary" onclick={onleftovers}>{fr.leftovers.search}</button>
    {/if}
  </div>
</dialog>

<style>
  dialog {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    width: min(var(--sheet-width), calc(100vw - 2 * var(--space-8)));
    padding: var(--space-6);
    border: 1px solid var(--separator);
    border-radius: var(--radius-lg);
    background: var(--material-thick);
    backdrop-filter: var(--material-blur);
    color: var(--color-text);
    animation: enter var(--motion-base) var(--ease-out);
  }
  dialog::backdrop {
    background: var(--scrim);
  }
  @keyframes enter {
    from {
      opacity: 0;
      transform: translateY(var(--space-6));
    }
  }
  h2 {
    margin: 0;
    font-size: var(--font-size-body);
    overflow-wrap: anywhere;
  }
  .publisher {
    margin: calc(-1 * var(--space-3)) 0 0;
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-height: calc(3 * 1lh);
  }
  .body p {
    margin: 0;
  }
  .hint {
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  .failed {
    color: var(--color-danger);
  }
  .check {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text-secondary);
  }
  .buttons {
    justify-content: flex-end;
  }
</style>
