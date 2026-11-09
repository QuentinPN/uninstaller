# Uninstaller — tests

## Commandes

- `npm run verify` : Biome, svelte-check, Vitest, build, cargo fmt, clippy `-D warnings`, cargo test. Doit sortir à 0.
- `npm test` : Vitest seul (utilisé par les prompts d'entretien).

## Niveaux

1. **Unitaires Rust** (`core/`) : analyse des chaînes de désinstallation, heuristiques de restes, protection, manifeste de quarantaine, transitions de chaque machine à états (une par ligne de table de `STATE_MACHINES.md`).
2. **Unitaires TS** : stores, formatage (tailles, dates en français), filtres et tris.
3. **Intégration Windows** : `fixtures/` contient un faux programme (installeur Inno ou NSIS minimal, généré par script) qui écrit des fichiers, des clés, un raccourci, une entrée Run et une tâche planifiée. Les tests l'installent, le désinstallent via l'application et vérifient qu'il ne reste rien et que la restauration remet tout. Jamais sur les programmes réels de l'utilisateur.
4. **Régression d'états** : chaque bug d'état corrigé ajoute un test qui rejoue la séquence (y compris reprise et concurrence).
5. **UI** : captures à 100/125/150 % ; clavier seul ; mouvement réduit.
6. **Performance** : mesure du démarrage, de la mémoire de travail au repos et après 50 cycles, du temps d'inventaire ; résultats datés dans `docs/perf.md`.

## Règle

Aucun test sauté en silence : un test qui exige l'administrateur ou Windows se marque explicitement et la CI Windows l'exécute.
