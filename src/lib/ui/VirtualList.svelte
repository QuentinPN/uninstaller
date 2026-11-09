<script lang="ts" generics="T">
import { onMount, type Snippet } from "svelte";
import { nextIndex } from "./listNav";

interface Props {
  items: readonly T[];
  key: (item: T) => string;
  row: Snippet<[T]>;
  label: string;
  /** Key of the selected row; follows clicks and arrow keys. */
  selected?: string | null;
  /** Space or double-click on a row. */
  onopen?: (item: T) => void;
}

let {
  items,
  key,
  row,
  label,
  selected = $bindable(null),
  onopen,
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

const start = $derived(
  rowHeight ? Math.max(0, Math.floor(scrollTop / rowHeight) - OVERSCAN) : 0,
);
const end = $derived(
  rowHeight
    ? Math.min(
        items.length,
        Math.ceil((scrollTop + height) / rowHeight) + OVERSCAN,
      )
    : 0,
);
const selectedIndex = $derived(
  selected == null ? -1 : items.findIndex((item) => key(item) === selected),
);

/** Gives the keyboard back to the list, e.g. when a side panel closes. */
export function focus() {
  viewport.focus();
}

const rowId = (index: number) => `${uid}-${index}`;

/** Scrolls just enough to show the row, like a native list. */
function reveal(index: number) {
  const top = index * rowHeight;
  if (top < viewport.scrollTop) viewport.scrollTop = top;
  else if (top + rowHeight > viewport.scrollTop + height)
    viewport.scrollTop = top + rowHeight - height;
}

function onkeydown(event: KeyboardEvent) {
  const current = selectedIndex;
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
  selected = key(items[next] as T);
  reveal(next);
}
</script>

<div
  class="viewport"
  role="listbox"
  tabindex="0"
  aria-label={label}
  aria-activedescendant={selectedIndex >= 0 ? rowId(selectedIndex) : undefined}
  bind:this={viewport}
  bind:clientHeight={height}
  onscroll={() => (scrollTop = viewport.scrollTop)}
  {onkeydown}
>
  <div class="spacer" style:height="{items.length * rowHeight}px">
    {#each items.slice(start, end) as item, i (key(item))}
      <!-- Options are not focusable: the listbox keeps focus and points at the
           selected row (aria-activedescendant), which also handles the keyboard. -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
      <div
        class="row"
        id={rowId(start + i)}
        role="option"
        aria-selected={start + i === selectedIndex}
        aria-setsize={items.length}
        aria-posinset={start + i + 1}
        style:transform="translateY({(start + i) * rowHeight}px)"
        onclick={() => (selected = key(item))}
        ondblclick={() => onopen?.(item)}
      >
        {@render row(item)}
      </div>
    {/each}
  </div>
</div>

<style>
  .viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
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
  .row:hover {
    background: var(--fill-hover);
  }
  .row[aria-selected="true"] {
    background: var(--fill-selected);
  }
</style>
