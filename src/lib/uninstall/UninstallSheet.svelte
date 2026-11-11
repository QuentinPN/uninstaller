<script lang="ts">
import { fr } from "../i18n/fr";
import { commands, type Program, type UninstallFailure } from "../ipc/bindings";
import { failedCall } from "../ipc/failed";
import { autofocus } from "../ui/autofocus";
import Steps from "./Steps.svelte";

interface Props {
  /** One program, or a batch run one after another (F7). */
  programs: Program[];
  /** The leftovers screen of one of a batch is open over the list. */
  hidden?: boolean;
  /** `ran`: an uninstaller was started, so the list has to be read again. */
  onclose: (ran: boolean) => void;
  /** Steps 3 and 4: the leftovers screen, for the program as listed before. */
  onleftovers: (program: Program) => void;
}

let { programs, hidden = false, onclose, onleftovers }: Props = $props();

const t = fr.uninstall;
type PointOutcome = keyof typeof fr.backups.point;
/** What became of one program's uninstaller. */
type Outcome = keyof typeof t.outcome | UninstallFailure;

/** STATE_MACHINES.md 1, up to the uninstallers' end. */
type Stage =
  | { state: "confirm" }
  | { state: "point" }
  | { state: "pointFailed"; outcome: PointOutcome }
  | { state: "running"; index: number }
  | { state: "failed"; failure: UninstallFailure }
  | { state: "stillListed" }
  | { state: "summary" };

let stage = $state<Stage>({ state: "confirm" });
let withPoint = $state(true);
/** Without the uninstallers' windows where the installer allows it: on for a batch,
 * which then runs unattended; one program keeps its uninstaller's options. */
// svelte-ignore state_referenced_locally
let silent = $state(programs.length > 1);
/** By index in `programs`, as each uninstaller ends. */
let outcomes = $state<Outcome[]>([]);
/** The batch stops once the running uninstaller ends. */
let stopping = $state(false);
let ran = false;
let dialog: HTMLDialogElement;
/** The control that had focus when the sheet was hidden. */
let restore: HTMLElement | null = null;

const single = $derived(programs.length === 1);
const first = $derived(programs[0] as Program);
const busy = $derived(stage.state === "point" || stage.state === "running");
const step = $derived(
  stage.state === "confirm" ||
    stage.state === "point" ||
    stage.state === "pointFailed"
    ? 0
    : stage.state === "summary"
      ? 2
      : 1,
);

function isFailure(outcome: Outcome): outcome is UninstallFailure {
  return outcome in t.failure;
}

/** The uninstaller started, so its leftovers are worth a search. */
function started(outcome: Outcome) {
  return (
    outcome === "done" || outcome === "stopped" || outcome === "stillListed"
  );
}

async function start() {
  if (!withPoint) return runQueue();
  stage = { state: "point" };
  const result = await commands.createRestorePoint().catch(failedCall);
  const outcome = result.status === "ok" ? result.data : result.error;
  // « skipped »: Windows keeps the recent point, which still covers this.
  if (outcome === "created" || outcome === "skipped") runQueue();
  else stage = { state: "pointFailed", outcome };
}

/** Runs the uninstallers not run yet, one after another. */
async function runQueue() {
  for (let index = outcomes.length; index < programs.length; index++) {
    if (stopping) {
      outcomes.push("skipped");
      continue;
    }
    stage = { state: "running", index };
    outcomes.push(await uninstall(programs[index] as Program));
  }
  finish();
}

async function uninstall(program: Program): Promise<Outcome> {
  const result = await commands
    .runUninstaller(program.id, silent)
    .catch(failedCall);
  if (result.status === "error") return result.error;
  ran = true;
  const { finished, stillListed } = result.data;
  if (!finished) return "stopped";
  return stillListed ? "stillListed" : "done";
}

/** A batch ends on its summary; one program goes on to its leftovers. */
function finish() {
  const [outcome] = outcomes;
  if (!single || outcome === undefined) stage = { state: "summary" };
  else if (isFailure(outcome)) stage = { state: "failed", failure: outcome };
  else if (outcome === "stillListed") stage = { state: "stillListed" };
  else onleftovers(first);
}

function retry() {
  outcomes.pop();
  runQueue();
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

$effect(() => {
  if (hidden) return;
  dialog.showModal();
  // showModal focuses the first control, the checkbox: focus goes back to the one
  // that had it when hidden, or else to the first button, the safe choice.
  (restore ?? dialog.querySelector<HTMLElement>(".buttons button"))?.focus();
  return () => {
    const active = document.activeElement;
    restore =
      active instanceof HTMLElement && dialog.contains(active) ? active : null;
    dialog.close();
  };
});
</script>

<dialog bind:this={dialog} {oncancel} aria-labelledby="uninstall-title">
  <h2 id="uninstall-title">{single ? t.title(first.name) : t.titleMany(programs.length)}</h2>
  {#if single && first.publisher}<p class="publisher">{first.publisher}</p>{/if}
  <Steps current={step} />

  <div class="body" role="status" aria-busy={busy}>
    {#if stage.state === "confirm"}
      <p>{single ? t.intro : t.introMany}</p>
      {#if !single}
        <ul class="queue" aria-label={t.queueLabel}>
          {#each programs as program (program.id)}<li>{program.name}</li>{/each}
        </ul>
      {/if}
      <label class="check">
        <input type="checkbox" bind:checked={withPoint} />
        {t.withPoint}
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={silent} />
        {t.silent}
      </label>
    {:else if stage.state === "point"}
      <p>{fr.backups.creatingPoint}</p>
      <p class="hint">{t.pointHint}</p>
    {:else if stage.state === "pointFailed"}
      <p class="failed">{fr.backups.point[stage.outcome]}</p>
      <p class="hint">{t.withoutPointHint}</p>
    {:else if stage.state === "running"}
      {#if !single}
        <p class="num">
          {t.progress(stage.index + 1, programs.length, programs[stage.index]?.name ?? "")}
        </p>
      {/if}
      <p>{silent ? t.runningSilent : t.running}</p>
      <p class="hint">{stopping ? t.queueStopping : t.runningHint}</p>
    {:else if stage.state === "failed"}
      <p class="failed">{t.failure[stage.failure]}</p>
    {:else if stage.state === "stillListed"}
      <p>{t.stillListed(first.name)}</p>
    {:else}
      <p class="hint">{t.summaryHint}</p>
      <ul class="queue summary" aria-label={t.summaryLabel}>
        {#each programs as program, i (program.id)}
          {@const outcome = outcomes[i] ?? "skipped"}
          <li>
            <div class="outcome">
              <span>{program.name}</span>
              <span class="hint" class:failed={isFailure(outcome)}>
                {isFailure(outcome) ? t.failure[outcome] : t.outcome[outcome]}
              </span>
            </div>
            {#if started(outcome)}
              <button class="action" onclick={() => onleftovers(program)}>
                {fr.leftovers.search}
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="buttons">
    <!-- Each later stage focuses its safe choice, so the keyboard never lands on the page. -->
    {#if stage.state === "confirm"}
      <button class="action" onclick={close}>{t.cancel}</button>
      <button class="action primary" onclick={start}>{t.start}</button>
    {:else if stage.state === "pointFailed"}
      <button class="action" use:autofocus onclick={close}>{t.cancel}</button>
      <button class="action primary" onclick={runQueue}>{t.withoutPoint}</button>
    {:else if stage.state === "running"}
      {#if !single}
        <button class="action" disabled={stopping} onclick={() => (stopping = true)}>
          {t.stopQueue}
        </button>
      {/if}
      <button class="action" use:autofocus onclick={stopWaiting}>{t.stopWaiting}</button>
    {:else if stage.state === "failed"}
      <button class="action" use:autofocus onclick={close}>{t.close}</button>
      {#if stage.failure === "refused"}
        <button class="action primary" onclick={retry}>{fr.common.retry}</button>
      {/if}
    {:else if stage.state === "stillListed"}
      <button class="action" use:autofocus onclick={close}>{t.close}</button>
      <button class="action primary" onclick={() => onleftovers(first)}>
        {fr.leftovers.search}
      </button>
    {:else}
      <button class="action" use:autofocus onclick={close}>{t.close}</button>
    {/if}
  </div>
</dialog>

<style>
  dialog {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    width: min(var(--sheet-width), calc(100vw - 2 * var(--space-8)));
    max-height: calc(100vh - 2 * var(--space-8));
    padding: var(--space-6);
    border: 1px solid var(--separator);
    border-radius: var(--radius-lg);
    background: var(--material-thick);
    backdrop-filter: var(--material-blur);
    color: var(--color-text);
    animation: enter var(--motion-base) var(--ease-out);
  }
  dialog:not([open]) {
    display: none;
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
    overflow-y: auto;
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
  .queue {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-wrap: anywhere;
  }
  .summary {
    gap: var(--space-3);
  }
  .summary li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }
  .outcome {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .summary .action {
    flex-shrink: 0;
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
