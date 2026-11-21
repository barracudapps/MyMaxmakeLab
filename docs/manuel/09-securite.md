<sub>[← Machines et connexion](08-machines.md) · [Sommaire](README.md) · [Palpage et caméra →](10-palpage-camera.md)</sub>

# 9. Sécurité

La sécurité de l'opérateur et de la machine passe avant tout : des
**verrous de démarrage**, une **surveillance en direct**, et des calculs qui
empêchent les collisions.

> L'arrêt d'urgence logiciel ne remplace jamais l'arrêt d'urgence physique.

## Avant de démarrer

```mermaid
flowchart TB
    S["Démarrer"] --> P{"Porte ouverte ?"}
    P -- oui --> X["Bloqué"]
    P -- non --> T{"Travail dans la course<br/>de la machine (position réelle) ?"}
    T -- non --> X
    T -- oui --> L{"Laser : cadrage approuvé ?"}
    L -- non --> X
    L -- oui --> C{"Fraisage : origine palpée,<br/>outil mesuré, aucune collision ?"}
    C -- non --> X
    C -- oui --> K["Liste de vérification<br/>(capot, lunettes, bridage, extincteur)"]
    K --> G["Travail lancé et surveillé"]
```

## Longueur réelle d'outil et collisions

La sécurité du fraisage repose sur la **longueur réelle de l'outil une fois
installé**, mesurée sur la machine, et non sur une valeur théorique.

![Étalonnage du capteur d'outil](images/etalonnage-capteur.svg)

1. **Étalonnage, une fois par machine** : installez une pige (une tige ou un
   outil) dont vous avez mesuré la longueur sortie au pied à coulisse,
   placez-vous au-dessus du capteur d'outil, *Étalonner le capteur… ›
   Palper le capteur avec la pige*.
2. **Mesure de chaque outil installé** : *Mesurer l'outil installé*. La
   longueur réelle vaut la longueur de la pige plus l'écart de hauteur au
   contact.
   *Sans capteur d'outil* : **Relever à la main**. Descendez l'outil jusqu'au
   contact d'une référence fixe (cale vissée au plateau), avec la même
   feuille de papier pour la pige et pour chaque outil : son épaisseur
   s'annule.
3. La mesure est **oubliée à chaque pause du programme** (là où l'on change
   d'outil ou de tête) : elle est redemandée avant de repartir.

Avec cette longueur, la longueur de coupe, le diamètre de l'écrou de pince,
l'épaisseur du brut et les **obstacles** (brides, étau, gabarit dessinés
comme des formes sur un calque *Bride / obstacle* avec leur hauteur), la
profondeur permise est calculée **en chaque point** :

![Limites de profondeur](images/collision-outil.svg)

- le programme entier est vérifié avant le démarrage ; chaque collision est
  listée avec sa ligne, sa position, la profondeur en trop et sa cause
  (table, queue au-dessus de la coupe, écrou de pince, bride) ;
- la carte affiche **en direct** la profondeur permise sous la tête ;
- le démarrage est bloqué tant que l'outil n'est pas mesuré ou qu'une
  collision subsiste.

![Carte Longueur d'outil et collisions](images/machine-cnc.png)

## Homing doux et surveillé

Les moteurs pas à pas ne sentent rien : pendant le homing, rien dans le
contrôleur ne détecte une main ou un objet sur le trajet. Le logiciel
limite donc ce qu'un obstacle subit :

- **bouton maintenu** : le homing ne continue que tant que vous maintenez
  le bouton en surveillant la machine. Le relâcher (ou appuyer sur Échap)
  réinitialise le contrôleur à l'instant. Désactivable en mode avancé
  seulement ;
- **durée limitée** : un homing qui dure un quart de plus que le plus lent
  des derniers est coupé, bien avant la limite de GRBL (1,5 fois la course) ;
- **vitesse** : une recherche rapide est signalée ; **Ralentir le homing**
  écrit 500 mm/min pour la recherche (`$25`) et 50 mm/min pour l'approche
  (`$24`), ce qui réduit fortement le choc.

La **force** de poussée, elle, dépend du courant des moteurs : elle se règle
sur les drivers (potentiomètre, ou courant de homing des drivers Trinamic
sous grblHAL), pas depuis le logiciel.

## Pendant le travail

| Surveillance | Réaction |
|---|---|
| La tête sort de la zone du programme | pause immédiate |
| L'axe rotatif tourne sans être commandé | pause |
| Un fin de course est actionné | pause |
| Plus de réponse du contrôleur depuis 1,5 s | alerte |
| Retour à l'origine 40 % plus lent que d'habitude | alerte (axe qui force, courroie détendue) |
| Caméra : tête vue loin de sa position | pause (voir [Palpage et caméra](10-palpage-camera.md)) |

**Arrêter** garde la position (pause puis réinitialisation) ; **Arrêt
d'urgence** est toujours visible.

## Ordinateur en veille et notifications

Pendant un travail, l'ordinateur ne se met **pas en veille** (une mise en
veille arrêterait la machine en plein travail). À la fin, une notification
système s'affiche et la fenêtre demande l'attention (Dock, barre des
tâches).

---

<sub>[← Machines et connexion](08-machines.md) · [Sommaire](README.md) · [Palpage et caméra →](10-palpage-camera.md)</sub>
