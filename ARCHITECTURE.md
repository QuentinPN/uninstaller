# Uninstaller — architecture

## Vue d'ensemble

```
Svelte 5 (WebView2)  ──invoke/Channel──▶  Tauri commands (typées, validées)
                                              │
                                     core (Rust, sans Tauri)
   inventory ─ uninstall ─ leftovers ─ quarantine ─ restore_point ─ monitor
   hunter ─ startup ─ cleaners ─ shredder ─ browsers ─ appx ─ export
                                              │
                                    win/ (seul module unsafe)
              registre · Shell/icônes · processus · WMI · ETW · PackageManager
```

- `src-tauri/src/core/` : logique pure, testable sans fenêtre, sans `tauri` en dépendance. Un module par fonctionnalité de `PRODUCT.md`.
- `src-tauri/src/win/` : wrappers fins et sûrs autour de la crate `windows`. Tout `unsafe` vit ici.
- `src-tauri/src/commands.rs` : seule frontière IPC. Chaque commande valide ses entrées (identifiants opaques, jamais de chemin libre venant de l'UI pour une suppression).
- `src/lib/ipc/bindings.ts` : généré depuis Rust (specta ou ts-rs). L'UI n'appelle jamais `invoke` directement, seulement ces fonctions.
- `src/lib/<fonction>/` : un dossier par écran (store `$state`, composants, tests).

## Choix de la pile (décidé le 2026-11-09, à formaliser dans l'ADR 0001)

Tauri 2 + Svelte 5 retenu face à egui/eframe et fastframe : l'application n'est ouverte que ponctuellement, donc la mémoire de base de WebView2 compte peu ; ce qui compte est la vitesse pendant l'usage, assurée par le cœur Rust. Tauri garde le thème et les composants communs avec Viewer et Hub, l'accessibilité HTML et le flou natif. fastframe écarté (v0.17, hors crates.io, forks d'egui/winit). `core/` reste indépendant de Tauri, donc l'interface reste remplaçable.

## Performance et mémoire

- Priorité au démarrage à froid : inventaire lancé sur un thread dès `run()`, en parallèle du démarrage de WebView2, et remis au premier `list_programs` (les suivants relisent le registre), page initiale légère, écrans secondaires chargés à la demande (`import()`).

- Inventaire : HKLM 64 et 32 bits, HKCU, puis chaque autre compte chargé sous HKU (`core/profiles.rs`, sources décrites une fois dans `core/registry/paths.rs` ; sauvegarde `.reg` d'une clé et réimport par `core/registry/reg_file.rs`) ; lecture registre parallèle (une tâche par ruche), résultat initial sans taille ni icône ; tailles calculées ensuite (`std::fs::read_dir`, qui s'appuie sur `FindFirstFileExW` et lit la taille dans l'entrée du répertoire sans ouvrir le fichier ; 4 dossiers en parallèle ; jonctions et liens non suivis ; dossiers partagés refusés par `core/protect.rs`) et poussées par `tauri::ipc::Channel` par paquets.
- Icônes (ADR 0004) : extraites à la demande pour les lignes visibles (`loading="lazy"`), converties en PNG 48 px (net jusqu'à 150 %), servies par le schéma `icon://` à partir de l'identifiant opaque du programme ; cache HTTP de la WebView (`max-age=3600`), pas encore de cache disque. Jamais de base64 dans l'IPC.
- Liste virtualisée (rendu des seules lignes visibles ; en grille pour les vues Icônes et Liste, lignes de hauteur fixe, en-têtes de groupe sur une ligne entière, navigation clavier en 2D dans `ui/listNav.ts`), recherche côté Rust si > 1000 éléments, sinon en mémoire côté UI.
- Scan des restes : itérateurs, pas de collecte complète en mémoire ; annulation par `CancellationToken` vérifié dans chaque boucle.
- Profil release : `lto`, `codegen-units = 1`, `strip`, `opt-level = 3` (comme Viewer). Pas de runtime tokio multi-thread si un pool borné suffit.
- Budget mesuré dans `docs/perf.md` à chaque jalon (démarrage, mémoire de travail, temps d'inventaire).
- Mémoire : la cible de 150 Mo porte sur la mémoire privée **propre** (processus de l'application + processus de rendu WebView2), seule partie que notre code fait varier. Le socle WebView2 (navigateur, GPU, réseau, stockage, crashpad ≈ 135 Mo privés au repos, dont ≈ 78 Mo pour le GPU) est fixe : des options de lancement WebView2 n'y changent rien de mesurable (essai du 2026-11-09) et couper le GPU sacrifierait le flou. Le total reste mesuré et suivi.

## Élévation (ADR 0002)

L'application démarre non élevée (lecture, recherche, export). Les opérations qui exigent l'administrateur passent par un **broker élevé** lancé à la demande (le même exécutable avec `--broker <tube>`) qui parle par tube nommé limité à l'utilisateur (`win/pipe.rs`), protocole versionné en trames JSON et liste fermée d'opérations (`core/broker.rs`). La WebView n'est jamais élevée.

## Export (F19)

Le contenu (HTML, CSV, TXT) est construit côté UI (`src/lib/programs/export.ts`) : elle seule connaît l'ordre affiché, les filtres, les tailles mesurées et les libellés français. Rust ne fait qu'ouvrir la boîte « Enregistrer sous » (`win/dialog.rs`, `IFileSaveDialog` sur un thread COM STA dédié) et écrire le fichier ; pas de module `core/export`. L'impression (« Imprimer… » du même menu) réutilise le même tableau (`listDocument`) : `PrintSheet.svelte` l'ajoute à la fin de `<body>`, `@media print` (`app.css`) masque `#app`, puis `window.print()` ouvre la boîte d'impression de WebView2 ; la feuille disparaît à `afterprint`. Pas d'`iframe` ni de style en ligne : la CSP `style-src 'self'` reste inchangée.

## Données locales

`%LOCALAPPDATA%\Uninstaller\` : `quarantine/<lot>/` (fichiers déplacés, `.reg` des clés supprimées et `manifest.json` portant l'empreinte SHA-256 de son contenu, ADR 0003 et 0005), `traces/` (journaux de suivi d'installation), `cache/icons/`, `settings.json`. Mode portable : même arborescence à côté de l'exécutable.

## Machines à états

Désinstallation, scan, suivi d'installation, quarantaine, mode Chasseur : décrites dans `STATE_MACHINES.md`, implémentées par des `enum` Rust aux transitions exhaustives (`match` sans `_`).
