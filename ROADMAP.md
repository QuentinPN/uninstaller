# Uninstaller — feuille de route

Statuts : À faire · En cours · Fait (avec preuve : commande, test ou capture).

| Phase | Contenu | Fonctions | Statut |
|-------|---------|-----------|--------|
| 0 | Socle Tauri 2 + Svelte 5, config reprise d'Viewer, `npm run verify`, CI, ADR 0001-0003, barre de titre, navigation, mesure à vide | — | En cours — `npm run verify` à 0, `cargo deny check` ok, mesure dans `docs/perf.md` (fenêtre 32 ms, mémoire propre 34,8 Mo), icône = logo 12 « U-corbeille ». Reste : captures 125/150 %, CI jamais exécutée (pas de dépôt distant) |
| 1 | Inventaire registre + MSI, liste virtualisée, recherche, tri, détails, icônes et tailles en streaming, export | F1 F2 F3 F19 | À faire |
| 2 | Quarantaine, sauvegardes `.reg`, liste de protection, point de restauration, écran Sauvegardes | F12 | À faire |
| 3 | Désinstallation guidée + scan des restes Sûr/Modéré/Avancé + arbre à cases | F4 F5 | À faire |
| 4 | Désinstallation forcée, lots silencieux | F6 F7 | À faire |
| 5 | Applications Windows, extensions de navigateur | F8 F9 | À faire |
| 6 | Mode Chasseur | F10 | À faire |
| 7 | Gestionnaire de démarrage, nettoyeurs, broyeur, outils Windows | F13–F18 | À faire |
| 8 | Suivi d'installation (instantanés, puis ETW) et désinstallation par journal | F11 | À faire |
| 9 | Ligne de commande, mode portable, installeur NSIS, audit sécurité et perf final | F20 | À faire |

Chaque phase se termine par : `npm run verify` à 0, tests sur `fixtures/` ou VM, captures 100/125/150 %, `docs/perf.md` et `STATE_MACHINES.md` à jour.
