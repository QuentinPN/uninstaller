<script lang="ts" generics="T">
import { onMount, type Snippet } from "svelte";
import {
  EMPTY_SELECTION,
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

/** A rendered line: a group heading (with its item count) or an item. */
type Slot = { heading: string; count: number } | { index: number };

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
// Read from --row-height once mounted: the CSS token stays the single source.
let rowHeight = $state(0);

onMount(() => {
  rowHeight = Number.parseFloat(
    getComputedStyle(viewport).getPropertyValue("--row-height"),
  );
});

/** Lines in display order, and the line of each item (headings shift items down). */
const layout = $derived.by(() => {
  const slots: Slot[] = [];
  const lineOf: number[] = [];
  let current: { heading: string; count: number } | null = null;
  items.forEach((item, index) => {
    const heading = group?.(item) ?? null;
    if (heading !== null && heading !== current?.heading) {
      current = { heading, count: 0 };
      slots.push(current);
    }
    if (current && heading !== null) current.count++;
    lineOf.push(slots.length);
    slots.push({ index });
  });
  return { slots, lineOf };
});

const start = $derived(
  rowHeight ? Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN) : 0,
);
const end = $derived(
  rowHeight
    ? Math.min(
        layout.slots.length,
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
  const next = nextIndex(event.code, current, items.length, page);
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
  onscroll={() => (scrollTop = viewport.scrollTop)}
  {onkeydown}
>
  <div class="spacer" style:height="{layout.slots.length * rowHeight}px">
    {#each layout.slots.slice(start, end) as slot, i ("index" in slot ? key(items[slot.index] as T) : `heading:${slot.heading}`)}
      {#if "index" in slot}
        {@const item = items[slot.index] as T}
        <!-- Options are not focusable: the listbox keeps focus and points at the
             selected row (aria-activedescendant), which also handles the keyboard. -->
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
        <div
          class="row"
          id={rowId(slot.index)}
          role="option"
          aria-selected={selection.keys.has(key(item))}
          aria-setsize={items.length}
          aria-posinset={slot.index + 1}
          style:transform="translateY({(start + i) * rowHeight}px)"
          onclick={(event) =>
            (selection = pick(order, selection, key(item), pickMode(event)))}
          ondblclick={() => onopen?.(item)}
        >
          {@render row(item)}
        </div>
      {:else}
        <div
          class="heading"
          role="presentation"
          style:transform="translateY({(start + i) * rowHeight}px)"
        >
          {slot.heading}<span class="count num">{slot.count}</span>
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
    inset: 0 0 auto;
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
