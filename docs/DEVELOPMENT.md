# Guide développeur

Documentation technique de Uninstaller (Tauri 2 + Svelte 5 + Rust, Windows 10/11 x64). Pour la présentation du produit, voir le [README](../README.md) ([version française](../README.fr.md)).

## Prérequis

- Windows 10 ou 11 (x64) avec WebView2 (livré avec Windows 11 et les Windows 10 à jour).
- Node.js 22 ou plus (`engines` de `package.json`).
- Rust 1.88 ou plus (édition 2024) avec la chaîne MSVC complète (Build Tools Visual Studio, charge de travail « Développement Desktop en C++ »).

## Démarrer

```bash
npm install
npm run tauri dev      # fenêtre de développement (Vite + Rust)
npm run tauri build    # installeur NSIS (cible `nsis` de src-tauri/tauri.conf.json)
```

`npm run dev` seul lance l'interface dans un navigateur sur `http://localhost:1420` : sans Tauri, les commandes IPC échouent, donc la liste des programmes affiche une erreur (utile seulement avec une IPC simulée).

## Commandes

| Commande | Effet |
|----------|-------|
| `npm run lint` · `npm run lint:fix` | Biome (`biome check .`), sans ou avec correction |
| `npm run check` | `svelte-check` (avertissements bloquants) |
| `npm test` | Vitest (logique TypeScript) |
| `npm run build` | Build web (Vite) |
| `npm run verify:web` | Lint, check, tests et build : le contrôle de la CI |
| `npm run verify:rust` | `cargo fmt --check`, `clippy -D warnings`, `cargo test` |
| `npm run verify:deps` | `npm audit` et `cargo deny` |
| `npm run verify` | Les trois ensemble : doit sortir à 0 avant de fusionner |

## Documentation du dépôt

| Fichier | Rôle |
|---------|------|
| [`PRODUCT.md`](../PRODUCT.md) | Fonctions F1-F20 et critères d'expérience |
| [`ARCHITECTURE.md`](../ARCHITECTURE.md) | Tauri 2 + Rust `core/` + `win/`, IPC typée, performance, élévation |
| [`DESIGN.md`](../DESIGN.md) | Thème, écrans, états |
| [`SECURITY.md`](../SECURITY.md) | Quarantaine, protection, broker, tests obligatoires |
| [`STATE_MACHINES.md`](../STATE_MACHINES.md) | Machines à états (document vivant) |
| [`ROADMAP.md`](../ROADMAP.md) | Phases 0-9 et avancement |
| [`TESTING.md`](../TESTING.md) | Stratégie de tests (unitaires, intégration Windows avec `fixtures/`) |
| [`CHANGELOG.md`](../CHANGELOG.md) | Historique des versions |
| [`CLAUDE.md`](../CLAUDE.md) · [`PROMPT_CLAUDE.md`](../PROMPT_CLAUDE.md) | Règles permanentes du projet et prompt de départ pour Claude Code |
| [`docs/adr/`](adr) | Décisions d'architecture (pile, élévation, quarantaine, icônes, manifeste) |
| [`docs/perf.md`](perf.md) | Mesures de performance et de mémoire |
| [`docs/MAINTENANCE_PROMPTS.md`](MAINTENANCE_PROMPTS.md) | Prompts d'entretien (code mort, doublons, dépendances, worktrees) |

Le dossier `.claude/` regroupe les agents (`explorer`, `fixer`, `checker`) et les skills du projet (`windows-uninstall`, `security-review`, `quality-gates`, `apple-design`). `src/app.css` porte les tokens du thème partagé avec les autres applications du même auteur.

## Hooks git (hk)

Les hooks sont gérés par [hk](https://hk.jdx.dev) (`hk.pkl`). Une seule fois par poste : installer hk 2.13.0 (Windows : `hk.exe` de la [release GitHub](https://github.com/jdx/hk/releases/tag/v2.13.0), sinon `cargo install hk --version 2.13.0 --locked`), puis lancer `hk install` dans le clone, après `npm install`. Facultatif : `git config hk.stashUntracked false` évite environ 1 s au pre-commit quand des fichiers non suivis traînent.

- `pre-commit` : Biome (format, lint, imports) et `rustfmt` sur les seuls fichiers indexés, avec correction et réindexation ; une erreur restante bloque le commit. Environ 0,3 s (rustfmt compris).
- `pre-push` : `lint`, `check`, `test` et `build` (le `verify:web` de la CI), environ 5 s ; `cargo fmt` et `clippy` (environ 5 s à chaud) quand des fichiers de `src-tauri/` sont poussés, ce qui demande une chaîne MSVC complète. `cargo test`, `npm audit` et `cargo deny` restent à `npm run verify`.
