# MC Exporter

Exporter for **Minecraft Java** worlds and schematics to 3D models:

- **OBJ + MTL + PNG textures**, for Blender, Cinema 4D, Maya, 3ds Max, Unreal, Unity, and more;
- **glTF (.glb)**, a single file with embedded textures and proper transparency;
- **STL** and **3MF** for **3D printing**: closed, manifold models at real size in millimetres.

It is a native Windows desktop application: a single, self-contained `.exe`, with no Java, no browser and no installation. The idea is the same as jMc2Obj or Mineways, with two additions:

- support for **every Java version**, from Beta worlds to the latest versions (26.x and the new `dimensions/` folder);
- very broad **entity** support;
- a built-in **3D preview** of the selection.

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
- [Resource packs](#resource-packs)
- [Export options](#export-options)
- [3D printing](#3d-printing)
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
4. Optionally check the result in the **3D preview** tab (left drag to rotate, right drag to move, wheel to zoom). It is rebuilt automatically when the selection or the options change.
5. Choose the format and **Export…**

### Game textures and models

The application ships with no Minecraft assets. It reads textures, block models, biomes and fonts directly from the jar of your installed version (`.minecraft/versions/…`), and automatically picks the one that matches the world. As a result:

- every vanilla block is supported, including blocks from future versions;
- **resource packs** can be layered on top (see [Resource packs](#resource-packs)).

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

- **Java worlds of every version**, with no conversion needed:

  | Versions | Format |
  |---|---|
  | Beta 1.3 – 1.1 | McRegion (`.mcr`), numeric block ids |
  | 1.2 – 1.12 | Anvil, numeric block ids, converted to modern blocks |
  | 1.13 – 1.17 | Anvil, block palettes under `Level` |
  | 1.18 → 26.x | current format, including the new `dimensions/<namespace>/<name>/region` layout |

  Old biome ids, old entity names (`EntityHorse`, `PigZombie`, `MinecartChest`…), old equipment lists and 2D item frame directions are converted too. Dimensions are detected automatically (Overworld, Nether, End, datapack dimensions).
- **Schematics**, imported into an empty world and processed exactly like a real world:

  | Format | Extension |
  |---|---|
  | Sponge v1, v2, v3 (WorldEdit, FAWE) | `.schem` |
  | MCEdit / WorldEdit, legacy pre-1.13 format with numeric ids | `.schematic` |
  | Litematica | `.litematic` |
  | Vanilla structures (structure block) | `.nbt` |

  Block entities and entities stored in schematics are exported too. Blocks renamed between versions (`grass` → `short_grass`, `grass_path` → `dirt_path`, `chain` → `iron_chain`…) are handled.

---

## Resource packs

Resource packs are set in **Settings**. Their textures, block models and blockstates replace the game's, in the order of the list (the first one has priority), for every format and for the 2D map and 3D preview.

- **Use the packs enabled in Minecraft** takes the packs and the order from the game's `options.txt`.
- Packs found in `.minecraft/resourcepacks` can be added in one click; any `.zip` or folder can be added too.
- Pack **overlays** (folders that a pack applies only to some game versions) are applied according to the version of the jar in use, and zips that wrap the pack in an extra folder are handled.
- Higher-resolution packs (32×, 64×, 128×…) are exported at their own resolution.

---

## Export options

| Option | Effect |
|---|---|
| Scale | size of one block in the 3D file |
| Offset | none, centered on the selection, or custom |
| Block selection | exclude specific blocks from the export |
| Unknown blocks | show mod blocks as checkerboard cubes |
| World sides and bottom | close the edges of the selection |
| Hide closed cavities | drop what cannot be seen from outside the selection: sealed caves and rooms, with the blocks, mobs and items inside (the selection edges and glass, leaves, slabs… count as openings) |
| Biome colors | tint grass, foliage and water according to the biome |
| Ores to stone | replace ores with the surrounding stone |
| Specially rendered blocks | chests, banners, heads, signs… |
| Sign text | sign text exported as a texture |
| Entities | mobs, displays, item frames, paintings… |
| Player head skins | read from the game cache, then downloaded from Mojang when needed |
| Ghasts on the ground | lift ghasts so that their tentacles touch the ground |
| Clouds | the cloud layer above the selection, as 12×4×12 boxes like the game draws them (height adjustable: 192 since 1.18, 128 before); can be turned off |
| One object per material / per chunk | how objects are split in the file |
| Optimize mesh | merges identical adjacent faces (much lighter files) |
| Double-sided faces | duplicates the faces of plants and thin panes |
| Random variants | picks model variants like the game does (grass, stone…) |
| OBJ groups | `g` instead of `o` (Maya compatible, OBJ only) |
| Textures | PNG export, ×1 to ×16 upscaling, alpha in separate files (OBJ only) |

Options, the interface language and the last selection are remembered between sessions.

---

## 3D printing

With the **STL** or **3MF** format, the selection is turned into a solid model ready for a slicer (Cura, PrusaSlicer, Bambu Studio…):

| Option | Effect |
|---|---|
| Size of one block | in millimetres; the resulting maximum size is shown |
| Detail | whole blocks, half blocks or quarter blocks: slabs, stairs, walls, beds, heads… keep their shape from the block models |
| Hollow | keeps walls of the chosen thickness (in blocks) and empties the inside, to save material |
| Remove floating pieces | deletes disconnected pieces smaller than the given number of blocks (stray leaves, lone blocks…) |

The model is always **closed and manifold**: every edge is shared by exactly two faces, enclosed air pockets are filled, and blocks that touch only by an edge or a corner are joined. Plants, panes, rails, water and other parts too thin to print are left out. The **3MF** file also carries one color per block (from its texture and biome), and Z points up in both formats.

---

## Output files

```
my_export.obj        geometry (vertices, UVs, normals, faces)
my_export.mtl        materials
my_export_tex/       one PNG texture per material

my_export.glb        everything in one file (geometry, materials, PNG textures)

my_export.stl        3D printing: binary STL, millimetres
my_export.3mf        3D printing: millimetres, with colors
```

Each material is named after its texture (`oak_planks`, `grass_block_top`…). Biome tints, banners, sign text, maps and trimmed armor are generated as separate textures. In the glTF file, cut-out textures (leaves, plants, glass panes) use alpha masking and translucent ones (water, stained glass, clouds) use blending; textures are sampled with nearest filtering to keep the pixel-art look.

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
# export an area of a world (the format follows the extension: .obj, .glb, .stl or .3mf)
mc-exporter.exe --export "<world folder>" minecraft:overworld x0 z0 x1 z1 y0 y1 output.obj

# export a whole schematic (jar is optional)
mc-exporter.exe --schem file.schem output.glb [path/to/minecraft.jar]

# map image of a region
mc-exporter.exe --map "<world folder>" minecraft:overworld rx rz map.png
```

Extra options, anywhere on the line:

| Option | Effect |
|---|---|
| `--pack <zip or folder>` | add a resource pack (repeat for several, first has priority) |
| `--hide-cavities` | hide closed cavities |
| `--no-clouds` | no cloud layer |
| `--block-mm <mm>` | 3D printing: size of one block (default 1) |
| `--detail <1, 2 or 4>` | 3D printing: cells per block edge (default 2) |
| `--hollow <blocks>` | 3D printing: hollow with this wall thickness |

The file `exemples/demo-entites.schem` contains one instance of every supported block entity and entity, and serves as a visual test.

---

## Known limitations

- LZ4-compressed regions are not supported.
- Clouds are static: they are placed as they appear at the start of a day, without their drift.
- Entities are frozen in a rest pose: there is no animation.
- Maps in item frames are not available for schematics, which do not contain map data.
- Mod blocks are not rendered (only a checkerboard cube).
- OptiFine-specific pack features (connected textures, emissive textures, random entities) are not used.
- 3D printing keeps one color per block, not the texture details.

---

## License

Released under the [MIT](LICENSE) license.

*Independent project, not affiliated with Mojang Studios or Microsoft. Minecraft is a trademark of Mojang Synergies AB. No game assets are distributed with this repository.*
