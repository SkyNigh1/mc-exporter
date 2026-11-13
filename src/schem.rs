//! Import de schematics dans un monde « vide » en mémoire :
//! Sponge v1/v2/v3 (.schem), MCEdit/WorldEdit ancien (.schematic), Litematica (.litematic),
//! structures vanilla (.nbt).

use crate::registry::Registry;
use crate::world::{MemWorld, build_chunks};
use anyhow::{Context, Result, anyhow, bail};
use fastnbt::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

pub struct Schematic {
    pub name: String,
    pub format: String,
    pub size: [i32; 3],
    pub blocks: usize,
    pub world: MemWorld,
    pub block_entities: usize,
    pub entities: usize,
}

#[derive(Default)]
pub struct Extra {
    bes: Vec<((i32, i32, i32), Value)>,
    ents: Vec<Value>,
}

fn compound_of(v: &Value) -> HashMap<String, Value> {
    match v {
        Value::Compound(m) => m.clone(),
        _ => HashMap::new(),
    }
}

fn dvals(v: Option<&Value>) -> Option<[f64; 3]> {
    let f = |x: &Value| match x {
        Value::Double(d) => Some(*d),
        Value::Float(d) => Some(*d as f64),
        Value::Int(i) => Some(*i as f64),
        _ => None,
    };
    match v? {
        Value::List(l) if l.len() >= 3 => Some([f(&l[0])?, f(&l[1])?, f(&l[2])?]),
        Value::IntArray(a) if a.len() >= 3 => Some([a[0] as f64, a[1] as f64, a[2] as f64]),
        _ => None,
    }
}

/// Bloc spécial au format Sponge (v1/v2 : données à plat, v3 : dans Data)
fn sponge_be(v: &Value) -> Option<((i32, i32, i32), Value)> {
    let p = dvals(get(v, "Pos"))?;
    let id = match get(v, "Id").or_else(|| get(v, "id")) {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    let mut m = match get(v, "Data") {
        Some(d @ Value::Compound(_)) => compound_of(d),
        _ => compound_of(v),
    };
    let (x, y, z) = (p[0] as i32, p[1] as i32, p[2] as i32);
    m.insert("id".into(), Value::String(id));
    m.insert("x".into(), Value::Int(x));
    m.insert("y".into(), Value::Int(y));
    m.insert("z".into(), Value::Int(z));
    Some(((x, y, z), Value::Compound(m)))
}

/// Entité : données (à plat ou dans Data) avec Pos dans le repère du schematic
fn make_entity(v: &Value, pos: [f64; 3]) -> Value {
    let mut m = match get(v, "Data") {
        Some(d @ Value::Compound(_)) => compound_of(d),
        _ => compound_of(v),
    };
    if let Some(Value::String(id)) = get(v, "Id") {
        m.insert("id".into(), Value::String(id.clone()));
    }
    m.insert("Pos".into(), Value::List(pos.iter().map(|d| Value::Double(*d)).collect()));
    Value::Compound(m)
}

pub const EXTENSIONS: &[&str] = &["schem", "schematic", "litematic", "nbt"];

fn get<'a>(v: &'a Value, k: &str) -> Option<&'a Value> {
    match v {
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

fn longs(v: Option<&Value>) -> Option<Vec<i64>> {
    match v? {
        Value::LongArray(a) => Some(a.iter().copied().collect()),
        _ => None,
    }
}

fn ints(v: Option<&Value>) -> Option<Vec<i32>> {
    match v? {
        Value::IntArray(a) => Some(a.iter().copied().collect()),
        Value::List(l) => l.iter().map(|x| int(Some(x)).map(|i| i as i32)).collect(),
        _ => None,
    }
}

/// "minecraft:oak_stairs[facing=east,half=bottom]" → (nom, propriétés)
pub fn parse_state_string(s: &str) -> (String, Vec<(String, String)>) {
    let (name, rest) = match s.find('[') {
        Some(i) => (&s[..i], s[i + 1..].trim_end_matches(']')),
        None => (s, ""),
    };
    let mut props: Vec<(String, String)> = rest
        .split(',')
        .filter_map(|kv| kv.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect();
    props.sort();
    (name.trim().to_string(), props)
}

fn state_from_compound(v: &Value, reg: &Registry) -> u32 {
    let name = match get(v, "Name") {
        Some(Value::String(s)) => s.clone(),
        _ => return 0,
    };
    let mut props: Vec<(String, String)> = match get(v, "Properties") {
        Some(Value::Compound(m)) => m
            .iter()
            .filter_map(|(k, v)| match v {
                Value::String(s) => Some((k.clone(), s.clone())),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    props.sort();
    reg.state_id(&name, props)
}

fn read_varints(data: &[u8], count: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(count);
    let mut i = 0;
    while i < data.len() && out.len() < count {
        let mut value: u32 = 0;
        let mut shift = 0;
        loop {
            let b = data[i];
            i += 1;
            value |= ((b & 0x7f) as u32) << shift;
            shift += 7;
            if b & 0x80 == 0 || i >= data.len() || shift > 28 {
                break;
            }
        }
        out.push(value);
    }
    out
}

pub fn load(path: &Path, reg: &Registry) -> Result<Schematic> {
    let raw = std::fs::read(path).with_context(|| format!("lecture de {}", path.display()))?;
    let data = if raw.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&raw[..]).read_to_end(&mut out).context("décompression gzip")?;
        out
    } else {
        raw
    };
    let opts = fastnbt::DeOpts::new().max_seq_len(usize::MAX);
    let root: Value = fastnbt::from_bytes_with_opts(&data, opts).context("fichier NBT illisible")?;
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

    let mut ex = Extra::default();
    let (format, blocks): (String, Vec<(i32, i32, i32, u32)>) = if get(&root, "Regions").is_some() {
        litematic(&root, reg, &mut ex)?
    } else if get(&root, "blocks").is_some() && (get(&root, "palette").is_some() || get(&root, "palettes").is_some()) {
        structure(&root, reg, &mut ex)?
    } else {
        let inner = match get(&root, "Schematic") {
            Some(s @ Value::Compound(_)) => s,
            _ => &root,
        };
        if get(inner, "Materials").is_some() || matches!(get(inner, "Blocks"), Some(Value::ByteArray(_))) {
            legacy(inner, reg, &mut ex)?
        } else {
            sponge(inner, reg, &mut ex)?
        }
    };
    if blocks.is_empty() {
        bail!("le schematic ne contient aucun bloc");
    }
    // ramène le coin minimum en (0, 0, 0)
    let min = blocks.iter().fold([i32::MAX; 3], |m, b| [m[0].min(b.0), m[1].min(b.1), m[2].min(b.2)]);
    let max = blocks.iter().fold([i32::MIN; 3], |m, b| [m[0].max(b.0), m[1].max(b.1), m[2].max(b.2)]);
    let mut size = [max[0] - min[0] + 1, max[1] - min[1] + 1, max[2] - min[2] + 1];
    let count = blocks.len();
    let bes: HashMap<(i32, i32, i32), Value> = ex
        .bes
        .into_iter()
        .map(|((x, y, z), mut v)| {
            let k = (x - min[0], y - min[1], z - min[2]);
            if let Value::Compound(m) = &mut v {
                m.insert("x".into(), Value::Int(k.0));
                m.insert("y".into(), Value::Int(k.1));
                m.insert("z".into(), Value::Int(k.2));
            }
            (k, v)
        })
        .collect();
    let n_be = bes.len();
    let mut entities = Vec::new();
    for e in ex.ents {
        let mut all = Vec::new();
        crate::world::flatten_passengers(&e, &mut all);
        for mut v in all {
            if let Some(p) = dvals(get(&v, "Pos")) {
                let np = [p[0] - min[0] as f64, p[1] - min[1] as f64, p[2] - min[2] as f64];
                if let Value::Compound(m) = &mut v {
                    m.insert("Pos".into(), Value::List(np.iter().map(|d| Value::Double(*d)).collect()));
                    m.remove("Passengers");
                }
                entities.push(v);
            }
        }
    }
    // la sélection englobe aussi les entités (créatures volantes, displays au-dessus du build…)
    for e in &entities {
        if let Some(p) = dvals(get(e, "Pos")) {
            for i in 0..3 {
                size[i] = size[i].max(p[i].ceil() as i32 + 1);
            }
        }
    }
    let n_ent = entities.len();
    let chunks = build_chunks(blocks.into_iter().map(|(x, y, z, id)| (x - min[0], y - min[1], z - min[2], id)), bes);
    Ok(Schematic { name, format, size, blocks: count, world: MemWorld { chunks, entities }, block_entities: n_be, entities: n_ent })
}

fn sponge(s: &Value, reg: &Registry, ex: &mut Extra) -> Result<(String, Vec<(i32, i32, i32, u32)>)> {
    let version = int(get(s, "Version")).unwrap_or(1);
    let w = int(get(s, "Width")).ok_or_else(|| anyhow!("Width manquant : format non reconnu"))? as i32 & 0xffff;
    let h = int(get(s, "Height")).unwrap_or(0) as i32 & 0xffff;
    let l = int(get(s, "Length")).unwrap_or(0) as i32 & 0xffff;
    let (palette, data) = match get(s, "Blocks") {
        Some(b @ Value::Compound(_)) => (get(b, "Palette"), bytes(get(b, "Data"))),
        _ => (get(s, "Palette"), bytes(get(s, "BlockData"))),
    };
    let Some(Value::Compound(pal)) = palette else { bail!("palette de blocs absente") };
    let mut ids = vec![0u32; pal.len().max(1)];
    let mut map: HashMap<u32, u32> = HashMap::new();
    for (k, v) in pal {
        let (n, p) = parse_state_string(k);
        let gid = reg.state_id(&n, p);
        if let Some(i) = int(Some(v)) {
            map.insert(i as u32, gid);
            if (i as usize) < ids.len() {
                ids[i as usize] = gid;
            }
        }
    }
    let data = data.ok_or_else(|| anyhow!("données de blocs absentes"))?;
    let n = (w * h * l) as usize;
    let vals = read_varints(&data, n);
    let mut out = Vec::new();
    for (i, v) in vals.iter().enumerate() {
        let gid = *map.get(v).unwrap_or(&0);
        if gid == 0 {
            continue;
        }
        let i = i as i32;
        let x = i % w;
        let z = (i / w) % l;
        let y = i / (w * l);
        out.push((x, y, z, gid));
    }
    let be_list = match get(s, "Blocks") {
        Some(b @ Value::Compound(_)) => get(b, "BlockEntities"),
        _ => get(s, "BlockEntities").or_else(|| get(s, "TileEntities")),
    };
    if let Some(Value::List(l)) = be_list {
        ex.bes.extend(l.iter().filter_map(sponge_be));
    }
    if let Some(Value::List(l)) = get(s, "Entities") {
        for v in l {
            if let Some(p) = dvals(get(v, "Pos")) {
                ex.ents.push(make_entity(v, p));
            }
        }
    }
    Ok((format!("Sponge v{version}"), out))
}

fn litematic(root: &Value, reg: &Registry, ex: &mut Extra) -> Result<(String, Vec<(i32, i32, i32, u32)>)> {
    let Some(Value::Compound(regions)) = get(root, "Regions") else { bail!("Regions invalide") };
    let mut out = Vec::new();
    for (_, r) in regions {
        let pos = get(r, "Position");
        let size = get(r, "Size");
        let p = |v: Option<&Value>, k: &str| int(v.and_then(|v| get(v, k))).unwrap_or(0) as i32;
        let (px, py, pz) = (p(pos, "x"), p(pos, "y"), p(pos, "z"));
        let (sx, sy, sz) = (p(size, "x"), p(size, "y"), p(size, "z"));
        let (ax, ay, az) = (sx.abs(), sy.abs(), sz.abs());
        if ax == 0 || ay == 0 || az == 0 {
            continue;
        }
        let ox = px + if sx < 0 { sx + 1 } else { 0 };
        let oy = py + if sy < 0 { sy + 1 } else { 0 };
        let oz = pz + if sz < 0 { sz + 1 } else { 0 };
        if let Some(Value::List(l)) = get(r, "TileEntities") {
            for v in l {
                let g = |k: &str| int(get(v, k)).unwrap_or(0) as i32;
                let (x, y, z) = (ox + g("x"), oy + g("y"), oz + g("z"));
                let mut m = compound_of(v);
                m.insert("x".into(), Value::Int(x));
                m.insert("y".into(), Value::Int(y));
                m.insert("z".into(), Value::Int(z));
                ex.bes.push(((x, y, z), Value::Compound(m)));
            }
        }
        if let Some(Value::List(l)) = get(r, "Entities") {
            for v in l {
                if let Some(p) = dvals(get(v, "Pos")) {
                    ex.ents.push(make_entity(v, [p[0] + ox as f64, p[1] + oy as f64, p[2] + oz as f64]));
                }
            }
        }
        let Some(Value::List(pal)) = get(r, "BlockStatePalette") else { continue };
        let ids: Vec<u32> = pal.iter().map(|v| state_from_compound(v, reg)).collect();
        let arr = longs(get(r, "BlockStates")).unwrap_or_default();
        let bits = (usize::BITS - (ids.len().max(2) - 1).leading_zeros()).max(2) as u64;
        let mask = (1u64 << bits) - 1;
        let vol = (ax as u64) * (ay as u64) * (az as u64);
        for idx in 0..vol {
            let start = idx * bits;
            let sl = (start / 64) as usize;
            let el = ((start + bits - 1) / 64) as usize;
            let off = start % 64;
            let a = *arr.get(sl).unwrap_or(&0) as u64;
            let v = if sl == el {
                (a >> off) & mask
            } else {
                let b = *arr.get(el).unwrap_or(&0) as u64;
                ((a >> off) | (b << (64 - off))) & mask
            };
            let gid = *ids.get(v as usize).unwrap_or(&0);
            if gid == 0 {
                continue;
            }
            let idx = idx as i64;
            let x = (idx % ax as i64) as i32;
            let z = ((idx / ax as i64) % az as i64) as i32;
            let y = (idx / (ax as i64 * az as i64)) as i32;
            out.push((ox + x, oy + y, oz + z, gid));
        }
    }
    let v = int(get(root, "Version")).unwrap_or(0);
    Ok((format!("Litematica v{v}"), out))
}

fn structure(root: &Value, reg: &Registry, ex: &mut Extra) -> Result<(String, Vec<(i32, i32, i32, u32)>)> {
    let pal = match get(root, "palette") {
        Some(Value::List(l)) => l.clone(),
        _ => match get(root, "palettes") {
            Some(Value::List(ps)) => match ps.first() {
                Some(Value::List(l)) => l.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        },
    };
    let ids: Vec<u32> = pal.iter().map(|v| state_from_compound(v, reg)).collect();
    let Some(Value::List(blocks)) = get(root, "blocks") else { bail!("liste de blocs absente") };
    let mut out = Vec::new();
    for b in blocks {
        let Some(p) = ints(get(b, "pos")) else { continue };
        let s = int(get(b, "state")).unwrap_or(0) as usize;
        let gid = *ids.get(s).unwrap_or(&0);
        if gid != 0 && p.len() >= 3 {
            out.push((p[0], p[1], p[2], gid));
            if let Some(n @ Value::Compound(_)) = get(b, "nbt") {
                let mut m = compound_of(n);
                m.insert("x".into(), Value::Int(p[0]));
                m.insert("y".into(), Value::Int(p[1]));
                m.insert("z".into(), Value::Int(p[2]));
                ex.bes.push(((p[0], p[1], p[2]), Value::Compound(m)));
            }
        }
    }
    if let Some(Value::List(l)) = get(root, "entities") {
        for v in l {
            if let (Some(p), Some(n)) = (dvals(get(v, "pos")), get(v, "nbt")) {
                ex.ents.push(make_entity(n, p));
            }
        }
    }
    Ok(("Structure vanilla (.nbt)".into(), out))
}

// ───────────────────────── ancien format MCEdit (identifiants numériques) ─────────────────────────

const COLORS: [&str; 16] = [
    "white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan", "purple", "blue",
    "brown", "green", "red", "black",
];
const WOODS: [&str; 6] = ["oak", "spruce", "birch", "jungle", "acacia", "dark_oak"];

fn base_name(id: u16) -> &'static str {
    match id {
        1 => "stone", 2 => "grass_block", 3 => "dirt", 4 => "cobblestone", 7 => "bedrock", 8 | 9 => "water", 10 | 11 => "lava",
        12 => "sand", 13 => "gravel", 14 => "gold_ore", 15 => "iron_ore", 16 => "coal_ore", 19 => "sponge", 20 => "glass",
        21 => "lapis_ore", 22 => "lapis_block", 23 => "dispenser", 24 => "sandstone", 25 => "note_block", 26 => "red_bed",
        27 => "powered_rail", 28 => "detector_rail", 29 => "sticky_piston", 30 => "cobweb", 32 => "dead_bush", 33 => "piston",
        37 => "dandelion", 39 => "brown_mushroom", 40 => "red_mushroom", 41 => "gold_block", 42 => "iron_block", 45 => "bricks",
        46 => "tnt", 47 => "bookshelf", 48 => "mossy_cobblestone", 49 => "obsidian", 51 => "fire", 52 => "spawner",
        54 => "chest", 55 => "redstone_wire", 56 => "diamond_ore", 57 => "diamond_block", 58 => "crafting_table",
        59 => "wheat", 60 => "farmland", 61 | 62 => "furnace", 63 => "oak_sign", 64 => "oak_door", 65 => "ladder", 66 => "rail",
        68 => "oak_wall_sign", 69 => "lever", 70 => "stone_pressure_plate", 71 => "iron_door", 72 => "oak_pressure_plate",
        73 | 74 => "redstone_ore", 77 => "stone_button", 78 => "snow", 79 => "ice", 80 => "snow_block", 81 => "cactus",
        82 => "clay", 83 => "sugar_cane", 84 => "jukebox", 85 => "oak_fence", 86 => "carved_pumpkin", 87 => "netherrack",
        88 => "soul_sand", 89 => "glowstone", 90 => "nether_portal", 91 => "jack_o_lantern", 92 => "cake", 93 | 94 => "repeater",
        96 => "oak_trapdoor", 97 => "infested_stone", 99 => "brown_mushroom_block", 100 => "red_mushroom_block",
        101 => "iron_bars", 102 => "glass_pane", 103 => "melon", 104 => "pumpkin_stem", 105 => "melon_stem", 106 => "vine",
        107 => "oak_fence_gate", 110 => "mycelium", 111 => "lily_pad", 112 => "nether_bricks", 113 => "nether_brick_fence",
        115 => "nether_wart", 116 => "enchanting_table", 117 => "brewing_stand", 118 => "cauldron", 119 => "end_portal",
        120 => "end_portal_frame", 121 => "end_stone", 122 => "dragon_egg", 123 | 124 => "redstone_lamp", 127 => "cocoa",
        129 => "emerald_ore", 130 => "ender_chest", 131 => "tripwire_hook", 132 => "tripwire", 133 => "emerald_block",
        137 => "command_block", 138 => "beacon", 139 => "cobblestone_wall", 140 => "flower_pot", 141 => "carrots",
        142 => "potatoes", 143 => "oak_button", 144 => "skeleton_skull", 145 => "anvil", 146 => "trapped_chest",
        147 => "light_weighted_pressure_plate", 148 => "heavy_weighted_pressure_plate", 149 | 150 => "comparator",
        151 | 178 => "daylight_detector", 152 => "redstone_block", 153 => "nether_quartz_ore", 154 => "hopper",
        157 => "activator_rail", 158 => "dropper", 165 => "slime_block", 166 => "barrier", 167 => "iron_trapdoor",
        169 => "sea_lantern", 170 => "hay_block", 172 => "terracotta", 173 => "coal_block", 174 => "packed_ice",
        176 => "white_banner", 177 => "white_wall_banner", 183 => "spruce_fence_gate", 184 => "birch_fence_gate",
        185 => "jungle_fence_gate", 186 => "dark_oak_fence_gate", 187 => "acacia_fence_gate", 188 => "spruce_fence",
        189 => "birch_fence", 190 => "jungle_fence", 191 => "dark_oak_fence", 192 => "acacia_fence", 193 => "spruce_door",
        194 => "birch_door", 195 => "jungle_door", 196 => "acacia_door", 197 => "dark_oak_door", 198 => "end_rod",
        199 => "chorus_plant", 200 => "chorus_flower", 201 => "purpur_block", 202 => "purpur_pillar", 206 => "end_stone_bricks",
        207 => "beetroots", 208 => "dirt_path", 209 => "end_gateway", 210 => "repeating_command_block",
        211 => "chain_command_block", 212 => "frosted_ice", 213 => "magma_block", 214 => "nether_wart_block",
        215 => "red_nether_bricks", 216 => "bone_block", 217 => "structure_void", 218 => "observer", 255 => "structure_block",
        _ => "",
    }
}

fn stairs_name(id: u16) -> Option<&'static str> {
    Some(match id {
        53 => "oak_stairs", 67 => "cobblestone_stairs", 108 => "brick_stairs", 109 => "stone_brick_stairs",
        114 => "nether_brick_stairs", 128 => "sandstone_stairs", 134 => "spruce_stairs", 135 => "birch_stairs",
        136 => "jungle_stairs", 156 => "quartz_stairs", 163 => "acacia_stairs", 164 => "dark_oak_stairs",
        180 => "red_sandstone_stairs", 203 => "purpur_stairs",
        _ => return None,
    })
}

type Props = Vec<(&'static str, String)>;

/// Convertit (id, data) en état moderne. `below` donne (id, data) du bloc du dessous (portes, plantes hautes).
fn legacy_state(id: u16, d: u8, below: (u16, u8)) -> (String, Props) {
    let s = |n: &str| n.to_string();
    let axis = |d: u8| match (d >> 2) & 3 {
        1 => "x",
        2 => "z",
        _ => "y",
    };
    let h4 = |d: u8| ["north", "south", "west", "east"][((d.saturating_sub(2)) & 3) as usize];
    if let Some(n) = stairs_name(id) {
        let facing = ["east", "west", "south", "north"][(d & 3) as usize];
        let half = if d & 4 != 0 { "top" } else { "bottom" };
        return (s(n), vec![("facing", s(facing)), ("half", s(half)), ("shape", s("straight"))]);
    }
    match id {
        0 | 36 => (s("air"), vec![]),
        1 => (s(["stone", "granite", "polished_granite", "diorite", "polished_diorite", "andesite", "polished_andesite"][(d as usize).min(6)]), vec![]),
        3 => (s(["dirt", "coarse_dirt", "podzol"][(d as usize).min(2)]), vec![]),
        5 => (format!("{}_planks", WOODS[(d as usize % 6)]), vec![]),
        6 => (format!("{}_sapling", WOODS[(d as usize & 7) % 6]), vec![]),
        12 => (s(if d == 1 { "red_sand" } else { "sand" }), vec![]),
        17 | 162 => {
            let w = if id == 17 { WOODS[(d & 3) as usize] } else { WOODS[4 + (d & 1) as usize] };
            if (d >> 2) & 3 == 3 {
                (format!("{w}_wood"), vec![("axis", s("y"))])
            } else {
                (format!("{w}_log"), vec![("axis", s(axis(d)))])
            }
        }
        18 | 161 => {
            let w = if id == 18 { WOODS[(d & 3) as usize] } else { WOODS[4 + (d & 1) as usize] };
            (format!("{w}_leaves"), vec![("persistent", s("true"))])
        }
        19 => (s(if d == 1 { "wet_sponge" } else { "sponge" }), vec![]),
        24 | 179 => {
            let p = if id == 24 { "" } else { "red_" };
            (format!("{}{p}sandstone", ["", "chiseled_", "cut_"][(d as usize).min(2)]), vec![])
        }
        31 => (s(["dead_bush", "short_grass", "fern"][(d as usize).min(2)]), vec![]),
        35 => (format!("{}_wool", COLORS[(d & 15) as usize]), vec![]),
        159 => (format!("{}_terracotta", COLORS[(d & 15) as usize]), vec![]),
        95 => (format!("{}_stained_glass", COLORS[(d & 15) as usize]), vec![]),
        160 => (format!("{}_stained_glass_pane", COLORS[(d & 15) as usize]), vec![]),
        171 => (format!("{}_carpet", COLORS[(d & 15) as usize]), vec![]),
        251 => (format!("{}_concrete", COLORS[(d & 15) as usize]), vec![]),
        252 => (format!("{}_concrete_powder", COLORS[(d & 15) as usize]), vec![]),
        219..=234 => (format!("{}_shulker_box", COLORS[(id - 219) as usize]), vec![]),
        235..=250 => (
            format!("{}_glazed_terracotta", COLORS[(id - 235) as usize]),
            vec![("facing", s(["south", "west", "north", "east"][(d & 3) as usize]))],
        ),
        38 => (
            s(["poppy", "blue_orchid", "allium", "azure_bluet", "red_tulip", "orange_tulip", "white_tulip", "pink_tulip", "oxeye_daisy"]
                [(d as usize).min(8)]),
            vec![],
        ),
        43 | 44 | 181 | 182 | 125 | 126 | 204 | 205 => {
            let n = match id {
                43 | 44 => ["smooth_stone", "sandstone", "oak", "cobblestone", "brick", "stone_brick", "nether_brick", "quartz"][(d & 7) as usize].to_string(),
                125 | 126 => WOODS[((d & 7) as usize) % 6].to_string(),
                181 | 182 => "red_sandstone".to_string(),
                _ => "purpur".to_string(),
            };
            let double = matches!(id, 43 | 125 | 181 | 204);
            let t = if double { "double" } else if d & 8 != 0 { "top" } else { "bottom" };
            (format!("{n}_slab"), vec![("type", s(t))])
        }
        98 => (s(["stone_bricks", "mossy_stone_bricks", "cracked_stone_bricks", "chiseled_stone_bricks"][(d as usize).min(3)]), vec![]),
        155 => match d {
            1 => (s("chiseled_quartz_block"), vec![]),
            2 => (s("quartz_pillar"), vec![("axis", s("y"))]),
            3 => (s("quartz_pillar"), vec![("axis", s("x"))]),
            4 => (s("quartz_pillar"), vec![("axis", s("z"))]),
            _ => (s("quartz_block"), vec![]),
        },
        168 => (s(["prismarine", "prismarine_bricks", "dark_prismarine"][(d as usize).min(2)]), vec![]),
        175 => {
            const P: [&str; 6] = ["sunflower", "lilac", "tall_grass", "large_fern", "rose_bush", "peony"];
            if d & 8 != 0 {
                let lower = if below.0 == 175 { below.1 & 7 } else { 2 };
                (s(P[(lower as usize).min(5)]), vec![("half", s("upper"))])
            } else {
                (s(P[((d & 7) as usize).min(5)]), vec![("half", s("lower"))])
            }
        }
        50 | 75 | 76 => {
            let (floor, wall) = if id == 50 { ("torch", "wall_torch") } else { ("redstone_torch", "redstone_wall_torch") };
            let mut p: Props = vec![];
            if id != 50 {
                p.push(("lit", s(if id == 76 { "true" } else { "false" })));
            }
            match d {
                1..=4 => {
                    p.push(("facing", s(["east", "west", "south", "north"][(d - 1) as usize])));
                    (s(wall), p)
                }
                _ => (s(floor), p),
            }
        }
        64 | 71 | 193..=197 => {
            let name = base_name(id);
            let (lower_d, upper) = if d & 8 != 0 { (if below.0 == id { below.1 } else { 0 }, true) } else { (d, false) };
            let facing = ["east", "south", "west", "north"][(lower_d & 3) as usize];
            let open = if lower_d & 4 != 0 { "true" } else { "false" };
            let hinge = if upper && d & 1 != 0 { "right" } else { "left" };
            (
                s(name),
                vec![("facing", s(facing)), ("half", s(if upper { "upper" } else { "lower" })), ("hinge", s(hinge)), ("open", s(open))],
            )
        }
        59 | 141 | 142 => (s(base_name(id)), vec![("age", (d & 7).to_string())]),
        207 => (s("beetroots"), vec![("age", (d & 3).to_string())]),
        78 => (s("snow"), vec![("layers", ((d & 7) + 1).to_string())]),
        86 | 91 => (s(base_name(id)), vec![("facing", s(["south", "west", "north", "east"][(d & 3) as usize]))]),
        170 | 202 | 216 => (s(base_name(id)), vec![("axis", s(axis(d)))]),
        23 | 54 | 61 | 62 | 65 | 68 | 130 | 146 | 158 => {
            let mut p: Props = vec![("facing", s(h4(d)))];
            if id == 61 || id == 62 {
                p.push(("lit", s(if id == 62 { "true" } else { "false" })));
            }
            (s(base_name(id)), p)
        }
        106 => (
            s("vine"),
            vec![
                ("south", (d & 1 != 0).to_string()),
                ("west", (d & 2 != 0).to_string()),
                ("north", (d & 4 != 0).to_string()),
                ("east", (d & 8 != 0).to_string()),
                ("up", s("false")),
            ],
        ),
        8 | 9 | 10 | 11 => (s(base_name(id)), vec![("level", (d & 15).to_string())]),
        123 | 124 => (s("redstone_lamp"), vec![("lit", s(if id == 124 { "true" } else { "false" }))]),
        _ => {
            let n = base_name(id);
            if n.is_empty() { (format!("legacy_{id}"), vec![]) } else { (s(n), vec![]) }
        }
    }
}

fn legacy(s: &Value, reg: &Registry, ex: &mut Extra) -> Result<(String, Vec<(i32, i32, i32, u32)>)> {
    let w = int(get(s, "Width")).unwrap_or(0) as i32 & 0xffff;
    let h = int(get(s, "Height")).unwrap_or(0) as i32 & 0xffff;
    let l = int(get(s, "Length")).unwrap_or(0) as i32 & 0xffff;
    let blocks = bytes(get(s, "Blocks")).ok_or_else(|| anyhow!("Blocks absent"))?;
    let data = bytes(get(s, "Data")).unwrap_or_default();
    let add = bytes(get(s, "AddBlocks")).unwrap_or_default();
    let n = (w * h * l) as usize;
    let id_at = |i: usize| -> u16 {
        let mut id = *blocks.get(i).unwrap_or(&0) as u16;
        if !add.is_empty() {
            let a = *add.get(i >> 1).unwrap_or(&0);
            let hi = if i & 1 == 0 { a >> 4 } else { a & 15 };
            id |= (hi as u16) << 8;
        }
        id
    };
    let mut cache: HashMap<(u16, u8, u16, u8), u32> = HashMap::new();
    let mut out = Vec::new();
    for i in 0..n.min(blocks.len()) {
        let id = id_at(i);
        if id == 0 {
            continue;
        }
        let d = *data.get(i).unwrap_or(&0) & 15;
        let ii = i as i32;
        let x = ii % w;
        let z = (ii / w) % l;
        let y = ii / (w * l);
        let below = if y > 0 {
            let j = i - (w * l) as usize;
            (id_at(j), *data.get(j).unwrap_or(&0) & 15)
        } else {
            (0, 0)
        };
        let key = (id, d, below.0, below.1);
        let gid = *cache.entry(key).or_insert_with(|| {
            let (name, props) = legacy_state(id, d, below);
            let mut p: Vec<(String, String)> = props.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
            p.sort();
            reg.state_id(&format!("minecraft:{name}"), p)
        });
        if gid != 0 {
            out.push((x, y, z, gid));
        }
    }
    if let Some(Value::List(l)) = get(s, "TileEntities") {
        for v in l {
            let g = |k: &str| int(get(v, k)).unwrap_or(0) as i32;
            ex.bes.push(((g("x"), g("y"), g("z")), v.clone()));
        }
    }
    Ok(("MCEdit / WorldEdit ancien (.schematic)".into(), out))
}
