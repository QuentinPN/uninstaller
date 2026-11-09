<script lang="ts">
import { Channel } from "@tauri-apps/api/core";
import { onMount } from "svelte";
import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { formatDate, formatSize } from "../format";
import { fr } from "../i18n/fr";
import { commands, type Program, type ProgramSize } from "../ipc/bindings";
import EmptyState from "../ui/EmptyState.svelte";
import { EMPTY_SELECTION, type Selection } from "../ui/listNav";
import VirtualList from "../ui/VirtualList.svelte";
import AppIcon from "./AppIcon.svelte";
import ProgramDetails from "./ProgramDetails.svelte";
import {
  type GroupKey,
  groupLabel,
  OPTIONAL_KINDS,
  type OptionalKind,
  type SortKey,
  sizeKib,
  totalSizeKib,
  visiblePrograms,
} from "./view";

const SORT_KEYS: SortKey[] = ["name", "size", "date"];
const GROUP_KEYS: GroupKey[] = ["none", "publisher", "date", "size"];
/** Placeholder rows while the inventory loads. */
const SKELETON_ROWS = 8;

let programs = $state<Program[] | null>(null);
let failed = $state(false);
let query = $state("");
let sort = $state<SortKey>("name");
let group = $state<GroupKey>("none");
let selection = $state<Selection>(EMPTY_SELECTION);
/** Sizes measured on disk, streamed in after the list is shown. */
const sizes = new SvelteMap<string, number>();
let sizing = $state(false);
/** Bumped by each load, so a stale stream from before a retry is ignored. */
let generation = 0;
/** System components and updates the user chose to show (hidden by default, F1). */
const shown = new SvelteSet<OptionalKind>();

function toggle(kind: OptionalKind) {
  if (!shown.delete(kind)) shown.add(kind);
}
let detailsOpen = $state(false);
let list = $state<VirtualList<Program>>();

const details = $derived(
  detailsOpen ? programs?.find((p) => p.id === selection.active) : undefined,
);

/** Shown in the header once more than one row is selected. */
const chosen = $derived(
  selection.keys.size > 1
    ? (programs ?? []).filter((p) => selection.keys.has(p.id))
    : [],
);

const visible = $derived(
  programs
    ? visiblePrograms(programs, { query, sort, group, sizes, shown })
    : [],
);

function load() {
  const current = ++generation;
  programs = null;
  failed = false;
  sizes.clear();
  commands.listPrograms().then(
    (list) => {
      programs = list;
      measure(current);
    },
    () => (failed = true),
  );
}

function measure(current: number) {
  const onSizes = new Channel<ProgramSize[]>();
  onSizes.onmessage = (batch) => {
    if (current !== generation) return;
    for (const { id, sizeKib } of batch) sizes.set(id, sizeKib);
  };
  sizing = true;
  // Without sizes the list still works on the declared ones: nothing to report.
  commands
    .computeSizes(onSizes)
    .catch(() => {})
    .finally(() => {
      if (current === generation) sizing = false;
    });
}

function closeDetails() {
  detailsOpen = false;
  list?.focus();
}

function onkeydown(event: KeyboardEvent) {
  if (event.code !== "Escape" || !detailsOpen) return;
  event.preventDefault();
  closeDetails();
}

onMount(load);
</script>

<header class="head">
  <h1>{fr.nav.programs}</h1>
  {#if programs}
    <p class="summary num">
      {fr.programs.count(visible.length)} · {formatSize(totalSizeKib(visible, sizes))}
    </p>
    {#if chosen.length}
      <p class="summary chosen num">
        {fr.programs.selected(chosen.length)} · {formatSize(totalSizeKib(chosen, sizes))}
      </p>
    {/if}
    {#if sizing}
      <p class="summary" role="status">{fr.programs.sizing}</p>
    {/if}
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
  <label class="sort">
    {fr.programs.groupBy}
    <select bind:value={group}>
      {#each GROUP_KEYS as key (key)}
        <option value={key}>{fr.programs.group[key]}</option>
      {/each}
    </select>
  </label>
  <div class="kinds" role="group" aria-label={fr.programs.show}>
    {#each OPTIONAL_KINDS as kind (kind)}
      <button class="kind-toggle" aria-pressed={shown.has(kind)} onclick={() => toggle(kind)}>
        {fr.programs.kinds[kind]}
      </button>
    {/each}
  </div>
</div>

<!-- Escape bubbles up from the list or the panel. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="body" {onkeydown}>
<div class="content">
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
  <VirtualList
    bind:this={list}
    bind:selection
    items={visible}
    key={(p) => p.id}
    label={fr.programs.list}
    onopen={() => (detailsOpen = true)}
    group={(p) => groupLabel(p, group, sizes)}
  >
    {#snippet row(p: Program)}
      {@const size = sizeKib(p, sizes)}
      <div class="program">
        <AppIcon program={p} />
        <div class="names">
          <span class="name">{p.name}</span>
          {#if p.kind !== "app" || p.publisher}
            <span class="publisher">
              {[p.kind !== "app" ? fr.programs.kind[p.kind] : null, p.publisher]
                .filter(Boolean)
                .join(" · ")}
            </span>
          {/if}
        </div>
        <span class="size num">
          {size != null ? formatSize(size) : ""}
        </span>
        <span class="date num">{p.installDate ? formatDate(p.installDate) : ""}</span>
        <span class="badge" title={fr.programs.scopeHint[p.scope]}>
          {p.profile ?? fr.programs.scope[p.scope]}
        </span>
      </div>
    {/snippet}
  </VirtualList>
{/if}
</div>
{#if details}
  <ProgramDetails program={details} diskSizeKib={sizes.get(details.id)} onclose={closeDetails} />
{/if}
</div>

<style>
  .body {
    display: flex;
    flex: 1;
    gap: var(--space-4);
    min-height: 0;
  }
  .content {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
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
  .chosen {
    color: var(--color-accent);
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
  .kinds {
    display: flex;
    gap: var(--space-2);
    margin-left: auto;
  }
  .kind-toggle {
    padding: var(--space-1) var(--space-3);
    border: none;
    border-radius: var(--radius-pill);
    background: var(--fill-glass);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }
  .kind-toggle:hover {
    background: var(--fill-hover);
  }
  .kind-toggle[aria-pressed="true"] {
    background: var(--color-primary);
    color: var(--color-on-primary);
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
    grid-template-columns: var(--icon-size) minmax(0, 1fr) 10ch 12ch 11ch;
    align-items: center;
    gap: var(--space-4);
    height: 100%;
    padding: 0 var(--space-3);
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
