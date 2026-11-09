# ADR 0004 — Icônes des programmes en PNG via un schéma d'URI

- Statut : accepté
- Date : 2026-11-09

## Contexte

La liste affiche l'icône de chaque programme (`DisplayIcon` du registre : `chemin`, `chemin,index` ou `"chemin",index`). La WebView ne lit pas un `HICON` : il faut des pixels dans un format d'image web. Passer chaque icône en base64 par l'IPC alourdirait la réponse de `list_programs` et bloquerait le premier affichage.

## Décision

- **Extraction** (`win/icon.rs`) : `PrivateExtractIconsW` à 48 px, puis `GetIconInfo` + `GetDIBits` en BGRA 32 bits ; le masque fournit l'alpha des icônes anciennes. Repli sur `%SystemRoot%\SystemResources\<nom>.mun`, où Windows 10 1903+ range les icônes des fichiers système.
- **Encodage** : dépendance directe à `png` (0.18), déjà présente dans l'arbre de Tauri. Aucun poids ajouté au binaire.
- **Transport** : schéma asynchrone `icon://localhost/<id>` (`http://icon.localhost/<id>` sous WebView2), une tâche bloquante par icône. L'interface ne passe que l'identifiant opaque du programme, jamais un chemin : le chemin est relu dans le registre par `inventory::find`. Réponse 404 si pas d'icône, `Cache-Control: max-age=3600`.
- **CSP** : `img-src 'self' http://icon.localhost`.

## Alternatives écartées

- **Encodeur PNG écrit à la main** (stockage sans compression + CRC + Adler) : du code à maintenir pour rien, la crate est déjà compilée.
- **BMP / data URI** : plus lourd, et le BMP 32 bits avec alpha est mal pris en charge par les navigateurs.
- **`SHDefExtractIconW`** : renvoie un succès avec une icône nulle sur certains fichiers système de ce poste ; `PrivateExtractIconsW` donne directement la taille voulue.

## Conséquences

- Mesuré sur ce poste : 224 icônes sur 236 `DisplayIcon` en 0,85 s au total (≈ 3,6 ms chacune), hors du fil principal. Les 12 échecs pointent vers des fichiers absents.
- Le schéma ne lit que des fichiers désignés par le registre de désinstallation, en lecture seule ; il n'écrit rien.
