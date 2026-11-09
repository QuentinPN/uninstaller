// What the Programs screen shows: default filter (F1), search and sort (F2).

import type { Kind, Program } from "../ipc/bindings";

export type SortKey = "name" | "size" | "date";

/** Sizes measured on disk (KiB) by program id, filled in as they arrive. */
export type Sizes = ReadonlyMap<string, number>;

/** Measured size, else the size the installer declared. */
export function sizeKib(program: Program, sizes: Sizes): number | null {
  return sizes.get(program.id) ?? program.estimatedSizeKib ?? null;
}

const collator = new Intl.Collator("fr", {
  sensitivity: "base",
  numeric: true,
});

/** Lowercase without accents, so "editeur" finds "Éditeur". */
function normalize(text: string): string {
  return text
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}

function matches(program: Program, query: string): boolean {
  return [
    program.name,
    program.publisher,
    program.comments,
    program.installLocation,
  ].some((field) => field != null && normalize(field).includes(query));
}

/** Missing values sort last whatever the direction. */
function descending(a?: number | string | null, b?: number | string | null) {
  if (a == null) return b == null ? 0 : 1;
  if (b == null) return -1;
  return a < b ? 1 : a > b ? -1 : 0;
}

type Compare = (a: Program, b: Program) => number;

const COMPARE: Record<SortKey, (sizes: Sizes) => Compare> = {
  name: () => (a, b) => collator.compare(a.name, b.name),
  size: (sizes) => (a, b) => descending(sizeKib(a, sizes), sizeKib(b, sizes)),
  date: () => (a, b) => descending(a.installDate, b.installDate),
};

/** Kinds hidden by default (F1), each shown on request. */
export const OPTIONAL_KINDS = ["systemComponent", "update"] as const;
export type OptionalKind = (typeof OPTIONAL_KINDS)[number];

export interface ListView {
  query: string;
  sort: SortKey;
  sizes: Sizes;
  /** Optional kinds the user chose to show; applications are always shown. */
  shown: ReadonlySet<OptionalKind>;
}

function isShown(kind: Kind, shown: ReadonlySet<OptionalKind>): boolean {
  return kind === "app" || shown.has(kind);
}

/** Programs of the shown kinds matching the search, sorted. */
export function visiblePrograms(
  all: readonly Program[],
  { query, sort, sizes, shown }: ListView,
): Program[] {
  const needle = normalize(query.trim());
  return all
    .filter(
      (p) => isShown(p.kind, shown) && (needle === "" || matches(p, needle)),
    )
    .sort(COMPARE[sort](sizes));
}

export function totalSizeKib(
  programs: readonly Program[],
  sizes: Sizes,
): number {
  return programs.reduce((sum, p) => sum + (sizeKib(p, sizes) ?? 0), 0);
}
