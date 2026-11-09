<script lang="ts">
import { Channel } from "@tauri-apps/api/core";
import { onMount, tick } from "svelte";
import { SvelteMap, SvelteSet } from "svelte/reactivity";
import { formatDate, formatSize, isoDate } from "../format";
import { fr } from "../i18n/fr";
import {
  commands,
  type ExportFormat,
  type Program,
  type ProgramSize,
} from "../ipc/bindings";
import EmptyState from "../ui/EmptyState.svelte";
import { EMPTY_SELECTION, type Selection } from "../ui/listNav";
import VirtualList from "../ui/VirtualList.svelte";
import AppIcon from "./AppIcon.svelte";
import {
  EXPORT_FORMATS,
  exportContent,
  type ListDocument,
  listDocument,
} from "./export";
import PrintSheet from "./PrintSheet.svelte";
import ProgramDetails from "./ProgramDetails.svelte";
import {
  COLUMNS,
  type Column,
  DEFAULT_COLUMNS,
  FILTERS,
  type Filter,
  type GroupKey,
  groupLabel,
  isDescending,
  isRecent,
  OPTIONAL_KINDS,
  type OptionalKind,
  RECENT_DAYS,
  recentSince,
  SORT_KEYS,
  type SortKey,
  scopeLabel,
  sizeKib,
  totalSizeKib,
  visiblePrograms,
} from "./view";

const GROUP_KEYS: GroupKey[] = ["none", "publisher", "date", "size"];
/** Détails: a row with columns; Icônes and Liste: a grid of named icons. */
type View = keyof typeof fr.programs.views;
const VIEWS: View[] = ["details", "icons", "list"];
/** Grid track of each optional column. */
const COLUMN_WIDTH: Record<Column, string> = {
  version: "14ch",
  size: "10ch",
  date: "12ch",
  scope: "11ch",
};
const FILTER_HINT: Record<Filter, string> = {
  recent: fr.programs.recentHint(RECENT_DAYS),
  orphan: fr.programs.orphanHint,
};
/** Placeholder rows while the inventory loads. */
const SKELETON_ROWS = 8;

let programs = $state<Program[] | null>(null);
let failed = $state(false);
let query = $state("");
let sort = $state<SortKey>("name");
let reversed = $state(false);
let group = $state<GroupKey>("none");
let view = $state<View>("details");
let selection = $state<Selection>(EMPTY_SELECTION);
/** Sizes measured on disk, streamed in after the list is shown. */
const sizes = new SvelteMap<string, number>();
let sizing = $state(false);
/** Bumped by each load, so a stale stream from before a retry is ignored. */
let generation = 0;
/** The first list shown was reported (startup measurement, docs/perf.md). */
let reported = false;
/** System components and updates the user chose to show (hidden by default, F1). */
const shown = new SvelteSet<OptionalKind>();
const filters = new SvelteSet<Filter>();
/** Fixed for the session: recent means recent when the screen opened. */
const since = recentSince(new Date());

/** A column heading sorts by its key; clicking it again reverses the order. */
function sortBy(key: SortKey) {
  reversed = key === sort && !reversed;
  sort = key;
}

function toggle<T>(set: SvelteSet<T>, value: T) {
  if (!set.delete(value)) set.add(value);
}
const columnSet = new SvelteSet<Column>(DEFAULT_COLUMNS);
/** Chosen columns in their fixed order. */
const columns = $derived(COLUMNS.filter((c) => columnSet.has(c)));
const tracks = $derived(
  [
    "var(--icon-size)",
    "minmax(0, 1fr)",
    ...columns.map((c) => COLUMN_WIDTH[c]),
  ].join(" "),
);
/** Outcome of the last export, until the next one. */
let exported = $state<{ text: string; failed: boolean } | null>(null);

/** Saves the list as shown (filters, search, order) where the user chooses. */
async function exportAs(format: ExportFormat) {
  const t = fr.programs.export;
  const now = new Date();
  const result = await commands.saveExport({
    format,
    content: exportContent(format, visible, sizes, now),
    dialogTitle: t.dialogTitle,
    fileName: t.fileName(isoDate(now)),
    typeName: `${t.formats[format]} (*.${format})`,
  });
  if (result.status === "error") exported = { text: t.failed, failed: true };
  else if (result.data)
    exported = { text: t.saved(result.data), failed: false };
}
/** The sheet laid out for printing, until the print dialog closes. */
let printing = $state<ListDocument | null>(null);

/** Prints the list as shown, through the system print dialog. */
async function print() {
  printing = listDocument(
    visible,
    sizes,
    new Date(),
    fr.programs.export.printedOn,
  );
  await tick();
  window.print();
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
    ? visiblePrograms(programs, {
        query,
        sort,
        reversed,
        group,
        sizes,
        shown,
        filters,
        recentSince: since,
      })
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
      reportShown();
    },
    () => (failed = true),
  );
}

/** Once rendered and painted (a timeout queued from a frame callback runs after
 * that frame's paint), with the time since the page started loading. */
function reportShown() {
  if (reported) return;
  reported = true;
  tick().then(() =>
    requestAnimationFrame(() =>
      setTimeout(() => commands.listShown(Math.round(performance.now()))),
    ),
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

<svelte:window onafterprint={() => (printing = null)} />

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
    {#if exported}
      <p class="summary exported" class:failed={exported.failed} role="status">
        {exported.text}
      </p>
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
    <select bind:value={sort} onchange={() => (reversed = false)}>
      {#each SORT_KEYS as key (key)}
        <option value={key}>{fr.programs.field[key]}</option>
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
  <label class="sort">
    {fr.programs.viewAs}
    <select bind:value={view}>
      {#each VIEWS as key (key)}
        <option value={key}>{fr.programs.views[key]}</option>
      {/each}
    </select>
  </label>
  <button
    class="menu-button"
    style:anchor-name="--columns"
    popovertarget="columns-menu"
    disabled={view !== "details"}
    title={view !== "details" ? fr.programs.columnsDetailsOnly : undefined}
  >
    {fr.programs.columns}
  </button>
  <div class="menu" id="columns-menu" style:position-anchor="--columns" popover>
    {#each COLUMNS as column (column)}
      <label class="menu-item">
        <input
          type="checkbox"
          checked={columnSet.has(column)}
          onchange={() => toggle(columnSet, column)}
        />
        {fr.programs.field[column]}
      </label>
    {/each}
  </div>
  <button class="menu-button" style:anchor-name="--export" popovertarget="export-menu">
    {fr.programs.export.button}
  </button>
  <div class="menu" id="export-menu" style:position-anchor="--export" popover>
    {#each EXPORT_FORMATS as format (format)}
      <button
        class="menu-item"
        popovertarget="export-menu"
        popovertargetaction="hide"
        onclick={() => exportAs(format)}
      >
        {fr.programs.export.formats[format]}
      </button>
    {/each}
    <button
      class="menu-item"
      popovertarget="export-menu"
      popovertargetaction="hide"
      onclick={print}
    >
      {fr.programs.export.print}
    </button>
  </div>
  <div class="toggles push" role="group" aria-label={fr.programs.filter}>
    {#each FILTERS as filter (filter)}
      <button
        class="toggle"
        aria-pressed={filters.has(filter)}
        title={FILTER_HINT[filter]}
        onclick={() => toggle(filters, filter)}
      >
        {fr.programs.filters[filter]}
      </button>
    {/each}
  </div>
  <div class="toggles" role="group" aria-label={fr.programs.show}>
    {#each OPTIONAL_KINDS as kind (kind)}
      <button class="toggle" aria-pressed={shown.has(kind)} onclick={() => toggle(shown, kind)}>
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
  {#if view === "details"}
  <div class="columns-head" style:grid-template-columns={tracks}>
    <span></span>
    {#each ["name", ...columns] as SortKey[] as key (key)}
      {@const field = fr.programs.field[key]}
      <button
        class="column-head"
        class:end={key !== "name"}
        aria-pressed={sort === key}
        title={fr.programs.sortHint(field)}
        onclick={() => sortBy(key)}
      >
        <span class="head-label">{fr.programs.columnHead[key] ?? field}</span>
        {#if sort === key}
          <span aria-hidden="true">{isDescending(sort, reversed) ? "↓" : "↑"}</span>
        {/if}
      </button>
    {/each}
  </div>
  {/if}
  <!-- Remounted per view: the list reads its cell size from the view's tokens. -->
  {#key view}
  <div class="list-host {view}">
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
      {#if view === "details"}
        {@render detailsRow(p)}
      {:else}
        <div class="cell-item" title={p.orphan ? `${p.name} · ${fr.programs.orphan}` : p.name}>
          <AppIcon program={p} />
          <span class="cell-name" class:orphan={p.orphan}>{p.name}</span>
        </div>
      {/if}
    {/snippet}
  </VirtualList>
  </div>
  {/key}
{/if}
</div>
{#snippet detailsRow(p: Program)}
      {@const size = sizeKib(p, sizes)}
      {@const tags = [p.kind !== "app" ? fr.programs.kind[p.kind] : null, p.publisher]
        .filter(Boolean)
        .join(" · ")}
      <div class="program" style:grid-template-columns={tracks}>
        <AppIcon program={p} />
        <div class="names">
          <span class="name">{p.name}</span>
          {#if p.orphan || tags}
            <span class="publisher">
              {#if p.orphan}
                <span class="orphan" title={fr.programs.orphanHint}>{fr.programs.orphan}</span>
                {#if tags}·{/if}
              {/if}
              {tags}
            </span>
          {/if}
        </div>
        {#each columns as column (column)}
          {#if column === "version"}
            <span class="cell">{p.version ?? ""}</span>
          {:else if column === "size"}
            <span class="cell num">{size != null ? formatSize(size) : ""}</span>
          {:else if column === "date"}
            <span class="cell num" class:recent={isRecent(p, since)}>
              {p.installDate ? formatDate(p.installDate) : ""}
            </span>
          {:else}
            <span class="badge" title={fr.programs.scopeHint[p.scope]}>
              {scopeLabel(p)}
            </span>
          {/if}
        {/each}
      </div>
{/snippet}
{#if details}
  <ProgramDetails program={details} diskSizeKib={sizes.get(details.id)} onclose={closeDetails} />
{/if}
{#if printing}
  <PrintSheet doc={printing} />
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
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-4);
    margin-bottom: var(--space-3);
  }
  .search {
    flex: 1;
    min-width: 20ch;
    max-width: 44ch;
    padding: var(--space-2) var(--space-4);
    border: none;
    border-radius: var(--radius-pill);
    background: var(--fill-glass);
  }
  .search::placeholder {
    color: var(--color-text-muted);
  }
  .sort,
  .menu-button,
  .menu-item,
  .toggle {
    white-space: nowrap;
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
  .toggles {
    display: flex;
    gap: var(--space-2);
  }
  .push {
    margin-left: auto;
  }
  .toggle {
    padding: var(--space-1) var(--space-3);
    border: none;
    border-radius: var(--radius-pill);
    background: var(--fill-glass);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }
  .toggle:hover {
    background: var(--fill-hover);
  }
  .toggle[aria-pressed="true"] {
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
  .list-host {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }
  .icons {
    --row-height: var(--tile-height);
    --cell-width: var(--tile-width);
    --icon-size: var(--icon-size-large);
  }
  .list {
    --row-height: var(--row-height-compact);
    --cell-width: var(--list-cell-width);
    --icon-size: var(--icon-size-small);
  }
  .cell-item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 100%;
    padding: 0 var(--space-3);
  }
  .icons .cell-item {
    flex-direction: column;
    justify-content: center;
    padding: var(--space-2);
    text-align: center;
  }
  .cell-name {
    overflow: hidden;
    min-width: 0;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .icons .cell-name {
    display: -webkit-box;
    width: 100%;
    font-size: var(--font-size-sm);
    white-space: normal;
    overflow-wrap: anywhere;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }
  .program {
    display: grid;
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
  .cell {
    overflow: hidden;
    color: var(--color-text-secondary);
    text-align: right;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .recent {
    color: var(--color-accent);
  }
  .orphan {
    color: var(--color-danger);
  }
  .columns-head {
    display: grid;
    gap: var(--space-4);
    padding: 0 var(--space-3) var(--space-1);
    /* Same width as the list below, whose scrollbar keeps its gutter. */
    overflow: hidden;
    scrollbar-gutter: stable;
  }
  .column-head {
    display: flex;
    gap: var(--space-1);
    min-width: 0;
    padding: 0;
    border: none;
    background: none;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }
  .column-head.end {
    justify-content: flex-end;
  }
  .column-head:hover,
  .column-head[aria-pressed="true"] {
    color: var(--color-text);
  }
  .head-label {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .exported {
    overflow: hidden;
    min-width: 0;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .failed {
    color: var(--color-danger);
  }
  .menu-button {
    padding: var(--space-1) var(--space-3);
    border: none;
    border-radius: var(--radius-pill);
    background: var(--fill-glass);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }
  .menu-button:hover:enabled {
    background: var(--fill-hover);
  }
  .menu-button:disabled {
    color: var(--color-text-muted);
    cursor: default;
  }
  /* Anchored to its button (`position-anchor` set on each menu). */
  .menu {
    inset: auto;
    top: anchor(bottom);
    left: anchor(left);
    /* Near the window's right edge, align to the button's right instead. */
    position-try-fallbacks: flip-inline;
    margin: var(--space-2) 0 0;
    padding: var(--space-2);
    border: none;
    border-radius: var(--radius-md);
    background: var(--material-thick);
    backdrop-filter: var(--material-blur);
    box-shadow: var(--shadow-lift);
    color: var(--color-text);
  }
  .menu-item {
    display: flex;
    width: 100%;
    border: none;
    background: none;
    color: inherit;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }
  .menu-item:hover {
    background: var(--fill-hover);
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
