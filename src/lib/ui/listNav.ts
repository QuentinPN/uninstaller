/**
 * The row a navigation key moves to in a list of `count` rows, or null if the key
 * is not a navigation key. `current` is -1 when nothing is selected.
 */
export function nextIndex(
  code: string,
  current: number,
  count: number,
  pageSize: number,
): number | null {
  if (count === 0) return null;
  const last = count - 1;
  const clamp = (i: number) => Math.min(last, Math.max(0, i));
  switch (code) {
    case "ArrowDown":
      return current < 0 ? 0 : clamp(current + 1);
    case "ArrowUp":
      return current < 0 ? 0 : clamp(current - 1);
    case "PageDown":
      return clamp(current + pageSize);
    case "PageUp":
      return clamp(current - pageSize);
    case "Home":
      return 0;
    case "End":
      return last;
    default:
      return null;
  }
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
