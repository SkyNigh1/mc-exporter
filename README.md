# MC Exporter

Exporteur de mondes et de schematics **Minecraft Java** vers des modèles 3D **OBJ + MTL + textures PNG**, prêts pour Blender, Cinema 4D, Maya, 3ds Max, Unreal, Unity, etc.

C'est un logiciel de bureau natif pour Windows : un seul `.exe` autonome, sans Java, sans navigateur et sans installation. L'idée est la même que celle de jMc2Obj ou Mineways, avec en plus :

- la compatibilité avec les **dernières versions** du jeu (26.x et le nouveau dossier `dimensions/`) ;
- une prise en charge très large des **entités**.

## Téléchargement

La dernière version est disponible dans les [Releases](https://github.com/SkyNigh1/mc-exporter/releases/latest). Télécharge `mc-exporter.exe` et lance-le : il n'y a rien à installer.

---

## Sommaire

- [Fonctionnement](#fonctionnement)
- [Ce qui peut être exporté](#ce-qui-peut-être-exporté)
  - [Blocs](#blocs)
  - [Blocs à rendu spécial](#blocs-à-rendu-spécial)
  - [Entités décoratives](#entités-décoratives)
  - [Mobs](#mobs)
  - [Équipement et décorations](#équipement-et-décorations)
  - [Projectiles et petites entités](#projectiles-et-petites-entités)
- [Sources lisibles](#sources-lisibles)
- [Options d'export](#options-dexport)
- [Fichiers produits](#fichiers-produits)
- [Compiler](#compiler)
- [Ligne de commande](#ligne-de-commande)
- [Limites connues](#limites-connues)

---

## Fonctionnement

1. Choisir un monde dans `.minecraft/saves` (ou n'importe quel dossier), ou **glisser un schematic** sur la fenêtre.
2. La carte vue de dessus se dessine. Clic gauche pour se déplacer, molette pour zoomer, **clic droit (ou Maj + clic) et glisser pour sélectionner** la zone.
3. Régler le plancher et le plafond, puis les options d'export.
4. **Exporter…**

### Textures et modèles du jeu

Le logiciel n'embarque aucune ressource de Minecraft. Il lit les textures, les modèles de blocs, les biomes et les polices directement dans le jar de ta version installée (`.minecraft/versions/…`), et choisit automatiquement celle qui correspond au monde. Résultat :

- tous les blocs vanilla sont gérés, y compris ceux des futures versions ;
- les **packs de ressources** peuvent être ajoutés par-dessus (`.zip` ou dossier).

---

## Ce qui peut être exporté

### Blocs

- **Tous les blocs vanilla**, d'après les blockstates et modèles JSON du jeu : variantes, multipart, rotations, `uvlock`, éléments tournés, faces texturées.
- **Couleurs des biomes** : herbe, feuillage, feuilles mortes, eau (dont les modificateurs « forêt noire » et « marais »).
- **Teintes fixes** : bouleau, épicéa, nénuphar, tiges, redstone (selon sa puissance).
- **Fluides** (eau et lave) avec leur hauteur, et **blocs immergés** (`waterlogged`, algues, varech…).
- **Blocs inconnus** (mods) affichés en cubes damier magenta, si l'option est activée.
- **Culling** : les faces cachées entre blocs opaques sont supprimées, ainsi qu'entre vitres et glaces identiques.

### Blocs à rendu spécial

Ces blocs sont dessinés par du code dans le jeu, et sont absents des autres exporteurs ou incomplets. Leurs dimensions sont reprises des modèles du jeu.

| Bloc | Détails |
|---|---|
| Coffres | simple, double (gauche/droite), piégé, ender, cuivre (tous les stades d'oxydation, cirés ou non) |
| Boîtes de shulker | 17 couleurs, 6 orientations |
| Têtes | squelette, wither, zombie, creeper, piglin, dragon, **joueur avec sa vraie skin** |
| Bannières | au sol (16 rotations) et murales, **avec tous leurs motifs** (format actuel et ancien) |
| Panneaux | debout, muraux, suspendus et suspendus muraux, **avec le texte** des deux faces (couleurs, texte lumineux, accents, composants JSON/NBT) |
| Lits | toutes couleurs (y compris pour les versions où le jeu ne fournit pas leur modèle) |
| Pot décoré | avec les **tessons** sur chaque face |
| Cloche | corps de la cloche en plus de son support |
| Conduit | coque |
| Portails de l'End | portail et passerelle |
| Statue de golem de cuivre | 4 poses (debout, course, assis, étoile), 4 stades d'oxydation |

### Entités décoratives

| Entité | Détails |
|---|---|
| **Block display** | n'importe quel bloc, avec rotation et matrice de transformation complète (`left_rotation`, `scale`, `right_rotation`, `translation`, ou matrice 4×4) |
| **Item display** | objets plats en relief ou objets-blocs, selon le mode d'affichage (`fixed`, `head`, `ground`, `gui`, `none`…) |
| **Text display** | texte coloré, fond, alignement, retour à la ligne selon `line_width` |
| **Cadres et cadres lumineux** | avec l'objet dedans et sa rotation, ou **l'image de la carte** (palette exacte du jeu) |
| **Tableaux** | toutes les variantes et tailles, dos et tranches |
| **Porte-armures** | pose complète (tête, corps, bras, jambes), bras visibles ou non, socle, petite taille, **armure**, objet sur la tête, objets en main |
| **Wagonnets** | vides ou chargés (coffre, four, TNT, entonnoir, générateur, bloc de commande, bloc personnalisé) |
| **Bateaux et bateaux à coffre** | toutes les essences |
| **Blocs qui tombent** | sable, gravier, etc. |

### Mobs

Les dimensions sont lues dans les classes de modèles du jeu, les textures dans ta version installée. Les bébés sont rendus à demi-taille quand le jeu le fait.

- **Animaux de ferme** : cochon, vache, champimeuh, mouton (laine de la bonne couleur, tondu ou non), poule.
- **Animaux** :
  - lapin, loup (variantes, apprivoisé), renard (roux et des neiges), chat (toutes les races), ocelot ;
  - cheval (7 robes, 5 marques), âne, mule, cheval squelette, cheval zombie ;
  - lama (4 couleurs), lama du marchand, chameau, chameau momifié ;
  - chèvre (avec ou sans cornes), panda (gènes), ours polaire, tatou, renifleur ;
  - abeille, perroquet (5 couleurs), chauve-souris, grenouille (3 variantes), tortue, axolotl (5 couleurs).
- **Créatures aquatiques** : calmar, calmar luisant, dauphin, morue, saumon, poisson-globe (3 états de gonflement), poisson tropical (2 formes, 6 motifs, couleurs du corps et du motif), têtard, nautile, nautile zombie.
- **Golems et alliés** : golem de fer, golem de neige (avec citrouille), golem de cuivre, allay.
- **Villageois** : villageois (7 biomes, tous les métiers), marchand ambulant, zombie-villageois, sorcière.
- **Morts-vivants** : zombie, husk, noyé, géant, squelette, vagabond, squelette wither, bogged, parched, piglin zombifié.
- **Nether** : piglin, piglin brute, hoglin, zoglin, blaze, cube de magma, ghast, happy ghast, arpenteur.
- **Monstres** : creeper, araignée, araignée venimeuse, enderman, endermite, poisson d'argent, slime, cube de soufre, phantom, vex, breeze, creaking, shulker (couleur et face d'accroche), gardien, gardien ancien, warden.
- **Illageois** : pillard, vindicateur, évocateur, illusionniste, ravageur.
- **Boss** : wither, dragon de l'Ender.

### Équipement et décorations

- **Armures** sur les porte-armures et les mobs humanoïdes : cuir (teint ou non), maille, fer, or, diamant, netherite, cuivre, carapace de tortue.
- **Garnitures d'armure (trims)** : tous les motifs et matériaux, y compris la variante plus foncée quand le matériau est celui de l'armure. Elles sont fusionnées dans la texture de l'armure, sans faces superposées.
- **Objets tenus** en main droite et gauche (zombies, squelettes, piglins, pillards…), bras croisés pour les villageois et les sorcières.
- **Objets sur la tête** : citrouilles, têtes, **bannière du capitaine pillard**…
- **Selles** : cochon, cheval, âne, mule, chevaux squelette et zombie, arpenteur, chameau (avec rênes et bride), nautile.
- **Armures d'animaux** : cheval (cuir teint, fer, or, diamant), loup (écailles de tatou, teintes), nautile.
- **Lamas** : tapis des 16 couleurs, coffres.
- **Ânes et mules** : coffres sur les flancs.
- **Happy ghast** : harnais et lunettes.

### Projectiles et petites entités

- Flèches (normales et spectrales) et trident, orientés selon leur trajectoire.
- Orbes d'expérience (taille selon la valeur).
- Nœuds de laisse.
- Projectiles lancés : boules de neige, œufs, perles de l'Ender, œils de l'Ender, potions, fioles d'expérience, boules de feu, fusées, charges de vent.
- Crânes de wither, TNT amorcée, cristaux de l'End, objets lâchés au sol.

---

## Sources lisibles

- **Mondes Java 1.18 et plus récents**, dont le nouveau dossier `dimensions/<namespace>/<nom>/region` des versions 26.x (sans copie ni conversion). Les dimensions sont détectées automatiquement (Overworld, Nether, End, dimensions de datapacks).
- **Schematics**, importés dans un monde vide et traités exactement comme un vrai monde :

  | Format | Extension |
  |---|---|
  | Sponge v1, v2, v3 (WorldEdit, FAWE) | `.schem` |
  | MCEdit / WorldEdit, ancien format avec identifiants numériques d'avant la 1.13 | `.schematic` |
  | Litematica | `.litematic` |
  | Structures vanilla (bloc de structure) | `.nbt` |

  Les blocs spéciaux et les entités contenus dans les schematics sont exportés aussi. Les blocs renommés entre versions (`grass` → `short_grass`, `grass_path` → `dirt_path`, `chain` → `iron_chain`…) sont gérés.

---

## Options d'export

| Option | Effet |
|---|---|
| Échelle | taille d'un bloc dans le fichier 3D |
| Décalage | aucun, centré sur la sélection, ou personnalisé |
| Choix des blocs | exclure des blocs précis de l'export |
| Blocs inconnus | afficher les blocs de mods en cubes damier |
| Côtés et dessous du monde | fermer les bords de la sélection |
| Couleurs des biomes | teinter herbe, feuillage et eau selon le biome |
| Minerais en pierre | remplacer les minerais par la roche environnante |
| Blocs à rendu spécial | coffres, bannières, têtes, panneaux… |
| Texte des panneaux | texte des panneaux exporté en texture |
| Entités | mobs, displays, cadres, tableaux… |
| Skins des têtes de joueur | lecture du cache du jeu, puis téléchargement chez Mojang si nécessaire |
| Ghasts posés au sol | remonter les ghasts pour que leurs tentacules touchent le sol |
| Un objet par matériau / par chunk | découpage des objets dans le fichier |
| Optimiser le maillage | fusion des faces identiques adjacentes (fichiers beaucoup plus légers) |
| Faces recto-verso | doubler les faces des plantes et vitres fines |
| Variantes aléatoires | choisir les variantes de modèles comme le jeu (herbe, pierre…) |
| Groupes OBJ | `g` au lieu de `o` (compatible Maya) |
| Textures | export des PNG, agrandissement ×1 à ×16, alpha dans des fichiers séparés |

Les options et la dernière sélection sont mémorisées entre deux lancements.

---

## Fichiers produits

```
mon_export.obj        géométrie (sommets, UV, normales, faces)
mon_export.mtl        matériaux
mon_export_tex/       une texture PNG par matériau
```

Chaque matériau porte le nom de sa texture (`oak_planks`, `grass_block_top`…). Les teintes de biome, les bannières, les textes de panneaux, les cartes et les armures garnies sont générés comme textures à part.

---

## Compiler

Il faut [Rust](https://rustup.rs) (édition 2024) et, sous Windows, les outils de compilation C++ de Visual Studio.

```sh
cargo build --release
```

L'exécutable est produit dans `target/release/mc-exporter.exe`. Le runtime C est lié en statique (voir `.cargo/config.toml`), donc l'exe fonctionne seul sur n'importe quel Windows 64 bits.

Pour fonctionner, le logiciel a besoin d'**une version de Minecraft Java déjà lancée au moins une fois** sur la machine, puisqu'il y lit les ressources du jeu. On peut aussi désigner un `.jar` à la main dans les Paramètres.

---

## Ligne de commande

Pratique pour automatiser :

```sh
# exporter une zone d'un monde
mc-exporter.exe --export "<dossier du monde>" minecraft:overworld x0 z0 x1 z1 y0 y1 sortie.obj

# exporter un schematic entier (jar facultatif)
mc-exporter.exe --schem fichier.schem sortie.obj [chemin/vers/minecraft.jar]

# image de la carte d'une région
mc-exporter.exe --map "<dossier du monde>" minecraft:overworld rx rz carte.png
```

Le fichier `exemples/demo-entites.schem` contient un exemplaire de chaque bloc spécial et de chaque entité gérés. Il sert de test visuel.

---

## Limites connues

- Les mondes d'avant la 1.18 (ancien format de chunks) et la compression LZ4 des régions ne sont pas lus.
- Les entités sont figées dans une pose de repos : pas d'animation.
- Les cartes dans les cadres ne sont pas disponibles pour les schematics, qui ne contiennent pas les données des cartes.
- Pas de rendu des blocs de mods (seulement un cube damier).

---

## Licence

Distribué sous licence [MIT](LICENSE).

*Projet indépendant, sans lien avec Mojang Studios ni Microsoft. Minecraft est une marque de Mojang Synergies AB. Aucune ressource du jeu n'est distribuée avec ce dépôt.*
