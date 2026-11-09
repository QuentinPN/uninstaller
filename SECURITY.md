# Uninstaller — sécurité et sûreté système

## Principes

1. **Jamais de suppression directe.** Fichiers et dossiers sont déplacés (même volume : `MoveFileExW`, sinon copie vérifiée par hash puis suppression) vers `quarantine/<lot>/` ; les clés de registre sont exportées en `.reg` puis supprimées. Le `manifest.json` du lot liste chaque élément, son origine et son hash. Purge seulement après N jours (réglage, défaut 14) ou à la demande confirmée.
2. **Liste de protection unique** (`core/protect.rs`) consultée par toute suppression : `%WINDIR%`, `System32`, `WinSxS`, racines de `Program Files`, `Common Files`, `ProgramData` et `AppData` elles-mêmes, profils utilisateurs, `HKLM\SYSTEM\CurrentControlSet` (hors services du programme), `HKLM\SOFTWARE\Microsoft\Windows` et ses parents, `HKCR` racine, pilotes. Toute correspondance refuse l'opération et journalise. Tests dédiés.
3. **Liens et jonctions** : jamais suivis pendant un scan ou une suppression (`FILE_ATTRIBUTE_REPARSE_POINT` ignoré, la jonction elle-même seule peut être retirée).
4. **Point de restauration** avant toute désinstallation ou lot (WMI `SystemRestore.CreateRestorePoint`). Attention : Windows limite à un point par 24 h par défaut (`SystemRestorePointCreationFrequency`) ; détecter, prévenir, ne pas échouer en silence.
5. **Confiance des restes** : un élément n'est « sûr » que s'il est rattaché au programme par au moins deux indices indépendants (dossier d'installation, éditeur, nom de produit, GUID MSI, chemin d'exécutable). Mode Sûr = uniquement ces éléments.
6. **Désinstalleurs natifs** : la chaîne de désinstallation est analysée (exécutable + arguments), exécutée sans shell (`CreateProcessW`, pas de `cmd /c`), avec vérification que l'exécutable existe ; `MsiExec` reconnu et appelé avec le GUID validé.

## Surface d'attaque

- WebView jamais élevée ; broker élevé avec liste fermée d'opérations, entrées validées, identifiants opaques (pas de chemins libres venant de l'UI).
- CSP stricte : `default-src 'self'`, pas de contenu distant, `connect-src ipc:` ; aucune requête réseau sauf « Chercher en ligne » qui ouvre le navigateur par défaut.
- Capacités Tauri minimales (événements, fenêtre). Aucun plugin shell / fs générique.
- Pas de secrets ; journaux sans données personnelles inutiles ; chemins utilisateur abrégés dans les journaux exportés.
- Dépendances épinglées, `cargo deny` (licences, avis RustSec) et `npm audit` en CI ; actions GitHub épinglées par SHA.

## Tests obligatoires

Refus des chemins protégés ; aller-retour quarantaine → restauration identique (fichiers et `.reg`) ; reprise d'un lot interrompu ; aucun suivi de jonction ; désinstalleur introuvable → erreur claire, rien supprimé.
