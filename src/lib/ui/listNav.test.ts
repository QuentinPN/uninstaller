import { describe, expect, it } from "vitest";
import { EMPTY_SELECTION, nextIndex, pick } from "./listNav";

describe("nextIndex", () => {
  it("starts at the first row when nothing is selected", () => {
    expect(nextIndex("ArrowDown", -1, 5, 3)).toBe(0);
    expect(nextIndex("ArrowUp", -1, 5, 3)).toBe(0);
  });

  it("moves and stays inside the list", () => {
    expect(nextIndex("ArrowDown", 4, 5, 3)).toBe(4);
    expect(nextIndex("ArrowUp", 0, 5, 3)).toBe(0);
    expect(nextIndex("PageDown", 3, 5, 3)).toBe(4);
    expect(nextIndex("PageUp", 1, 5, 3)).toBe(0);
    expect(nextIndex("End", 0, 5, 3)).toBe(4);
    expect(nextIndex("Home", 4, 5, 3)).toBe(0);
  });

  it("ignores other keys and empty lists", () => {
    expect(nextIndex("KeyA", 0, 5, 3)).toBeNull();
    expect(nextIndex("ArrowDown", -1, 0, 3)).toBeNull();
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
