import { describe, expect, it } from "vitest";
import { EMPTY_SELECTION, layoutLines, nextIndex, pick } from "./listNav";

describe("layoutLines", () => {
  it("fills lines up to the column count and starts each group on a new one", () => {
    const { lines, lineOf } = layoutLines(["A", "A", "A", "B"], 2);
    expect(lines).toEqual([
      { heading: "A", count: 3 },
      { items: [0, 1] },
      { items: [2] },
      { heading: "B", count: 1 },
      { items: [3] },
    ]);
    expect(lineOf).toEqual([1, 1, 2, 4]);
  });
});

describe("nextIndex", () => {
  const list = layoutLines([null, null, null, null, null], 1);
  const next = (code: string, current: number) =>
    nextIndex(code, current, list, 1, 3);

  it("starts at the first row when nothing is selected", () => {
    expect(next("ArrowDown", -1)).toBe(0);
    expect(next("ArrowUp", -1)).toBe(0);
  });

  it("moves and stays inside the list", () => {
    expect(next("ArrowDown", 4)).toBe(4);
    expect(next("ArrowUp", 0)).toBe(0);
    expect(next("PageDown", 3)).toBe(4);
    expect(next("PageUp", 1)).toBe(0);
    expect(next("End", 0)).toBe(4);
    expect(next("Home", 4)).toBe(0);
  });

  it("ignores other keys, left and right in a list, and empty lists", () => {
    expect(next("KeyA", 0)).toBeNull();
    expect(next("ArrowRight", 0)).toBeNull();
    expect(nextIndex("ArrowDown", -1, layoutLines([], 1), 1, 3)).toBeNull();
  });

  it("moves in two dimensions in a grid, across group headings", () => {
    // A: [0 1 2] [3]   B: [4 5]
    const grid = layoutLines(["A", "A", "A", "A", "B", "B"], 3);
    const move = (code: string, current: number) =>
      nextIndex(code, current, grid, 3, 1);
    expect(move("ArrowRight", 2)).toBe(3);
    expect(move("ArrowLeft", 0)).toBe(0);
    expect(move("ArrowDown", 1)).toBe(3);
    expect(move("ArrowDown", 3)).toBe(4);
    expect(move("ArrowUp", 5)).toBe(3);
    expect(move("ArrowUp", 3)).toBe(0);
    expect(move("ArrowDown", 5)).toBe(5);
  });
});

describe("pick", () => {
  const order = ["a", "b", "c", "d"];
  const only = (key: string) => pick(order, EMPTY_SELECTION, key, "only");
  const keys = (s: { keys: ReadonlySet<string> }) => [...s.keys].sort();

  it("plain pick selects one row and moves the anchor", () => {
    expect(only("b")).toEqual({
      active: "b",
      anchor: "b",
      keys: new Set(["b"]),
    });
  });

  it("Ctrl toggles rows in and out", () => {
    const two = pick(order, only("a"), "c", "toggle");
    expect(keys(two)).toEqual(["a", "c"]);
    expect(keys(pick(order, two, "a", "toggle"))).toEqual(["c"]);
  });

  it("Shift selects from the anchor, both directions, anchor kept", () => {
    const down = pick(order, only("b"), "d", "range");
    expect(keys(down)).toEqual(["b", "c", "d"]);
    expect(down.anchor).toBe("b");
    expect(keys(pick(order, down, "a", "range"))).toEqual(["a", "b"]);
  });

  it("Shift with an anchor no longer shown falls back to one row", () => {
    const stale = { ...only("b"), anchor: "gone" };
    expect(keys(pick(order, stale, "c", "range"))).toEqual(["c"]);
  });
});
