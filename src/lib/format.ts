// Number and date formatting for the French UI, shared by every list.

const UNITS = ["Ko", "Mo", "Go", "To"] as const;
const decimal = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 });
const shortDate = new Intl.DateTimeFormat("fr-FR", {
  dateStyle: "short",
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

/** `yyyy-mm-dd` to "15/03/2024". */
export function formatDate(iso: string): string {
  return shortDate.format(new Date(`${iso}T00:00:00Z`));
}
