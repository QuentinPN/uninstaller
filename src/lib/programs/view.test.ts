import { describe, expect, it } from "vitest";
import type { Program } from "../ipc/bindings";
import {
  type Filter,
  type GroupKey,
  groupLabel,
  type ListView,
  type OptionalKind,
  recentSince,
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
  group: GroupKey = "none",
  filters: ReadonlySet<Filter> = new Set(),
): ListView {
  return {
    query,
    sort,
    reversed: false,
    group,
    sizes,
    shown,
    filters,
    recentSince: "2024-04-28",
  };
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
    profile: null,
    kind: "app",
    orphan: false,
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

  it("reverses the order, unknown values still last", () => {
    const reversed = (sort: SortKey) =>
      names(visiblePrograms(all, { ...view("", sort), reversed: true }));
    expect(reversed("size")).toEqual(["zip", "Éditeur", "archive"]);
    expect(reversed("name")).toEqual(["zip", "Éditeur", "archive"]);
  });

  it("sorts versions as numbers and installs by their label", () => {
    const versions = [
      program("a", { version: "1.10" }),
      program("b", { version: "1.9" }),
      program("c"),
    ];
    expect(names(visiblePrograms(versions, view("", "version")))).toEqual([
      "b",
      "a",
      "c",
    ]);
    const scopes = [
      program("a", { scope: "user" }),
      program("b", { scope: "otherUser", profile: "Marie" }),
      program("c", { scope: "machine32" }),
    ];
    expect(names(visiblePrograms(scopes, view("", "scope")))).toEqual([
      "c",
      "b",
      "a",
    ]);
  });
});

describe("filters", () => {
  const all = [
    program("ancien", { installDate: "2024-04-27" }),
    program("limite", { installDate: "2024-04-28" }),
    program("neuf", { installDate: "2024-05-05", orphan: true }),
    program("perdu", { orphan: true }),
    program("sansDate"),
  ];
  const only = (...filters: Filter[]) =>
    names(
      visiblePrograms(
        all,
        view("", "name", NONE, new Set(), "none", new Set(filters)),
      ),
    );

  it("keeps programs installed since the cutoff", () => {
    expect(only("recent")).toEqual(["limite", "neuf"]);
  });

  it("keeps orphans, and both filters combine", () => {
    expect(only("orphan")).toEqual(["neuf", "perdu"]);
    expect(only("recent", "orphan")).toEqual(["neuf"]);
  });

  it("counts back seven local days, across a month", () => {
    expect(recentSince(new Date(2024, 4, 5, 23, 30))).toBe("2024-04-28");
    expect(recentSince(new Date(2024, 0, 3))).toBe("2023-12-27");
  });
});

describe("grouping", () => {
  const all = [
    program("b", {
      publisher: "Zeta",
      installDate: "2024-05-02",
      estimatedSizeKib: 5,
    }),
    program("a", { publisher: "Alpha", installDate: "2023-01-09" }),
    program("c", { installDate: "2024-05-20", estimatedSizeKib: 2_000_000 }),
    program("d", { publisher: "Alpha", estimatedSizeKib: 50_000 }),
  ];
  const grouped = (group: GroupKey, sort: SortKey = "name") =>
    visiblePrograms(all, view("", sort, NONE, new Set(), group)).map(
      (p) => `${groupLabel(p, group, NONE)}: ${p.name}`,
    );

  it("orders groups, unknown last, and sorts within each", () => {
    expect(grouped("publisher")).toEqual([
      "Alpha: a",
      "Alpha: d",
      "Zeta: b",
      "Éditeur inconnu: c",
    ]);
    expect(grouped("date", "size")).toEqual([
      "Mai 2024: c",
      "Mai 2024: b",
      "Janvier 2023: a",
      "Date inconnue: d",
    ]);
    expect(grouped("size")).toEqual([
      "1 Go et plus: c",
      "10 à 100 Mo: d",
      "Moins de 10 Mo: b",
      "Taille inconnue: a",
    ]);
  });

  it("has no heading when not grouped", () => {
    expect(groupLabel(program("a"), "none", NONE)).toBeNull();
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
