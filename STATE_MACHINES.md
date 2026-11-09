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

Une source de registre illisible (clé absente, accès refusé) est ignorée côté Rust sans faire échouer l'ensemble. Filtre de type (tranche 6) : `shown` (vide au départ) ; chaque bascule ajoute ou retire « Composants système » ou « Mises à jour » ; les applications restent toujours visibles. Pas de persistance : chaque lancement repart du filtre par défaut. Vue (tranche 13) : `view` = Détails au départ ; « Icônes » et « Liste » remontent la liste (taille de cellule lue dans les jetons CSS) en gardant sélection, tri, groupes et filtres ; le nombre de cellules par ligne suit la largeur ; « Colonnes » est désactivé hors Détails ; non persisté. Export (tranche 12) : `exported` vide au départ ; un export enregistré affiche son chemin, un échec d'écriture un message d'erreur, une annulation ne change rien ; l'état reste jusqu'au prochain export ; plusieurs boîtes peuvent s'ouvrir l'une après l'autre, chacune sur son thread. Tri (tranche 11) : `sort` = nom, `reversed` = faux au départ ; cliquer l'en-tête de la colonne triée bascule `reversed`, cliquer une autre colonne ou choisir dans « Trier par » la sélectionne avec `reversed` = faux ; non persisté. Filtres (tranche 10) : `filters` vide au départ ; « Récents » et « Orphelins » se cumulent (un programme doit passer chacun), en plus du filtre de type et de la recherche. La date limite des récents est fixée à l'ouverture de l'écran ; l'état orphelin est calculé à chaque inventaire (`scan`, `find`) ; non persisté. Colonnes (tranche 9) : Taille, Date et Installation cochées au départ ; chaque case ajoute ou retire sa colonne, l'ordre reste fixe ; non persisté. Regroupement (tranche 8) : `group` vaut `none` au départ ; changer de groupe, de tri, de recherche ou recevoir des tailles recalcule l'ordre et les en-têtes sans toucher à la sélection ; non persisté. Autres comptes (tranche 7) : lus à chaque inventaire parmi les ruches chargées sous HKU à cet instant ; un compte qui se déconnecte disparaît au rechargement suivant, et un identifiant `hku\…` dont la ruche n'est plus chargée se lit comme un programme disparu.

Icônes (tranche 4) : chaque ligne demande `icon://<id>` à l'affichage ; 404 ou image illisible → glyphe, retenu pour cette URL.

Tailles calculées (tranche 5) : à chaque passage en Loaded, `computeSizes` part avec un nouveau `Channel` et `sizing = true` (« Calcul des tailles… » dans l'en-tête). Chaque paquet remplit `sizes` (id → Kio) ; liste, tri et totaux préfèrent la taille mesurée à la taille déclarée. Fin ou erreur → `sizing = false`, sans message : les tailles déclarées restent. Un « Réessayer » vide `sizes` et incrémente `generation` : les paquets d'un flux plus ancien sont ignorés.

### 3. Élément d'un lot en quarantaine (`core/quarantine.rs`, Phase 2 tranche 1)

| État | Événement | État suivant | Effet |
|------|-----------|--------------|-------|
| — | `quarantine(chemin)`, chemin protégé | — | `Error::Protected`, manifeste inchangé |
| — | `quarantine(chemin)` | Pending | ligne écrite dans le manifeste **avant** le déplacement |
| Pending | déplacement réussi | Moved | manifeste réécrit |
| Pending | déplacement échoué (dossier ou lien sur un autre volume : `OtherVolume` ; copie d'un fichier dont l'empreinte diffère : `Altered`) | — | copie partielle retirée du lot, ligne retirée, origine intacte |
| Pending | démarrage (`resume_all`), origine présente | — | copie partielle éventuelle retirée du lot, ligne retirée |
| Pending | démarrage, origine absente, élément dans le lot | Moved | manifeste réécrit |
| Pending | démarrage, origine et lot vides | Pending | `Error::Corrupt`, lot laissé tel quel |
| Moved | `restore()`, origine libre, contenu intact | Restored | élément remis en place (même volume : renommage ; sinon fichier copié, vérifié, puis retiré du lot), manifeste réécrit |
| Moved | `restore()`, origine occupée | Moved | `Error::Exists`, arrêt, rien écrasé |
| Moved | `restore()`, empreinte différente | Moved | `Error::Altered`, arrêt |
| Restored | `restore()` | Restored | ignoré |

Le manifeste est réécrit par fichier temporaire + renommage ; une empreinte qui ne correspond plus donne `Error::Corrupt` à l'ouverture. Seul le dernier élément peut être `Pending` ; sinon `Error::Corrupt` et le lot reste tel quel. La reprise tourne sur un thread au lancement (`run()`), sur `%LOCALAPPDATA%\Uninstaller\quarantine`, et ne fait rien si le dossier n'existe pas. Pas encore faits : états du lot (`Active → Restoring → Restored` / `Purging → Purged`).

### 2. Scan des restes · 4. Session de suivi d'installation · 5. Mode Chasseur

À détailler en Phase 3-8, au même format.

## États ou transitions invalides trouvés

| # | Machine | Description | Preuve | Impact | Statut |
|---|---------|-------------|--------|--------|--------|
| — | — | — | — | — | — |
