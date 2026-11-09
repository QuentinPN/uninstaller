// What the Programs screen shows: default filter (F1), search and sort (F2).

import type { Program } from "../ipc/bindings";

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

/** Applications only (system components and updates stay hidden), filtered and sorted. */
export function visiblePrograms(
  all: readonly Program[],
  query: string,
  sort: SortKey,
  sizes: Sizes,
): Program[] {
  const needle = normalize(query.trim());
  return all
    .filter((p) => p.kind === "app" && (needle === "" || matches(p, needle)))
    .sort(COMPARE[sort](sizes));
}

export function totalSizeKib(
  programs: readonly Program[],
  sizes: Sizes,
): number {
  return programs.reduce((sum, p) => sum + (sizeKib(p, sizes) ?? 0), 0);
}
