# Mesures de performance

Cibles (`PRODUCT.md`) : fenêtre utilisable < 1 s à froid, liste visible < 300 ms, mémoire privée propre < 150 Mo pendant l'usage.

Méthode : `scripts/measure-startup.ps1` sur le binaire release (`npx tauri build --no-bundle`), 5 lancements, médiane.
- **Fenêtre** : du lancement à l'existence de la fenêtre principale (`MainWindowHandle`). Ce n'est pas encore le premier rendu utilisable : l'instrumentation du premier rendu arrive avec la liste des programmes (Phase 1).
- **Mémoire**, 5 s après l'ouverture, au repos :
  - **Privé propre** : processus de l'application + processus de rendu WebView2, la partie que notre code fait varier (cible).
  - **Privé total** : somme sur l'application et tous ses processus WebView2.
  - **Working set** : compte aussi les pages partagées des DLL, donné pour comparaison avec le Gestionnaire des tâches.

## 2026-11-09 — Phase 0 (barre de titre, navigation, écran vide)

Machine : Ryzen 5 5600X, 32 Go, RTX 3080, Windows 10 Pro N 22H2 (19045), WebView2 157.0.4341.42. Exécutable : 7,2 Mo.

| Lancement | Fenêtre (ms) | Processus | Working set (Mo) | Privé total (Mo) | Privé propre (Mo) |
|---|---|---|---|---|---|
| 1 | 103 | 7 | 314.9 | 169.5 | 34.8 |
| 2 | 47 | 7 | 313.1 | 169.8 | 34.7 |
| 3 | 31 | 7 | 313.3 | 169.1 | 34.8 |
| 4 | 32 | 7 | 316.8 | 171 | 35.9 |
| 5 | 31 | 7 | 312.9 | 168.5 | 34.8 |
| **Médiane** | **32** | | **313.3** | **169.5** | **34.8** |

Répartition d'un lancement (privé) : application 7,6 Mo, rendu 27,2 Mo ; socle WebView2 : GPU 77,6, navigateur 36,8, réseau 11,7, stockage 7,3, crashpad 3,0.

Essai sans effet : `additionalBrowserArgs` (`SpareRendererForSitePerProcess` désactivé, `--disable-background-networking`, `--disable-component-update`, `--renderer-process-limit=1`) → 160,6 Mo total contre 164,1, dans le bruit entre séries ; retiré.

Constat : création de la fenêtre bien sous 1 s ; mémoire propre 34,8 Mo, sous la cible de 150 Mo.
