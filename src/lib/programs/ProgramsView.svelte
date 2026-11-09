<script lang="ts">
import { onMount } from "svelte";
import { formatDate, formatSize } from "../format";
import { fr } from "../i18n/fr";
import { commands, type Program } from "../ipc/bindings";
import EmptyState from "../ui/EmptyState.svelte";
import VirtualList from "../ui/VirtualList.svelte";
import { type SortKey, totalSizeKib, visiblePrograms } from "./view";

const SORT_KEYS: SortKey[] = ["name", "size", "date"];
/** Placeholder rows while the inventory loads. */
const SKELETON_ROWS = 8;

let programs = $state<Program[] | null>(null);
let failed = $state(false);
let query = $state("");
let sort = $state<SortKey>("name");

const visible = $derived(
  programs ? visiblePrograms(programs, query, sort) : [],
);

function load() {
  programs = null;
  failed = false;
  commands.listPrograms().then(
    (list) => (programs = list),
    () => (failed = true),
  );
}

onMount(load);
</script>

<header class="head">
  <h1>{fr.nav.programs}</h1>
  {#if programs}
    <p class="summary num">
      {fr.programs.count(visible.length)} · {formatSize(totalSizeKib(visible))}
    </p>
  {/if}
</header>

<div class="toolbar">
  <input
    class="search"
    type="search"
    placeholder={fr.programs.search}
    aria-label={fr.programs.searchHint}
    title={fr.programs.searchHint}
    bind:value={query}
  />
  <label class="sort">
    {fr.programs.sortBy}
    <select bind:value={sort}>
      {#each SORT_KEYS as key (key)}
        <option value={key}>{fr.programs.sort[key]}</option>
      {/each}
    </select>
  </label>
</div>

{#if failed}
  <EmptyState title={fr.programs.error.title} text={fr.programs.error.text}>
    <button class="retry" onclick={load}>{fr.programs.error.retry}</button>
  </EmptyState>
{:else if !programs}
  <div class="skeleton" aria-busy="true">
    {#each { length: SKELETON_ROWS }, i (i)}
      <div class="bar"></div>
    {/each}
  </div>
{:else if visible.length === 0}
  <EmptyState title={fr.programs.noResults.title} text={fr.programs.noResults.text} />
{:else}
  <VirtualList items={visible} key={(p) => p.id} label={fr.programs.list}>
    {#snippet row(p: Program)}
      <div class="program">
        <div class="names">
          <span class="name">{p.name}</span>
          {#if p.publisher}<span class="publisher">{p.publisher}</span>{/if}
        </div>
        <span class="size num">
          {p.estimatedSizeKib != null ? formatSize(p.estimatedSizeKib) : ""}
        </span>
        <span class="date num">{p.installDate ? formatDate(p.installDate) : ""}</span>
        <span class="badge" title={fr.programs.scopeHint[p.scope]}>
          {fr.programs.scope[p.scope]}
        </span>
      </div>
    {/snippet}
  </VirtualList>
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
  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin-bottom: var(--space-3);
  }
  .search {
    flex: 1;
    max-width: 44ch;
    padding: var(--space-2) var(--space-4);
    border: none;
    border-radius: var(--radius-pill);
    background: var(--fill-glass);
  }
  .search::placeholder {
    color: var(--color-text-muted);
  }
  .sort {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  select {
    padding: var(--space-1) var(--space-2);
    border: none;
    border-radius: var(--radius-sm);
    background: var(--fill-glass);
    color: var(--color-text);
  }
  option {
    background: var(--color-elevated);
  }
  .retry {
    padding: var(--space-2) var(--space-4);
    border: none;
    border-radius: var(--radius-pill);
    background: var(--color-primary);
    color: var(--color-on-primary);
    cursor: pointer;
  }
  .program {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 10ch 12ch 11ch;
    align-items: center;
    gap: var(--space-4);
    height: 100%;
    padding: 0 var(--space-3);
    border-radius: var(--radius-sm);
    transition: background var(--motion-fast);
  }
  .program:hover {
    background: var(--fill-hover);
  }
  .names {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name,
  .publisher {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .publisher {
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  .size,
  .date {
    color: var(--color-text-secondary);
    text-align: right;
  }
  .badge {
    justify-self: end;
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--fill-glass);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
  }
  .skeleton {
    display: flex;
    flex-direction: column;
  }
  .bar {
    height: var(--row-height);
    border-bottom: var(--space-2) solid var(--color-bg);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
  }
</style>
