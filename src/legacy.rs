//! Older world formats:
//! - 1.13 – 1.17: paletted sections under `Level` (entries spanning two longs before 1.16);
//! - 1.2 – 1.12 (Anvil): numeric block ids (`Blocks` / `Add` / `Data`);
//! - Beta 1.3 – 1.1 (McRegion, `.mcr`): a single 128-block-high column;
//! plus numeric biomes, block entities stored in chunks, and old entity names.

use crate::registry::Registry;
use crate::world::{Chunk, Section};
use fastnbt::Value;
use std::collections::HashMap;

fn get<'a>(v: Option<&'a Value>, k: &str) -> Option<&'a Value> {
    match v? {
        Value::Compound(m) => m.get(k),
        _ => None,
    }
}

fn int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Byte(x) => Some(*x as i64),
        Value::Short(x) => Some(*x as i64),
        Value::Int(x) => Some(*x as i64),
        Value::Long(x) => Some(*x),
        _ => None,
    }
}

fn bytes(v: Option<&Value>) -> Option<Vec<u8>> {
    match v? {
        Value::ByteArray(a) => Some(a.iter().map(|b| *b as u8).collect()),
        _ => None,
    }
}

/// Numeric biome id (before 1.18) → modern biome name
pub fn legacy_biome(id: i64) -> &'static str {
    match id {
        0 => "ocean", 1 => "plains", 2 | 17 | 130 => "desert", 3 | 20 => "windswept_hills", 4 | 18 => "forest",
        5 | 19 | 133 => "taiga", 6 | 134 => "swamp", 7 => "river", 8 => "nether_wastes", 9 => "the_end",
        10 => "frozen_ocean", 11 => "frozen_river", 12 | 13 => "snowy_plains", 14 | 15 => "mushroom_fields",
        16 => "beach", 21 | 22 | 149 => "jungle", 23 | 151 => "sparse_jungle", 24 => "deep_ocean", 25 => "stony_shore",
        26 => "snowy_beach", 27 | 28 => "birch_forest", 29 | 157 => "dark_forest", 30 | 31 | 158 => "snowy_taiga",
        32 | 33 => "old_growth_pine_taiga", 34 => "windswept_forest", 35 => "savanna", 36 => "savanna_plateau",
        37 | 39 | 167 => "badlands", 38 | 166 => "wooded_badlands", 40 => "small_end_islands", 41 => "end_midlands",
        42 => "end_highlands", 43 => "end_barrens", 44 => "warm_ocean", 45 => "lukewarm_ocean", 46 => "cold_ocean",
        47 | 48 => "deep_lukewarm_ocean", 49 => "deep_cold_ocean", 50 => "deep_frozen_ocean", 127 => "the_void",
        129 => "sunflower_plains", 131 | 162 => "windswept_gravelly_hills", 132 => "flower_forest", 140 => "ice_spikes",
        155 | 156 => "old_growth_birch_forest", 160 | 161 => "old_growth_spruce_taiga", 163 | 164 => "windswept_savanna",
        165 => "eroded_badlands", 168 | 169 => "bamboo_jungle", 170 => "soul_sand_valley", 171 => "crimson_forest",
        172 => "warped_forest", 173 => "basalt_deltas", 174 => "dripstone_caves", 175 => "lush_caves",
        _ => "plains",
    }
}

/// Old entity names (before 1.11 and 1.11 – 1.12) → modern ids
fn modern_entity_id(id: &str, e: &Value) -> String {
    let s = id.trim_start_matches("minecraft:");
    let n = match s {
        "Pig" => "pig", "Cow" => "cow", "Sheep" => "sheep", "Chicken" => "chicken", "Squid" => "squid", "Wolf" => "wolf",
        "Ozelot" | "ozelot" => "ocelot", "Villager" => "villager", "VillagerGolem" | "villager_golem" => "iron_golem",
        "SnowMan" | "snowman" => "snow_golem", "Zombie" => "zombie", "PigZombie" | "zombie_pigman" => "zombified_piglin",
        "Creeper" => "creeper", "Spider" => "spider", "CaveSpider" => "cave_spider", "Enderman" => "enderman",
        "Silverfish" => "silverfish", "Blaze" => "blaze", "LavaSlime" => "magma_cube", "Slime" => "slime",
        "Ghast" => "ghast", "Witch" => "witch", "Endermite" => "endermite", "Shulker" => "shulker", "Rabbit" => "rabbit",
        "Bat" => "bat", "MushroomCow" => "mooshroom", "PolarBear" => "polar_bear", "ArmorStand" => "armor_stand",
        "ItemFrame" => "item_frame", "Painting" => "painting", "MinecartRideable" | "Minecart" => "minecart",
        "MinecartChest" => "chest_minecart", "MinecartFurnace" => "furnace_minecart", "MinecartTNT" => "tnt_minecart",
        "MinecartHopper" => "hopper_minecart", "MinecartSpawner" => "spawner_minecart",
        "MinecartCommandBlock" | "commandblock_minecart" => "command_block_minecart", "Boat" => "boat", "Item" => "item",
        "XPOrb" | "xp_orb" => "experience_orb", "LeashKnot" => "leash_knot", "Arrow" => "arrow", "PrimedTnt" => "tnt",
        "FallingSand" => "falling_block", "EnderCrystal" | "ender_crystal" => "end_crystal", "EnderDragon" => "ender_dragon",
        "WitherBoss" => "wither", "Snowball" => "snowball", "ThrownEgg" => "egg", "ThrownEnderpearl" => "ender_pearl",
        "Fireball" => "fireball", "SmallFireball" => "small_fireball", "WitherSkull" => "wither_skull",
        "evocation_illager" => "evoker", "vindication_illager" => "vindicator", "illusion_illager" => "illusioner",
        "fireworks_rocket" => "firework_rocket", "Guardian" => {
            if int(get(Some(e), "Elder")).unwrap_or(0) != 0 { "elder_guardian" } else { "guardian" }
        }
        "Skeleton" => match int(get(Some(e), "SkeletonType")).unwrap_or(0) {
            1 => "wither_skeleton",
            2 => "stray",
            _ => "skeleton",
        },
        "EntityHorse" => match int(get(Some(e), "Type")).unwrap_or(0) {
            1 => "donkey",
            2 => "mule",
            3 => "zombie_horse",
            4 => "skeleton_horse",
            _ => "horse",
        },
        other => return format!("minecraft:{}", other.to_lowercase()),
    };
    format!("minecraft:{n}")
}

/// "SkullAndRoses" → "skull_and_roses"
fn snake(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// Brings an entity from an old chunk up to the modern format the renderers understand.
pub fn normalize_entity(e: &mut Value, before_1_13: bool) {
    let id = match get(Some(e), "id") {
        Some(Value::String(s)) => s.clone(),
        _ => return,
    };
    let new_id = modern_entity_id(&id, e);
    let Value::Compound(m) = e else { return };
    m.insert("id".into(), Value::String(new_id.clone()));
    // item frames: before 1.13, Facing was a 2D direction (0 south, 1 west, 2 north, 3 east)
    if before_1_13 && new_id == "minecraft:item_frame" {
        let f = int(m.get("Facing")).or_else(|| int(m.get("Direction"))).unwrap_or(0);
        let f3 = match f & 3 {
            0 => 3,
            1 => 4,
            2 => 2,
            _ => 5,
        };
        m.insert("Facing".into(), Value::Byte(f3));
    }
    if new_id == "minecraft:painting" {
        if let Some(Value::String(mo)) = m.get("Motive").cloned() {
            m.insert("variant".into(), Value::String(format!("minecraft:{}", snake(&mo))));
        }
        if !m.contains_key("facing") {
            if let Some(f) = int(m.get("Facing")).or_else(|| int(m.get("Direction"))) {
                m.insert("facing".into(), Value::Byte(f as i8));
            }
        }
    }
    // equipment before 1.9: Equipment = [hand, feet, legs, chest, head]
    if let Some(Value::List(eq)) = m.get("Equipment").cloned() {
        if !m.contains_key("ArmorItems") && eq.len() >= 5 {
            m.insert("HandItems".into(), Value::List(vec![eq[0].clone(), Value::Compound(HashMap::new())]));
            m.insert("ArmorItems".into(), Value::List(eq[1..5].to_vec()));
        }
    }
}

fn unpack_spanning(data: &[i64], bits: usize, count: usize) -> Vec<u32> {
    let mask = (1u64 << bits) - 1;
    (0..count)
        .map(|i| {
            let start = i * bits;
            let (li, off) = (start / 64, start % 64);
            let a = *data.get(li).unwrap_or(&0) as u64;
            let v = if off + bits <= 64 {
                a >> off
            } else {
                let b = *data.get(li + 1).unwrap_or(&0) as u64;
                (a >> off) | (b << (64 - off))
            };
            (v & mask) as u32
        })
        .collect()
}

fn unpack_aligned(data: &[i64], bits: usize, count: usize) -> Vec<u32> {
    let per = 64 / bits;
    let mask = (1u64 << bits) - 1;
    (0..count).map(|i| ((*data.get(i / per).unwrap_or(&0) as u64 >> ((i % per) * bits)) & mask) as u32).collect()
}

fn bits_for(n: usize) -> usize {
    if n <= 1 { 0 } else { (usize::BITS - (n - 1).leading_zeros()) as usize }
}

fn nibble(arr: &[u8], i: usize) -> u8 {
    let b = *arr.get(i >> 1).unwrap_or(&0);
    if i & 1 == 0 { b & 15 } else { b >> 4 }
}

/// Converts numeric ids into modern states (with a small cache)
struct Converter<'a> {
    reg: &'a Registry,
    cache: HashMap<(u16, u8, u16, u8), u32>,
}

impl Converter<'_> {
    fn state(&mut self, id: u16, d: u8, below: (u16, u8), te: Option<&Value>) -> u32 {
        if id == 0 {
            return 0;
        }
        let reg = self.reg;
        let make = |te: Option<&Value>| {
            let (name, mut p) = crate::schem::legacy_state_te(id, d, below, te);
            p.sort();
            reg.state_id(&format!("minecraft:{name}"), p)
        };
        if crate::schem::legacy_needs_te(id) {
            return make(te);
        }
        *self.cache.entry((id, d, below.0, below.1)).or_insert_with(|| make(None))
    }
}

/// Parses a chunk written by any version before 1.18. Returns None for unfinished chunks.
pub fn parse(nbt: &[u8], reg: &Registry) -> Option<(Chunk, Vec<Value>)> {
    let opts = fastnbt::DeOpts::new().max_seq_len(usize::MAX);
    let root: Value = fastnbt::from_bytes_with_opts(nbt, opts).ok()?;
    let level = get(Some(&root), "Level")?;
    let dv = int(get(Some(&root), "DataVersion")).unwrap_or(0);
    if let Some(Value::String(st)) = get(Some(level), "Status") {
        let st = st.trim_start_matches("minecraft:");
        if !matches!(st, "full" | "fullchunk" | "postprocessed" | "finalized" | "mobs_spawned") {
            return None;
        }
    }
    let cx = int(get(Some(level), "xPos"))? as i32;
    let cz = int(get(Some(level), "zPos"))? as i32;

    // block entities (absolute coordinates)
    let mut tes: HashMap<(i32, i32, i32), Value> = HashMap::new();
    if let Some(Value::List(l)) = get(Some(level), "TileEntities") {
        for t in l {
            if let (Some(x), Some(y), Some(z)) = (int(get(Some(t), "x")), int(get(Some(t), "y")), int(get(Some(t), "z"))) {
                tes.insert((x as i32, y as i32, z as i32), t.clone());
            }
        }
    }

    // biomes: 1024 cells (1.15+), 256 columns as ints (1.13–1.14) or bytes (before 1.13)
    let biome_names: Vec<&'static str> = match get(Some(level), "Biomes") {
        Some(Value::IntArray(a)) => a.iter().map(|b| legacy_biome(*b as i64)).collect(),
        Some(Value::ByteArray(a)) => a.iter().map(|b| legacy_biome(*b as u8 as i64)).collect(),
        _ => Vec::new(),
    };
    let biome_ids: Vec<u16> = biome_names.iter().map(|n| reg.biome_id(&format!("minecraft:{n}"))).collect();
    let biomes_for = |sy: i32| -> Vec<u16> {
        if biome_ids.len() == 1024 {
            // 4×4×4 cells, 64 layers covering y 0..255
            (0..64)
                .map(|c| {
                    let (lx, lz, ly) = (c % 4, (c / 4) % 4, c / 16);
                    let layer = ((sy * 16) / 4 + ly).clamp(0, 63) as usize;
                    biome_ids[layer * 16 + (lz as usize) * 4 + lx as usize]
                })
                .collect()
        } else if biome_ids.len() == 256 {
            (0..64)
                .map(|c| {
                    let (lx, lz) = ((c % 4) * 4, ((c / 4) % 4) * 4);
                    biome_ids[(lz as usize) * 16 + lx as usize]
                })
                .collect()
        } else {
            vec![0]
        }
    };

    let mut conv = Converter { reg, cache: HashMap::new() };
    let mut sections: Vec<Section> = Vec::new();
    let before_1_13 = dv < 1444;

    if let Some(Value::List(secs)) = get(Some(level), "Sections") {
        for s in secs {
            let sy = int(get(Some(s), "Y")).unwrap_or(0) as i8 as i32;
            let blocks: Vec<u32> = if let Some(Value::List(pal)) = get(Some(s), "Palette") {
                // 1.13 – 1.17
                let ids: Vec<u32> = pal
                    .iter()
                    .map(|p| {
                        let name = match get(Some(p), "Name") {
                            Some(Value::String(n)) => n.clone(),
                            _ => "minecraft:air".into(),
                        };
                        let mut props: Vec<(String, String)> = match get(Some(p), "Properties") {
                            Some(Value::Compound(m)) => m
                                .iter()
                                .filter_map(|(k, v)| if let Value::String(s) = v { Some((k.clone(), s.clone())) } else { None })
                                .collect(),
                            _ => Vec::new(),
                        };
                        props.sort();
                        reg.state_id(&name, props)
                    })
                    .collect();
                let data: Vec<i64> = match get(Some(s), "BlockStates") {
                    Some(Value::LongArray(a)) => a.iter().copied().collect(),
                    _ => Vec::new(),
                };
                if ids.len() <= 1 || data.is_empty() {
                    vec![*ids.first().unwrap_or(&0)]
                } else {
                    let bits = bits_for(ids.len()).max(4);
                    // entries no longer span two longs since 20w17a (DataVersion 2529)
                    let raw = if dv >= 2529 { unpack_aligned(&data, bits, 4096) } else { unpack_spanning(&data, bits, 4096) };
                    raw.into_iter().map(|i| *ids.get(i as usize).unwrap_or(&0)).collect()
                }
            } else if let Some(bl) = bytes(get(Some(s), "Blocks")) {
                // Anvil, numeric ids
                let add = bytes(get(Some(s), "Add")).unwrap_or_default();
                let data = bytes(get(Some(s), "Data")).unwrap_or_default();
                let id_at = |i: usize| -> u16 {
                    let mut id = *bl.get(i).unwrap_or(&0) as u16;
                    if !add.is_empty() {
                        id |= (nibble(&add, i) as u16) << 8;
                    }
                    id
                };
                (0..4096)
                    .map(|i| {
                        let id = id_at(i);
                        if id == 0 {
                            return 0;
                        }
                        let d = nibble(&data, i);
                        let below = if i >= 256 { (id_at(i - 256), nibble(&data, i - 256)) } else { (0, 0) };
                        let (x, y, z) = ((i & 15) as i32, (i >> 8) as i32 + sy * 16, ((i >> 4) & 15) as i32);
                        conv.state(id, d, below, tes.get(&(cx * 16 + x, y, cz * 16 + z)))
                    })
                    .collect()
            } else {
                continue;
            };
            if blocks.iter().all(|b| *b == 0) {
                continue;
            }
            sections.push(Section { y: sy, blocks, biomes: biomes_for(sy) });
        }
    } else if let Some(bl) = bytes(get(Some(level), "Blocks")) {
        // McRegion (Beta): 16×16×128 column, index = y + z*128 + x*2048
        let data = bytes(get(Some(level), "Data")).unwrap_or_default();
        for sy in 0..8 {
            let blocks: Vec<u32> = (0..4096)
                .map(|i| {
                    let (x, ly, z) = (i & 15, i >> 8, (i >> 4) & 15);
                    let y = sy * 16 + ly;
                    let idx = y + z * 128 + x * 2048;
                    let id = *bl.get(idx).unwrap_or(&0) as u16;
                    if id == 0 {
                        return 0;
                    }
                    let d = nibble(&data, idx);
                    let below = if y > 0 { (*bl.get(idx - 1).unwrap_or(&0) as u16, nibble(&data, idx - 1)) } else { (0, 0) };
                    conv.state(id, d, below, tes.get(&(cx * 16 + x as i32, y as i32, cz * 16 + z as i32)))
                })
                .collect();
            if blocks.iter().any(|b| *b != 0) {
                sections.push(Section { y: sy as i32, blocks, biomes: biomes_for(sy as i32) });
            }
        }
    } else {
        return None;
    }
    sections.sort_by_key(|s| s.y);

    let mut entities = Vec::new();
    if let Some(Value::List(l)) = get(Some(level), "Entities") {
        for e in l {
            let mut all = Vec::new();
            crate::world::flatten_passengers(e, &mut all);
            for mut v in all {
                normalize_entity(&mut v, before_1_13);
                entities.push(v);
            }
        }
    }
    Some((Chunk { cx, cz, sections, block_entities: tes }, entities))
}

/// Entities stored inside an old chunk (before 1.17), normalized
pub fn chunk_entities(nbt: &[u8]) -> Vec<Value> {
    let opts = fastnbt::DeOpts::new().max_seq_len(usize::MAX);
    let Ok(root) = fastnbt::from_bytes_with_opts::<Value>(nbt, opts) else { return Vec::new() };
    let before_1_13 = int(get(Some(&root), "DataVersion")).unwrap_or(0) < 1444;
    let mut out = Vec::new();
    if let Some(Value::List(l)) = get(get(Some(&root), "Level"), "Entities") {
        for e in l {
            let mut all = Vec::new();
            crate::world::flatten_passengers(e, &mut all);
            for mut v in all {
                normalize_entity(&mut v, before_1_13);
                out.push(v);
            }
        }
    }
    out
}
