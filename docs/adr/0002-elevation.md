# ADR 0002 — Élévation par broker à la demande

- Statut : accepté
- Date : 2026-11-09

## Contexte

Lire l'inventaire, chercher et exporter ne demandent aucun droit. Désinstaller, écrire dans HKLM, créer un point de restauration ou toucher Program Files exigent l'administrateur. Une WebView élevée exposerait une surface d'attaque (contenu web, IPC) avec des droits système.

## Décision

- L'application démarre **non élevée** (`asInvoker`). La WebView n'est jamais élevée.
- Les opérations administrateur passent par un **broker** : le même exécutable lancé avec l'argument `--broker <nom-de-tube>` via `ShellExecuteExW` avec le verbe `runas` (une invite UAC par session de broker, pas par opération). Le broker n'ouvre aucune fenêtre et se termine quand le tube se ferme ou après 5 minutes d'inactivité.
- **Canal** : tube nommé créé par l'application non élevée, avec un nom aléatoire et un DACL qui n'autorise que le SID de l'utilisateur courant ; `PIPE_REJECT_REMOTE_CLIENTS` ; une seule instance. Le broker vérifie que le processus à l'autre bout est bien son parent (`GetNamedPipeServerProcessId`).
- **Protocole** : messages JSON préfixés par leur longueur, avec un champ `version` (refus en cas de version différente), et une liste fermée d'opérations définie par une `enum` Rust (une seule définition, partagée par les deux côtés). Aucune commande shell ni chemin libre : uniquement des identifiants opaques résolus et validés côté broker, et chaque suppression passe par `core/protect.rs` puis par la quarantaine (ADR 0003).
- Refus de l'UAC : erreur claire dans l'UI, l'opération est annulée, l'état reste cohérent (`Cancelled`).
- La Phase 0 n'expose que l'information `elevated` (indicateur de la commande `app_info`) ; le broker arrive avec la première opération qui en a besoin (Phase 2 : point de restauration).

## Alternatives écartées

- **Toute l'application élevée** (`requireAdministrator`) : une invite UAC à chaque ouverture, et une WebView avec des droits système.
- **Service Windows** : contraire à « aucun processus résident ».
- **Tâche planifiée élevée** pour contourner l'UAC : processus persistant et contournement de l'UAC, contraires à `SECURITY.md`.

## Conséquences

- Deux modes dans le même binaire : le point d'entrée analyse les arguments avant de lancer Tauri.
- Les tests du protocole tournent sans élévation (tube local, broker simulé) ; les tests de bout en bout du broker sont marqués « administrateur » et exécutés en CI Windows.
