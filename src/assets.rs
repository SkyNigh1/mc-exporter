//! Game resources read directly from the installed Minecraft jar
//! (optionally with resource packs on top).

use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const PREFIXES: &[&str] = &[
    "assets/minecraft/blockstates/",
    "assets/minecraft/models/block/",
    "assets/minecraft/textures/block/",
    "assets/minecraft/textures/colormap/",
    "assets/minecraft/textures/entity/",
    "data/minecraft/worldgen/biome/",
    "assets/minecraft/items/",
    "assets/minecraft/models/item/",
    "assets/minecraft/textures/item/",
    "assets/minecraft/textures/painting/",
    "assets/minecraft/textures/font/",
    "assets/minecraft/font/",
    "data/minecraft/painting_variant/",
    "data/minecraft/banner_pattern/",
    "assets/minecraft/equipment/",
    "assets/minecraft/textures/trims/",
    "assets/minecraft/textures/environment/",
    "data/minecraft/trim_material/",
    "data/minecraft/trim_pattern/",
    "assets/minecraft/textures/models/armor/",
];

enum Layer {
    Mem(HashMap<String, Vec<u8>>),
    /// mod jar: only the list of files is kept in memory, files are read on demand
    Zip(Mutex<zip::ZipArchive<std::io::BufReader<fs::File>>>, std::collections::HashSet<String>),
}

impl Layer {
    fn get(&self, path: &str) -> Option<Vec<u8>> {
        match self {
            Layer::Mem(m) => m.get(path).cloned(),
            Layer::Zip(z, names) => {
                if !names.contains(path) {
                    return None;
                }
                let mut z = z.lock().ok()?;
                let mut e = z.by_name(path).ok()?;
                let mut buf = Vec::with_capacity(e.size() as usize);
                e.read_to_end(&mut buf).ok()?;
                Some(buf)
            }
        }
    }
}

/// Opens a mod jar lazily (resources only)
fn open_mod(path: &Path) -> Result<Layer> {
    let f = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let z = zip::ZipArchive::new(std::io::BufReader::new(f))?;
    let names = z
        .file_names()
        .filter_map(|n| n.ok().map(|n| n.to_string()))
        .filter(|n| pack_file_wanted(n) && (n.starts_with("assets/") || n.starts_with("data/")) && !n.ends_with('/'))
        .collect();
    Ok(Layer::Zip(Mutex::new(z), names))
}

/// Mod jars of the game folder that holds a world (…/saves/<world> → …/mods)
pub fn find_mods(world_or_game_dir: &Path) -> Vec<PathBuf> {
    let candidates = [
        world_or_game_dir.join("mods"),
        world_or_game_dir.parent().and_then(|p| p.parent()).map(|p| p.join("mods")).unwrap_or_default(),
    ];
    for dir in candidates {
        if let Ok(rd) = fs::read_dir(&dir) {
            let mut v: Vec<PathBuf> = rd
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("jar")).unwrap_or(false))
                .collect();
            v.sort();
            return v;
        }
    }
    Vec::new()
}

fn load_zip(path: &Path, any_namespace: bool) -> Result<HashMap<String, Vec<u8>>> {
    let f = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(f))?;
    let mut m = HashMap::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i)?;
        let name = e.name()?.to_string();
        let keep = PREFIXES.iter().any(|p| name.starts_with(p)) || (any_namespace && pack_file_wanted(&name));
        if keep && e.is_file() {
            let mut buf = Vec::with_capacity(e.size() as usize);
            e.read_to_end(&mut buf)?;
            m.insert(name, buf);
        }
    }
    Ok(m)
}

/// Files of a resource pack that matter for rendering (sounds, languages… are skipped)
fn pack_file_wanted(name: &str) -> bool {
    if name.ends_with("pack.mcmeta") {
        return true;
    }
    let Some(i) = name.find("assets/").or_else(|| name.find("data/")) else { return false };
    let rest = &name[i..];
    !(rest.contains("/sounds/") || rest.ends_with(".ogg") || rest.contains("/lang/") || rest.contains("/texts/") || rest.contains("/shaders/"))
}

fn load_dir(root: &Path) -> HashMap<String, Vec<u8>> {
    let mut m = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(rel) = p.strip_prefix(root) {
                let name = rel.to_string_lossy().replace('\\', "/");
                if pack_file_wanted(&name) {
                    if let Ok(b) = fs::read(&p) {
                        m.insert(name, b);
                    }
                }
            }
        }
    }
    m
}

/// Resource pack format of a game jar (from its version.json), as major + minor / 1000
fn jar_pack_format(jar: &Path) -> Option<f64> {
    let f = fs::File::open(jar).ok()?;
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(f)).ok()?;
    let mut e = z.by_name("version.json").ok()?;
    let mut s = String::new();
    e.read_to_string(&mut s).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    let pv = v.get("pack_version")?;
    if let Some(n) = pv.as_f64() {
        return Some(n);
    }
    if let Some(n) = pv.get("resource").and_then(|n| n.as_f64()) {
        return Some(n);
    }
    let major = pv.get("resource_major")?.as_f64()?;
    let minor = pv.get("resource_minor").and_then(|n| n.as_f64()).unwrap_or(0.0);
    Some(major + minor / 1000.0)
}

/// A pack format value: 42, or [major, minor]
fn format_num(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::Array(a) => Some(a.first()?.as_f64()? + a.get(1).and_then(|m| m.as_f64()).unwrap_or(0.0) / 1000.0),
        _ => None,
    }
}

/// Does an overlay entry apply to this pack format?
fn overlay_applies(e: &serde_json::Value, fmt: f64) -> bool {
    let in_range = |lo: f64, hi: f64| fmt >= lo.floor() && fmt < hi.floor() + 1.0;
    if let Some(f) = e.get("formats") {
        return match f {
            serde_json::Value::Number(n) => n.as_f64().map(|n| in_range(n, n)).unwrap_or(false),
            serde_json::Value::Array(a) if a.len() == 2 => match (format_num(&a[0]), format_num(&a[1])) {
                (Some(lo), Some(hi)) => in_range(lo, hi),
                _ => false,
            },
            serde_json::Value::Object(o) => {
                let lo = o.get("min_inclusive").and_then(format_num).unwrap_or(f64::MIN);
                let hi = o.get("max_inclusive").and_then(format_num).unwrap_or(1e9);
                in_range(lo, hi)
            }
            _ => false,
        };
    }
    // 1.21.9+: min_format / max_format
    let lo = e.get("min_format").and_then(format_num).unwrap_or(f64::MIN);
    let hi = e.get("max_format").and_then(format_num).unwrap_or(1e9);
    fmt >= lo && fmt < hi.floor() + 1.0
}

/// Loads a resource pack (zip or folder): strips a wrapping folder and applies the
/// overlays that match the game version.
fn load_pack(p: &Path, fmt: Option<f64>) -> Result<HashMap<String, Vec<u8>>> {
    let raw = if p.is_dir() { load_dir(p) } else { load_zip(p, true)? };
    // zips that wrap everything in a folder ("MyPack/assets/…")
    let root = raw
        .keys()
        .filter_map(|k| k.strip_suffix("pack.mcmeta"))
        .filter(|r| r.is_empty() || r.ends_with('/'))
        .min_by_key(|r| r.len())
        .map(|r| r.to_string())
        .unwrap_or_default();
    let base: HashMap<String, Vec<u8>> =
        raw.into_iter().filter_map(|(k, v)| k.strip_prefix(&root).map(|r| (r.to_string(), v))).collect();
    let mut out: HashMap<String, Vec<u8>> =
        base.iter().filter(|(k, _)| k.starts_with("assets/") || k.starts_with("data/")).map(|(k, v)| (k.clone(), v.clone())).collect();
    // overlays: folders applied on top of the pack for some game versions
    let meta: Option<serde_json::Value> = base.get("pack.mcmeta").and_then(|b| serde_json::from_slice(b).ok());
    if let (Some(meta), Some(fmt)) = (meta, fmt) {
        if let Some(entries) = meta.get("overlays").and_then(|o| o.get("entries")).and_then(|e| e.as_array()) {
            for e in entries {
                let Some(dir) = e.get("directory").and_then(|d| d.as_str()) else { continue };
                if !overlay_applies(e, fmt) {
                    continue;
                }
                let pre = format!("{dir}/");
                for (k, v) in &base {
                    if let Some(rest) = k.strip_prefix(&pre) {
                        out.insert(rest.to_string(), v.clone());
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Resource packs enabled in the game (options.txt), highest priority first
pub fn enabled_packs(mc_dir: &Path) -> Vec<PathBuf> {
    let Ok(txt) = fs::read_to_string(mc_dir.join("options.txt")) else { return Vec::new() };
    let Some(line) = txt.lines().find(|l| l.starts_with("resourcePacks:")) else { return Vec::new() };
    let list: Vec<String> = serde_json::from_str(&line["resourcePacks:".len()..]).unwrap_or_default();
    let mut v: Vec<PathBuf> = list
        .iter()
        .filter_map(|n| n.strip_prefix("file/"))
        .map(|n| mc_dir.join("resourcepacks").join(n))
        .filter(|p| p.exists())
        .collect();
    // options.txt lists packs from the bottom of the stack to the top
    v.reverse();
    v
}

/// Resource packs found in .minecraft/resourcepacks
pub fn installed_packs(mc_dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(mc_dir.join("resourcepacks"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir() || p.extension().map(|e| e.eq_ignore_ascii_case("zip")).unwrap_or(false))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

pub struct Texture {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
    /// at least one non-opaque pixel
    pub has_alpha: bool,
    /// at least one semi-transparent pixel (stained glass, water…)
    pub translucent: bool,
    /// average color (alpha-weighted), average alpha
    pub avg: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct BiomeColors {
    pub grass: [u8; 3],
    pub foliage: [u8; 3],
    pub dry_foliage: [u8; 3],
    pub water: [u8; 3],
}

pub struct Assets {
    layers: Vec<Layer>, // decreasing priority
    pub jar_path: PathBuf,
    textures: Mutex<HashMap<String, Option<Arc<Texture>>>>,
    grass_map: Option<Arc<Texture>>,
    foliage_map: Option<Arc<Texture>>,
    dry_foliage_map: Option<Arc<Texture>>,
    biomes: Mutex<HashMap<String, BiomeColors>>,
}

pub fn parse_color(v: &serde_json::Value) -> Option<[u8; 3]> {
    let n = match v {
        serde_json::Value::String(s) => u32::from_str_radix(s.trim_start_matches('#'), 16).ok()?,
        serde_json::Value::Number(n) => n.as_i64()? as u32,
        _ => return None,
    };
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

impl Assets {
    pub fn load(jar: &Path, packs: &[PathBuf]) -> Result<Assets> {
        let mut layers = Vec::new();
        let fmt = jar_pack_format(jar);
        // resource packs first, then mod jars (both in priority order)
        for p in packs {
            if p.extension().map(|e| e.eq_ignore_ascii_case("jar")).unwrap_or(false) {
                if let Ok(l) = open_mod(p) {
                    layers.push(l);
                }
            } else if let Ok(m) = load_pack(p, fmt) {
                layers.push(Layer::Mem(m));
            }
        }
        let jar_files = load_zip(jar, false)?;
        if !jar_files.contains_key("assets/minecraft/blockstates/stone.json") {
            return Err(anyhow!("{} does not contain the game resources", jar.display()));
        }
        layers.push(Layer::Mem(jar_files));
        let mut a = Assets {
            layers,
            jar_path: jar.to_path_buf(),
            textures: Mutex::new(HashMap::new()),
            grass_map: None,
            foliage_map: None,
            dry_foliage_map: None,
            biomes: Mutex::new(HashMap::new()),
        };
        a.grass_map = a.load_png("assets/minecraft/textures/colormap/grass.png", false).map(Arc::new);
        a.foliage_map = a.load_png("assets/minecraft/textures/colormap/foliage.png", false).map(Arc::new);
        a.dry_foliage_map = a.load_png("assets/minecraft/textures/colormap/dry_foliage.png", false).map(Arc::new);
        Ok(a)
    }

    pub fn get(&self, path: &str) -> Option<Vec<u8>> {
        self.layers.iter().find_map(|l| l.get(path))
    }

    pub fn json(&self, path: &str) -> Option<serde_json::Value> {
        serde_json::from_slice(&self.get(path)?).ok()
    }

    fn load_png(&self, path: &str, first_frame: bool) -> Option<Texture> {
        let bytes = self.get(path)?;
        let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?.to_rgba8();
        let (w, mut h) = img.dimensions();
        let mut rgba = img.into_raw();
        // animated textures: keep only the first frame (only when a .mcmeta exists)
        if first_frame && h > w && h % w == 0 && self.get(&format!("{path}.mcmeta")).is_some() {
            h = w;
            rgba.truncate((w * h * 4) as usize);
        }
        let mut has_alpha = false;
        let mut translucent = false;
        let mut sum = [0f64; 4];
        for p in rgba.chunks_exact(4) {
            let a = p[3];
            if a < 255 {
                has_alpha = true;
                if a > 0 {
                    translucent = true;
                }
            }
            let af = a as f64 / 255.0;
            sum[0] += p[0] as f64 * af;
            sum[1] += p[1] as f64 * af;
            sum[2] += p[2] as f64 * af;
            sum[3] += af;
        }
        let n = (w * h) as f64;
        let avg = if sum[3] > 0.0 {
            [
                (sum[0] / sum[3]) as f32,
                (sum[1] / sum[3]) as f32,
                (sum[2] / sum[3]) as f32,
                (sum[3] / n) as f32,
            ]
        } else {
            [0.0; 4]
        };
        Some(Texture { w, h, rgba, has_alpha, translucent, avg })
    }

    /// Registers a generated texture (sign text, composed banner…)
    pub fn put_texture(&self, name: &str, w: u32, h: u32, rgba: Vec<u8>) {
        let mut has_alpha = false;
        let mut translucent = false;
        let mut sum = [0f64; 4];
        for p in rgba.chunks_exact(4) {
            if p[3] < 255 {
                has_alpha = true;
                if p[3] > 0 {
                    translucent = true;
                }
            }
            let a = p[3] as f64 / 255.0;
            sum[0] += p[0] as f64 * a;
            sum[1] += p[1] as f64 * a;
            sum[2] += p[2] as f64 * a;
            sum[3] += a;
        }
        let n = (w * h).max(1) as f64;
        let avg = if sum[3] > 0.0 {
            [(sum[0] / sum[3]) as f32, (sum[1] / sum[3]) as f32, (sum[2] / sum[3]) as f32, (sum[3] / n) as f32]
        } else {
            [0.0; 4]
        };
        let t = Texture { w, h, rgba, has_alpha, translucent, avg };
        self.textures.lock().unwrap().insert(name.to_string(), Some(Arc::new(t)));
    }

    pub fn has_texture(&self, name: &str) -> bool {
        matches!(self.textures.lock().unwrap().get(name), Some(Some(_)))
    }

    /// First existing texture among several candidate paths (names change between versions)
    pub fn first_texture(&self, ids: &[String]) -> Option<(String, Arc<Texture>)> {
        ids.iter().find_map(|id| self.texture(id).map(|t| (id.clone(), t)))
    }

    /// `id` such as "minecraft:block/stone" or "block/stone"
    pub fn texture(&self, id: &str) -> Option<Arc<Texture>> {
        if let Some(t) = self.textures.lock().unwrap().get(id) {
            return t.clone();
        }
        let (ns, p) = id.split_once(':').unwrap_or(("minecraft", id));
        let t = self.load_png(&format!("assets/{ns}/textures/{p}.png"), true).map(Arc::new);
        self.textures.lock().unwrap().insert(id.to_string(), t.clone());
        t
    }

    fn colormap(map: &Option<Arc<Texture>>, temp: f32, down: f32, fallback: [u8; 3]) -> [u8; 3] {
        let Some(m) = map else { return fallback };
        let t = temp.clamp(0.0, 1.0);
        let d = down.clamp(0.0, 1.0) * t;
        let x = (((1.0 - t) * (m.w - 1) as f32) as u32).min(m.w - 1);
        let y = (((1.0 - d) * (m.h - 1) as f32) as u32).min(m.h - 1);
        let i = ((y * m.w + x) * 4) as usize;
        [m.rgba[i], m.rgba[i + 1], m.rgba[i + 2]]
    }

    pub fn biome_colors(&self, name: &str) -> BiomeColors {
        if let Some(c) = self.biomes.lock().unwrap().get(name) {
            return *c;
        }
        let (ns, p) = name.split_once(':').unwrap_or(("minecraft", name));
        let j = self.json(&format!("data/{ns}/worldgen/biome/{p}.json"));
        let c = match &j {
            Some(j) => {
                let temp = j.get("temperature").and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
                let down = j.get("downfall").and_then(|v| v.as_f64()).unwrap_or(0.4) as f32;
                let fx = j.get("effects");
                let eff = |k: &str| fx.and_then(|f| f.get(k)).and_then(parse_color);
                let mut grass = eff("grass_color")
                    .unwrap_or_else(|| Self::colormap(&self.grass_map, temp, down, [0x79, 0xc0, 0x5a]));
                match fx.and_then(|f| f.get("grass_color_modifier")).and_then(|v| v.as_str()) {
                    Some("dark_forest") => {
                        let n = u32::from_be_bytes([0, grass[0], grass[1], grass[2]]);
                        let m = ((n & 0xFEFEFE) + 0x28340A) >> 1;
                        grass = [(m >> 16) as u8, (m >> 8) as u8, m as u8];
                    }
                    Some("swamp") => grass = [0x4c, 0x76, 0x3c],
                    _ => {}
                }
                BiomeColors {
                    grass,
                    foliage: eff("foliage_color")
                        .unwrap_or_else(|| Self::colormap(&self.foliage_map, temp, down, [0x59, 0xae, 0x30])),
                    dry_foliage: eff("dry_foliage_color")
                        .unwrap_or_else(|| Self::colormap(&self.dry_foliage_map, temp, down, [0x7b, 0x53, 0x34])),
                    water: eff("water_color").unwrap_or([0x3f, 0x76, 0xe4]),
                }
            }
            None => BiomeColors {
                grass: [0x91, 0xbd, 0x59],
                foliage: [0x77, 0xab, 0x2f],
                dry_foliage: [0x7b, 0x53, 0x34],
                water: [0x3f, 0x76, 0xe4],
            },
        };
        self.biomes.lock().unwrap().insert(name.to_string(), c);
        c
    }
}

pub fn minecraft_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".minecraft")
}

#[derive(Clone, Debug)]
pub struct JarInfo {
    pub path: PathBuf,
    pub game_version: String,
    pub id: String,
}

fn jar_has_assets(p: &Path) -> bool {
    let Ok(f) = fs::File::open(p) else { return false };
    let Ok(mut z) = zip::ZipArchive::new(std::io::BufReader::new(f)) else { return false };
    z.by_name("assets/minecraft/blockstates/stone.json").is_ok()
}

/// Lists the jars in .minecraft/versions that contain the game resources.
pub fn list_jars(mc_dir: &Path) -> Vec<JarInfo> {
    let mut v: Vec<(std::time::SystemTime, JarInfo)> = Vec::new();
    let Ok(rd) = fs::read_dir(mc_dir.join("versions")) else { return Vec::new() };
    for e in rd.flatten() {
        let id = e.file_name().to_string_lossy().to_string();
        let jar = e.path().join(format!("{id}.jar"));
        if !jar.is_file() || !jar_has_assets(&jar) {
            continue;
        }
        let mut game_version = id.clone();
        if let Ok(js) = fs::read(e.path().join(format!("{id}.json"))) {
            if let Ok(j) = serde_json::from_slice::<serde_json::Value>(&js) {
                if let Some(s) = j.get("inheritsFrom").and_then(|v| v.as_str()) {
                    game_version = s.to_string();
                }
            }
        }
        let t = jar.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        v.push((t, JarInfo { path: jar, game_version, id }));
    }
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|x| x.1).collect()
}

/// Picks the jar that best matches the world version.
pub fn pick_jar(jars: &[JarInfo], world_version: Option<&str>) -> Option<JarInfo> {
    if let Some(wv) = world_version {
        if let Some(j) = jars.iter().find(|j| j.game_version == wv && j.id == wv) {
            return Some(j.clone());
        }
        if let Some(j) = jars.iter().find(|j| j.game_version == wv) {
            return Some(j.clone());
        }
    }
    jars.first().cloned()
}

impl Assets {
    /// World data version of the game jar (version.json "world_version")
    pub fn jar_data_version(&self) -> Option<i32> {
        let f = fs::File::open(&self.jar_path).ok()?;
        let mut z = zip::ZipArchive::new(std::io::BufReader::new(f)).ok()?;
        let mut e = z.by_name("version.json").ok()?;
        let mut s = String::new();
        e.read_to_string(&mut s).ok()?;
        let v: serde_json::Value = serde_json::from_str(&s).ok()?;
        v.get("world_version")?.as_i64().map(|n| n as i32)
    }
}
