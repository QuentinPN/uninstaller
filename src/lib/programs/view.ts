// What the Programs screen shows: default filter (F1), search and sort (F2).

import type { Program } from "../ipc/bindings";

export type SortKey = "name" | "size" | "date";

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

const COMPARE: Record<SortKey, (a: Program, b: Program) => number> = {
  name: (a, b) => collator.compare(a.name, b.name),
  size: (a, b) => descending(a.estimatedSizeKib, b.estimatedSizeKib),
  date: (a, b) => descending(a.installDate, b.installDate),
};

/** Applications only (system components and updates stay hidden), filtered and sorted. */
export function visiblePrograms(
  all: readonly Program[],
  query: string,
  sort: SortKey,
): Program[] {
  const needle = normalize(query.trim());
  return all
    .filter((p) => p.kind === "app" && (needle === "" || matches(p, needle)))
    .sort(COMPARE[sort]);
}

export function totalSizeKib(programs: readonly Program[]): number {
  return programs.reduce((sum, p) => sum + (p.estimatedSizeKib ?? 0), 0);
}
