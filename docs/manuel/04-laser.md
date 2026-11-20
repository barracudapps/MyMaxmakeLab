<sub>[← Conception](03-conception.md) · [Sommaire](README.md) · [Fraisage CNC →](05-fraisage.md)</sub>

# 4. Gravure et découpe laser

![Préparation d'un travail laser](images/preparation.png)

## Laser ligne

| Réglage | Effet |
|---|---|
| Vitesse, puissance, passes | la puissance est en % du maximum (`$30`) ; plusieurs passes pour les matériaux épais |
| Descente Z par passe | abaisse la tête entre les passes (contreplaqué épais) |
| Style plein / pointillé | pointillés pour les lignes de pliage et les perforations |
| **Ponts** | petits ponts non découpés qui retiennent la pièce dans la planche ; en automatique, ils vont sur les portions droites, loin des angles |
| Assistance d'air | `M8` pendant le calque |

Les trous sont découpés avant les contours, et l'ordre des tracés réduit
les déplacements à vide (voir [Production](11-production-statistiques.md)).

## Laser remplissage

- **Motif** : *lignes* à n'importe quel angle (boutons 0°, 45°, 90°, 135°
  pour la diagonale), *cercles* autour du centre, *contours* qui suivent la
  forme vers l'intérieur.
- Intervalle entre lignes, hachures croisées, aller-retour, **dépassement**
  (la tête atteint sa vitesse avant de brûler).

## Laser image

Niveaux de gris (puissance variable), seuil, Bayer, Floyd–Steinberg,
Atkinson, Jarvis, Stucki, **similigravure (vintage)**, **gravure au trait** ;
motif de balayage en lignes, **diagonale** ou **cercles** ; résolution
(dpi), luminosité, contraste, négatif.

## Gravure sur cylindre

Dans **Machine**, *Rotatif* : mandrin sur un vrai axe A, ou rouleaux à la
place de Y avec une seule valeur d'étalonnage. Le dessin est déroulé : sa
hauteur vaut π × diamètre.

## Cadrage (passage à blanc)

Avant tout travail laser, **Cadrer** fait le tour du dessin à très faible
puissance : vous vérifiez la position sur la pièce, puis vous
**approuvez**, corrigez ou relancez. Le démarrage reste bloqué tant que le
cadrage n'est pas approuvé.

---

<sub>[← Conception](03-conception.md) · [Sommaire](README.md) · [Fraisage CNC →](05-fraisage.md)</sub>
