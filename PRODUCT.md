# Uninstaller — produit

Usage personnel, Windows 10/11 x64, interface en français. Parité fonctionnelle avec Revo Uninstaller Free + Pro (référence : revouninstaller.com, novembre 2026), sous une identité visuelle propre.

## Fonctionnalités (parité)

| # | Fonction | Revo | Notes Uninstaller |
|---|----------|------|-----------------------|
| F1 | Liste des programmes installés : nom, icône, taille, version, 32/64 bits, date d'installation, éditeur, site, commentaire, chaîne de désinstallation, dossier | Free | Registre HKLM, HKLM\WOW6432Node, HKCU (+ autres profils chargés), MSI. Composants système et mises à jour masqués par défaut. |
| F2 | Recherche instantanée, tri par colonne, choix des colonnes, vues Détails / Icônes / Liste, regroupement (éditeur, date, taille) | Free | Recherche sur nom, éditeur, commentaire, dossier. |
| F3 | Mise en avant des installations récentes, signalement des entrées orphelines (programme absent du disque) | Pro | |
| F4 | Désinstallation guidée : point de restauration → désinstalleur natif → scan des restes → choix → suppression en quarantaine | Free | Trois profondeurs : Sûr, Modéré, Avancé. |
| F5 | Scan des restes : dossiers/fichiers (Program Files, ProgramData, AppData, raccourcis), registre (Software, Classes, services, tâches planifiées, démarrage, extensions shell, règles pare-feu) | Free / Pro élargi | Arbre à cases à cocher, éléments en gras = confiance haute. |
| F6 | Désinstallation forcée d'un programme déjà retiré ou cassé (par nom, dossier, exécutable ou entrée orpheline) | Pro | |
| F7 | Désinstallation rapide / multiple en lot (silencieuse si le désinstalleur le permet : MSI `/qn`, Inno `/VERYSILENT`, NSIS `/S`) | Pro | File séquentielle, un point de restauration pour le lot. |
| F8 | Applications Windows (UWP/MSIX) : liste et désinstallation, y compris préinstallées | Free/Pro | Via API `PackageManager`. |
| F9 | Extensions de navigateur (Chrome, Edge, Brave, Firefox) : liste et suppression | Pro | Navigateur fermé requis. |
| F10 | Mode Chasseur : cible flottante à glisser sur une fenêtre / icône → désinstaller, tuer le processus, retirer du démarrage, ouvrir le dossier, chercher en ligne | Free/Pro | |
| F11 | Suivi d'installation en temps réel : capture des changements registre + fichiers pendant une installation → journal → désinstallation parfaite plus tard | Pro | Instantanés avant/après d'abord, ETW ensuite. Pas de base de journaux communautaire (propriété de Revo) : uniquement mes journaux locaux. |
| F12 | Sauvegardes multiniveaux : points de restauration, sauvegarde complète du registre, sauvegarde de chaque élément supprimé ; gestionnaire de sauvegardes avec restauration | Pro | Quarantaine + `.reg` (voir `SECURITY.md`). |
| F13 | Gestionnaire de démarrage (Run, RunOnce, dossiers Démarrage, tâches, services) : activer/désactiver/supprimer | Free | |
| F14 | Nettoyeur de fichiers inutiles (temp, caches, journaux, corbeille, Windows Update) | Free/Pro | Aperçu et taille avant suppression. |
| F15 | Nettoyeur de navigateurs (historique, cache, cookies) | Free/Pro | |
| F16 | Nettoyeur d'historique Office et Windows (MRU, documents récents, Exécuter, recherche) | Free/Pro | |
| F17 | Suppression irrécupérable (broyeur de fichiers) et effacement de l'espace libre | Free/Pro | Avertissement SSD/TRIM honnête. |
| F18 | Raccourcis vers les outils Windows (gestion des disques, services, msconfig…) | Free | |
| F19 | Export de la liste (HTML, CSV, TXT) et impression | Free/Pro | |
| F20 | Ligne de commande (`uninstaller uninstall <id> --mode=avance`) et mode portable | Pro | |

## Hors périmètre

Assistance Revo, base de journaux communautaire, notation par les utilisateurs, licence, mises à jour automatiques réseau (optionnel plus tard).

## Critères d'expérience

- Usage ponctuel : ouverte pour supprimer, puis fermée. Aucun processus résident, aucune tâche planifiée, aucun service.
- Fenêtre utilisable < 1 s à froid ; liste visible < 300 ms après le début du chargement de la page (le démarrage de WebView2, ≈ 400 ms, n'est pas compté : décision du 2026-11-09, `docs/perf.md`) ; tailles et icônes en streaming sans saut de mise en page ; l'UI reste fluide pendant scans et désinstallations.
- Mémoire privée propre (application + rendu) < 150 Mo pendant l'usage, hors socle fixe WebView2 (voir `ARCHITECTURE.md`) ; aucune fuite après 50 désinstallations simulées.
- Toute action destructive : aperçu, compteur, confirmation, puis « Annuler » possible depuis la quarantaine.
