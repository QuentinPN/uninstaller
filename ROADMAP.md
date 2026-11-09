# Uninstaller — feuille de route

Statuts : À faire · En cours · Fait (avec preuve : commande, test ou capture).

| Phase | Contenu | Fonctions | Statut |
|-------|---------|-----------|--------|
| 0 | Socle Tauri 2 + Svelte 5, config reprise d'Viewer, `npm run verify` (local, pas de CI), ADR 0001-0003, barre de titre, navigation, mesure à vide | — | Fait — `npm run verify` à 0 (deps comprises), mesure dans `docs/perf.md` (fenêtre 32 ms, mémoire propre 34,8 Mo), captures 100/125/150 % (`scripts/capture-scaled.ps1`, mise à l'échelle simulée dans WebView2), icône = logo 12 « U-corbeille » |
| 1 | Inventaire registre + MSI, liste virtualisée, recherche, tri, détails, icônes et tailles en streaming, export | F1 F2 F3 F19 | En cours — tranche 1 faite : inventaire HKLM 64/32 + HKCU (`core/inventory.rs`, 257 programmes lus en ≈ 55 ms), liste virtualisée, recherche, tri ; tranche 2 : sélection au clavier (flèches, Pg préc./suiv., Début/Fin), panneau de détails (Espace/double-clic, Échap), « Copier », « Ouvrir le dossier » par identifiant opaque. tranche 3 : sélection multiple (Ctrl/Maj + clic, Maj + flèches, Ctrl+A, nombre et taille sélectionnés) ; tranche 4 : icônes (ADR 0004, `PrivateExtractIconsW` + repli `.mun`, 224/236 extraites en ≈ 3,6 ms chacune, glyphe si absente) ; tranche 5 : tailles sur le disque en flux (`core/sizes.rs`, `Channel`, 4 dossiers en parallèle, jonctions non suivies, dossiers partagés refusés par `core/protect.rs` ; 175 programmes mesurés en 2,7-4,9 s, premier paquet ≈ 130 ms) ; tranche 6 : bascules « Composants système » et « Mises à jour » (masqués par défaut, étiquette sur la ligne). Reste : profils HKU, colonnes/vues/regroupement, récents/orphelins, export |
| 2 | Quarantaine, sauvegardes `.reg`, liste de protection, point de restauration, écran Sauvegardes | F12 | À faire |
| 3 | Désinstallation guidée + scan des restes Sûr/Modéré/Avancé + arbre à cases | F4 F5 | À faire |
| 4 | Désinstallation forcée, lots silencieux | F6 F7 | À faire |
| 5 | Applications Windows, extensions de navigateur | F8 F9 | À faire |
| 6 | Mode Chasseur | F10 | À faire |
| 7 | Gestionnaire de démarrage, nettoyeurs, broyeur, outils Windows | F13–F18 | À faire |
| 8 | Suivi d'installation (instantanés, puis ETW) et désinstallation par journal | F11 | À faire |
| 9 | Ligne de commande, mode portable, installeur NSIS, audit sécurité et perf final | F20 | À faire |

Chaque phase se termine par : `npm run verify` à 0, tests sur `fixtures/` ou VM, captures 100/125/150 % (`scripts/capture-scaled.ps1`), `docs/perf.md` et `STATE_MACHINES.md` à jour.
