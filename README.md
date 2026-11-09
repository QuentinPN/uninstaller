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
