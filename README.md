# MC Exporter

Exporter for **Minecraft Java** worlds and schematics to **OBJ + MTL + PNG textures** 3D models, ready for Blender, Cinema 4D, Maya, 3ds Max, Unreal, Unity, and more.

It is a native Windows desktop application: a single, self-contained `.exe`, with no Java, no browser and no installation. The idea is the same as jMc2Obj or Mineways, with two additions:

- support for the **latest versions** of the game (26.x and the new `dimensions/` folder);
- very broad **entity** support.

The interface is available in **English** (default), **French**, **Spanish**, **German**, **Portuguese** and **Italian**, selectable from the top bar.

## Download

The latest version is available on the [Releases](https://github.com/SkyNigh1/mc-exporter/releases/latest) page. Download `mc-exporter.exe` and run it; there is nothing to install.

---

## Contents

- [How it works](#how-it-works)
- [What can be exported](#what-can-be-exported)
  - [Blocks](#blocks)
  - [Specially rendered blocks](#specially-rendered-blocks)
  - [Decorative entities](#decorative-entities)
  - [Mobs](#mobs)
  - [Equipment and decorations](#equipment-and-decorations)
  - [Projectiles and small entities](#projectiles-and-small-entities)
- [Supported sources](#supported-sources)
- [Export options](#export-options)
- [Output files](#output-files)
- [Building](#building)
- [Command line](#command-line)
- [Known limitations](#known-limitations)
- [License](#license)

---

## How it works

1. Pick a world from `.minecraft/saves` (or any folder), or **drag a schematic** onto the window.
2. The top-down map is drawn. Left-drag to pan, use the mouse wheel to zoom, **right-drag (or Shift + drag) to select** the area.
3. Set the floor and ceiling, then the export options.
4. **Export…**

### Game textures and models

The application ships with no Minecraft assets. It reads textures, block models, biomes and fonts directly from the jar of your installed version (`.minecraft/versions/…`), and automatically picks the one that matches the world. As a result:

- every vanilla block is supported, including blocks from future versions;
- **resource packs** can be layered on top (`.zip` or folder).

---

## What can be exported

### Blocks

- **Every vanilla block**, from the game's blockstates and model JSON files: variants, multipart, rotations, `uvlock`, rotated elements, textured faces.
- **Biome colors**: grass, foliage, dry foliage, water (including the dark forest and swamp modifiers).
- **Fixed tints**: birch, spruce, lily pad, stems, redstone (by power level).
- **Fluids** (water and lava) with their height, and **waterlogged blocks** (seagrass, kelp…).
- **Unknown blocks** (mods) shown as magenta checkerboard cubes, when the option is enabled.
- **Culling**: hidden faces between opaque blocks are removed, as well as between identical glass and ice blocks.

### Specially rendered blocks

These blocks are drawn by code in the game, and are missing or incomplete in other exporters. Their dimensions come from the game's models.

| Block | Details |
|---|---|
| Chests | single, double (left/right), trapped, ender, copper (all oxidation stages, waxed or not) |
| Shulker boxes | 17 colors, 6 orientations |
| Heads | skeleton, wither skeleton, zombie, creeper, piglin, dragon, **player heads with their real skin** |
| Banners | standing (16 rotations) and wall banners, **with all their patterns** (current and legacy formats) |
| Signs | standing, wall, hanging and wall hanging signs, **with the text** on both sides (colors, glowing text, accents, JSON/NBT components) |
| Beds | all colors (including versions where the game does not ship a bed model) |
| Decorated pot | with the **sherds** on each side |
| Bell | the bell body in addition to its frame |
| Conduit | shell |
| End portals | portal and gateway |
| Copper golem statue | 4 poses (standing, running, sitting, star), 4 oxidation stages |

### Decorative entities

| Entity | Details |
|---|---|
| **Block display** | any block, with rotation and full transformation (`left_rotation`, `scale`, `right_rotation`, `translation`, or a 4×4 matrix) |
| **Item display** | flat items extruded in 3D or block items, according to the display mode (`fixed`, `head`, `ground`, `gui`, `none`…) |
| **Text display** | colored text, background, alignment, line wrapping according to `line_width` |
| **Item frames and glow item frames** | with the item inside and its rotation, or **the map image** (exact game palette) |
| **Paintings** | all variants and sizes, back and edges |
| **Armor stands** | full pose (head, body, arms, legs), arms shown or hidden, base plate, small size, **armor**, item on the head, held items |
| **Minecarts** | empty or loaded (chest, furnace, TNT, hopper, spawner, command block, custom block) |
| **Boats and chest boats** | every wood type |
| **Falling blocks** | sand, gravel, etc. |

### Mobs

Dimensions are read from the game's model classes, textures from your installed version. Baby mobs are rendered at half size when the game does so.

- **Farm animals**: pig, cow, mooshroom, sheep (wool in the right color, sheared or not), chicken.
- **Animals**:
  - rabbit, wolf (variants, tamed), fox (red and snow), cat (all breeds), ocelot;
  - horse (7 coats, 5 markings), donkey, mule, skeleton horse, zombie horse;
  - llama (4 colors), trader llama, camel, camel husk;
  - goat (with or without horns), panda (genes), polar bear, armadillo, sniffer;
  - bee, parrot (5 colors), bat, frog (3 variants), turtle, axolotl (5 colors).
- **Aquatic creatures**: squid, glow squid, dolphin, cod, salmon, pufferfish (3 puff states), tropical fish (2 shapes, 6 patterns, body and pattern colors), tadpole, nautilus, zombie nautilus.
- **Golems and allies**: iron golem, snow golem (with pumpkin), copper golem, allay.
- **Villagers**: villager (7 biomes, all professions), wandering trader, zombie villager, witch.
- **Undead**: zombie, husk, drowned, giant, skeleton, stray, wither skeleton, bogged, parched, zombified piglin.
- **Nether**: piglin, piglin brute, hoglin, zoglin, blaze, magma cube, ghast, happy ghast, strider.
- **Monsters**: creeper, spider, cave spider, enderman, endermite, silverfish, slime, sulfur cube, phantom, vex, breeze, creaking, shulker (color and attached face), guardian, elder guardian, warden.
- **Illagers**: pillager, vindicator, evoker, illusioner, ravager.
- **Bosses**: wither, ender dragon.

### Equipment and decorations

- **Armor** on armor stands and humanoid mobs: leather (dyed or not), chainmail, iron, gold, diamond, netherite, copper, turtle shell.
- **Armor trims**: every pattern and material, including the darker variant when the trim material matches the armor material. Trims are merged into the armor texture, so there are no overlapping faces.
- **Held items** in the main and off hand (zombies, skeletons, piglins, pillagers…), crossed arms for villagers and witches.
- **Items on the head**: pumpkins, heads, the **pillager captain's banner**…
- **Saddles**: pig, horse, donkey, mule, skeleton and zombie horses, strider, camel (with reins and bridle), nautilus.
- **Animal armor**: horse (dyed leather, iron, gold, diamond), wolf (armadillo scute, dyed), nautilus.
- **Llamas**: carpets in all 16 colors, chests.
- **Donkeys and mules**: chests on their sides.
- **Happy ghast**: harness and goggles.

### Projectiles and small entities

- Arrows (regular and spectral) and tridents, oriented along their flight path.
- Experience orbs (size based on their value).
- Leash knots.
- Thrown items: snowballs, eggs, ender pearls, eyes of ender, potions, bottles o' enchanting, fireballs, firework rockets, wind charges.
- Wither skulls, primed TNT, end crystals, dropped items.

---

## Supported sources

- **Java worlds from 1.18 onwards**, including the new `dimensions/<namespace>/<name>/region` layout of 26.x versions (no copy or conversion needed). Dimensions are detected automatically (Overworld, Nether, End, datapack dimensions).
- **Schematics**, imported into an empty world and processed exactly like a real world:

  | Format | Extension |
  |---|---|
  | Sponge v1, v2, v3 (WorldEdit, FAWE) | `.schem` |
  | MCEdit / WorldEdit, legacy pre-1.13 format with numeric ids | `.schematic` |
  | Litematica | `.litematic` |
  | Vanilla structures (structure block) | `.nbt` |

  Block entities and entities stored in schematics are exported too. Blocks renamed between versions (`grass` → `short_grass`, `grass_path` → `dirt_path`, `chain` → `iron_chain`…) are handled.

---

## Export options

| Option | Effect |
|---|---|
| Scale | size of one block in the 3D file |
| Offset | none, centered on the selection, or custom |
| Block selection | exclude specific blocks from the export |
| Unknown blocks | show mod blocks as checkerboard cubes |
| World sides and bottom | close the edges of the selection |
| Biome colors | tint grass, foliage and water according to the biome |
| Ores to stone | replace ores with the surrounding stone |
| Specially rendered blocks | chests, banners, heads, signs… |
| Sign text | sign text exported as a texture |
| Entities | mobs, displays, item frames, paintings… |
| Player head skins | read from the game cache, then downloaded from Mojang when needed |
| Ghasts on the ground | lift ghasts so that their tentacles touch the ground |
| One object per material / per chunk | how objects are split in the file |
| Optimize mesh | merges identical adjacent faces (much lighter files) |
| Double-sided faces | duplicates the faces of plants and thin panes |
| Random variants | picks model variants like the game does (grass, stone…) |
| OBJ groups | `g` instead of `o` (Maya compatible) |
| Textures | PNG export, ×1 to ×16 upscaling, alpha in separate files |

Options, the interface language and the last selection are remembered between sessions.

---

## Output files

```
my_export.obj        geometry (vertices, UVs, normals, faces)
my_export.mtl        materials
my_export_tex/       one PNG texture per material
```

Each material is named after its texture (`oak_planks`, `grass_block_top`…). Biome tints, banners, sign text, maps and trimmed armor are generated as separate textures.

---

## Building

You need [Rust](https://rustup.rs) (2024 edition) and, on Windows, the Visual Studio C++ build tools.

```sh
cargo build --release
```

The executable is written to `target/release/mc-exporter.exe`. The C runtime is statically linked (see `.cargo/config.toml`), so the exe runs on its own on any 64-bit Windows.

At runtime, the application needs **a version of Minecraft Java that has been launched at least once** on the machine, since it reads the game's resources from it. A `.jar` can also be selected manually in the Settings.

---

## Command line

Useful for automation:

```sh
# export an area of a world
mc-exporter.exe --export "<world folder>" minecraft:overworld x0 z0 x1 z1 y0 y1 output.obj

# export a whole schematic (jar is optional)
mc-exporter.exe --schem file.schem output.obj [path/to/minecraft.jar]

# map image of a region
mc-exporter.exe --map "<world folder>" minecraft:overworld rx rz map.png
```

The file `exemples/demo-entites.schem` contains one instance of every supported block entity and entity, and serves as a visual test.

---

## Known limitations

- Worlds older than 1.18 (old chunk format) and LZ4-compressed regions are not supported.
- Entities are frozen in a rest pose: there is no animation.
- Maps in item frames are not available for schematics, which do not contain map data.
- Mod blocks are not rendered (only a checkerboard cube).

---

## License

Released under the [MIT](LICENSE) license.

*Independent project, not affiliated with Mojang Studios or Microsoft. Minecraft is a trademark of Mojang Synergies AB. No game assets are distributed with this repository.*
