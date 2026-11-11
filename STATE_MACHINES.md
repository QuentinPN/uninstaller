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
| NativeUninstaller | exit (lui et tout ce qu'il a lancé) | AwaitingScan | code de sortie journalisé |
| NativeUninstaller | stop (attente arrêtée, désinstalleur laissé) | AwaitingScan | `finished` faux |
| NativeUninstaller | refused (invite UAC) / notFound / noUninstaller / unsupported / missing / failed | Failed | rien n'a été lancé |
| AwaitingScan | scan(mode) | Scanning | |
| Scanning | done | AwaitingSelection | |
| AwaitingSelection | confirm(sélection non vide) | Quarantining | sélection ∩ protégés = ∅ |
| Quarantining | done | Done | manifeste complet |
| * (avant Quarantining) | cancel | Cancelled | |
| * | error | Failed | |

Interface (tranche 17, `src/lib/uninstall/UninstallSheet.svelte`) : feuille `confirm → point → running`, puis l'écran des restes (`AwaitingScan` et la suite). `confirm` sans point coché va droit à `running`. `point` → `pointFailed` si Windows refuse ou échoue (`disabled`, `refused`, `failed`) ; `pointFailed` → `running` sur « Continuer sans point ». `running` → `failed{cause}` sur un refus, « Réessayer » seulement après `refused`. Désinstalleur fini mais programme encore listé → `stillListed`, qui mène à l'écran des restes sur « Rechercher les restes ». Échap et fermeture ignorés pendant `point` et `running` ; la liste est relue dès que le désinstalleur a été lancé.

Lot (Phase 4 tranche 1) : `outcomes` reçoit un résultat par programme, dans l'ordre ; `running{index}` passe au suivant quand un désinstalleur se termine, même sur un refus (le lot ne s'arrête pas). « Arrêter après celui-ci » pose `stopping` : les programmes suivants reçoivent `skipped` sans être lancés. La fin d'un lot mène toujours à `summary` (jamais `failed` ni `stillListed`) ; depuis `summary`, « Rechercher les restes » sur une ligne `done`, `stopped` ou `stillListed` masque la feuille (`hidden`, dialogue fermé mais état gardé) et ouvre l'écran des restes ; le quitter rouvre la feuille au bilan, focus sur le bouton quitté. « Réessayer » (refus de l'invite, programme seul) retire le dernier résultat et relance la file. Un programme seul garde les transitions ci-dessus. Désinstallation forcée (tranche 3) : `failed{missing | noUninstaller | unsupported}` propose « Forcer la désinstallation », qui mène à l'écran des restes comme après un désinstalleur ; dans un bilan, la ligne de ces refus porte le même bouton. Programme non listé (tranche 4) : feuille à part, `idle → checking → (failure | leftovers)` ; un refus (`tooShort`, `noFolder`, `protected`, `listed`, `failed`) reste affiché jusqu'à la saisie suivante ; Échap ou « Annuler » ferme la feuille sauf pendant `checking`  ; l'écran des restes qui suit est celui d'un programme désinstallé, passe administrateur comprise. Le choix « sans fenêtres » (tranche 2) est lu à chaque lancement, « Réessayer » compris ; il ne change aucune transition.

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
| Pending | renommage réussi (même volume) | Moved | manifeste réécrit |
| Pending | autre volume : fichier ou arborescence copiés, chaque fichier vérifié par SHA-256 | Copied | manifeste réécrit, **puis** origine retirée |
| Copied | origine retirée | Moved | manifeste réécrit |
| — | `quarantine_key(clé)`, clé protégée ou racine inconnue | — | `Error::Protected`, manifeste inchangé |
| — | `quarantine_key(clé)`, clé, sous-clé ou valeur illisible | — | `Error::Unreadable`, rien écrit ni retiré |
| Pending | clé : `.reg` écrit dans le lot, vérifié par SHA-256 | Copied | manifeste réécrit, **puis** `RegDeleteTreeW` |
| Pending | échec (lien, ou dossier contenant un lien, sur un autre volume : `OtherVolume` ; empreinte différente : `Altered` ; E/S) | — | copie partielle retirée du lot, ligne retirée, origine intacte |
| Pending | démarrage (`resume_all`), origine présente | — | copie partielle éventuelle retirée du lot, ligne retirée |
| Pending | démarrage, origine absente, élément dans le lot | Moved | manifeste réécrit |
| Copied | démarrage, élément dans le lot | Moved | reste de l'origine retiré, manifeste réécrit |
| Pending / Copied | démarrage, élément absent du lot (et origine absente pour Pending) | inchangé | `Error::Corrupt`, lot laissé tel quel |
| Moved | `restore()`, origine libre, contenu intact | Restored | élément remis en place (même volume : renommage ; sinon copié, vérifié, puis retiré du lot ; à l'échec la copie partielle à l'origine est retirée), manifeste réécrit |
| Moved | `restore()`, clé : `.reg` relu (clé et sous-clés seulement, sinon `Error::Corrupt`) et réimporté | Restored | `.reg` retiré du lot ; à l'échec la clé recréée est retirée |
| Moved | `restore()`, origine occupée | Moved | `Error::Exists`, arrêt, rien écrasé |
| Moved | `restore()`, empreinte différente | Moved | `Error::Altered`, arrêt |
| Restored | `restore()` | Restored | ignoré |

Le manifeste est réécrit par fichier temporaire + renommage ; une empreinte qui ne correspond plus donne `Error::Corrupt` à l'ouverture. Seul le dernier élément peut être `Pending` ou `Copied` ; sinon `Error::Corrupt` et le lot reste tel quel. La reprise tourne sur un thread au lancement (`run()`), sur `%LOCALAPPDATA%\Uninstaller\quarantine`, et ne fait rien si le dossier n'existe pas. Limite connue : un plantage pendant une restauration d'un autre volume, après la copie vérifiée, laisse l'élément `Moved` avec l'origine complète ; la restauration suivante répond `Exists`, sans perte. États du lot : `Active → Purging → (dossier retiré)`. `Batch::purge` termine d'abord une étape interrompue, écrit `Purging`, retire `items\`, puis le manifeste et le dossier ; un plantage entre deux laisse `Purging`, que la reprise au lancement termine. Un lot `Purging` refuse `quarantine`, `quarantine_key` et `restore` (`Error::Purging`). `purge_expired` choisit les lots dont la date de création (secondes Unix, dans le manifeste et son empreinte) a au moins le délai donné ; il tourne au lancement juste après la reprise, avec le délai réglé (`settings.json`, 14 jours par défaut ; fichier absent, illisible ou valeur non proposée → défaut). Pas encore fait : `Restoring → Restored` au niveau du lot.

### 7. Session du broker (`core/broker.rs`, Phase 2 tranches 10 à 12)

| État | Événement | Suivant |
| --- | --- | --- |
| Aucun | `Client::start` : tube créé, invite UAC | Lancement |
| Lancement | invite refusée ou échec de `ShellExecuteExW` | Aucun (erreur, tube fermé) |
| Lancement | client connecté, PID égal au broker lancé | Prêt |
| Lancement | autre PID, ou broker mort avant de se connecter | Aucun (refusé) |
| Prêt | requête | Occupé |
| Occupé | réponse envoyée | Prêt |
| Prêt | tube fermé par l'application | Terminé (code 0) |
| Prêt | 5 min sans requête | Terminé (code 0) |

Côté broker, un tube créé par un autre processus que son parent donne le code 1 avant toute requête. Il ne quitte jamais pendant `Occupé`. Côté application (`commands.rs`), la session est gardée après la première opération et réutilisée tant que le broker vit ; un broker qui a quitté ou un appel en échec la ferme, et l'opération suivante en relance une (nouvelle invite). Invite refusée : `ElevationFailure::Refused`, autre échec : `Failed`. « Restaurer » et « Purger » envoient au broker la partie du lot qui est sous `%ProgramData%\Uninstaller\quarantine` (`Request::Batch`, nom seulement), puis l'application traite la partie sous `%LOCALAPPDATA%` ; aucune des deux → `Missing`. Le broker ne touche que la racine administrateur, après en avoir vérifié le propriétaire et la DACL ; à son lancement, il reprend et purge les lots échus de cette racine (`quarantine::settle`). `CreateRestorePoint` répond toujours (`Created`, `Skipped`, `Disabled` ou `Failed`) : un échec du point ne ferme pas la session. `ListRestorePoints` répond `None` si WMI échoue et `OpenSystemRestore` `false` si `rstrui.exe` ne démarre pas ; ni l'un ni l'autre ne ferme la session.

### 2. Scan des restes

Tranches 1 à 5 : `leftovers::Target::scan`, `shortcuts`, `keys`, `services` et `tasks` sont des fonctions pures de lecture (aucun état, aucune écriture) ; relancer est toujours sûr. Tranche 6 : un scan à la fois. `scan_leftovers` : en cours → terminé (`Found`) ; `cancel_leftovers` ou un nouveau scan lève le drapeau du scan en cours → annulé (`cancelled`, vérifié avant chaque type de reste, le type en cours se termine) ; programme absent → `notFound`. Tranche 7, écran : en cours (« Annuler » → `cancel_leftovers`) → terminé (sûrs présélectionnés ; choix libre) | rien trouvé | annulé, échec (« Réessayer » relance) | introuvable (retour seulement). Une réponse d'un scan antérieur (relancé, ou écran quitté) est ignorée ; quitter l'écran pendant le scan l'annule. Tranche 8 : terminé → confirmation (« Mettre en quarantaine », désactivé sans sélection ; Échap ou « Annuler » referme) → en quarantaine (cases et bouton désactivés, Échap sans effet) → bilan dans l'en-tête puis nouveau scan. `quarantine_leftovers` consomme le scan : un numéro périmé ou déjà utilisé → `stale` (rien déplacé, nouveau scan) ; dossier de quarantaine introuvable → `failed`. Quitter l'écran pendant l'envoi ignore la réponse. Tranche 9 : bilan avec des éléments « droits d'administrateur » → « Continuer en administrateur » → en quarantaine (invite UAC si aucun courtier ne tourne) → bilan puis nouveau scan ; UAC refusé ou courtier en échec → message, éléments gardés côté Rust pour un nouvel essai ; réponse du courtier → éléments oubliés. Une nouvelle mise en quarantaine remplace les éléments gardés. Tranche 10 : une tâche planifiée suit les états d'une clé (`Pending` → définition écrite et vérifiée → `Copied` → tâche supprimée → `Moved`) ; suppression refusée → élément retiré du lot, tâche en place. Tranche 11 : un service suit les mêmes états (`.reg` de sa clé écrit et vérifié → `Copied` → service arrêté puis supprimé → `Moved`) ; arrêt trop long (`TimedOut`) ou refus → élément retiré du lot, service en place ; restauré → clé réimportée, service connu de Windows au redémarrage suivant. Tranche 12 : la passe du courtier rouvre le lot de la première passe s'il est encore `Active` (lot introuvable, illisible ou `Purging` → nouveau lot) ; un lot rouvert qui ne reçoit rien reste tel quel.

### 4. Session de suivi d'installation · 5. Mode Chasseur

À détailler en Phase 3-8, au même format.

## États ou transitions invalides trouvés

| # | Machine | Description | Preuve | Impact | Statut |
|---|---------|-------------|--------|--------|--------|
| — | — | — | — | — | — |
