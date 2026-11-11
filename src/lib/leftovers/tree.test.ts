import { describe, expect, it } from "vitest";
import type { Found } from "../ipc/bindings";
import {
  check,
  count,
  groups,
  picks,
  safeIds,
  toggle,
  toggleAll,
} from "./tree";

const FOUND: Found = {
  folders: [
    { confidence: "safe", path: "C:Program FilesAcme" },
    { confidence: "advanced", path: "C:UsersaAppDataRoamingAcm" },
  ],
  shortcuts: [],
  keys: [{ confidence: "safe", name: "HKEY_CURRENT_USERSoftwareAcme" }],
  services: [{ confidence: "moderate", name: "AcmeUpdate" }],
  tasks: [],
};

describe("groups", () => {
  it("keeps the tab's non-empty lists, in order, with stable ids", () => {
    expect(groups(FOUND, "files").map((g) => g.key)).toEqual(["folders"]);
    expect(groups(FOUND, "registry").map((g) => g.key)).toEqual([
      "keys",
      "services",
    ]);
    expect(groups(FOUND, "files")[0]?.items[1]).toEqual({
      id: "folders:1",
      label: "C:UsersaAppDataRoamingAcm",
      confidence: "advanced",
    });
  });

  it("counts every item of a tab", () => {
    expect(count(FOUND, "files")).toBe(2);
    expect(count(FOUND, "registry")).toBe(2);
  });
});

describe("selection", () => {
  it("preselects the safe items only, across both tabs", () => {
    expect([...safeIds(FOUND)].sort()).toEqual(["folders:0", "keys:0"]);
  });

  it("gives a group three states and fills a partial one", () => {
    const folders = groups(FOUND, "files")[0] ?? { items: [] };
    const some = safeIds(FOUND);
    expect(check(folders.items, some)).toBe("some");
    const all = toggleAll(folders.items, some);
    expect(check(folders.items, all)).toBe("all");
    expect(all.has("keys:0")).toBe(true);
    expect(check(folders.items, toggleAll(folders.items, all))).toBe("none");
  });

  it("toggles one item without touching the others", () => {
    const next = toggle("folders:0", safeIds(FOUND));
    expect([...next]).toEqual(["keys:0"]);
    expect([...toggle("folders:0", next)].sort()).toEqual([
      "folders:0",
      "keys:0",
    ]);
  });
});

describe("picks", () => {
  it("sends lists and indexes, and preselects safe services and tasks", () => {
    expect(picks(new Set(["folders:1", "keys:0"]))).toEqual([
      { list: "folders", index: 1 },
      { list: "keys", index: 0 },
    ]);
    const services = {
      ...FOUND,
      services: [{ confidence: "safe" as const, name: "Acme" }],
      tasks: [{ confidence: "safe" as const, name: "\\Acme Update" }],
    };
    expect(safeIds(services).has("services:0")).toBe(true);
    expect(safeIds(services).has("tasks:0")).toBe(true);
  });
});
