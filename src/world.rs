//! Reading of Minecraft Java worlds: level.dat, dimensions, region files (Anvil)
//! and 1.18+ chunks (paletted sections).

use crate::registry::Registry;
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct DimInfo {
    pub id: String,
    pub source: Source,
}

#[derive(Clone)]
pub struct WorldInfo {
    pub path: PathBuf,
    pub name: String,
    pub version_name: Option<String>,
    pub spawn: (i32, i32),
    pub dims: Vec<DimInfo>,
}

fn nbt_get<'a>(v: &'a fastnbt::Value, key: &str) -> Option<&'a fastnbt::Value> {
    match v {
        fastnbt::Value::Compound(m) => m.get(key),
        _ => None,
    }
}

fn nbt_int(v: &fastnbt::Value) -> Option<i64> {
    use fastnbt::Value::*;
    match v {
        Byte(x) => Some(*x as i64),
        Short(x) => Some(*x as i64),
        Int(x) => Some(*x as i64),
        Long(x) => Some(*x),
        Float(x) => Some(*x as i64),
        Double(x) => Some(*x as i64),
        _ => None,
    }
}

impl WorldInfo {
    pub fn open(path: &Path) -> Result<WorldInfo> {
        let level = path.join("level.dat");
        let raw = fs::read(&level).with_context(|| crate::i18n::tf("level.dat not found in {0}", &[&path.display()]))?;
        let mut data = Vec::new();
        flate2::read::GzDecoder::new(&raw[..])
            .read_to_end(&mut data)
            .context("unreadable level.dat (gzip)")?;
        let root: fastnbt::Value = fastnbt::from_bytes(&data).context("unreadable level.dat (NBT)")?;
        let d = nbt_get(&root, "Data").ok_or_else(|| anyhow!("level.dat has no Data"))?;

        let name = match nbt_get(d, "LevelName") {
            Some(fastnbt::Value::String(s)) => s.clone(),
            _ => path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        };
        let version_name = nbt_get(d, "Version").and_then(|v| match nbt_get(v, "Name") {
            Some(fastnbt::Value::String(s)) => Some(s.clone()),
            _ => None,
        });

        // Spawn point: old format (SpawnX/SpawnZ) or new one (spawn.pos)
        let mut spawn = (0, 0);
        if let (Some(x), Some(z)) = (
            nbt_get(d, "SpawnX").and_then(nbt_int),
            nbt_get(d, "SpawnZ").and_then(nbt_int),
        ) {
            spawn = (x as i32, z as i32);
        } else if let Some(pos) = nbt_get(d, "spawn").and_then(|s| nbt_get(s, "pos")) {
            if let fastnbt::Value::IntArray(a) = pos {
                let v: Vec<i32> = a.iter().copied().collect();
                if v.len() >= 3 {
                    spawn = (v[0], v[2]);
                }
            }
        }

        let dims = find_dimensions(path);
        Ok(WorldInfo { path: path.to_path_buf(), name, version_name, spawn, dims })
    }
}

fn has_mca(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|rd| rd.flatten().any(|e| { let n = e.file_name().to_string_lossy().to_string(); n.ends_with(".mca") || n.ends_with(".mcr") }))
        .unwrap_or(false)
}

/// Handles the old layout (region/, DIM-1/, DIM1/) and the new one (dimensions/<ns>/<name>/region).
pub fn find_dimensions(world: &Path) -> Vec<DimInfo> {
    let mut out: Vec<DimInfo> = Vec::new();
    let mut push = |id: String, dir: PathBuf| {
        if dir.is_dir() && !out.iter().any(|d| d.id == id) {
            out.push(DimInfo { id, source: Source::Dir(dir) });
        }
    };
    // New layout first when it contains regions
    let dims_root = world.join("dimensions");
    if let Ok(nss) = fs::read_dir(&dims_root) {
        let mut found: Vec<(String, PathBuf)> = Vec::new();
        for ns in nss.flatten() {
            let ns_name = ns.file_name().to_string_lossy().to_string();
            walk_dims(&ns.path(), &ns_name, "", 0, &mut found);
        }
        found.sort();
        for (id, dir) in found {
            if has_mca(&dir) {
                push(id, dir);
            }
        }
    }
    push("minecraft:overworld".into(), world.join("region"));
    push("minecraft:the_nether".into(), world.join("DIM-1").join("region"));
    push("minecraft:the_end".into(), world.join("DIM1").join("region"));
    // Dimensions without any region go last
    out.sort_by_key(|d| {
        let order = match d.id.as_str() {
            "minecraft:overworld" => 0,
            "minecraft:the_nether" => 1,
            "minecraft:the_end" => 2,
            _ => 3,
        };
        (d.source.regions().is_empty(), order)
    });
    out
}

fn walk_dims(dir: &Path, ns: &str, rel: &str, depth: u32, out: &mut Vec<(String, PathBuf)>) {
    if depth > 4 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        if e.path().join("region").is_dir() {
            out.push((format!("{ns}:{r}"), e.path().join("region")));
        }
        if !matches!(name.as_str(), "region" | "entities" | "poi" | "data") {
            walk_dims(&e.path(), ns, &r, depth + 1, out);
        }
    }
}

/// Lists the regions (rx, rz) present in a folder.
pub fn list_regions(dir: &Path) -> Vec<(i32, i32)> {
    let mut v = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            let parts: Vec<&str> = n.split('.').collect();
            if parts.len() == 4 && parts[0] == "r" && (parts[3] == "mca" || parts[3] == "mcr") {
                if let (Ok(x), Ok(z)) = (parts[1].parse(), parts[2].parse()) {
                    if e.metadata().map(|m| m.len() >= 8192).unwrap_or(false) {
                        v.push((x, z));
                    }
                }
            }
        }
    }
    // a converted world can contain both .mca and .mcr files for the same region
    v.sort();
    v.dedup();
    v
}

pub struct RegionFile {
    data: Vec<u8>,
    dir: PathBuf,
    rx: i32,
    rz: i32,
}

impl RegionFile {
    pub fn open(dir: &Path, rx: i32, rz: i32) -> Option<RegionFile> {
        // Anvil (.mca) first, then McRegion (.mcr, Beta 1.3 – 1.1)
        let data = fs::read(dir.join(format!("r.{rx}.{rz}.mca"))).or_else(|_| fs::read(dir.join(format!("r.{rx}.{rz}.mcr")))).ok()?;
        if data.len() < 8192 {
            return None;
        }
        Some(RegionFile { data, dir: dir.to_path_buf(), rx, rz })
    }

    /// Decompressed NBT of the local chunk (lx, lz) ∈ [0,32)
    pub fn chunk_nbt(&self, lx: usize, lz: usize) -> Option<Vec<u8>> {
        let i = 4 * (lx + lz * 32);
        let h = &self.data[i..i + 4];
        let offset = ((h[0] as usize) << 16 | (h[1] as usize) << 8 | h[2] as usize) * 4096;
        if offset == 0 || offset + 5 > self.data.len() {
            return None;
        }
        let len = u32::from_be_bytes(self.data[offset..offset + 4].try_into().ok()?) as usize;
        let comp = self.data[offset + 4];
        let external;
        let payload: &[u8] = if comp & 0x80 != 0 {
            let cx = self.rx * 32 + lx as i32;
            let cz = self.rz * 32 + lz as i32;
            external = fs::read(self.dir.join(format!("c.{cx}.{cz}.mcc"))).ok()?;
            &external[..]
        } else {
            if len < 1 || offset + 4 + len > self.data.len() {
                return None;
            }
            &self.data[offset + 5..offset + 4 + len]
        };
        let mut out = Vec::new();
        match comp & 0x7f {
            1 => flate2::read::GzDecoder::new(payload).read_to_end(&mut out).ok()?,
            2 => flate2::read::ZlibDecoder::new(payload).read_to_end(&mut out).ok()?,
            3 => {
                out.extend_from_slice(payload);
                0
            }
            _ => return None, // LZ4 and custom formats: not supported
        };
        Some(out)
    }
}

#[derive(Deserialize)]
struct RawChunk {
    #[serde(rename = "xPos")]
    x_pos: i32,
    #[serde(rename = "zPos")]
    z_pos: i32,
    #[serde(rename = "Status", default)]
    status: Option<String>,
    #[serde(default)]
    sections: Vec<RawSection>,
    #[serde(default)]
    block_entities: Vec<fastnbt::Value>,
}

#[derive(Deserialize)]
struct RawSection {
    #[serde(rename = "Y")]
    y: i8,
    block_states: Option<RawStates>,
    biomes: Option<RawBiomes>,
}

#[derive(Deserialize)]
struct RawStates {
    palette: Vec<RawPal>,
    data: Option<fastnbt::LongArray>,
}

#[derive(Deserialize)]
struct RawPal {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Properties", default)]
    props: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct RawBiomes {
    palette: Vec<String>,
    data: Option<fastnbt::LongArray>,
}

pub struct Section {
    pub y: i32,
    /// 4096 state ids (y*256 + z*16 + x), or a single one if the section is uniform
    pub blocks: Vec<u32>,
    /// 64 biome ids (4×4×4 cells), or a single one
    pub biomes: Vec<u16>,
}

pub struct Chunk {
    pub cx: i32,
    pub cz: i32,
    pub sections: Vec<Section>, // sorted by increasing y
    /// block entity data (chests, signs, banners…), absolute coordinates
    pub block_entities: HashMap<(i32, i32, i32), fastnbt::Value>,
}

fn unpack(data: &[i64], bits: usize, count: usize) -> Vec<u32> {
    let per_long = 64 / bits;
    let mask = (1u64 << bits) - 1;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let l = data.get(i / per_long).copied().unwrap_or(0) as u64;
        out.push(((l >> ((i % per_long) * bits)) & mask) as u32);
    }
    out
}

fn bits_for(n: usize) -> usize {
    if n <= 1 { 0 } else { (usize::BITS - (n - 1).leading_zeros()) as usize }
}

impl Chunk {
    pub fn parse(nbt: &[u8], reg: &Registry) -> Option<Chunk> {
        let raw: RawChunk = match fastnbt::from_bytes(nbt) {
            Ok(r) => r,
            // older formats (everything under "Level")
            Err(_) => return crate::legacy::parse(nbt, reg).map(|(c, _)| c),
        };
        if let Some(st) = &raw.status {
            let s = st.trim_start_matches("minecraft:");
            if s != "full" {
                return None;
            }
        }
        let mut sections = Vec::new();
        for s in raw.sections {
            let Some(bs) = s.block_states else { continue };
            if bs.palette.is_empty() {
                continue;
            }
            let ids: Vec<u32> = bs
                .palette
                .iter()
                .map(|p| {
                    let mut props: Vec<(String, String)> =
                        p.props.as_ref().map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
                    props.sort();
                    reg.state_id(&p.name, props)
                })
                .collect();
            let blocks = if ids.len() == 1 || bs.data.is_none() {
                vec![ids[0]]
            } else {
                let bits = bits_for(ids.len()).max(4);
                let data: Vec<i64> = bs.data.unwrap().iter().copied().collect();
                unpack(&data, bits, 4096).into_iter().map(|i| *ids.get(i as usize).unwrap_or(&0)).collect()
            };
            if blocks.len() == 1 && blocks[0] == 0 {
                continue; // empty section
            }
            let biomes = match s.biomes {
                Some(b) if !b.palette.is_empty() => {
                    let bids: Vec<u16> = b.palette.iter().map(|n| reg.biome_id(n)).collect();
                    if bids.len() == 1 || b.data.is_none() {
                        vec![bids[0]]
                    } else {
                        let bits = bits_for(bids.len()).max(1);
                        let data: Vec<i64> = b.data.unwrap().iter().copied().collect();
                        unpack(&data, bits, 64).into_iter().map(|i| *bids.get(i as usize).unwrap_or(&0)).collect()
                    }
                }
                _ => vec![0],
            };
            sections.push(Section { y: s.y as i32, blocks, biomes });
        }
        sections.sort_by_key(|s| s.y);
        let mut block_entities = HashMap::new();
        for be in raw.block_entities {
            if let fastnbt::Value::Compound(m) = &be {
                let g = |k: &str| match m.get(k) { Some(fastnbt::Value::Int(v)) => Some(*v), _ => None };
                if let (Some(x), Some(y), Some(z)) = (g("x"), g("y"), g("z")) {
                    block_entities.insert((x, y, z), be.clone());
                }
            }
        }
        Some(Chunk { cx: raw.x_pos, cz: raw.z_pos, sections, block_entities })
    }

    #[inline]
    pub fn section(&self, sy: i32) -> Option<&Section> {
        // few sections: linear search
        self.sections.iter().find(|s| s.y == sy)
    }

    /// Local x,z coordinates ∈ [0,16), absolute y
    #[inline]
    pub fn block(&self, x: usize, y: i32, z: usize) -> u32 {
        match self.section(y >> 4) {
            Some(s) => {
                if s.blocks.len() == 1 {
                    s.blocks[0]
                } else {
                    s.blocks[((y & 15) as usize) * 256 + z * 16 + x]
                }
            }
            None => 0,
        }
    }

    #[inline]
    pub fn biome(&self, x: usize, y: i32, z: usize) -> u16 {
        match self.section(y >> 4) {
            Some(s) => {
                if s.biomes.len() == 1 {
                    s.biomes[0]
                } else {
                    s.biomes[(((y & 15) as usize) >> 2) * 16 + (z >> 2) * 4 + (x >> 2)]
                }
            }
            None => 0,
        }
    }

    pub fn max_y(&self) -> i32 {
        self.sections.last().map(|s| s.y * 16 + 15).unwrap_or(i32::MIN)
    }
}

/// Where chunks come from: a region folder, or an in-memory world (schematic).
#[derive(Clone)]
pub enum Source {
    Dir(PathBuf),
    Mem(Arc<MemWorld>),
}

/// In-memory world (imported schematic)
pub struct MemWorld {
    pub chunks: HashMap<(i32, i32), Arc<Chunk>>,
    /// entities with their position (Pos) already moved into the empty world's frame
    pub entities: Vec<fastnbt::Value>,
}

impl Source {
    /// Root folder of the world (the one containing level.dat)
    pub fn world_dir(&self) -> Option<PathBuf> {
        match self {
            Source::Dir(d) => d.ancestors().find(|p| p.join("level.dat").is_file()).map(|p| p.to_path_buf()),
            Source::Mem(_) => None,
        }
    }

    /// Entities of chunks [cx0..=cx1] × [cz0..=cz1]
    pub fn entities(&self, cx0: i32, cz0: i32, cx1: i32, cz1: i32) -> Vec<fastnbt::Value> {
        let inside = |e: &fastnbt::Value| {
            crate::entity::entity_pos(e)
                .map(|p| {
                    let (cx, cz) = ((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4);
                    cx >= cx0 && cx <= cx1 && cz >= cz0 && cz <= cz1
                })
                .unwrap_or(false)
        };
        match self {
            Source::Mem(m) => m.entities.iter().filter(|e| inside(e)).cloned().collect(),
            Source::Dir(dir) => {
                // 1.17+: entities are stored in <dimension>/entities/r.x.z.mca
                let Some(edir) = dir.parent().map(|p| p.join("entities")) else { return Vec::new() };
                let mut out = Vec::new();
                for rx in cx0.div_euclid(32)..=cx1.div_euclid(32) {
                    for rz in cz0.div_euclid(32)..=cz1.div_euclid(32) {
                        let Some(rf) = RegionFile::open(&edir, rx, rz) else {
                            // before 1.17: entities are stored inside the region chunks
                            if let Some(rf) = RegionFile::open(dir, rx, rz) {
                                for lz in 0..32 {
                                    for lx in 0..32 {
                                        let (cx, cz) = (rx * 32 + lx as i32, rz * 32 + lz as i32);
                                        if cx < cx0 || cx > cx1 || cz < cz0 || cz > cz1 {
                                            continue;
                                        }
                                        if let Some(nbt) = rf.chunk_nbt(lx, lz) {
                                            out.extend(crate::legacy::chunk_entities(&nbt));
                                        }
                                    }
                                }
                            }
                            continue;
                        };
                        for lz in 0..32 {
                            for lx in 0..32 {
                                let (cx, cz) = (rx * 32 + lx as i32, rz * 32 + lz as i32);
                                if cx < cx0 || cx > cx1 || cz < cz0 || cz > cz1 {
                                    continue;
                                }
                                let Some(nbt) = rf.chunk_nbt(lx, lz) else { continue };
                                let Ok(v) = fastnbt::from_bytes::<fastnbt::Value>(&nbt) else { continue };
                                if let Some(fastnbt::Value::List(l)) = nbt_get(&v, "Entities") {
                                    for e in l {
                                        flatten_passengers(e, &mut out);
                                    }
                                }
                            }
                        }
                    }
                }
                out
            }
        }
    }

    pub fn regions(&self) -> Vec<(i32, i32)> {
        match self {
            Source::Dir(d) => list_regions(d),
            Source::Mem(m) => {
                let mut v: Vec<(i32, i32)> = m.chunks.keys().map(|(cx, cz)| (cx.div_euclid(32), cz.div_euclid(32))).collect();
                v.sort();
                v.dedup();
                v
            }
        }
    }

    /// All chunks of a region.
    pub fn region_chunks(&self, rx: i32, rz: i32, reg: &Registry) -> Vec<Arc<Chunk>> {
        match self {
            Source::Mem(m) => m
                .chunks
                .iter()
                .filter(|((cx, cz), _)| cx.div_euclid(32) == rx && cz.div_euclid(32) == rz)
                .map(|(_, c)| c.clone())
                .collect(),
            Source::Dir(dir) => {
                let Some(rf) = RegionFile::open(dir, rx, rz) else { return Vec::new() };
                let mut out = Vec::new();
                for lz in 0..32 {
                    for lx in 0..32 {
                        if let Some(nbt) = rf.chunk_nbt(lx, lz) {
                            if let Some(c) = Chunk::parse(&nbt, reg) {
                                out.push(Arc::new(c));
                            }
                        }
                    }
                }
                out
            }
        }
    }

    /// Chunks [cx0..=cx1] × [cz0..=cz1].
    pub fn chunks(&self, cx0: i32, cz0: i32, cx1: i32, cz1: i32, reg: &Registry) -> HashMap<(i32, i32), Arc<Chunk>> {
        use rayon::prelude::*;
        match self {
            Source::Mem(m) => m
                .chunks
                .iter()
                .filter(|((cx, cz), _)| *cx >= cx0 && *cx <= cx1 && *cz >= cz0 && *cz <= cz1)
                .map(|(k, c)| (*k, c.clone()))
                .collect(),
            Source::Dir(dir) => {
                let mut regions = Vec::new();
                for rx in cx0.div_euclid(32)..=cx1.div_euclid(32) {
                    for rz in cz0.div_euclid(32)..=cz1.div_euclid(32) {
                        regions.push((rx, rz));
                    }
                }
                let chunks: Vec<Chunk> = regions
                    .par_iter()
                    .flat_map_iter(|&(rx, rz)| {
                        let mut out = Vec::new();
                        if let Some(rf) = RegionFile::open(dir, rx, rz) {
                            for lz in 0..32 {
                                for lx in 0..32 {
                                    let cx = rx * 32 + lx as i32;
                                    let cz = rz * 32 + lz as i32;
                                    if cx < cx0 || cx > cx1 || cz < cz0 || cz > cz1 {
                                        continue;
                                    }
                                    if let Some(nbt) = rf.chunk_nbt(lx, lz) {
                                        if let Some(c) = Chunk::parse(&nbt, reg) {
                                            out.push(c);
                                        }
                                    }
                                }
                            }
                        }
                        out
                    })
                    .collect();
                chunks.into_iter().map(|c| ((c.cx, c.cz), Arc::new(c))).collect()
            }
        }
    }
}

/// Adds an entity and its passengers (an armor stand in a minecart…)
pub fn flatten_passengers(e: &fastnbt::Value, out: &mut Vec<fastnbt::Value>) {
    out.push(e.clone());
    if let Some(fastnbt::Value::List(p)) = nbt_get(e, "Passengers") {
        for x in p {
            flatten_passengers(x, out);
        }
    }
}

/// Builds in-memory chunks from a list of blocks (x, y, z, state).
pub fn build_chunks(
    blocks: impl Iterator<Item = (i32, i32, i32, u32)>,
    mut bes: HashMap<(i32, i32, i32), fastnbt::Value>,
) -> HashMap<(i32, i32), Arc<Chunk>> {
    let mut secs: HashMap<(i32, i32), HashMap<i32, Vec<u32>>> = HashMap::new();
    for (x, y, z, id) in blocks {
        if id == 0 {
            continue;
        }
        let s = secs.entry((x >> 4, z >> 4)).or_default().entry(y >> 4).or_insert_with(|| vec![0; 4096]);
        s[((y & 15) as usize) * 256 + ((z & 15) as usize) * 16 + (x & 15) as usize] = id;
    }
    secs.into_iter()
        .map(|((cx, cz), m)| {
            let mut sections: Vec<Section> = m.into_iter().map(|(y, blocks)| Section { y, blocks, biomes: vec![0] }).collect();
            sections.sort_by_key(|s| s.y);
            let block_entities: HashMap<(i32, i32, i32), fastnbt::Value> = bes
                .keys()
                .filter(|(x, _, z)| x >> 4 == cx && z >> 4 == cz)
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|k| bes.remove(&k).map(|v| (k, v)))
                .collect();
            ((cx, cz), Arc::new(Chunk { cx, cz, sections, block_entities }))
        })
        .collect()
}

/// Lists the saves in the .minecraft/saves folder
pub fn list_saves(mc_dir: &Path) -> Vec<(String, PathBuf)> {
    let mut v = Vec::new();
    if let Ok(rd) = fs::read_dir(mc_dir.join("saves")) {
        for e in rd.flatten() {
            if e.path().join("level.dat").is_file() {
                let modified = e.metadata().and_then(|m| m.modified()).ok();
                v.push((modified, e.file_name().to_string_lossy().to_string(), e.path()));
            }
        }
    }
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, n, p)| (n, p)).collect()
}
