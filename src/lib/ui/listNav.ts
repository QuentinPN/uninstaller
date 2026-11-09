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
