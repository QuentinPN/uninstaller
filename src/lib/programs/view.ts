// What the Programs screen shows: default filter (F1), search, sort and grouping (F2).

import { formatMonth } from "../format";
import { fr } from "../i18n/fr";
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
function nullsLast<V>(
  a: V | null | undefined,
  b: V | null | undefined,
  compare: (a: V, b: V) => number,
): number {
  if (a == null) return b == null ? 0 : 1;
  if (b == null) return -1;
  return compare(a, b);
}

function descending(a?: number | string | null, b?: number | string | null) {
  return nullsLast(a, b, (x, y) => (x < y ? 1 : x > y ? -1 : 0));
}

type Compare = (a: Program, b: Program) => number;

const COMPARE: Record<SortKey, (sizes: Sizes) => Compare> = {
  name: () => (a, b) => collator.compare(a.name, b.name),
  size: (sizes) => (a, b) => descending(sizeKib(a, sizes), sizeKib(b, sizes)),
  date: () => (a, b) => descending(a.installDate, b.installDate),
};

export type GroupKey = "none" | "publisher" | "date" | "size";
type Grouped = Exclude<GroupKey, "none">;

/** Lower bounds of the size ranges in KiB, largest first (`fr.programs.sizeRanges`). */
const SIZE_RANGES_KIB = [1024 * 1024, 100 * 1024, 10 * 1024, 0] as const;

interface Grouping {
  /** Group key, compared as text (numbers in order); `null` when unknown. */
  of: (program: Program, sizes: Sizes) => string | null;
  descending: boolean;
  label: (key: string) => string;
}

const GROUPS: Record<Grouped, Grouping> = {
  publisher: {
    of: (p) => p.publisher ?? null,
    descending: false,
    label: (k) => k,
  },
  date: {
    of: (p) => p.installDate?.slice(0, 7) ?? null,
    descending: true,
    label: formatMonth,
  },
  size: {
    of: (p, sizes) => {
      const size = sizeKib(p, sizes);
      return size == null
        ? null
        : String(SIZE_RANGES_KIB.findIndex((min) => size >= min));
    },
    descending: false,
    label: (k) => fr.programs.sizeRanges[Number(k)] ?? k,
  },
};

function compareGroups(group: Grouped, sizes: Sizes): Compare {
  const { of, descending } = GROUPS[group];
  const sign = descending ? -1 : 1;
  return (a, b) =>
    nullsLast(
      of(a, sizes),
      of(b, sizes),
      (x, y) => sign * collator.compare(x, y),
    );
}

/** Heading of the program's group; `null` when the list is not grouped. */
export function groupLabel(
  program: Program,
  group: GroupKey,
  sizes: Sizes,
): string | null {
  if (group === "none") return null;
  const key = GROUPS[group].of(program, sizes);
  return key == null
    ? fr.programs.groupUnknown[group]
    : GROUPS[group].label(key);
}

/** Kinds hidden by default (F1), each shown on request. */
export const OPTIONAL_KINDS = ["systemComponent", "update"] as const;
export type OptionalKind = (typeof OPTIONAL_KINDS)[number];

export interface ListView {
  query: string;
  sort: SortKey;
  /** Groups come first, in their own order; `sort` applies within each. */
  group: GroupKey;
  sizes: Sizes;
  /** Optional kinds the user chose to show; applications are always shown. */
  shown: ReadonlySet<OptionalKind>;
}

function isShown(kind: Kind, shown: ReadonlySet<OptionalKind>): boolean {
  return kind === "app" || shown.has(kind);
}

/** Programs of the shown kinds matching the search, grouped then sorted. */
export function visiblePrograms(
  all: readonly Program[],
  { query, sort, group, sizes, shown }: ListView,
): Program[] {
  const needle = normalize(query.trim());
  const within = COMPARE[sort](sizes);
  const groups = group === "none" ? null : compareGroups(group, sizes);
  return all
    .filter(
      (p) => isShown(p.kind, shown) && (needle === "" || matches(p, needle)),
    )
    .sort((a, b) => (groups?.(a, b) ?? 0) || within(a, b));
}

export function totalSizeKib(
  programs: readonly Program[],
  sizes: Sizes,
): number {
  return programs.reduce((sum, p) => sum + (sizeKib(p, sizes) ?? 0), 0);
}
