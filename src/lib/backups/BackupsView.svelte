<script lang="ts">
import { onMount } from "svelte";
import { formatDate, formatMoment, formatSize, isoDate } from "../format";
import { fr } from "../i18n/fr";
import { type BatchSummary, commands } from "../ipc/bindings";
import EmptyState from "../ui/EmptyState.svelte";

const t = fr.backups;

let batches = $state<BatchSummary[] | null>(null);
let failed = $state(false);
const totalKib = $derived(
  batches?.reduce((sum, batch) => sum + batch.sizeKib, 0) ?? 0,
);

function load() {
  batches = null;
  failed = false;
  commands.listQuarantine().then(
    (list) => (batches = list),
    () => (failed = true),
  );
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
</header>

{#if failed}
  <EmptyState title={t.error.title} text={t.error.text} onretry={load} />
{:else if batches && !batches.length}
  <EmptyState title={t.empty.title} text={t.empty.text} />
{:else if batches}
  <h2>{t.quarantine}</h2>
  <ul class="batches">
    {#each batches as batch (batch.name)}
      <li>
        <div class="lines">
          <span class="name">{batch.name}</span>
          <span class="meta">
            {formatMoment(batch.created)} · {t.contents(batch.files, batch.keys)}
          </span>
        </div>
        <div class="lines end">
          <span class="num">{formatSize(batch.sizeKib)}</span>
          <span class="meta">
            {batch.purging ? t.purging : t.purgeOn(day(batch.purgeOn))}
          </span>
        </div>
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
    display: flex;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-3) var(--space-4);
  }
  li + li {
    border-top: 1px solid var(--separator);
  }
  .lines {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }
  .end {
    align-items: flex-end;
    flex-shrink: 0;
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
</style>
