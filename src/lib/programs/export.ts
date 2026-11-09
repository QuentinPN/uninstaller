// List export (F19): what the list shows, in its order, as HTML, CSV or text.

import { formatDate, formatSize, isoDate } from "../format";
import { fr } from "../i18n/fr";
import type { ExportFormat, Program } from "../ipc/bindings";
import { type Sizes, scopeLabel, sizeKib, totalSizeKib } from "./view";

export const EXPORT_FORMATS: readonly ExportFormat[] = ["html", "csv", "txt"];

interface Field {
  head: string;
  text: (program: Program, sizes: Sizes) => string;
}

/** Exported fields, in order. */
const FIELDS: readonly Field[] = [
  { head: fr.programs.field.name, text: (p) => p.name },
  { head: fr.programs.group.publisher, text: (p) => p.publisher ?? "" },
  { head: fr.programs.field.version, text: (p) => p.version ?? "" },
  {
    head: fr.programs.field.size,
    text: (p, sizes) => {
      const size = sizeKib(p, sizes);
      return size == null ? "" : formatSize(size);
    },
  },
  {
    head: fr.programs.field.date,
    text: (p) => (p.installDate ? formatDate(p.installDate) : ""),
  },
  { head: fr.programs.field.scope, text: scopeLabel },
  { head: fr.programs.details.folder, text: (p) => p.installLocation ?? "" },
];

/** Heading row, then one row of texts per program. */
function table(programs: readonly Program[], sizes: Sizes): string[][] {
  return [
    FIELDS.map((f) => f.head),
    ...programs.map((p) => FIELDS.map((f) => f.text(p, sizes))),
  ];
}

/** "891 programmes · 120 Go · exporté le 09/11/2026". */
function summary(programs: readonly Program[], sizes: Sizes, now: Date) {
  return [
    fr.programs.count(programs.length),
    formatSize(totalSizeKib(programs, sizes)),
    fr.programs.export.exportedOn(formatDate(isoDate(now))),
  ].join(" · ");
}

/** Semicolon-separated for French spreadsheets; quoted when needed; a leading
 * `= + - @` is defused so no cell runs as a formula. BOM so Excel reads UTF-8. */
function csv(rows: string[][]): string {
  const cell = (text: string) => {
    const safe = /^[=+\-@\t\r]/.test(text) ? `'${text}` : text;
    return /[;"\r\n]/.test(safe) ? `"${safe.replaceAll('"', '""')}"` : safe;
  };
  return `﻿${rows.map((r) => r.map(cell).join(";")).join("\r\n")}\r\n`;
}

/** Columns padded to their widest cell. */
function txt(rows: string[][], title: string, about: string): string {
  const widths = FIELDS.map((_, i) =>
    Math.max(...rows.map((r) => (r[i] ?? "").length)),
  );
  const line = (r: string[]) =>
    r
      .map((text, i) => text.padEnd(widths[i] ?? 0))
      .join("  ")
      .trimEnd();
  return `${[title, about, "", ...rows.map(line)].join("\r\n")}\r\n`;
}

function escapeHtml(text: string): string {
  return text.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ] ?? c,
  );
}

/** A standalone page: it opens outside the app, so it carries its own styles. */
function html(rows: string[][], title: string, about: string): string {
  const [head = [], ...body] = rows;
  const cells = (r: string[], tag: string) =>
    `<tr>${r.map((t) => `<${tag}>${escapeHtml(t)}</${tag}>`).join("")}</tr>`;
  return `<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<title>${escapeHtml(title)}</title>
<style>
body { font: 14px system-ui, sans-serif; margin: 2rem; }
table { border-collapse: collapse; width: 100%; }
th, td { padding: 4px 8px; border-bottom: 1px solid #ddd; text-align: left; }
th { position: sticky; top: 0; background: #fff; }
</style>
</head>
<body>
<h1>${escapeHtml(title)}</h1>
<p>${escapeHtml(about)}</p>
<table>
<thead>${cells(head, "th")}</thead>
<tbody>
${body.map((r) => cells(r, "td")).join("\n")}
</tbody>
</table>
</body>
</html>
`;
}

/** The file's content for `format`. */
export function exportContent(
  format: ExportFormat,
  programs: readonly Program[],
  sizes: Sizes,
  now: Date,
): string {
  const rows = table(programs, sizes);
  const title = fr.programs.list;
  const about = summary(programs, sizes, now);
  switch (format) {
    case "csv":
      return csv(rows);
    case "txt":
      return txt(rows, title, about);
    case "html":
      return html(rows, title, about);
  }
}
