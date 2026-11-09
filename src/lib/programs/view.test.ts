import { describe, expect, it } from "vitest";
import type { Program } from "../ipc/bindings";
import {
  type ListView,
  type OptionalKind,
  type Sizes,
  type SortKey,
  totalSizeKib,
  visiblePrograms,
} from "./view";

const NONE = new Map<string, number>();

function view(
  query: string,
  sort: SortKey,
  sizes: Sizes = NONE,
  shown: ReadonlySet<OptionalKind> = new Set(),
): ListView {
  return { query, sort, sizes, shown };
}

function program(name: string, extra: Partial<Program> = {}): Program {
  return {
    id: `hklm64\\${name}`,
    name,
    publisher: null,
    version: null,
    installDate: null,
    estimatedSizeKib: null,
    installLocation: null,
    uninstallString: null,
    quietUninstallString: null,
    displayIcon: null,
    url: null,
    comments: null,
    msi: false,
    registryKey: `HKEY_LOCAL_MACHINE\\${name}`,
    scope: "machine64",
    kind: "app",
    ...extra,
  };
}

const names = (list: Program[]) => list.map((p) => p.name);

describe("visiblePrograms", () => {
  const all = [
    program("zip", { estimatedSizeKib: 10, installDate: "2023-01-01" }),
    program("Éditeur", { estimatedSizeKib: 300, publisher: "Uninstaller" }),
    program("archive", { installDate: "2024-05-01" }),
    program("Pilote", { kind: "systemComponent" }),
    program("KB123", { kind: "update" }),
  ];

  it("hides system components and updates", () => {
    expect(names(visiblePrograms(all, view("", "name")))).toEqual([
      "archive",
      "Éditeur",
      "zip",
    ]);
  });

  it("shows system components and updates on request", () => {
    const shown = new Set<OptionalKind>(["update"]);
    expect(names(visiblePrograms(all, view("", "name", NONE, shown)))).toEqual([
      "archive",
      "Éditeur",
      "KB123",
      "zip",
    ]);
    shown.add("systemComponent");
    expect(
      names(visiblePrograms(all, view("pilote", "name", NONE, shown))),
    ).toEqual(["Pilote"]);
  });

  it("searches name, publisher, comments and folder, ignoring accents and case", () => {
    expect(names(visiblePrograms(all, view("EDITEUR", "name")))).toEqual([
      "Éditeur",
    ]);
    expect(names(visiblePrograms(all, view("uninstaller", "name")))).toEqual([
      "Éditeur",
    ]);
    const withFolder = [program("a", { installLocation: "C:\\Outils\\A" })];
    expect(names(visiblePrograms(withFolder, view("outils", "name")))).toEqual([
      "a",
    ]);
  });

  it("sorts by measured size when there is one", () => {
    const measured = new Map([[program("archive").id, 1000]]);
    expect(names(visiblePrograms(all, view("", "size", measured)))).toEqual([
      "archive",
      "Éditeur",
      "zip",
    ]);
  });

  it("sorts size and date descending with unknown values last", () => {
    expect(names(visiblePrograms(all, view("", "size")))).toEqual([
      "Éditeur",
      "zip",
      "archive",
    ]);
    expect(names(visiblePrograms(all, view("", "date")))).toEqual([
      "archive",
      "zip",
      "Éditeur",
    ]);
  });
});

describe("totalSizeKib", () => {
  it("ignores unknown sizes", () => {
    expect(
      totalSizeKib([program("a", { estimatedSizeKib: 5 }), program("b")], NONE),
    ).toBe(5);
  });

  it("prefers the size measured on disk", () => {
    const measured = new Map([[program("b").id, 7]]);
    const list = [
      program("a", { estimatedSizeKib: 5 }),
      program("b", { estimatedSizeKib: 99 }),
    ];
    expect(totalSizeKib(list, measured)).toBe(12);
  });
});
