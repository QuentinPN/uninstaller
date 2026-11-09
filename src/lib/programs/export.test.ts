import { describe, expect, it } from "vitest";
import { exportContent } from "./export";
import { program } from "./fixture";

const NOW = new Date(2026, 10, 9);
const programs = [
  program("Outil", {
    publisher: "Uninstaller; Cie",
    version: "1.2",
    installDate: "2024-03-15",
    estimatedSizeKib: 2048,
    installLocation: "C:\\Outils",
  }),
  program('=1+1 "piège"', { scope: "user" }),
];
const content = (format: "html" | "csv" | "txt") =>
  exportContent(format, programs, new Map(), NOW);

describe("exportContent", () => {
  it("writes CSV for French spreadsheets, quoted and without formulas", () => {
    const lines = content("csv").split("\r\n");
    expect(lines[0]).toBe(
      "\uFEFFNom;Éditeur;Version;Taille;Date d'installation;Installation;Dossier",
    );
    expect(lines[1]).toBe(
      'Outil;"Uninstaller; Cie";1.2;2 Mo;15/03/2024;64 bits;C:\\Outils',
    );
    expect(lines[2]).toBe(`"'=1+1 ""piège""";;;;;Utilisateur;`);
  });

  it("aligns text columns under a title and a summary", () => {
    const lines = content("txt").split("\r\n");
    expect(lines[0]).toBe("Programmes installés");
    expect(lines[1]).toBe("2 programmes · 2 Mo · exporté le 09/11/2026");
    const [head = "", first = ""] = lines.slice(3);
    expect(first.indexOf("Uninstaller")).toBe(head.indexOf("Éditeur"));
  });

  it("escapes HTML", () => {
    const html = content("html");
    expect(html).toContain("<td>=1+1 &quot;piège&quot;</td>");
    expect(html).not.toContain('"piège"');
  });
});
