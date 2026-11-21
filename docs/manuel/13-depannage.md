<sub>[← Assistants IA (MCP)](12-assistants-ia.md) · [Sommaire](README.md)</sub>

# 13. Dépannage et questions fréquentes

## La machine ne répond pas

- Vérifiez le câble, puis **Détecter les machines** : la vitesse et le
  protocole sont trouvés tout seuls.
- Une Carvera ou une machine Smoothieware demande le protocole
  *Smoothieware / Carvera*.
- En Wi-Fi, l'ordinateur et la machine doivent être sur le même réseau ; un
  pare-feu peut bloquer la recherche (port UDP 3333 pour la Carvera).

## Alarmes

Les alarmes GRBL et les raisons d'arrêt de la Carvera sont expliquées en
clair avec l'action à faire. Les plus fréquentes :

| Alarme | Que faire |
|---|---|
| 1 Fin de course | position perdue : refaites le retour à l'origine |
| 2 Limite logicielle | la cible dépasse la course : déverrouillez, corrigez le placement |
| 5 Palpage | la sonde n'a rien touché : vérifiez la distance et le câblage |
| 8, 9 Retour à l'origine | dégagement ou fin de course : vérifiez le câblage et la mécanique |

## Le homing ne marche pas

Après un échec, un encadré sous les boutons de la machine dit quoi faire :

- **Homing désactivé (`$22=0`)** : fréquent sur les machines sans fins de
  course. Placez la tête à la main puis *Déverrouiller*. *Activer le
  homing* seulement si des fins de course sont câblés.
- **Fin de course introuvable (alarme 9)** : vérifiez le câblage (un
  contacteur enfoncé à la main doit apparaître dans « Entrées actives ») et
  la course réglée, affichée dans l'encadré.
- **Dégagement raté (alarme 8)** : un bouton augmente le recul à 3 mm
  (`$27`).
- **Homing interrompu** : avec *Homing en maintenant le bouton*, gardez le
  bouton enfoncé jusqu'à la fin, ou décochez l'option dans les préférences.
- **Trop long** : un axe force ou une courroie patine ; vérifiez avant de
  relancer.

## Le démarrage est bloqué

Le panneau Travail affiche la raison : cadrage à approuver, origine à
palper, outil à mesurer, collision dans le programme, porte ouverte,
travail hors course. Le **mode avancé** permet de lever les verrous, mais
la surveillance en direct reste active.

## Questions fréquentes

**Mes projets MaxmakeLAB s'ouvrent-ils ?** Les SVG et DXF s'importent ; la
bibliothèque de matériaux s'importe au premier lancement.

**Puis-je utiliser mes polices ?** Oui : ajoutez-les dans le dossier
indiqué dans les préférences.

**Les réglages sont-ils sur une clé USB ?** Avec
`MYMAXMAKELAB_CONFIG_DIR=/chemin/de/la/cle`.

**Où signaler un problème ?** Sur
[GitHub](https://github.com/barracudapps/MyMaxmakeLab/issues).

## Glossaire

| Terme | Sens |
|---|---|
| Cadrage | passage à blanc à faible puissance autour du dessin |
| Drop-cutter | calcul qui descend l'outil jusqu'à toucher la surface sous toute son empreinte |
| G93 | avance en temps inverse, pour les mouvements qui mêlent rotation et translation |
| Longueur réelle | longueur de l'outil qui dépasse de l'écrou de pince, mesurée après installation |
| Martyr | plaque sacrifiée sous la pièce pour les découpes traversantes |
| Palpage | mesure par contact d'une sonde (origine, surface, outil) |
| Pont, attache | petite partie non découpée qui retient la pièce |

---

<sub>[← Assistants IA (MCP)](12-assistants-ia.md) · [Sommaire](README.md)</sub>
