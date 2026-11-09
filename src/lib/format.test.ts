import { describe, expect, it } from "vitest";
import { formatDate, formatSize } from "./format";

describe("formatSize", () => {
  it("picks the largest unit below 1024", () => {
    expect(formatSize(512)).toBe("512 Ko");
    expect(formatSize(1536)).toBe("1,5 Mo");
    expect(formatSize(2 * 1024 * 1024)).toBe("2 Go");
  });
});

describe("formatDate", () => {
  it("formats in French without time-zone drift", () => {
    expect(formatDate("2024-03-15")).toBe("15/03/2024");
  });
});
