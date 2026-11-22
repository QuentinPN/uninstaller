# Uninstaller

Désinstalleur Windows 10/11 personnel : rapide, sobre, sûr, agréable. Parité fonctionnelle avec Revo Uninstaller Free + Pro, identité visuelle propre (thème partagé avec Viewer et Hub).

## Starter-pack

| Fichier | Rôle |
|---------|------|
| `PROMPT_CLAUDE.md` | Prompt à coller dans une nouvelle session Claude Code pour lancer la Phase 0 |
| `CLAUDE.md` | Règles permanentes du projet (DRY, agents, diff avant écriture, sécurité) |
| `PRODUCT.md` | Fonctions F1-F20 et critères d'expérience |
| `ARCHITECTURE.md` | Tauri 2 + Rust `core/` + `win/`, IPC typée, perf, élévation |
| `DESIGN.md` | Thème, écrans, états |
| `SECURITY.md` | Quarantaine, protection, broker, tests obligatoires |
| `STATE_MACHINES.md` | Machines à états (document vivant) |
| `ROADMAP.md` · `TESTING.md` | Phases 0-9, stratégie de tests |
| `docs/MAINTENANCE_PROMPTS.md` | Prompts d'entretien (code mort, doublons, dépendances, worktrees) |
| `.claude/agents/` | `explorer` (haiku), `fixer` (sonnet), `checker` (sonnet) |
| `.claude/skills/` | `windows-uninstall`, `security-review`, `quality-gates`, `apple-design` |
| `src/app.css`, configs | Tokens et outillage repris d'Viewer |

## Démarrer

```bash
npm install
```

Puis ouvrir Claude Code ici et coller `PROMPT_CLAUDE.md`.

## Hooks git (hk)

Les hooks sont gérés par [hk](https://hk.jdx.dev) (`hk.pkl`). Une seule fois par poste : installer hk 2.13.0 (Windows : `hk.exe` de la [release GitHub](https://github.com/jdx/hk/releases/tag/v2.13.0), sinon `cargo install hk --version 2.13.0 --locked`), puis lancer `hk install` dans le clone, après `npm install`. Facultatif : `git config hk.stashUntracked false` évite environ 1 s au pre-commit quand des fichiers non suivis traînent.

- `pre-commit` : Biome (format, lint, imports) et `rustfmt` sur les seuls fichiers indexés, avec correction et réindexation ; une erreur restante bloque le commit. Environ 0,3 s (rustfmt compris).
- `pre-push` : `lint`, `check`, `test` et `build` (le `verify:web` de la CI), environ 5 s ; `cargo fmt` et `clippy` (environ 5 s à chaud) quand des fichiers de `src-tauri/` sont poussés, ce qui demande une chaîne MSVC complète. `cargo test`, `npm audit` et `cargo deny` restent à `npm run verify`.
