# Uninstaller — design

## Direction

Même famille que Viewer et Hub : direction « Apple TV » sombre, matériaux translucides flottants, focus par élévation, mouvements à ressort calmes. Identité propre, aucun élément de Revo ni d'Apple (logos, polices, icônes). Raccourcis Windows uniquement (Ctrl, Alt, Maj ; jamais ⌘). Textes en français. Skills : `ui-ux-pro-max`, `apple-design`, audit `unslop`.

## Tokens (`src/app.css`, repris d'Viewer)

- Fond `#000` ; surfaces `#1C1C1E` / `#2C2C2E`.
- Texte `#F5F5F7`, secondaire `rgb(235 235 245 / 0.7)`, atténué `rgb(235 235 245 / 0.55)`.
- Matériau flottant `rgb(40 40 46 / 0.6)` + `blur(20px) saturate(180%)`, bordure `rgb(255 255 255 / 0.08)`. Jamais de flou sur la liste qui défile.
- Bouton principal blanc texte noir ; secondaire verre `rgb(120 120 128 / 0.36)`. Accent `#8FD3F5` (focus, liens, marqueurs), succès `#64C69A`, danger `#FF6961`.
- Rayons 10/14/22 px, pilules pour la navigation ; espacements sur une grille de 4 px.
- Typo : Segoe UI Variable Display (titres, −0,02 em) / Text (reste) ; chiffres tabulaires pour tailles et dates.
- Mouvement : 120/200/320 ms, ressort `cubic-bezier(0.2, 0.9, 0.25, 1.15)` ; tout à 0 et sans échelle en mouvement réduit.

## Fenêtre

Sans barre de titre Windows : bande de 32 px avec les trois boutons système à droite (fermer rouge au survol), zone vide pour déplacer, double-clic pour agrandir (comme Viewer). Taille mini 900 × 600.

## Navigation

Pilule translucide en haut au centre : recherche (Ctrl+F ou /), « Programmes » (Ctrl+1), « Applications Windows » (Ctrl+2), « Extensions » (Ctrl+3), « Outils » (Ctrl+4), « Sauvegardes » (Ctrl+5), « Suivi » (Ctrl+6), réglages (engrenage). Bouton rond « Chasseur » flottant en bas à droite (Ctrl+H).

## Écrans

- **Programmes** : en-tête avec nombre et taille totale ; barre d'outils verre (Trier par, Regrouper, Vue, Colonnes, « Récents », « Orphelins ») ; liste virtualisée à lignes de 44 px (icône 28 px, nom, éditeur en secondaire, taille alignée à droite en chiffres tabulaires, date, badge 32/64 bits discret). Sélection multiple Ctrl/Maj, Entrée = désinstaller, Suppr = désinstaller, Espace = détails. Panneau de détails latéral en matériau (dossier, chaîne de désinstallation, site, clé de registre, « Ouvrir le dossier », « Copier »).
- **Désinstallation** (feuille modale en matériau, étapes visibles « 1 Point de restauration · 2 Désinstalleur · 3 Restes · 4 Nettoyage ») : choix de profondeur Sûr / Modéré / Avancé (segmenté), progression réelle par étape, annulation possible tant que rien n'est supprimé.
- **Restes** : deux onglets Registre / Fichiers, arbre à cases (tri-état), confiance haute en gras, compteur « 37 éléments · 412 Mo sélectionnés », « Tout sélectionner (sûrs) ». Bouton danger « Mettre en quarantaine ». Confirmation qui répète le compteur, puis toast « 37 éléments en quarantaine · Annuler ».
- **Applications Windows / Extensions** : même liste, même panneau de détails (un seul composant de liste partagé).
- **Outils** : grille de cartes (Démarrage, Fichiers inutiles, Navigateurs, Historique Windows, Historique Office, Broyeur, Effacer l'espace libre, Outils Windows). Chaque nettoyeur : analyse → aperçu par catégorie avec tailles → nettoyage.
- **Sauvegardes** : chronologie (points de restauration, sauvegardes registre, lots en quarantaine) avec « Restaurer » et « Purger » ; espace occupé et purge automatique après N jours.
- **Suivi** : « Suivre une installation » (choisir l'installeur ou suivre en direct), journaux enregistrés avec nombre de changements, « Désinstaller avec ce journal ».
- **Mode Chasseur** : petite cible ronde translucide toujours au premier plan ; glisser sur une fenêtre ou une icône ouvre un menu contextuel (Désinstaller, Arrêter le processus, Retirer du démarrage, Ouvrir le dossier, Propriétés, Chercher en ligne). Échap le quitte.

## États

Squelettes de lignes pendant l'inventaire ; « Calcul… » discret pour les tailles ; listes vides avec phrase utile ; erreurs avec cause, conseil et « Réessayer ». Toute action destructive montre le nombre et la taille, jamais de bouton danger sans compteur.

## Définition de « fini » pour l'UI

Cohérent à 100/125/150 % ; entièrement au clavier ; focus visible ; contraste AA ; pas de débordement de 900 px à 4K ; mouvement réduit respecté ; états chargement/vide/erreur montrés par captures ou tests ; audit `unslop` passé.
