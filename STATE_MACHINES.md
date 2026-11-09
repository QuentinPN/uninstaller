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

### 2. Scan des restes · 3. Lot en quarantaine (`Active → Restoring → Restored` / `Purging → Purged`) · 4. Session de suivi d'installation · 5. Mode Chasseur · 6. Chargement de l'inventaire (liste, tailles, icônes)

À détailler en Phase 1-8, au même format.

## États ou transitions invalides trouvés

| # | Machine | Description | Preuve | Impact | Statut |
|---|---------|-------------|--------|--------|--------|
| — | — | — | — | — | — |
