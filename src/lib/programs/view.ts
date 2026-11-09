// What the Programs screen shows: default filter (F1), search, sort and grouping (F2),
// recent and orphan filters (F3).

import { formatMonth, isoDate } from "../format";
import { fr } from "../i18n/fr";
import type { Kind, Program } from "../ipc/bindings";

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

type Compare = (a: Program, b: Program) => number;
type Value = string | number;

/** What a list is ordered by: a value per program (`null` when unknown) and its
 * natural direction. */
interface Ordering<V extends Value> {
  of: (program: Program, sizes: Sizes) => V | null | undefined;
  descending: boolean;
}

/** Numbers by value, text in French order (digits as numbers); unknown values last. */
function compareBy<V extends Value>(
  { of }: Ordering<V>,
  descending: boolean,
  sizes: Sizes,
): Compare {
  const sign = descending ? -1 : 1;
  return (a, b) =>
    nullsLast(of(a, sizes), of(b, sizes), (x, y) =>
      typeof x === "number" && typeof y === "number"
        ? sign * (x - y)
        : sign * collator.compare(String(x), String(y)),
    );
}

/** Badge of the Installation column: the other account's name, else the scope. */
export function scopeLabel(program: Program): string {
  return program.profile ?? fr.programs.scope[program.scope];
}

export type GroupKey = "none" | "publisher" | "date" | "size";
type Grouped = Exclude<GroupKey, "none">;

/** Lower bounds of the size ranges in KiB, largest first (`fr.programs.sizeRanges`). */
const SIZE_RANGES_KIB = [1024 * 1024, 100 * 1024, 10 * 1024, 0] as const;

interface Grouping extends Ordering<string> {
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

/** Optional columns of the list, in display order; the name is always shown (F2). */
export const COLUMNS = ["version", "size", "date", "scope"] as const;
export type Column = (typeof COLUMNS)[number];
export const DEFAULT_COLUMNS: readonly Column[] = ["size", "date", "scope"];

/** Any column sorts the list, the name first. */
export type SortKey = "name" | Column;
export const SORT_KEYS: readonly SortKey[] = ["name", ...COLUMNS];

const SORTS: Record<SortKey, Ordering<Value>> = {
  name: { of: (p) => p.name, descending: false },
  version: { of: (p) => p.version, descending: false },
  size: { of: (p, sizes) => sizeKib(p, sizes), descending: true },
  date: { of: (p) => p.installDate, descending: true },
  scope: { of: scopeLabel, descending: false },
};

/** Largest and newest first by default; `reversed` flips it. */
export function isDescending(sort: SortKey, reversed: boolean): boolean {
  return SORTS[sort].descending !== reversed;
}

/** Kinds hidden by default (F1), each shown on request. */
export const OPTIONAL_KINDS = ["systemComponent", "update"] as const;
export type OptionalKind = (typeof OPTIONAL_KINDS)[number];

/** Filters that narrow the list to what needs a look (F3). */
export const FILTERS = ["recent", "orphan"] as const;
export type Filter = (typeof FILTERS)[number];

/** Installed this many days ago or less counts as recent. */
export const RECENT_DAYS = 7;

/** Earliest install date (`yyyy-mm-dd`, local time) still recent on `today`. */
export function recentSince(today: Date): string {
  return isoDate(
    new Date(
      today.getFullYear(),
      today.getMonth(),
      today.getDate() - RECENT_DAYS,
    ),
  );
}

/** Dates are `yyyy-mm-dd`, so they compare as text. */
export function isRecent(program: Program, since: string): boolean {
  return program.installDate != null && program.installDate >= since;
}

const PASSES: Record<Filter, (program: Program, since: string) => boolean> = {
  recent: isRecent,
  orphan: (p) => p.orphan,
};

export interface ListView {
  query: string;
  sort: SortKey;
  /** Against the key's natural direction (second click on its column). */
  reversed: boolean;
  /** Groups come first, in their own order; `sort` applies within each. */
  group: GroupKey;
  sizes: Sizes;
  /** Optional kinds the user chose to show; applications are always shown. */
  shown: ReadonlySet<OptionalKind>;
  /** Active filters; a program must pass every one. */
  filters: ReadonlySet<Filter>;
  /** From `recentSince`. */
  recentSince: string;
}

function isShown(kind: Kind, shown: ReadonlySet<OptionalKind>): boolean {
  return kind === "app" || shown.has(kind);
}

/** Programs of the shown kinds passing the filters and the search, grouped then sorted. */
export function visiblePrograms(
  all: readonly Program[],
  {
    query,
    sort,
    reversed,
    group,
    sizes,
    shown,
    filters,
    recentSince,
  }: ListView,
): Program[] {
  const needle = normalize(query.trim());
  const within = compareBy(SORTS[sort], isDescending(sort, reversed), sizes);
  const groups =
    group === "none"
      ? null
      : compareBy(GROUPS[group], GROUPS[group].descending, sizes);
  return all
    .filter(
      (p) =>
        isShown(p.kind, shown) &&
        [...filters].every((f) => PASSES[f](p, recentSince)) &&
        (needle === "" || matches(p, needle)),
    )
    .sort((a, b) => (groups?.(a, b) ?? 0) || within(a, b));
}

export function totalSizeKib(
  programs: readonly Program[],
  sizes: Sizes,
): number {
  return programs.reduce((sum, p) => sum + (sizeKib(p, sizes) ?? 0), 0);
}
