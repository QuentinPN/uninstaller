# Uninstaller — instructions Claude Code

## Mission

Désinstalleur Windows 10/11 personnel, rapide, sobre en mémoire, sûr et agréable, qui couvre toutes les fonctionnalités de Revo Uninstaller Free et Pro (`PRODUCT.md`) sous une identité originale. Interface en français.

## Ordre de lecture

Au début de chaque tâche : `PRODUCT.md`, `ARCHITECTURE.md`, `DESIGN.md`, `SECURITY.md`, `ROADMAP.md`, `STATE_MACHINES.md` et la partie utile de `TESTING.md`. Inspecte le dépôt réel avant de modifier. Si deux documents se contredisent, signale-le et propose une résolution.

## Organisation des agents

- La session principale reste sur **Opus 5.5** : elle planifie, délègue, lit uniquement les résumés et valide. Elle ne lit pas de gros fichiers elle-même quand `explorer` peut résumer.
- `explorer` → recherche et résumé (< 200 mots). `fixer` → plus petit patch. `checker` → revue du patch contre ses appelants, après chaque patch de `fixer`.
- Écris directement, sans montrer de diff ; réponses concises. Ne demande que pour une vraie décision (choix produit ou design) ou une action listée dans « Sécurité système ».

## Règles de code

- **DRY** : Code MUST be DRY. Do not duplicate definitions or logic. When the same constant, type, validation, or block of logic appears in more than one place, abstract it into a shared helper/utility and reuse it. Flag copy-pasted logic and repeated literals/definitions that should be centralized.
  - Types d'IPC : définis une fois en Rust, exportés en TS par génération (`src/lib/ipc/bindings.ts`, jamais édité à la main).
  - Chemins et clés protégés, chemins de registre de désinstallation, libellés : un seul module chacun (`core/protect.rs`, `core/registry/paths.rs`, `src/lib/i18n/fr.ts`).
  - Couleurs, rayons, espacements, durées : uniquement via les variables de `src/app.css`, jamais de valeur brute dans un composant.
- Pas de code mort : toute fonction ou fichier de `src/` et `src-tauri/src/` a un appelant.
- `unsafe` Rust : uniquement dans `src-tauri/src/win/`, chaque bloc commenté `// SAFETY:`.
- Petites tranches verticales ; pas de réécriture spéculative ni d'abstraction prématurée.

## Barrières de qualité

`npm run verify` (Biome, svelte-check, Vitest, build, cargo fmt, clippy `-D warnings`, cargo test) doit sortir à 0 avant de déclarer une tâche finie. Ne prétends jamais qu'un test passe, qu'une mesure est atteinte ou qu'une fonctionnalité marche sans l'avoir exécuté.

## Sécurité système (voir `SECURITY.md`)

- Rien n'est supprimé directement : déplacement en quarantaine + sauvegarde `.reg`, purge après délai.
- `core/protect.rs` est consulté par **chaque** opération de suppression ; un test vérifie que les chemins système sont refusés.
- Demande avant : désinstallation réelle sur la machine de l'utilisateur, écriture dans son registre, point de restauration, purge de quarantaine, push.
- Tests destructifs : seulement sur `fixtures/` (programme factice installé par le script de test) ou dans une VM.

## Skills

- UI/UX → `ui-ux-pro-max` (global), `apple-design` (projet), audit `unslop` (global) de chaque écran.
- Registre, MSI, UWP, scan des restes → `.claude/skills/windows-uninstall/SKILL.md`
- Lint/tests/build/CI → `.claude/skills/quality-gates/SKILL.md`
- Élévation, IPC, quarantaine, suppression → `.claude/skills/security-review/SKILL.md`
- Prompts d'entretien (code mort, doublons, dépendances, worktrees, machines à états) → `docs/MAINTENANCE_PROMPTS.md`

## Fin de tâche

Résumé, fichiers modifiés, commandes et résultats, limites, prochaine tâche. Mettre à jour `ROADMAP.md`, `STATE_MACHINES.md` et les ADR quand le comportement change.

## Compaction

Auto-compaction à 20 % (`.claude/settings.local.json`). À la compaction, garder : branche, fichiers non commités, dernier commit ; décisions et raisons ; demandes et refus de l'utilisateur dans ses mots ; reste à faire ; fichiers clés et pièges rencontrés.
