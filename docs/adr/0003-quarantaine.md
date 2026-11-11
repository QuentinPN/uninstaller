# ADR 0003 — Quarantaine plutôt que suppression

- Statut : accepté
- Date : 2026-11-09

## Contexte

Les heuristiques de restes peuvent se tromper. Une suppression directe d'un fichier ou d'une clé de registre partagée peut casser Windows ou un autre programme. Il faut pouvoir revenir en arrière, même après un plantage au milieu d'un lot.

## Décision

- **Lot** : chaque nettoyage crée `quarantine/<horodatage>-<id>/` sous `%LOCALAPPDATA%\Uninstaller\` (ou à côté de l'exécutable en mode portable).
- **Fichiers et dossiers** : `MoveFileExW` s'ils sont sur le même volume ; sinon copie, vérification du hash (SHA-256), puis suppression de la source. Attributs et dates conservés. Les points d'analyse (jonctions, liens) ne sont jamais suivis : seul le lien lui-même est déplacé.
- **Registre** : export de la clé en `.reg` (UTF-16, format `regedit`) dans le lot, puis suppression de la clé.
- **Tâches planifiées** (2026-11-11) : définition XML de la tâche (telle que Task Scheduler l'écrit) gardée dans le lot, vérifiée par son SHA-256, puis suppression de la tâche ; restauration par réenregistrement sous le compte et le type d'ouverture de session qu'elle nomme. Une tâche qui ouvre sa session avec un mot de passe enregistré est refusée (impossible à réenregistrer sans ce mot de passe) ; une suppression refusée par Windows retire l'élément du lot.
- **Services** (2026-11-11) : clé du service (`HKLM\SYSTEM\CurrentControlSet\Services\<nom>`) gardée en `.reg` et vérifiée, puis service arrêté (30 s au plus) et supprimé par le gestionnaire de services, qui retire sa clé. Seul un service que le scan des restes proposerait est accepté (Win32, programme hors dossiers protégés). Restauration par réimport du `.reg` : le gestionnaire ne relit ses services qu'au démarrage, le service revient donc au redémarrage suivant. Arrêt trop long ou refus → élément retiré du lot.
- **Manifeste** (`manifest.json`) : écrit **avant** chaque déplacement (journal en écriture anticipée). Chaque élément porte son origine, sa destination, son type, son hash et un état (`pending`, `moved`, `restored`). Il est mis à jour de façon atomique (fichier temporaire + remplacement, ADR 0005), et il contient le hash de son propre contenu pour détecter une corruption.
- **Reprise** : au démarrage, un lot dont le manifeste contient des éléments `pending` est repris (terminer ou annuler l'élément selon son état réel sur le disque) ; l'opération est idempotente.
- **Protection** : `core/protect.rs` est consulté avant d'écrire la ligne du manifeste ; un refus est journalisé et l'élément est exclu.
- **Restauration** : opération inverse, élément par élément, refusée si la destination existe déjà (pas d'écrasement silencieux) ; les `.reg` sont réimportés sans passer par `regedit` ni par un shell.
- **Purge** : uniquement après N jours (réglage, défaut 14) ou à la demande, avec une confirmation qui montre le nombre d'éléments et la taille. C'est le seul endroit du code où `std::fs::remove_*` est autorisé.
- **Machine à états** du lot : `Active → Restoring → Restored` et `Active → Purging → Purged`, détaillée dans `STATE_MACHINES.md` en Phase 2.

## Alternatives écartées

- **Corbeille Windows** : pas de place pour le registre, pas de manifeste, vidable par l'utilisateur, aucun contrôle de reprise.
- **Suppression avec simple sauvegarde ZIP** : pas de restauration exacte des attributs, et une reprise après plantage plus difficile.

## Conséquences

- La quarantaine occupe de l'espace disque jusqu'à la purge ; l'écran Sauvegardes l'affiche.
- Déplacer vers un autre volume coûte du temps (copie + hash) ; la progression l'affiche.
- Tests obligatoires : aller-retour quarantaine → restauration identique (fichiers, attributs, `.reg`), reprise d'un lot interrompu, refus des chemins protégés, jonction non suivie.
