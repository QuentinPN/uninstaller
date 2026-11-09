/** A line of a virtual list: a group heading (with its item count) or up to
 * `columns` items, by index. */
export type Line = { heading: string; count: number } | { items: number[] };

/** Lines in display order, and the line of each item. */
export interface Layout {
  lines: Line[];
  lineOf: number[];
}

/**
 * Items laid out `columns` per line, a heading line starting each run of equal
 * headings (`null`: no heading, the item stays in the current run).
 */
export function layoutLines(
  headings: readonly (string | null)[],
  columns: number,
): Layout {
  const lines: Line[] = [];
  const lineOf: number[] = [];
  let group: { heading: string; count: number } | null = null;
  let open: { items: number[] } | null = null;
  headings.forEach((heading, index) => {
    if (heading !== null && heading !== group?.heading) {
      group = { heading, count: 0 };
      lines.push(group);
      open = null;
    }
    if (group && heading !== null) group.count++;
    if (!open || open.items.length === columns) {
      open = { items: [] };
      lines.push(open);
    }
    open.items.push(index);
    lineOf.push(lines.length - 1);
  });
  return { lines, lineOf };
}

/** The item `delta` item lines away (headings skipped), in the same column or the
 * last one of a shorter line. */
function byLines({ lines, lineOf }: Layout, current: number, delta: number) {
  const items = (line: number) => {
    const at = lines[line];
    return at && "items" in at ? at.items : null;
  };
  const from = lineOf[current] ?? 0;
  const column = items(from)?.indexOf(current) ?? 0;
  let target = from;
  let left = Math.abs(delta);
  const step = Math.sign(delta);
  for (let l = from + step; left > 0 && l >= 0 && l < lines.length; l += step) {
    if (items(l)) {
      target = l;
      left--;
    }
  }
  const row = items(target) ?? [current];
  return row[Math.min(column, row.length - 1)] ?? current;
}

/**
 * The item a navigation key moves to, or null if the key is not a navigation key.
 * `current` is -1 when nothing is selected; `page` counts lines. Left and right
 * only move in a grid (`columns` > 1).
 */
export function nextIndex(
  code: string,
  current: number,
  layout: Layout,
  columns: number,
  page: number,
): number | null {
  const count = layout.lineOf.length;
  const moves: Record<string, () => number> = {
    ArrowDown: () => byLines(layout, current, 1),
    ArrowUp: () => byLines(layout, current, -1),
    PageDown: () => byLines(layout, current, page),
    PageUp: () => byLines(layout, current, -page),
    Home: () => 0,
    End: () => count - 1,
  };
  if (columns > 1) {
    moves.ArrowRight = () => Math.min(count - 1, current + 1);
    moves.ArrowLeft = () => Math.max(0, current - 1);
  }
  const move = moves[code];
  if (count === 0 || !move) return null;
  return current < 0 ? 0 : move();
}

/** Selected rows of a list: `active` has the keyboard, `anchor` starts Shift ranges. */
export interface Selection {
  active: string | null;
  anchor: string | null;
  keys: ReadonlySet<string>;
}

export const EMPTY_SELECTION: Selection = {
  active: null,
  anchor: null,
  keys: new Set(),
};

/** How a click or key changes the selection: plain, Ctrl or Shift. */
export type PickMode = "only" | "toggle" | "range";

export function pickMode(event: MouseEvent | KeyboardEvent): PickMode {
  if (event.shiftKey) return "range";
  return event.ctrlKey && event instanceof MouseEvent ? "toggle" : "only";
}

/** The selection after picking `key`, with rows in display `order`. */
export function pick(
  order: readonly string[],
  selection: Selection,
  key: string,
  mode: PickMode,
): Selection {
  if (mode === "toggle") {
    const keys = new Set(selection.keys);
    if (!keys.delete(key)) keys.add(key);
    return { active: key, anchor: key, keys };
  }
  if (mode === "range") {
    const anchor = selection.anchor ?? key;
    const from = order.indexOf(anchor);
    const to = order.indexOf(key);
    if (from >= 0 && to >= 0) {
      const [lo, hi] = from < to ? [from, to] : [to, from];
      return { active: key, anchor, keys: new Set(order.slice(lo, hi + 1)) };
    }
  }
  return { active: key, anchor: key, keys: new Set([key]) };
}
