# ADR 0005 — Manifeste de quarantaine en JSON, empreintes SHA-256

- Statut : accepté
- Date : 2026-11-09

## Contexte

Le manifeste d'un lot de quarantaine (ADR 0003) doit survivre à un plantage, se relire au démarrage suivant, signaler sa propre corruption et vérifier qu'un fichier restauré est bien celui qui a été déplacé. Il faut un format lisible et une empreinte de contenu.

## Décision

- **Format** : `serde_json` (1.0) en dépendance directe, déjà présente dans l'arbre de Tauri.
- **Empreinte** : `sha2` (0.10) en dépendance directe, déjà présente dans l'arbre de Tauri. Chaque fichier déplacé porte le SHA-256 de son contenu ; le manifeste porte le SHA-256 de sa liste d'éléments (empreinte, pas signature : elle détecte une corruption, pas une falsification délibérée).
- **Écriture atomique** : fichier temporaire du lot puis `std::fs::rename`, qui remplace la cible par `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` sur le même volume. Équivalent à `ReplaceFileW` pour ce besoin, sans code `unsafe`.

## Alternatives écartées

- **SHA-256 par l'API Windows (`BCrypt`)** : aucune crate, mais du code `unsafe` dans `win/` pour un calcul que `sha2` fait déjà, compilé de toute façon.
- **Format maison (lignes de texte)** : à analyser et à échapper soi-même, sans gain.

## Conséquences

- Aucun crate ajouté au binaire : les deux étaient déjà compilés pour Tauri.
- `cargo deny` couvre ces dépendances comme les autres.
