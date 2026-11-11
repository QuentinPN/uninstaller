# ADR 0002 — Élévation par broker à la demande

- Statut : accepté
- Date : 2026-11-09

## Contexte

Lire l'inventaire, chercher et exporter ne demandent aucun droit. Désinstaller, écrire dans HKLM, créer un point de restauration ou toucher Program Files exigent l'administrateur. Une WebView élevée exposerait une surface d'attaque (contenu web, IPC) avec des droits système.

## Décision

- L'application démarre **non élevée** (`asInvoker`). La WebView n'est jamais élevée.
- Les opérations administrateur passent par un **broker** : le même exécutable lancé avec l'argument `--broker <nom-de-tube>` via `ShellExecuteExW` avec le verbe `runas` (une invite UAC par session de broker, pas par opération). Le broker n'ouvre aucune fenêtre et se termine quand le tube se ferme ou après 5 minutes d'inactivité.
- **Canal** : tube nommé créé par l'application non élevée, avec un nom aléatoire et un DACL qui n'autorise que le SID de l'utilisateur courant ; `PIPE_REJECT_REMOTE_CLIENTS` ; une seule instance. Le broker vérifie que le processus à l'autre bout est bien son parent (`GetNamedPipeServerProcessId`) : Windows donne au processus élevé le programme qui a demandé l'élévation comme parent, vérifié par le test `elevated_broker_answers`. L'application vérifie de son côté que le client est le processus lancé (`GetNamedPipeClientProcessId`).
- **Protocole** : messages JSON préfixés par leur longueur, avec un champ `version` (refus en cas de version différente), et une liste fermée d'opérations définie par une `enum` Rust (une seule définition, partagée par les deux côtés). Aucune commande shell ni chemin libre : uniquement des identifiants opaques résolus et validés côté broker, et chaque suppression passe par `core/protect.rs` puis par la quarantaine (ADR 0003).
- Refus de l'UAC : erreur claire dans l'UI, l'opération est annulée, l'état reste cohérent (`ElevationFailure::Refused`, renvoyé par chaque commande élevée sous la forme `Refused` de son propre type d'échec).
- La Phase 0 n'expose que l'information `elevated` (indicateur de la commande `app_info`) ; le broker arrive avec la première opération qui en a besoin (Phase 2 : point de restauration).

## Alternatives écartées

- **Toute l'application élevée** (`requireAdministrator`) : une invite UAC à chaque ouverture, et une WebView avec des droits système.
- **Service Windows** : contraire à « aucun processus résident ».
- **Tâche planifiée élevée** pour contourner l'UAC : processus persistant et contournement de l'UAC, contraires à `SECURITY.md`.

## Conséquences

- Deux modes dans le même binaire : le point d'entrée analyse les arguments avant de lancer Tauri.
- Restes refusés faute de droits (2026-11-11) : `Request::Leftovers` porte l'identifiant du programme et les éléments par valeur (chemins, clés), venus du scan de l'application et jamais de l'UI. Le courtier ne s'y fie pas : il refait le scan du programme et ne prend que les éléments que son propre scan liste, chacun passant ensuite par `protect` et la quarantaine. Les indices ne suffiraient pas : la première passe, non élevée, a déjà déplacé une partie des éléments.
- Programme désinstallé (2026-11-11, décision utilisateur, option B) : son entrée a disparu, donc le scan du courtier ne le retrouverait plus. Avant de lancer le désinstalleur, l'application envoie `Request::Remember` (l'identifiant seulement) à un courtier déjà lancé et libre (jamais d'attente derrière un autre appel élevé) ; le courtier lit lui-même le programme dans son inventaire et le garde en mémoire, et `Request::Leftovers` le reprend quand l'entrée manque. Écartés : (A) envoyer la description du programme au courtier, qui laisserait l'application non élevée orienter son scan ; (C) l'écrire dans la racine administrateur, plus de code pour survivre à l'arrêt du courtier. Limites : sans courtier lancé (point de restauration décoché), rien n'est gardé et aucune invite n'est ajoutée ; le courtier quitte après 5 min sans requête, et ce qu'il gardait part avec lui. Lot (Phase 4) : le courtier garde jusqu'à 64 programmes (`inventory::Kept`, le plus ancien oublié), un par `Request::Remember`.
- Les tests du protocole tournent sans élévation (tube local, broker simulé) ; les tests de bout en bout du broker sont marqués « administrateur » et exécutés en local depuis une console administrateur ou dans la VM.
