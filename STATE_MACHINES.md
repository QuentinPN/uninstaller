# Uninstaller — machines à états

> Document vivant. Pour chaque machine : états, invariants, table de transitions, états/transitions invalides atteignables (avec preuve), bugs liés. Mis à jour à chaque correction (prompt dans `docs/MAINTENANCE_PROMPTS.md`).

## Machines prévues

### 1. Tâche de désinstallation (`core/uninstall`)

États : `Idle → RestorePoint → NativeUninstaller → AwaitingScan → Scanning → AwaitingSelection → Quarantining → Done` ; terminaux `Cancelled`, `Failed{étape, cause}`.

Invariants : une seule tâche active par programme ; rien n'est supprimé avant `Quarantining` ; `Cancelled` n'est possible qu'avant `Quarantining` ; `Quarantining` interrompu reprend depuis le manifeste.

| De | Événement | Vers | Garde |
|----|-----------|------|-------|
| Idle | start | RestorePoint | programme pas déjà en cours |
| RestorePoint | ok / skipped(limite 24 h, accepté) | NativeUninstaller | |
| NativeUninstaller | exit | AwaitingScan | code de sortie journalisé |
| AwaitingScan | scan(mode) | Scanning | |
| Scanning | done | AwaitingSelection | |
| AwaitingSelection | confirm(sélection non vide) | Quarantining | sélection ∩ protégés = ∅ |
| Quarantining | done | Done | manifeste complet |
| * (avant Quarantining) | cancel | Cancelled | |
| * | error | Failed | |

### 6. Chargement de l'inventaire — liste (Phase 1, tranche 1)

`ProgramsView.svelte` : `programs = null` (chargement) · `programs = [...]` (chargé) · `failed = true` (erreur). Lecture seule, pas d'effet de bord : relancer est toujours sûr.

| État | Événement | Suivant | UI |
|---|---|---|---|
| Loading | `listPrograms` résolu | Loaded | liste virtualisée, ou « Aucun programme trouvé » si le filtre vide la liste |
| Loading | `listPrograms` rejeté | Failed | cause + « Réessayer » |
| Failed | Réessayer | Loading | squelettes de lignes |

Panneau de détails (tranche 2) : fermé → ouvert par Espace ou double-clic sur la ligne sélectionnée ; ouvert, il suit la sélection ; Échap ou ✕ le ferme et rend le focus à la liste. Si le programme sélectionné disparaît (rechargement), le panneau se ferme de lui-même.

Une source de registre illisible (clé absente, accès refusé) est ignorée côté Rust sans faire échouer l'ensemble. Tailles calculées et icônes en streaming : à ajouter ici avec leur tranche.

### 2. Scan des restes · 3. Lot en quarantaine (`Active → Restoring → Restored` / `Purging → Purged`) · 4. Session de suivi d'installation · 5. Mode Chasseur

À détailler en Phase 2-8, au même format.

## États ou transitions invalides trouvés

| # | Machine | Description | Preuve | Impact | Statut |
|---|---------|-------------|--------|--------|--------|
| — | — | — | — | — | — |
