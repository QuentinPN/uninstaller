import { describe, expect, it } from "vitest";
import { nextIndex } from "./listNav";

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
