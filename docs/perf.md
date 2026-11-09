# Mesures de performance

Cibles (`PRODUCT.md`) : fenêtre utilisable < 1 s à froid, liste visible < 300 ms, mémoire privée propre < 150 Mo pendant l'usage.

Méthode : `scripts/measure-startup.ps1` sur le binaire release (`npx tauri build --no-bundle`), 5 lancements, médiane.
- **Fenêtre** : du lancement à l'existence de la fenêtre principale (`MainWindowHandle`), encore vide.
- **Liste** : du lancement aux premières lignes de la liste des programmes peintes (Phase 1, tranche 14) ; la page appelle `list_shown` après la première image, qui écrit l'heure dans le fichier nommé par `UNINSTALLER_PERF_LOG`.
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

## 2026-11-09 — Phase 1, tranche 1 (inventaire registre)

Même machine. `core::inventory::scan()` en release (`cargo test --release --lib scans_this_machine`, 3 exécutions) : 0,05 – 0,06 s pour 257 programmes affichés (HKLM 64/32 + HKCU). Le temps jusqu'à la liste visible (cible < 300 ms) sera instrumenté avec le premier rendu.

## 2026-11-09 — Phase 1, tranche 14 (premier rendu de la liste)

Même machine, 891 entrées d'inventaire, écran Programmes à l'ouverture (tailles en cours de calcul pendant la mesure mémoire).

Répartition (binaire de diagnostic marquant aussi le début du script de la page, 4 lancements) : la page commence à s'exécuter à ≈ 410 ms (création de WebView2 et chargement de la page, avant tout code de l'application), la liste est peinte ≈ 110 ms plus tard, dont ≈ 70 ms de lecture du registre (`scans_this_machine` en release : 0,07-0,08 s).

Avant : inventaire lu à la demande de la page → liste à **564 ms** (médiane ; 747, 533, 564, 557, 576).

Après : inventaire lancé au démarrage du processus, en parallèle de WebView2 :

| Lancement | Fenêtre (ms) | Liste (ms) | Processus | Working set (Mo) | Privé total (Mo) | Privé propre (Mo) |
|---|---|---|---|---|---|---|
| 1 | 131 | 594 | 7 | 344.5 | 193.6 | 48.8 |
| 2 | 46 | 491 | 7 | 346.6 | 198.6 | 55.9 |
| 3 | 29 | 488 | 7 | 344.2 | 193.7 | 48.8 |
| 4 | 46 | 480 | 7 | 344.6 | 196.7 | 54.4 |
| 5 | 46 | 539 | 7 | 344.3 | 195.4 | 53.9 |
| **Médiane** | **46** | **491** | | **344.5** | **195.4** | **53.9** |

Constat : la liste est peinte ≈ 80 ms après le début de la page, mais 491 ms après le lancement ; la cible « liste visible < 300 ms » n'est tenue que comptée depuis l'interface prête, le démarrage de WebView2 (≈ 410 ms) dépassant seul 300 ms. Mémoire propre 53,9 Mo avec 891 entrées, sous la cible de 150 Mo.
