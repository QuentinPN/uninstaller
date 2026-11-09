// The app's sections, in navigation order, with their Ctrl+digit shortcut.
export const SECTIONS = [
  { id: "programs", digit: 1 },
  { id: "apps", digit: 2 },
  { id: "extensions", digit: 3 },
  { id: "tools", digit: 4 },
  { id: "backups", digit: 5 },
  { id: "tracking", digit: 6 },
] as const;

/** Settings sits apart in the navigation (gear) and has no digit shortcut. */
export type SectionId = (typeof SECTIONS)[number]["id"] | "settings";

type KeyLike = Pick<
  KeyboardEvent,
  "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
>;

/** Section for a Ctrl+digit press. Matches the physical key (`code`), so it works on
 *  AZERTY where the top row gives "&", "é"… without Shift. */
export function sectionForShortcut(event: KeyLike): SectionId | null {
  if (!event.ctrlKey || event.altKey || event.shiftKey || event.metaKey) {
    return null;
  }
  const match = /^(?:Digit|Numpad)(\d)$/.exec(event.code);
  if (!match) return null;
  const digit = Number(match[1]);
  return SECTIONS.find((s) => s.digit === digit)?.id ?? null;
}
