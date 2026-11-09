<script lang="ts" generics="T">
import { onMount, type Snippet } from "svelte";

interface Props {
  items: readonly T[];
  key: (item: T) => string;
  row: Snippet<[T]>;
  label: string;
}

let { items, key, row, label }: Props = $props();

/** Rows rendered above and below the viewport, so fast scrolling shows no gap. */
const OVERSCAN = 6;

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
</script>

<div
  class="viewport"
  role="list"
  aria-label={label}
  bind:this={viewport}
  bind:clientHeight={height}
  onscroll={() => (scrollTop = viewport.scrollTop)}
>
  <div class="spacer" style:height="{items.length * rowHeight}px">
    {#each items.slice(start, end) as item, i (key(item))}
      <div
        class="row"
        role="listitem"
        aria-setsize={items.length}
        aria-posinset={start + i + 1}
        style:transform="translateY({(start + i) * rowHeight}px)"
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
    scrollbar-color: var(--fill-glass) transparent;
  }
  .spacer {
    position: relative;
  }
  .row {
    position: absolute;
    inset: 0 0 auto;
    height: var(--row-height);
  }
</style>
