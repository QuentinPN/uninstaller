<script lang="ts">
import { onMount } from "svelte";
import { formatDate, formatMoment, formatSize, isoDate } from "../format";
import { fr } from "../i18n/fr";
import {
  type BatchFailure,
  type BatchSummary,
  commands,
} from "../ipc/bindings";
import EmptyState from "../ui/EmptyState.svelte";

const t = fr.backups;

let batches = $state<BatchSummary[] | null>(null);
let failed = $state(false);
/** The batch an action runs on; the others' buttons wait. */
let busy = $state<string | null>(null);
/** The batch whose purge waits for confirmation. */
let confirming = $state<string | null>(null);
let notice = $state<{ text: string; failed: boolean } | null>(null);
let creatingPoint = $state(false);
const totalKib = $derived(
  batches?.reduce((sum, batch) => sum + batch.sizeKib, 0) ?? 0,
);

function load() {
  failed = false;
  commands.listQuarantine().then(
    (list) => (batches = list),
    () => (failed = true),
  );
}

/** Runs a batch action, then rereads the list, which shows what it changed. */
async function act(
  batch: BatchSummary,
  run: (
    name: string,
  ) => Promise<{ status: "ok" } | { status: "error"; error: BatchFailure }>,
  done: (name: string) => string,
) {
  busy = batch.name;
  confirming = null;
  notice = null;
  const result = await run(batch.name).catch(
    () => ({ status: "error", error: "failed" }) as const,
  );
  notice =
    result.status === "ok"
      ? { text: done(batch.name), failed: false }
      : { text: t.failure[result.error], failed: true };
  busy = null;
  load();
}

/** Asks the broker for a restore point; the UAC prompt shows the first time. */
async function createPoint() {
  creatingPoint = true;
  notice = { text: t.creatingPoint, failed: false };
  const result = await commands
    .createRestorePoint()
    .catch(() => ({ status: "error", error: "failed" }) as const);
  const outcome = result.status === "ok" ? result.data : result.error;
  notice = { text: t.point[outcome], failed: outcome !== "created" };
  creatingPoint = false;
}

/** Moves keyboard focus to the safe choice when the confirmation opens. */
function focus(node: HTMLElement) {
  node.focus();
}

/** Seconds since the Unix epoch to the local "23/11/2026". */
function day(secs: number): string {
  return formatDate(isoDate(new Date(secs * 1000)));
}

onMount(load);
</script>

<header class="head">
  <h1>{fr.nav.backups}</h1>
  {#if batches?.length}
    <p class="summary num">
      {t.count(batches.length)} · {formatSize(totalKib)}
    </p>
  {/if}
  {#if notice}
    <p class="summary notice" class:failed={notice.failed} role="status">
      {notice.text}
    </p>
  {/if}
  <button class="action create" disabled={creatingPoint} onclick={createPoint}>
    {t.createPoint}
  </button>
</header>

{#if failed}
  <EmptyState title={t.error.title} text={t.error.text} onretry={load} />
{:else if batches && !batches.length}
  <EmptyState title={t.empty.title} text={t.empty.text} />
{:else if batches}
  <h2>{t.quarantine}</h2>
  <ul class="batches">
    {#each batches as batch (batch.name)}
      {@const contents = t.contents(batch.files, batch.keys)}
      <li>
        <div class="row">
          <div class="lines">
            <span class="name">{batch.name}</span>
            <span class="meta">{formatMoment(batch.created)} · {contents}</span>
          </div>
          <div class="lines end">
            <span class="num">{formatSize(batch.sizeKib)}</span>
            <span class="meta">
              {batch.purging ? t.purging : t.purgeOn(day(batch.purgeOn))}
            </span>
          </div>
          <!-- Hidden, not removed, so sizes and dates stay in one column. -->
          <div class="buttons" class:hidden={confirming === batch.name}>
            <button
              class="action"
              class:hidden={batch.files + batch.keys === 0 || batch.purging}
              disabled={busy !== null}
              onclick={() => act(batch, commands.restoreBatch, t.restored)}
            >
              {t.restore}
            </button>
            <button
              class="action"
              disabled={busy !== null}
              onclick={() => (confirming = batch.name)}
            >
              {t.purge}
            </button>
          </div>
        </div>
        {#if confirming === batch.name}
          <div class="confirm" role="alertdialog" aria-label={t.purge}>
            <p>{t.confirmPurge(contents, formatSize(batch.sizeKib))}</p>
            <div class="buttons">
              <button class="action" use:focus onclick={() => (confirming = null)}>
                {t.cancel}
              </button>
              <button
                class="action danger"
                disabled={busy !== null}
                onclick={() => act(batch, commands.purgeBatch, t.purged)}
              >
                {t.purge}
              </button>
            </div>
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .head {
    display: flex;
    align-items: baseline;
    gap: var(--space-4);
    margin: var(--space-6) 0 var(--space-4);
  }
  h1 {
    margin: 0;
    font-size: var(--font-size-title);
  }
  .summary {
    margin: 0;
    color: var(--color-text-secondary);
    white-space: nowrap;
  }
  .notice {
    min-width: 0;
    white-space: normal;
  }
  .failed {
    color: var(--color-danger);
  }
  .create {
    flex: none;
    margin-left: auto;
  }
  h2 {
    margin: 0 0 var(--space-2);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    font-weight: 500;
  }
  .batches {
    margin: 0;
    padding: 0;
    overflow-y: auto;
    border-radius: var(--radius-md);
    background: var(--color-surface);
    list-style: none;
  }
  li {
    padding: var(--space-3) var(--space-4);
  }
  li + li {
    border-top: 1px solid var(--separator);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-4);
  }
  .lines {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }
  .end {
    flex: none;
    align-items: flex-end;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  .hidden {
    visibility: hidden;
  }
  .buttons {
    display: flex;
    flex: none;
    gap: var(--space-2);
  }
  .confirm {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    margin-top: var(--space-3);
    padding-top: var(--space-3);
    border-top: 1px solid var(--separator);
  }
  .confirm p {
    margin: 0;
    color: var(--color-text-secondary);
  }
</style>
