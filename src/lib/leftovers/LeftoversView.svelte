<script lang="ts">
import { onMount } from "svelte";
import { fr } from "../i18n/fr";
import {
  commands,
  type Found,
  type LeftoverFailure,
  type Program,
} from "../ipc/bindings";
import EmptyState from "../ui/EmptyState.svelte";
import {
  check,
  count,
  groups,
  safeIds,
  TABS,
  type Tab,
  toggle,
  toggleAll,
} from "./tree";

interface Props {
  program: Program;
  onback: () => void;
}

let { program, onback }: Props = $props();

const t = fr.leftovers;
const TAB_KEYS = Object.keys(TABS) as Tab[];

type Scan =
  | { state: "scanning" }
  | { state: "done"; found: Found }
  | { state: "failed"; failure: LeftoverFailure };

let scan = $state<Scan>({ state: "scanning" });
let tab = $state<Tab>("files");
let selected = $state<Set<string>>(new Set());
/** Answers of an earlier scan (retried, or left) are ignored. */
let run = 0;

const found = $derived(scan.state === "done" ? scan.found : null);
const total = $derived(
  found ? TAB_KEYS.reduce((n, key) => n + count(found, key), 0) : 0,
);
const shown = $derived(found ? groups(found, tab) : []);

function start() {
  const current = ++run;
  scan = { state: "scanning" };
  commands.scanLeftovers(program.id).then(
    (result) => {
      if (current !== run) return;
      if (result.status === "error") {
        scan = { state: "failed", failure: result.error };
        return;
      }
      scan = { state: "done", found: result.data };
      selected = safeIds(result.data);
      tab = count(result.data, "files") ? "files" : "registry";
    },
    () => {
      if (current === run) scan = { state: "failed", failure: "failed" };
    },
  );
}

function cancel() {
  commands.cancelLeftovers().catch(() => {});
}

function onkeydown(event: KeyboardEvent) {
  if (event.code !== "Escape" || event.defaultPrevented) return;
  event.preventDefault();
  onback();
}

onMount(() => {
  start();
  return () => {
    run++;
    if (scan.state === "scanning") cancel();
  };
});
</script>

<svelte:window {onkeydown} />

<header class="page-head">
  <button class="back" aria-label={t.back} title={t.back} onclick={onback}>
    <span aria-hidden="true">&#xE72B;</span>
  </button>
  <h1>{t.title}</h1>
  <p class="summary">{program.name}</p>
  {#if total}
    <p class="summary num">{t.found(total)}</p>
  {/if}
</header>

{#if scan.state === "scanning"}
  <div role="status" aria-busy="true">
    <EmptyState title={t.scanning} text={t.scanningHint} onretry={cancel} action={t.cancel} />
  </div>
{:else if scan.state === "failed"}
  {@const failure = t.failure[scan.failure]}
  <EmptyState
    title={failure.title}
    text={failure.text}
    onretry={scan.failure === "notFound" ? undefined : start}
  />
{:else if !found || total === 0}
  <EmptyState title={t.empty.title} text={t.empty.text} />
{:else}
  <div class="tools">
    {#each TAB_KEYS as key (key)}
      <button class="toggle" aria-pressed={tab === key} onclick={() => (tab = key)}>
        {t.tabs[key]} <span class="num">{count(found, key)}</span>
      </button>
    {/each}
    <button class="toggle" onclick={() => (selected = safeIds(found))}>
      {t.selectSafe}
    </button>
    <p class="summary chosen num" role="status">{t.selected(selected.size)}</p>
  </div>

  <div class="groups">
    {#each shown as group (group.key)}
      {@const box = check(group.items, selected)}
      <section aria-label={t.group[group.key]}>
        <label class="group-head">
          <input
            type="checkbox"
            aria-label={t.selectGroup(t.group[group.key])}
            checked={box === "all"}
            indeterminate={box === "some"}
            onchange={() => (selected = toggleAll(group.items, selected))}
          />
          <span class="group-name">{t.group[group.key]}</span>
          <span class="num">{group.items.length}</span>
        </label>
        <ul>
          {#each group.items as item (item.id)}
            <li>
              <label class="row" title={t.confidenceHint[item.confidence]}>
                <input
                  type="checkbox"
                  checked={selected.has(item.id)}
                  onchange={() => (selected = toggle(item.id, selected))}
                />
                <span class="label" class:safe={item.confidence === "safe"}>
                  {item.label}
                </span>
                <span class="confidence">{t.confidence[item.confidence]}</span>
              </label>
            </li>
          {/each}
        </ul>
      </section>
    {:else}
      <p class="tab-empty">{t.tabEmpty}</p>
    {/each}
  </div>
{/if}

<style>
  .back {
    align-self: center;
    padding: var(--space-2);
    border: none;
    border-radius: var(--radius-pill);
    background: var(--fill-glass);
    color: var(--color-text);
    font-family: var(--font-icons);
    cursor: pointer;
    transition: background var(--motion-fast);
  }
  .back:hover {
    background: var(--fill-glass-hover);
  }
  .back:active {
    background: var(--fill-pressed);
  }
  .summary {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chosen {
    margin-left: auto;
    color: var(--color-text);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    margin-bottom: var(--space-4);
  }
  .groups {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--space-6);
    min-height: 0;
    padding-bottom: var(--space-4);
    overflow-y: auto;
  }
  input {
    flex: none;
    margin: 0;
    accent-color: var(--color-accent);
  }
  .group-head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin: 0 0 var(--space-2);
    padding: 0 var(--space-4);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    cursor: pointer;
  }
  .group-name {
    font-weight: 500;
  }
  .group-head .num {
    color: var(--color-text-muted);
  }
  ul {
    margin: 0;
    padding: 0;
    border-radius: var(--radius-md);
    background: var(--color-surface);
    list-style: none;
  }
  li + li {
    border-top: 1px solid var(--separator);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: var(--row-height);
    padding: var(--space-2) var(--space-4);
    cursor: pointer;
  }
  .row:hover {
    background: var(--fill-pressed);
  }
  li:first-child .row {
    border-radius: var(--radius-md) var(--radius-md) 0 0;
  }
  li:last-child .row {
    border-radius: 0 0 var(--radius-md) var(--radius-md);
  }
  li:only-child .row {
    border-radius: var(--radius-md);
  }
  .label {
    flex: 1;
    min-width: 0;
    color: var(--color-text-secondary);
    overflow-wrap: anywhere;
    user-select: text;
  }
  .label.safe {
    color: var(--color-text);
    font-weight: 600;
  }
  .confidence {
    flex: none;
    color: var(--color-text-muted);
    font-size: var(--font-size-sm);
  }
  .tab-empty {
    margin: var(--space-8) auto;
    color: var(--color-text-secondary);
  }
</style>
