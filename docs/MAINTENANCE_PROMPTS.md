# Prompts d'entretien

À coller tels quels dans Claude Code quand le code a grandi. Chacun travaille sur une branche à part ; rien n'est fusionné sans accord.

## 1. Machines à états

> Find the app's explicit and implicit state machines and document their states, invariants, and transition tables in one file (`STATE_MACHINES.md`). Identify invalid states or transitions the code can actually reach, with evidence, including through retries and concurrency. Prioritize confirmed bugs by impact, group related fixes into issues, then fix them with regression tests and keep the document updated.

## 2. Code sans appelant

> Sur une branche à part, plus aucune fonction ni fichier de src/ sans appelant. Liste-les avant de les retirer. npm test sort à 0 à la fin et aucun comportement ne change. Arrête-toi après 20 tours.

(Pour le Rust : même règle sur `src-tauri/src/`, avec `cargo test` à 0.)

## 3. Doublons

> Sur une branche à part, la même logique n'existe plus à deux endroits dans src/. Regroupe chaque doublon dans une seule fonction. npm test sort à 0 après chaque regroupement. Arrête-toi après 20 tours.

## 4. Dépendances inutilisées

> Sur une branche à part, chaque paquet de package.json est utilisé quelque part. Retire les autres un par un. npm test et le build sortent à 0 après chaque retrait. Montre-moi la liste finale.

## 5. Worktrees fusionnés

> git worktree list ne montre plus aucun worktree dont la branche est déjà fusionnée. Supprime seulement ceux-là, écris la liste dans MENAGE.md, et ne touche à aucune branche ni à aucun fichier non commité.

## Règle DRY (rappel pour toute revue)

> Code MUST be DRY. Do not duplicate definitions or logic. When the same constant, type, validation, or block of logic appears in more than one place, abstract it into a shared helper/utility and reuse it. Flag copy-pasted logic and repeated literals/definitions that should be centralized.
