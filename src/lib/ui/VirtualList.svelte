<script lang="ts" generics="T">
import { onMount, type Snippet } from "svelte";
import {
  EMPTY_SELECTION,
  layoutLines,
  nextIndex,
  pick,
  pickMode,
  type Selection,
} from "./listNav";

interface Props {
  items: readonly T[];
  key: (item: T) => string;
  row: Snippet<[T]>;
  label: string;
  /** Follows clicks (Ctrl, Shift) and arrow keys (Shift); Ctrl+A selects all. */
  selection?: Selection;
  /** Space or double-click on a row. */
  onopen?: (item: T) => void;
  /** Heading of the item's group: a heading row starts each run of equal headings. */
  group?: (item: T) => string | null;
}

let {
  items,
  key,
  row,
  label,
  selection = $bindable(EMPTY_SELECTION),
  onopen,
  group,
}: Props = $props();

/** Rows rendered above and below the viewport, so fast scrolling shows no gap. */
const OVERSCAN = 6;
const uid = $props.id();

let viewport: HTMLDivElement;
let scrollTop = $state(0);
let height = $state(0);
let width = $state(0);
// Read from the CSS tokens once mounted, so they stay the single source: a
// container sets --row-height, and --cell-width to lay items out as a grid.
let rowHeight = $state(0);
let cellWidth = $state(0);

onMount(() => {
  const style = getComputedStyle(viewport);
  const token = (name: string) =>
    Number.parseFloat(style.getPropertyValue(name)) || 0;
  rowHeight = token("--row-height");
  cellWidth = token("--cell-width");
});

/** Items per line: as many cells as fit, one without a cell width. */
const columns = $derived(
  cellWidth ? Math.max(1, Math.floor(width / cellWidth)) : 1,
);
const layout = $derived(
  layoutLines(
    items.map((item) => group?.(item) ?? null),
    columns,
  ),
);

const start = $derived(
  rowHeight ? Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN) : 0,
);
const end = $derived(
  rowHeight
    ? Math.min(
        layout.lines.length,
        Math.ceil((scrollTop + height) / rowHeight) + OVERSCAN,
      )
    : 0,
);
const order = $derived(items.map(key));
const activeIndex = $derived(
  selection.active == null ? -1 : order.indexOf(selection.active),
);

/** Gives the keyboard back to the list, e.g. when a side panel closes. */
export function focus() {
  viewport.focus();
}

const rowId = (index: number) => `${uid}-${index}`;

/** Scrolls just enough to show the item's row, like a native list. */
function reveal(index: number) {
  const top = (layout.lineOf[index] ?? index) * rowHeight;
  if (top < viewport.scrollTop) viewport.scrollTop = top;
  else if (top + rowHeight > viewport.scrollTop + height)
    viewport.scrollTop = top + rowHeight - height;
}

function onkeydown(event: KeyboardEvent) {
  const current = activeIndex;
  if (event.code === "KeyA" && event.ctrlKey) {
    event.preventDefault();
    selection = { ...selection, keys: new Set(order) };
    return;
  }
  if (event.code === "Space" && current >= 0) {
    event.preventDefault();
    onopen?.(items[current] as T);
    return;
  }
  const page = Math.max(1, Math.floor(height / rowHeight) - 1);
  const next = nextIndex(event.code, current, layout, columns, page);
  if (next === null) return;
  event.preventDefault();
  // nextIndex stays within the list.
  selection = pick(order, selection, order[next] as string, pickMode(event));
  reveal(next);
}
</script>

<div
  class="viewport"
  role="listbox"
  tabindex="0"
  aria-label={label}
  aria-multiselectable="true"
  aria-activedescendant={activeIndex >= 0 ? rowId(activeIndex) : undefined}
  bind:this={viewport}
  bind:clientHeight={height}
  bind:clientWidth={width}
  onscroll={() => (scrollTop = viewport.scrollTop)}
  {onkeydown}
>
  <div class="spacer" style:height="{layout.lines.length * rowHeight}px">
    {#each layout.lines.slice(start, end) as line, i ("items" in line ? key(items[line.items[0] ?? 0] as T) : `heading:${line.heading}`)}
      {@const top = (start + i) * rowHeight}
      {#if "items" in line}
        {#each line.items as index, column (key(items[index] as T))}
          {@const item = items[index] as T}
          <!-- Options are not focusable: the listbox keeps focus and points at the
               selected row (aria-activedescendant), which also handles the keyboard. -->
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
          <div
            class="row"
            id={rowId(index)}
            role="option"
            aria-selected={selection.keys.has(key(item))}
            aria-setsize={items.length}
            aria-posinset={index + 1}
            style:width="{100 / columns}%"
            style:transform="translate({column * 100}%, {top}px)"
            onclick={(event) =>
              (selection = pick(order, selection, key(item), pickMode(event)))}
            ondblclick={() => onopen?.(item)}
          >
            {@render row(item)}
          </div>
        {/each}
      {:else}
        <div class="heading" role="presentation" style:transform="translateY({top}px)">
          {line.heading}<span class="count num">{line.count}</span>
        </div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    /* A header above can line up with the rows whether or not there is a scrollbar. */
    scrollbar-gutter: stable;
    border-radius: var(--radius-sm);
    scrollbar-color: var(--fill-glass) transparent;
  }
  .viewport:focus-visible {
    outline-offset: -2px;
  }
  .spacer {
    position: relative;
  }
  .row {
    position: absolute;
    inset: 0 auto auto 0;
    height: var(--row-height);
    border-radius: var(--radius-sm);
    transition: background var(--motion-fast);
  }
  .heading {
    position: absolute;
    inset: 0 0 auto;
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
    height: var(--row-height);
    padding: 0 var(--space-3) var(--space-2);
    color: var(--color-text-secondary);
    font-size: var(--font-size-sm);
    font-weight: 600;
  }
  .count {
    color: var(--color-text-muted);
    font-weight: normal;
  }
  .row:hover {
    background: var(--fill-hover);
  }
  .row[aria-selected="true"] {
    background: var(--fill-selected);
  }
</style>
