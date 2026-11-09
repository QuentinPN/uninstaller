import { describe, expect, it } from "vitest";
import { formatDate, formatMoment, formatMonth, formatSize } from "./format";

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

describe("formatMoment", () => {
  it("gives the local date and time in French", () => {
    const secs = new Date(2026, 10, 9, 14, 32).getTime() / 1000;
    expect(formatMoment(secs)).toBe("9 novembre 2026 à 14:32");
  });
});

describe("formatMonth", () => {
  it("names the month in French, capitalized", () => {
    expect(formatMonth("2024-03")).toBe("Mars 2024");
  });
});
