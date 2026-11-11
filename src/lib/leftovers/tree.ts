import type { Confidence, Found, List, Pick } from "../ipc/bindings";

/** The lists of a scan, in the order the screen shows them. */
export type GroupKey = keyof Found;
export type Tab = "files" | "registry";

/** DESIGN.md: two tabs; scheduled tasks sit with the registry's system entries. */
export const TABS: Record<Tab, GroupKey[]> = {
  files: ["folders", "shortcuts"],
  registry: ["keys", "services", "tasks"],
};

/** What the quarantine takes for now; services wait. */
const REMOVABLE: ReadonlySet<GroupKey> = new Set<GroupKey>([
  "folders",
  "shortcuts",
  "keys",
  "tasks",
]);

export function removable(key: GroupKey): boolean {
  return REMOVABLE.has(key);
}

export interface Item {
  /** `group:index` in the scan's answer, stable while it is shown. */
  id: string;
  label: string;
  confidence: Confidence;
}

export interface Group {
  key: GroupKey;
  items: Item[];
}

export type Check = "all" | "some" | "none";

/** The non-empty groups of `tab`, each list kept surest first as the scan sent it. */
export function groups(found: Found, tab: Tab): Group[] {
  return TABS[tab]
    .map((key) => ({
      key,
      items: found[key].map((entry, i) => ({
        id: `${key}:${i}`,
        label: "path" in entry ? entry.path : entry.name,
        confidence: entry.confidence,
      })),
    }))
    .filter((group) => group.items.length);
}

export function count(found: Found, tab: Tab): number {
  return TABS[tab].reduce((n, key) => n + found[key].length, 0);
}

/** « Tout sélectionner (sûrs) », also the preselection: every safe item, nothing else. */
export function safeIds(found: Found): Set<string> {
  const ids = new Set<string>();
  for (const tab of Object.keys(TABS) as Tab[]) {
    for (const group of groups(found, tab)) {
      for (const item of group.items) {
        if (removable(group.key) && item.confidence === "safe")
          ids.add(item.id);
      }
    }
  }
  return ids;
}

export function check(items: Item[], selected: ReadonlySet<string>): Check {
  const n = items.filter((item) => selected.has(item.id)).length;
  return n === 0 ? "none" : n === items.length ? "all" : "some";
}

/** A group's box: a full group empties, a partial or empty one fills. */
export function toggleAll(
  items: Item[],
  selected: ReadonlySet<string>,
): Set<string> {
  const next = new Set(selected);
  const fill = check(items, selected) !== "all";
  for (const item of items) {
    if (fill) next.add(item.id);
    else next.delete(item.id);
  }
  return next;
}

export function toggle(id: string, selected: ReadonlySet<string>): Set<string> {
  const next = new Set(selected);
  if (!next.delete(id)) next.add(id);
  return next;
}

/** The selection as the backend takes it: a list and an index, never a path. */
export function picks(selected: ReadonlySet<string>): Pick[] {
  return [...selected].map((id) => {
    const [list, index] = id.split(":");
    return { list: list as List, index: Number(index) };
  });
}
