import { describe, expect, it } from "vitest";
import { SECTIONS, sectionForShortcut } from "./sections";

const press = (code: string, mods: Partial<Record<string, boolean>> = {}) => ({
  code,
  ctrlKey: true,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("sectionForShortcut", () => {
  it("maps Ctrl+1…6 to the sections in order", () => {
    expect(
      SECTIONS.map((s) => sectionForShortcut(press(`Digit${s.digit}`))),
    ).toEqual(SECTIONS.map((s) => s.id));
  });

  it("accepts the numeric keypad", () => {
    expect(sectionForShortcut(press("Numpad4"))).toBe("tools");
  });

  it("ignores digits without a section", () => {
    expect(sectionForShortcut(press("Digit7"))).toBeNull();
    expect(sectionForShortcut(press("Digit0"))).toBeNull();
  });

  it("requires Ctrl alone", () => {
    expect(sectionForShortcut(press("Digit1", { ctrlKey: false }))).toBeNull();
    expect(sectionForShortcut(press("Digit1", { shiftKey: true }))).toBeNull();
    expect(sectionForShortcut(press("Digit1", { altKey: true }))).toBeNull();
  });

  it("ignores other keys", () => {
    expect(sectionForShortcut(press("KeyF"))).toBeNull();
  });
});
