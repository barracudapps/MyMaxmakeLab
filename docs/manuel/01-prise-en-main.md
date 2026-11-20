<sub>[Sommaire](README.md) · [Interface →](02-interface.md)</sub>

# 1. Prise en main

MyMaxmakeLab prépare et pilote des travaux de **gravure laser** et de
**fraisage CNC** sur les machines GRBL (Marxmake HiMill et iLam, machines
GRBL / grblHAL / FluidNC, Makera Carvera…). Ce chapitre vous amène de
l'installation à votre première pièce.

## Installer

| Système | Comment |
|---|---|
| macOS | Téléchargez `MyMaxmakeLab-macos-universal.zip` depuis la [dernière version](https://github.com/barracudapps/MyMaxmakeLab/releases/latest), décompressez, glissez **MyMaxmakeLab.app** dans *Applications*. |
| Windows | Téléchargez `MyMaxmakeLab-windows-x86_64.zip`, décompressez où vous voulez, lancez `mymaxmakelab.exe`. |
| Linux | Téléchargez `MyMaxmakeLab-linux-x86_64.tar.gz`, décompressez dans `~/.local/bin`, lancez `mymaxmakelab`. |
| Depuis les sources | `git clone https://github.com/barracudapps/MyMaxmakeLab.git && cd MyMaxmakeLab && cargo run --release -p mml_app` |

L'application tient en un seul fichier d'environ 20 Mo, sans installation
de dépendances.

## Mises à jour automatiques

```mermaid
flowchart LR
    T["Tag de version<br/>v1.2.3 poussé"] --> CI["GitHub compile<br/>macOS · Linux · Windows"]
    CI --> R["Version publiée<br/>+ empreintes SHA-256"]
    R --> A["L'application vérifie<br/>au démarrage et toutes les 6 h"]
    A --> D["Téléchargement<br/>et vérification"]
    D --> I["Installée, active<br/>au prochain lancement"]
```

- Seules les **versions de production** (`v1.2.3`, sans suffixe) sont
  installées ; les versions de test sont ignorées.
- Le menu **Aide** permet de *Rechercher les mises à jour* tout de suite,
  de couper l'installation automatique et affiche la version.
- Une copie lancée depuis un dossier de compilation ne se remplace jamais.

## Premier lancement

1. **Tutoriel** : quelques bulles présentent les trois étapes ; vous pouvez
   le passer et le relancer depuis l'aide.
2. **Bibliothèque de matériaux** : si MaxmakeLAB ou LightBurn est installé,
   l'application propose d'importer ses préréglages (`.clb`).
3. **Machine** : choisissez votre profil dans le menu **Machine** (laser,
   CNC, CNC + laser, Makera Carvera) puis ajustez-le dans *Réglages de la
   machine* (taille du plateau, origine, puissance maximale `$30`…).

## Langue, thème, réglages portables

- Langues : français, anglais, allemand, italien, espagnol (menu
  **Affichage**). La langue du système est choisie au premier lancement.
- Thèmes clair et sombre.
- Pour emporter vos réglages sur une clé USB :
  `MYMAXMAKELAB_CONFIG_DIR=/Volumes/CLE/mml mymaxmakelab`.

![Interface en allemand](images/allemand.png)

## Votre première pièce en 3 minutes

1. **Conception** : faites glisser un SVG, une image ou un texte sur la
   fenêtre ([Conception](03-conception.md)).
2. Choisissez l'opération de chaque calque : découpe, gravure, image…
3. **Préparation** : vérifiez le rendu, la durée et le coût estimés.
4. **Machine** : connectez-vous, placez la pièce, faites le **cadrage**
   (passage à blanc), puis **Démarrer** ([Sécurité](09-securite.md)).

---

<sub>[Sommaire](README.md) · [Interface →](02-interface.md)</sub>
