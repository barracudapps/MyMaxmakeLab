<sub>[← 3D, 4 et 5 axes](06-3d-multi-axes.md) · [Sommaire](README.md) · [Machines et connexion →](08-machines.md)</sub>

# 7. Générateurs

**Édition › Générateurs…**

![Fenêtre des générateurs](images/generateurs.png)

| Générateur | Réglages |
|---|---|
| **Boîte à encoches** | dimensions intérieures (l'objet + 1 à 2 mm de jeu), épaisseur de planche, largeur des encoches, trait de coupe, couvercle |
| **Boîte ronde** | dimensions intérieures, rayon des coins, épaisseur, pas de la charnière, fermée ou non : la paroi est une seule bande pliée grâce à une charnière flexible, fond et dessus tenus par des tenons |
| **Labyrinthe** | cases, taille d'une case, *murs* (à graver ou découper) ou *couloirs* (à fraiser pour un labyrinthe à bille), solution |
| **Puzzle** | taille, nombre de pièces, taille des tenons, nouvelle variante ; ou **dans la forme sélectionnée** |
| **Grille de test laser** | plages de puissance et de vitesse, nombre de cases |
| **Engrenage** | nombre de dents, module, angle de pression (14,5°, 20° ou 25°), alésage, jeu |
| **QR / code-barres** | QR code ou Code 128, contenu, largeur, hauteur des barres, mode inversé |

Deux engrenages de même module s'engrènent : leur entraxe vaut la somme de
leurs diamètres primitifs divisée par deux (affichés sous les réglages). Le
QR code et le code-barres arrivent sur un calque de **remplissage** ; le mode
*inversé* grave les parties claires, pour les matériaux sombres qui
blanchissent au laser (aluminium anodisé, ardoise).

Les encoches de la boîte sont toujours en nombre impair, ce qui rend tous
les angles cohérents ; les panneaux sont agrandis de la moitié du trait de
coupe pour un assemblage serré sans colle.

**Puzzle dans une forme** : sélectionnez une forme fermée (cœur,
silhouette, texte épais, dessin importé…) puis **Puzzle…** dans le panneau
Sélection, ou cochez *Dans la forme sélectionnée* dans le générateur. La
grille de pièces couvre la forme, les coupes sont gardées à l'intérieur et
le contour devient le bord ; la forme est remplacée par le puzzle (une
seule annulation). La grille est placée pour éviter les toutes petites
pièces le long du bord, et celles qui restent trop petites (moins de 30 %
d'une pièce normale) sont réunies à leur voisine.

**Puzzle sur traits** (puzzle pour enfants) : sélectionnez un dessin
(vecteurs, ou une image : dessin scanné, coloriage, personnage en aplats)
puis **Puzzle sur traits…** dans le panneau Sélection. Les traits au moins
aussi épais que le seuil deviennent les coupes entre pièces, le contour du
dessin fait le bord ; les traits plus fins (yeux, détails) restent du
dessin à graver, et les grandes zones foncées (aplats) deviennent des
pièces à part entière. Une zone plus petite que la *pièce minimale*
rejoint sa voisine. En option, un **plateau** rectangulaire autour, à
découper dans la même planche pour y encastrer les pièces ; le dessin
vectoriel passe sur un calque *Gravure* exécuté avant les découpes.
Le seuil dépend de la taille du dessin : réduit à 90 mm de large, un
contour de dessin animé ne fait plus que 0,6 à 1 mm.

![Boîte, labyrinthes et puzzle](../images/makers.png)

## Motifs par style

**Édition › Générer un motif…** : viking, celte, africain, maya / aztèque,
grec, japonais, art déco, polynésien, islamique ; en **frises** ou en
**pavage** ; **Nouvelle variante** tire une autre combinaison, le numéro de
variante permet de la retrouver.

![Fenêtre des motifs](images/motifs.png)

![Motifs par style](../images/ornaments.png)

## Grappe (pièces dans un cadre)

**Édition › Grappe…** range les pièces sélectionnées en rangées dans un
cadre, comme une maquette ou une planche de puzzle 3D, et active les
**ponts** (laser) ou les **attaches** (CNC) placés automatiquement. Le
cadre est sur son propre calque, découpé en dernier.

![Fenêtre grappe](images/grappe.png)

---

<sub>[← 3D, 4 et 5 axes](06-3d-multi-axes.md) · [Sommaire](README.md) · [Machines et connexion →](08-machines.md)</sub>
