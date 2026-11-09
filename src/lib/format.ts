// Number and date formatting for the French UI, shared by every list.

const UNITS = ["Ko", "Mo", "Go", "To"] as const;
const decimal = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 });
const shortDate = new Intl.DateTimeFormat("fr-FR", {
  dateStyle: "short",
  timeZone: "UTC",
});
const moment = new Intl.DateTimeFormat("fr-FR", {
  dateStyle: "long",
  timeStyle: "short",
});
const monthYear = new Intl.DateTimeFormat("fr-FR", {
  month: "long",
  year: "numeric",
  timeZone: "UTC",
});

/** Size in KiB to "512 Ko", "1,5 Mo", "2 Go"… */
export function formatSize(kib: number): string {
  let value = kib;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${decimal.format(value)} ${UNITS[unit]}`;
}

/** A local date as `yyyy-mm-dd`, the form install dates use. */
export function isoDate(date: Date): string {
  return [date.getFullYear(), date.getMonth() + 1, date.getDate()]
    .map((n) => String(n).padStart(2, "0"))
    .join("-");
}

/** `yyyy-mm-dd` to "15/03/2024". */
export function formatDate(iso: string): string {
  return shortDate.format(new Date(`${iso}T00:00:00Z`));
}

/** Seconds since the Unix epoch to the local "9 novembre 2026 à 14:32". */
export function formatMoment(secs: number): string {
  return moment.format(new Date(secs * 1000));
}

/** `yyyy-mm` to "Mars 2024", capitalized to stand as a heading. */
export function formatMonth(month: string): string {
  const text = monthYear.format(new Date(`${month}-01T00:00:00Z`));
  return text.charAt(0).toUpperCase() + text.slice(1);
}
