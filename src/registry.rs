//! Registry of block states, biomes and textures, and resolution of block models
//! (the game's blockstates + model JSON) into quads.

use crate::assets::{Assets, BiomeColors};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Dir {
    Down = 0,
    Up = 1,
    North = 2,
    South = 3,
    West = 4,
    East = 5,
}

pub const DIRS: [Dir; 6] = [Dir::Down, Dir::Up, Dir::North, Dir::South, Dir::West, Dir::East];

impl Dir {
    pub fn vec(self) -> [i32; 3] {
        match self {
            Dir::Down => [0, -1, 0],
            Dir::Up => [0, 1, 0],
            Dir::North => [0, 0, -1],
            Dir::South => [0, 0, 1],
            Dir::West => [-1, 0, 0],
            Dir::East => [1, 0, 0],
        }
    }
    pub fn opposite(self) -> Dir {
        match self {
            Dir::Down => Dir::Up,
            Dir::Up => Dir::Down,
            Dir::North => Dir::South,
            Dir::South => Dir::North,
            Dir::West => Dir::East,
            Dir::East => Dir::West,
        }
    }
    pub fn from_name(s: &str) -> Option<Dir> {
        Some(match s {
            "down" | "bottom" => Dir::Down,
            "up" | "top" => Dir::Up,
            "north" => Dir::North,
            "south" => Dir::South,
            "west" => Dir::West,
            "east" => Dir::East,
            _ => return None,
        })
    }
    pub fn from_vec(v: [f32; 3]) -> Option<Dir> {
        let r = [v[0].round() as i32, v[1].round() as i32, v[2].round() as i32];
        DIRS.iter().copied().find(|d| d.vec() == r)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tint {
    None,
    Grass,
    Foliage,
    DryFoliage,
    Water,
    Fixed([u8; 3]),
}

impl Tint {
    pub fn color(self, b: &BiomeColors) -> Option<[u8; 3]> {
        match self {
            Tint::None => None,
            Tint::Grass => Some(b.grass),
            Tint::Foliage => Some(b.foliage),
            Tint::DryFoliage => Some(b.dry_foliage),
            Tint::Water => Some(b.water),
            Tint::Fixed(c) => Some(c),
        }
    }
}

/// Block quad, local coordinates in [0,1]³. Vertices in counter-clockwise order seen from outside.
#[derive(Clone, Debug)]
pub struct Quad {
    pub pos: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4], // OBJ convention (v points up)
    pub tex: u32,
    pub tint: Tint,
    pub cull: Option<Dir>,
    pub dir: Option<Dir>,
    pub flat: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fluid {
    None,
    Water(u8),
    Lava(u8),
}

pub struct BlockGeom {
    pub name: String,
    /// For each part, weighted variants (weight, quads)
    pub parts: Vec<Vec<(u32, Vec<Quad>)>>,
    pub occludes: [bool; 6],
    pub map_tex: Option<u32>,
    pub map_tint: Tint,
    pub fluid: Fluid,
    pub waterlogged: bool,
    pub unknown: bool,
    pub same_cull: bool,
    pub air: bool,
    /// special rendering (chest, banner, sign…)
    pub be: bool,
    /// the jar provides geometry for this block
    pub has_model: bool,
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct StateKey {
    pub name: String,
    pub props: Vec<(String, String)>,
}

impl StateKey {
    pub fn prop(&self, k: &str) -> Option<&str> {
        self.props.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
}

struct Interner<K: std::hash::Hash + Eq + Clone> {
    map: HashMap<K, u32>,
    list: Vec<K>,
}

impl<K: std::hash::Hash + Eq + Clone> Interner<K> {
    fn new() -> Self {
        Interner { map: HashMap::new(), list: Vec::new() }
    }
}

struct ResolvedModel {
    textures: HashMap<String, String>,
    elements: Vec<Value>,
    display: serde_json::Map<String, Value>,
    generated: bool,
}

pub const UNKNOWN_TEX: &str = "mcexport:unknown";

pub struct Registry {
    pub assets: Arc<Assets>,
    states: RwLock<Interner<StateKey>>,
    biomes: RwLock<Interner<String>>,
    textures: RwLock<Interner<String>>,
    geoms: RwLock<Vec<Option<Arc<BlockGeom>>>>,
    biome_colors: RwLock<Vec<Option<BiomeColors>>>,
    models: RwLock<HashMap<String, Option<Arc<ResolvedModel>>>>,
    font: std::sync::OnceLock<crate::font::Font>,
}

fn is_air(name: &str) -> bool {
    matches!(name, "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air" | "minecraft:__reserved__" | "__reserved__")
}

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn tint_for(name: &str, key: &StateKey) -> Tint {
    let n = short(name);
    match n {
        "grass_block" | "short_grass" | "grass" | "tall_grass" | "fern" | "large_fern" | "potted_fern"
        | "sugar_cane" | "bush" | "pink_petals" | "wildflowers" => Tint::Grass,
        "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves" | "mangrove_leaves" | "vine" => {
            Tint::Foliage
        }
        "leaf_litter" => Tint::DryFoliage,
        "birch_leaves" => Tint::Fixed([0x80, 0xa7, 0x55]),
        "spruce_leaves" => Tint::Fixed([0x61, 0x99, 0x61]),
        "lily_pad" => Tint::Fixed([0x20, 0x80, 0x30]),
        "water" | "water_cauldron" | "bubble_column" => Tint::Water,
        "attached_melon_stem" | "attached_pumpkin_stem" => Tint::Fixed([0xe0, 0xc7, 0x1c]),
        "melon_stem" | "pumpkin_stem" => {
            let age: f32 = key.prop("age").and_then(|a| a.parse().ok()).unwrap_or(0.0);
            Tint::Fixed([(age * 32.0) as u8, (255.0 - age * 8.0) as u8, (age * 4.0) as u8])
        }
        "redstone_wire" => {
            let p: f32 = key.prop("power").and_then(|a| a.parse().ok()).unwrap_or(0.0) / 15.0;
            let r = p * 0.6 + if p > 0.0 { 0.4 } else { 0.3 };
            let g = (p * p * 0.7 - 0.5).clamp(0.0, 1.0);
            let b = (p * p * 0.6 - 0.7).clamp(0.0, 1.0);
            Tint::Fixed([(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8])
        }
        _ => Tint::Grass,
    }
}

type M3 = [[f32; 3]; 3];

fn mat_mul(a: &M3, b: &M3) -> M3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    r
}

fn mat_vec(m: &M3, v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn rot(axis: char, deg: f32) -> M3 {
    let (s, c) = deg.to_radians().sin_cos();
    let (s, c) = (if s.abs() < 1e-6 { 0.0 } else { s }, if c.abs() < 1e-6 { 0.0 } else { c });
    match axis {
        'x' => [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]],
        'y' => [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]],
        _ => [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
    }
}

fn arr3(v: Option<&Value>) -> Option<[f32; 3]> {
    let a = v?.as_array()?;
    Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32])
}

/// "Canonical" UV (world projection) for a direction, from a local position.
pub fn canonical_uv(d: Dir, p: [f32; 3]) -> [f32; 2] {
    match d {
        Dir::Up => [p[0], 1.0 - p[2]],
        Dir::Down => [p[0], p[2]],
        Dir::North => [1.0 - p[0], p[1]],
        Dir::South => [p[0], p[1]],
        Dir::West => [p[2], p[1]],
        Dir::East => [1.0 - p[2], p[1]],
    }
}

fn json_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn when_matches(when: &Value, key: &StateKey) -> bool {
    let Some(obj) = when.as_object() else { return true };
    if let Some(or) = obj.get("OR").and_then(|v| v.as_array()) {
        return or.iter().any(|w| when_matches(w, key));
    }
    if let Some(and) = obj.get("AND").and_then(|v| v.as_array()) {
        return and.iter().all(|w| when_matches(w, key));
    }
    obj.iter().all(|(k, v)| {
        let want = json_str(v);
        let have = key.prop(k).unwrap_or("");
        let (neg, want) = match want.strip_prefix('!') {
            Some(w) => (true, w.to_string()),
            None => (false, want),
        };
        let m = want.split('|').any(|w| w == have);
        m != neg
    })
}

impl Registry {
    pub fn new(assets: Arc<Assets>) -> Registry {
        let r = Registry {
            assets,
            states: RwLock::new(Interner::new()),
            biomes: RwLock::new(Interner::new()),
            textures: RwLock::new(Interner::new()),
            geoms: RwLock::new(Vec::new()),
            biome_colors: RwLock::new(Vec::new()),
            models: RwLock::new(HashMap::new()),
            font: std::sync::OnceLock::new(),
        };
        // id 0 = air, biome 0 = plains
        r.state_id("minecraft:air", Vec::new());
        r.biome_id("minecraft:plains");
        r
    }

    pub fn state_id(&self, name: &str, props: Vec<(String, String)>) -> u32 {
        let name = if is_air(name) { "minecraft:air" } else { name };
        let name = if name.contains(':') { name.to_string() } else { format!("minecraft:{name}") };
        let props = if name == "minecraft:air" { Vec::new() } else { props };
        let key = StateKey { name, props };
        if let Some(&id) = self.states.read().unwrap().map.get(&key) {
            return id;
        }
        let mut w = self.states.write().unwrap();
        if let Some(&id) = w.map.get(&key) {
            return id;
        }
        let id = w.list.len() as u32;
        w.list.push(key.clone());
        w.map.insert(key, id);
        id
    }

    pub fn state(&self, id: u32) -> StateKey {
        self.states.read().unwrap().list[id as usize].clone()
    }

    pub fn state_count(&self) -> usize {
        self.states.read().unwrap().list.len()
    }

    pub fn block_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.states.read().unwrap().list.iter().map(|k| k.name.clone()).collect();
        v.sort();
        v.dedup();
        v.retain(|n| n != "minecraft:air");
        v
    }

    pub fn biome_id(&self, name: &str) -> u16 {
        if let Some(&id) = self.biomes.read().unwrap().map.get(name) {
            return id as u16;
        }
        let mut w = self.biomes.write().unwrap();
        if let Some(&id) = w.map.get(name) {
            return id as u16;
        }
        let id = w.list.len() as u32;
        w.list.push(name.to_string());
        w.map.insert(name.to_string(), id);
        id as u16
    }

    pub fn biome_colors(&self, id: u16) -> BiomeColors {
        if let Some(Some(c)) = self.biome_colors.read().unwrap().get(id as usize) {
            return *c;
        }
        let name = self.biomes.read().unwrap().list.get(id as usize).cloned().unwrap_or_default();
        let c = self.assets.biome_colors(&name);
        let mut w = self.biome_colors.write().unwrap();
        if w.len() <= id as usize {
            w.resize(id as usize + 1, None);
        }
        w[id as usize] = Some(c);
        c
    }

    pub fn font(&self) -> &crate::font::Font {
        self.font.get_or_init(|| crate::font::Font::load(&self.assets))
    }

    pub fn tex_id(&self, name: &str) -> u32 {
        if let Some(&id) = self.textures.read().unwrap().map.get(name) {
            return id;
        }
        let mut w = self.textures.write().unwrap();
        if let Some(&id) = w.map.get(name) {
            return id;
        }
        let id = w.list.len() as u32;
        w.list.push(name.to_string());
        w.map.insert(name.to_string(), id);
        id
    }

    pub fn tex_name(&self, id: u32) -> String {
        self.textures.read().unwrap().list[id as usize].clone()
    }

    pub fn geom(&self, id: u32) -> Arc<BlockGeom> {
        if let Some(Some(g)) = self.geoms.read().unwrap().get(id as usize) {
            return g.clone();
        }
        let g = Arc::new(self.build_geom(&self.state(id)));
        let mut w = self.geoms.write().unwrap();
        if w.len() <= id as usize {
            w.resize(id as usize + 1, None);
        }
        if let Some(existing) = &w[id as usize] {
            return existing.clone();
        }
        w[id as usize] = Some(g.clone());
        g
    }

    fn model(&self, id: &str) -> Option<Arc<ResolvedModel>> {
        if let Some(m) = self.models.read().unwrap().get(id) {
            return m.clone();
        }
        let m = self.load_model(id).map(Arc::new);
        self.models.write().unwrap().insert(id.to_string(), m.clone());
        m
    }

    fn load_model(&self, id: &str) -> Option<ResolvedModel> {
        let mut chain: Vec<Value> = Vec::new();
        let mut cur = id.to_string();
        for _ in 0..32 {
            if cur.starts_with("builtin/") {
                break;
            }
            let (ns, p) = cur.split_once(':').unwrap_or(("minecraft", &cur));
            let j = self.assets.json(&format!("assets/{ns}/models/{p}.json"))?;
            let parent = j.get("parent").and_then(|v| v.as_str()).map(|s| s.to_string());
            chain.push(j);
            match parent {
                Some(p) => cur = p,
                None => break,
            }
        }
        let mut textures = HashMap::new();
        for j in chain.iter().rev() {
            if let Some(t) = j.get("textures").and_then(|v| v.as_object()) {
                for (k, v) in t {
                    let s = match v {
                        Value::String(s) => s.clone(),
                        Value::Object(o) => o.get("sprite").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                        _ => continue,
                    };
                    textures.insert(k.clone(), s);
                }
            }
        }
        let elements = chain
            .iter()
            .find_map(|j| j.get("elements").and_then(|e| e.as_array()).cloned())
            .unwrap_or_default();
        let mut display = serde_json::Map::new();
        for j in chain.iter().rev() {
            if let Some(d) = j.get("display").and_then(|v| v.as_object()) {
                for (k, v) in d {
                    display.insert(k.clone(), v.clone());
                }
            }
        }
        let generated = chain.iter().any(|j| {
            j.get("parent").and_then(|p| p.as_str()).map(|p| p.ends_with("builtin/generated")).unwrap_or(false)
        });
        Some(ResolvedModel { textures, elements, display, generated })
    }

    /// Display transform of a model for a context (fixed, ground, gui…):
    /// (rotation in degrees, translation in blocks, scale)
    pub fn model_display(&self, model_id: &str, ctx: &str) -> Option<[[f32; 3]; 3]> {
        let m = self.model(model_id)?;
        let d = m.display.get(ctx)?;
        let a = |k: &str, def: f32| -> [f32; 3] {
            d.get(k)
                .and_then(|v| v.as_array())
                .map(|a| {
                    let g = |i: usize| a.get(i).and_then(|x| x.as_f64()).unwrap_or(def as f64) as f32;
                    [g(0), g(1), g(2)]
                })
                .unwrap_or([def; 3])
        };
        let t = a("translation", 0.0).map(|x| x.clamp(-80.0, 80.0) / 16.0);
        Some([a("rotation", 0.0), t, a("scale", 1.0).map(|x| x.clamp(-4.0, 4.0))])
    }

    /// Is the model a generated flat sprite (item/generated)? Returns its layers.
    pub fn model_layers(&self, model_id: &str) -> Option<Vec<String>> {
        let m = self.model(model_id)?;
        if !m.elements.is_empty() {
            return None;
        }
        let mut v = Vec::new();
        for i in 0..5 {
            match m.textures.get(&format!("layer{i}")).and_then(|t| self.resolve_tex(&m, t)) {
                Some(t) => v.push(t),
                None => break,
            }
        }
        if v.is_empty() && !m.generated { None } else { Some(v) }
    }

    /// Quads of an item model, tinted like the matching block (grass, leaves…)
    pub fn item_geometry(&self, model_id: &str, item_id: &str) -> Vec<Quad> {
        let key = StateKey { name: item_id.to_string(), props: Vec::new() };
        self.model_quads(model_id, 0, 0, false, tint_for(item_id, &key))
    }

    /// Quads of any model (no variant rotation)

    pub fn model_geometry(&self, model_id: &str) -> Vec<Quad> {
        self.model_quads(model_id, 0, 0, false, Tint::None)
    }

    /// Quads of the first variant of a block state
    pub fn state_quads(&self, id: u32) -> Vec<Quad> {
        let g = self.geom(id);
        g.parts.iter().filter_map(|p| p.first()).flat_map(|(_, q)| q.iter().cloned()).collect()
    }

    fn resolve_tex(&self, m: &ResolvedModel, t: &str) -> Option<String> {
        let mut cur = t.to_string();
        for _ in 0..16 {
            match cur.strip_prefix('#') {
                Some(k) => cur = m.textures.get(k)?.clone(),
                None => {
                    return Some(if cur.contains(':') { cur } else { format!("minecraft:{cur}") });
                }
            }
        }
        None
    }

    /// Builds the quads of a model with the variant rotation (x, y) and uvlock.
    fn model_quads(&self, model_id: &str, vx: i32, vy: i32, uvlock: bool, tint: Tint) -> Vec<Quad> {
        let Some(m) = self.model(model_id) else { return Vec::new() };
        let vrot = mat_mul(&rot('y', -vy as f32), &rot('x', -vx as f32));
        let identity = vx.rem_euclid(360) == 0 && vy.rem_euclid(360) == 0;
        let mut out = Vec::new();
        for e in &m.elements {
            let (Some(f), Some(t)) = (arr3(e.get("from")), arr3(e.get("to"))) else { continue };
            let flat = (0..3).any(|i| (f[i] - t[i]).abs() < 1e-4);
            // element rotation
            let mut erot: Option<(M3, [f32; 3], [f32; 3])> = None;
            if let Some(r) = e.get("rotation") {
                let origin = arr3(r.get("origin")).unwrap_or([8.0, 8.0, 8.0]);
                if let (Some(axis), Some(angle)) = (r.get("axis").and_then(|a| a.as_str()), r.get("angle").and_then(|a| a.as_f64())) {
                    let angle = angle as f32;
                    if angle.abs() > 1e-4 {
                        let ax = axis.chars().next().unwrap_or('y');
                        let mut scale = [1.0; 3];
                        if r.get("rescale").and_then(|v| v.as_bool()).unwrap_or(false) {
                            let s = 1.0 / angle.to_radians().cos();
                            for (i, c) in ['x', 'y', 'z'].iter().enumerate() {
                                if *c != ax {
                                    scale[i] = s;
                                }
                            }
                        }
                        erot = Some((rot(ax, angle), origin, scale));
                    }
                } else {
                    let g = |k: &str| r.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    let mm = mat_mul(&rot('z', g("z")), &mat_mul(&rot('y', g("y")), &rot('x', g("x"))));
                    erot = Some((mm, origin, [1.0; 3]));
                }
            }
            let Some(faces) = e.get("faces").and_then(|v| v.as_object()) else { continue };
            for (fname, face) in faces {
                let Some(fd) = Dir::from_name(fname) else { continue };
                let Some(tex) = face.get("texture").and_then(|v| v.as_str()).and_then(|t| self.resolve_tex(&m, t)) else {
                    continue;
                };
                let (x1, y1, z1, x2, y2, z2) = (f[0], f[1], f[2], t[0], t[1], t[2]);
                let uvr = face
                    .get("uv")
                    .and_then(|v| v.as_array())
                    .and_then(|a| {
                        Some([
                            a.first()?.as_f64()? as f32,
                            a.get(1)?.as_f64()? as f32,
                            a.get(2)?.as_f64()? as f32,
                            a.get(3)?.as_f64()? as f32,
                        ])
                    })
                    .unwrap_or(match fd {
                        Dir::Down => [x1, 16.0 - z2, x2, 16.0 - z1],
                        Dir::Up => [x1, z1, x2, z2],
                        Dir::North => [16.0 - x2, 16.0 - y2, 16.0 - x1, 16.0 - y1],
                        Dir::South => [x1, 16.0 - y2, x2, 16.0 - y1],
                        Dir::West => [z1, 16.0 - y2, z2, 16.0 - y1],
                        Dir::East => [16.0 - z2, 16.0 - y2, 16.0 - z1, 16.0 - y1],
                    });
                // corners TL, TR, BR, BL seen from outside
                let c: [[f32; 3]; 4] = match fd {
                    Dir::Down => [[x1, y1, z2], [x2, y1, z2], [x2, y1, z1], [x1, y1, z1]],
                    Dir::Up => [[x1, y2, z1], [x2, y2, z1], [x2, y2, z2], [x1, y2, z2]],
                    Dir::North => [[x2, y2, z1], [x1, y2, z1], [x1, y1, z1], [x2, y1, z1]],
                    Dir::South => [[x1, y2, z2], [x2, y2, z2], [x2, y1, z2], [x1, y1, z2]],
                    Dir::West => [[x1, y2, z1], [x1, y2, z2], [x1, y1, z2], [x1, y1, z1]],
                    Dir::East => [[x2, y2, z2], [x2, y2, z1], [x2, y1, z1], [x2, y1, z2]],
                };
                let uvc = [[uvr[0], uvr[1]], [uvr[2], uvr[1]], [uvr[2], uvr[3]], [uvr[0], uvr[3]]];
                let k = ((face.get("rotation").and_then(|v| v.as_i64()).unwrap_or(0) / 90).rem_euclid(4)) as usize;
                let mut pos = [[0f32; 3]; 4];
                let mut uv = [[0f32; 2]; 4];
                // counter-clockwise output order: BL, BR, TR, TL
                for (o, i) in [3usize, 2, 1, 0].iter().enumerate() {
                    let mut p = c[*i];
                    if let Some((m3, org, sc)) = &erot {
                        let d = [p[0] - org[0], p[1] - org[1], p[2] - org[2]];
                        let r = mat_vec(m3, d);
                        p = [org[0] + r[0] * sc[0], org[1] + r[1] * sc[1], org[2] + r[2] * sc[2]];
                    }
                    let mut p = [p[0] / 16.0, p[1] / 16.0, p[2] / 16.0];
                    if !identity {
                        let r = mat_vec(&vrot, [p[0] - 0.5, p[1] - 0.5, p[2] - 0.5]);
                        p = [r[0] + 0.5, r[1] + 0.5, r[2] + 0.5];
                    }
                    pos[o] = p;
                    let u = uvc[(i + 4 - k) % 4];
                    uv[o] = [u[0] / 16.0, 1.0 - u[1] / 16.0];
                }
                let wdir = if erot.is_none() {
                    if identity { Some(fd) } else { Dir::from_vec(mat_vec(&vrot, fd.vec().map(|x| x as f32))) }
                } else {
                    None
                };
                let cull = face
                    .get("cullface")
                    .and_then(|v| v.as_str())
                    .and_then(Dir::from_name)
                    .and_then(|cd| if identity { Some(cd) } else { Dir::from_vec(mat_vec(&vrot, cd.vec().map(|x| x as f32))) });
                if uvlock && !identity {
                    if let Some(wd) = wdir {
                        for o in 0..4 {
                            uv[o] = canonical_uv(wd, pos[o]);
                        }
                    }
                }
                let tq = if face.get("tintindex").and_then(|v| v.as_i64()).unwrap_or(-1) >= 0 { tint } else { Tint::None };
                out.push(Quad { pos, uv, tex: self.tex_id(&tex), tint: tq, cull, dir: wdir, flat });
            }
        }
        out
    }

    fn variant_quads(&self, v: &Value, tint: Tint) -> (u32, Vec<Quad>) {
        let model = v.get("model").and_then(|m| m.as_str()).unwrap_or("");
        let x = v.get("x").and_then(|m| m.as_i64()).unwrap_or(0) as i32;
        let y = v.get("y").and_then(|m| m.as_i64()).unwrap_or(0) as i32;
        let uvlock = v.get("uvlock").and_then(|m| m.as_bool()).unwrap_or(false);
        let w = v.get("weight").and_then(|m| m.as_u64()).unwrap_or(1).max(1) as u32;
        (w, self.model_quads(model, x, y, uvlock, tint))
    }

    fn variant_list(&self, v: &Value, tint: Tint) -> Vec<(u32, Vec<Quad>)> {
        match v {
            Value::Array(a) => a.iter().map(|x| self.variant_quads(x, tint)).collect(),
            _ => vec![self.variant_quads(v, tint)],
        }
    }

    fn build_geom(&self, key: &StateKey) -> BlockGeom {
        let name = key.name.clone();
        let n = short(&name).to_string();
        let mut g = BlockGeom {
            name: name.clone(),
            parts: Vec::new(),
            occludes: [false; 6],
            map_tex: None,
            map_tint: Tint::None,
            fluid: Fluid::None,
            waterlogged: key.prop("waterlogged") == Some("true")
                || matches!(n.as_str(), "seagrass" | "tall_seagrass" | "kelp" | "kelp_plant" | "bubble_column"),
            unknown: false,
            same_cull: (n.contains("glass") && !n.contains("pane")) || n == "ice" || n == "frosted_ice",
            air: name == "minecraft:air",
            be: crate::bent::is_block_entity(&n),
            has_model: false,
        };
        if g.air {
            return g;
        }
        let level: u8 = key.prop("level").and_then(|l| l.parse().ok()).unwrap_or(0);
        if n == "water" {
            g.fluid = Fluid::Water(level);
        } else if n == "lava" {
            g.fluid = Fluid::Lava(level);
        }
        let tint = tint_for(&name, key);
        let (ns, p) = name.split_once(':').unwrap_or(("minecraft", &name));
        let bs_json = self.assets.json(&format!("assets/{ns}/blockstates/{p}.json")).or_else(|| {
            // blocks renamed between versions
            let alt = match p {
                "grass" => "short_grass",
                "short_grass" => "grass",
                "grass_path" => "dirt_path",
                "dirt_path" => "grass_path",
                "chain" => "iron_chain",
                "iron_chain" => "chain",
                "sign" => "oak_sign",
                "wall_sign" => "oak_wall_sign",
                "stone_slab" => "smooth_stone_slab",
                "cobblestone_wall" => "cobblestone_wall",
                _ => return None,
            };
            self.assets.json(&format!("assets/{ns}/blockstates/{alt}.json"))
        });
        match bs_json {
            None => {
                g.unknown = true;
                let t = self.tex_id(UNKNOWN_TEX);
                g.parts.push(vec![(1, unit_cube(t))]);
                g.occludes = [true; 6];
                g.map_tex = Some(t);
                return g;
            }
            Some(bs) => {
                if let Some(vars) = bs.get("variants").and_then(|v| v.as_object()) {
                    let found = vars.iter().find(|(k, _)| {
                        k.is_empty()
                            || k.split(',').all(|kv| match kv.split_once('=') {
                                Some((a, b)) => key.prop(a) == Some(b),
                                None => true,
                            })
                    });
                    if let Some((_, v)) = found {
                        g.parts.push(self.variant_list(v, tint));
                    }
                } else if let Some(mp) = bs.get("multipart").and_then(|v| v.as_array()) {
                    for case in mp {
                        let ok = case.get("when").map(|w| when_matches(w, key)).unwrap_or(true);
                        if ok {
                            if let Some(a) = case.get("apply") {
                                g.parts.push(self.variant_list(a, tint));
                            }
                        }
                    }
                }
            }
        }
        g.has_model = g.parts.iter().any(|p| p.first().map(|(_, q)| !q.is_empty()).unwrap_or(false));
        // occlusion and map color, from the first variant of each part
        let mut best_up: Option<(f32, u32, Tint)> = None;
        let mut any: Option<(u32, Tint)> = None;
        for part in &g.parts {
            let Some((_, quads)) = part.first() else { continue };
            for q in quads {
                let tex = self.tex_name(q.tex);
                let t = self.assets.texture(&tex);
                if any.is_none() {
                    any = Some((q.tex, q.tint));
                }
                if q.dir == Some(Dir::Up) {
                    let y = q.pos[0][1];
                    if t.as_ref().map(|t| t.avg[3] > 0.1).unwrap_or(false) && best_up.map(|b| y >= b.0).unwrap_or(true) {
                        best_up = Some((y, q.tex, q.tint));
                    }
                }
                if let Some(d) = q.dir {
                    let opaque = t.as_ref().map(|t| !t.has_alpha).unwrap_or(false);
                    if opaque && covers_side(q, d) {
                        g.occludes[d as usize] = true;
                    }
                }
            }
        }
        if let Some((_, t, ti)) = best_up {
            g.map_tex = Some(t);
            g.map_tint = ti;
        } else if let Some((t, ti)) = any {
            g.map_tex = Some(t);
            g.map_tint = ti;
        }
        // blocks without geometry (chests, banners…): the model's "particle" texture for the 2D map
        if g.map_tex.is_none() && !matches!(n.as_str(), "barrier" | "light" | "structure_void" | "moving_piston") {
            if let Some(bs) = self.assets.json(&format!("assets/{ns}/blockstates/{p}.json")) {
                let first_model = |v: &Value| -> Option<String> {
                    let v = if let Value::Array(a) = v { a.first()? } else { v };
                    v.get("model").and_then(|m| m.as_str()).map(|s| s.to_string())
                };
                let mid = bs
                    .get("variants")
                    .and_then(|v| v.as_object())
                    .and_then(|vars| vars.values().next().and_then(first_model))
                    .or_else(|| bs.get("multipart").and_then(|m| m.as_array()).and_then(|a| a.first()).and_then(|c| c.get("apply")).and_then(first_model));
                if let Some(mid) = mid {
                    if let Some(m) = self.model(&mid) {
                        if let Some(t) = self.resolve_tex(&m, "#particle") {
                            g.map_tex = Some(self.tex_id(&t));
                        }
                    }
                }
            }
        }
        if let Fluid::Water(_) = g.fluid {
            g.map_tex = Some(self.tex_id("minecraft:block/water_still"));
            g.map_tint = Tint::Water;
        } else if let Fluid::Lava(_) = g.fluid {
            g.map_tex = Some(self.tex_id("minecraft:block/lava_still"));
        }
        g
    }
}

/// Does the quad fully cover face `d` of the unit cube?
fn covers_side(q: &Quad, d: Dir) -> bool {
    let (axis, val) = match d {
        Dir::Down => (1, 0.0),
        Dir::Up => (1, 1.0),
        Dir::North => (2, 0.0),
        Dir::South => (2, 1.0),
        Dir::West => (0, 0.0),
        Dir::East => (0, 1.0),
    };
    if q.pos.iter().any(|p| (p[axis] - val).abs() > 1e-4) {
        return false;
    }
    let others: Vec<usize> = (0..3).filter(|&i| i != axis).collect();
    others.iter().all(|&i| {
        let mn = q.pos.iter().map(|p| p[i]).fold(f32::MAX, f32::min);
        let mx = q.pos.iter().map(|p| p[i]).fold(f32::MIN, f32::max);
        mn < 1e-4 && mx > 1.0 - 1e-4
    })
}

pub fn unit_cube(tex: u32) -> Vec<Quad> {
    let mut v = Vec::new();
    for d in DIRS {
        let c: [[f32; 3]; 4] = match d {
            Dir::Down => [[0., 0., 1.], [1., 0., 1.], [1., 0., 0.], [0., 0., 0.]],
            Dir::Up => [[0., 1., 0.], [1., 1., 0.], [1., 1., 1.], [0., 1., 1.]],
            Dir::North => [[1., 1., 0.], [0., 1., 0.], [0., 0., 0.], [1., 0., 0.]],
            Dir::South => [[0., 1., 1.], [1., 1., 1.], [1., 0., 1.], [0., 0., 1.]],
            Dir::West => [[0., 1., 0.], [0., 1., 1.], [0., 0., 1.], [0., 0., 0.]],
            Dir::East => [[1., 1., 1.], [1., 1., 0.], [1., 0., 0.], [1., 0., 1.]],
        };
        let pos = [c[3], c[2], c[1], c[0]];
        let uv = [
            canonical_uv(d, pos[0]),
            canonical_uv(d, pos[1]),
            canonical_uv(d, pos[2]),
            canonical_uv(d, pos[3]),
        ];
        v.push(Quad { pos, uv, tex, tint: Tint::None, cull: Some(d), dir: Some(d), flat: false });
    }
    v
}
