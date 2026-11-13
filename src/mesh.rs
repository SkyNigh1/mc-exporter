//! Construction du maillage d'une sélection : culling, fluides, biomes, fusion des faces.

use crate::registry::{BlockGeom, Dir, Fluid, Registry, Tint, canonical_uv};
use crate::world::{Chunk, Source};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum OffsetMode {
    None,
    Center,
    Custom,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct ExportOptions {
    pub scale: f32,
    pub offset_mode: OffsetMode,
    pub offset_x: i32,
    pub offset_z: i32,
    pub render_unknown: bool,
    pub render_sides: bool,
    pub render_biomes: bool,
    pub ore_to_stone: bool,
    pub obj_per_material: bool,
    pub obj_per_chunk: bool,
    pub double_sided: bool,
    pub optimize: bool,
    pub randomize: bool,
    pub export_textures: bool,
    pub prescale: u32,
    pub alpha_separate: bool,
    pub use_groups: bool,
    /// blocs exclus de l'export (noms complets)
    pub excluded: Vec<String>,
    /// coffres, bannières, têtes, panneaux…
    pub render_block_entities: bool,
    /// texte des panneaux
    pub sign_text: bool,
    /// entités (displays, cadres, tableaux, porte-armures…)
    pub render_entities: bool,
    /// télécharger les skins des têtes de joueur absentes du cache du jeu
    pub download_skins: bool,
    /// remonter les ghasts pour que leurs tentacules touchent le sol
    pub ghast_on_ground: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            scale: 1.0,
            offset_mode: OffsetMode::Center,
            offset_x: 0,
            offset_z: 0,
            render_unknown: true,
            render_sides: true,
            render_biomes: true,
            ore_to_stone: false,
            obj_per_material: true,
            obj_per_chunk: false,
            double_sided: false,
            optimize: true,
            randomize: false,
            export_textures: true,
            prescale: 1,
            alpha_separate: false,
            use_groups: false,
            excluded: Vec::new(),
            render_block_entities: true,
            sign_text: true,
            render_entities: true,
            download_skins: true,
            ghast_on_ground: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub x0: i32,
    pub z0: i32,
    pub x1: i32,
    pub z1: i32,
    pub y0: i32,
    pub y1: i32,
}

impl Selection {
    pub fn normalized(self) -> Selection {
        Selection {
            x0: self.x0.min(self.x1),
            x1: self.x0.max(self.x1),
            z0: self.z0.min(self.z1),
            z1: self.z0.max(self.z1),
            y0: self.y0.min(self.y1),
            y1: self.y0.max(self.y1),
        }
    }
    fn contains(&self, x: i32, y: i32, z: i32) -> bool {
        x >= self.x0 && x <= self.x1 && z >= self.z0 && z <= self.z1 && y >= self.y0 && y <= self.y1
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct MatKey {
    pub tex: u32,
    pub tint: Option<[u8; 3]>,
}

pub struct OutQuad {
    /// coordonnées relatives à (x0, 0, z0) de la sélection
    pub pos: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
    pub mat: MatKey,
    pub group: (i32, i32),
}

pub struct Mesh {
    pub quads: Vec<OutQuad>,
    pub sel: Selection,
    pub chunks_loaded: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PlaneKey {
    dir: u8,
    plane: i32,
    mat: MatKey,
    group: (i32, i32),
}

struct Ctx<'a> {
    chunks: &'a HashMap<(i32, i32), Arc<Chunk>>,
    geoms: &'a [Arc<BlockGeom>],
    remap: &'a [u32],
    sel: Selection,
    opts: &'a ExportOptions,
    reg: &'a Registry,
    water_tex: u32,
    lava_tex: u32,
}

impl Ctx<'_> {
    #[inline]
    fn get(&self, x: i32, y: i32, z: i32) -> u32 {
        match self.chunks.get(&(x >> 4, z >> 4)) {
            Some(c) => self.remap[c.block((x & 15) as usize, y, (z & 15) as usize) as usize],
            None => 0,
        }
    }
    /// voisin vu depuis la sélection (air si hors sélection et « côtés du monde » activé)
    #[inline]
    fn neighbor(&self, x: i32, y: i32, z: i32) -> u32 {
        if !self.sel.contains(x, y, z) && self.opts.render_sides {
            0
        } else {
            self.get(x, y, z)
        }
    }
}

fn hash3(x: i32, y: i32, z: i32, s: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6b343) ^ (y as u32).wrapping_mul(0xd8163841) ^ (z as u32).wrapping_mul(0xcb1ab31f) ^ s.wrapping_mul(0x9e3779b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b3c6d);
    h ^= h >> 12;
    h
}

fn mergeable(pos: &[[f32; 3]; 4], uv: &[[f32; 2]; 4], d: Dir) -> bool {
    let axis = match d {
        Dir::Down | Dir::Up => 1,
        Dir::North | Dir::South => 2,
        _ => 0,
    };
    let pv = pos[0][axis];
    if pos.iter().any(|p| (p[axis] - pv).abs() > 1e-5) {
        return false;
    }
    for i in (0..3).filter(|&i| i != axis) {
        let mn = pos.iter().map(|p| p[i]).fold(f32::MAX, f32::min);
        let mx = pos.iter().map(|p| p[i]).fold(f32::MIN, f32::max);
        if mn.abs() > 1e-5 || (mx - 1.0).abs() > 1e-5 {
            return false;
        }
    }
    let c0 = canonical_uv(d, pos[0]);
    let d0 = [uv[0][0] - c0[0], uv[0][1] - c0[1]];
    if (d0[0] - d0[0].round()).abs() > 1e-4 || (d0[1] - d0[1].round()).abs() > 1e-4 {
        return false;
    }
    (1..4).all(|i| {
        let c = canonical_uv(d, pos[i]);
        (uv[i][0] - c[0] - d0[0]).abs() < 1e-4 && (uv[i][1] - c[1] - d0[1]).abs() < 1e-4
    })
}

#[derive(Default)]
struct Partial {
    quads: Vec<OutQuad>,
    cells: HashMap<PlaneKey, Vec<(i32, i32)>>,
}

impl Partial {
    #[allow(clippy::too_many_arguments)]
    fn emit(&mut self, ctx: &Ctx, bx: i32, by: i32, bz: i32, pos: [[f32; 3]; 4], uv: [[f32; 2]; 4], dir: Option<Dir>, mat: MatKey, group: (i32, i32), double: bool) {
        let lx = (bx - ctx.sel.x0) as f32;
        let lz = (bz - ctx.sel.z0) as f32;
        let ly = by as f32;
        if ctx.opts.optimize && !double {
            if let Some(d) = dir {
                if mergeable(&pos, &uv, d) {
                    let (plane, a, b) = match d {
                        Dir::Down | Dir::Up => ((by * 16) + (pos[0][1] * 16.0).round() as i32, bx - ctx.sel.x0, bz - ctx.sel.z0),
                        Dir::North | Dir::South => ((bz - ctx.sel.z0) * 16 + (pos[0][2] * 16.0).round() as i32, bx - ctx.sel.x0, by),
                        _ => ((bx - ctx.sel.x0) * 16 + (pos[0][0] * 16.0).round() as i32, bz - ctx.sel.z0, by),
                    };
                    self.cells.entry(PlaneKey { dir: d as u8, plane, mat, group }).or_default().push((a, b));
                    return;
                }
            }
        }
        let p = pos.map(|p| [p[0] + lx, p[1] + ly, p[2] + lz]);
        if double {
            self.quads.push(OutQuad { pos: [p[3], p[2], p[1], p[0]], uv: [uv[3], uv[2], uv[1], uv[0]], mat, group });
        } else {
            self.quads.push(OutQuad { pos: p, uv, mat, group });
        }
    }
}

fn fluid_kind(g: &BlockGeom) -> u8 {
    match g.fluid {
        Fluid::Water(_) => 1,
        Fluid::Lava(_) => 2,
        Fluid::None if g.waterlogged => 1,
        _ => 0,
    }
}

fn mesh_column(ctx: &Ctx, cx: i32, cz: i32) -> Partial {
    let mut out = Partial::default();
    let Some(chunk) = ctx.chunks.get(&(cx, cz)) else { return out };
    let group = if ctx.opts.obj_per_chunk { (cx, cz) } else { (0, 0) };
    let plains = ctx.reg.biome_colors(0);
    let y_top = chunk.max_y().min(ctx.sel.y1);
    for lz in 0..16 {
        let z = cz * 16 + lz;
        if z < ctx.sel.z0 || z > ctx.sel.z1 {
            continue;
        }
        for lx in 0..16 {
            let x = cx * 16 + lx;
            if x < ctx.sel.x0 || x > ctx.sel.x1 {
                continue;
            }
            for y in ctx.sel.y0..=y_top {
                let id = ctx.get(x, y, z);
                if id == 0 {
                    continue;
                }
                let g = &ctx.geoms[id as usize];
                if g.unknown && !ctx.opts.render_unknown {
                    continue;
                }
                let biome = if ctx.opts.render_biomes {
                    ctx.reg.biome_colors(chunk.biome(lx as usize, y, lz as usize))
                } else {
                    plains
                };
                for (pi, part) in g.parts.iter().enumerate() {
                    if part.is_empty() {
                        continue;
                    }
                    let quads = if ctx.opts.randomize && part.len() > 1 {
                        let total: u32 = part.iter().map(|p| p.0).sum();
                        let mut r = hash3(x, y, z, pi as u32) % total.max(1);
                        let mut pick = &part[0].1;
                        for (w, q) in part {
                            if r < *w {
                                pick = q;
                                break;
                            }
                            r -= w;
                        }
                        pick
                    } else {
                        &part[0].1
                    };
                    for q in quads {
                        if let Some(cd) = q.cull {
                            let v = cd.vec();
                            let nid = ctx.neighbor(x + v[0], y + v[1], z + v[2]);
                            let ng = &ctx.geoms[nid as usize];
                            if ng.occludes[cd.opposite() as usize] && !(ng.unknown && !ctx.opts.render_unknown) {
                                continue;
                            }
                            if g.same_cull && nid == id {
                                continue;
                            }
                        }
                        let tint = q.tint.color(&biome);
                        let mat = MatKey { tex: q.tex, tint: if q.tint == Tint::None { None } else { tint } };
                        out.emit(ctx, x, y, z, q.pos, q.uv, q.dir, mat, group, false);
                        if ctx.opts.double_sided && q.flat {
                            out.emit(ctx, x, y, z, q.pos, q.uv, q.dir, mat, group, true);
                        }
                    }
                }
                // blocs à rendu spécial (coffres, bannières, panneaux…)
                if g.be && ctx.opts.render_block_entities {
                    let key = ctx.reg.state(id);
                    let nbt = chunk.block_entities.get(&(x, y, z));
                    let bo = crate::bent::BeOpts { sign_text: ctx.opts.sign_text, download_skins: ctx.opts.download_skins };
                    for q in crate::bent::quads(ctx.reg, ctx.reg.font(), &key, nbt, g.has_model, &bo) {
                        let tint = q.tint.color(&biome);
                        let mat = MatKey { tex: q.tex, tint: if q.tint == Tint::None { None } else { tint } };
                        out.emit(ctx, x, y, z, q.pos, q.uv, None, mat, group, false);
                    }
                }
                // fluides
                let kind = fluid_kind(g);
                if kind != 0 {
                    let level = match g.fluid {
                        Fluid::Water(l) | Fluid::Lava(l) => l,
                        _ => 0,
                    };
                    let above = &ctx.geoms[ctx.get(x, y + 1, z) as usize];
                    let h = if fluid_kind(above) == kind {
                        1.0
                    } else {
                        let l = if level >= 8 { 0 } else { level };
                        (8 - l) as f32 / 9.0
                    };
                    let (tex, tint) = if kind == 1 { (ctx.water_tex, Some(biome.water)) } else { (ctx.lava_tex, None) };
                    let mat = MatKey { tex, tint };
                    for d in crate::registry::DIRS {
                        let v = d.vec();
                        let nid = ctx.neighbor(x + v[0], y + v[1], z + v[2]);
                        let ng = &ctx.geoms[nid as usize];
                        if fluid_kind(ng) == kind {
                            continue;
                        }
                        if d != Dir::Up && ng.occludes[d.opposite() as usize] {
                            continue;
                        }
                        let c: [[f32; 3]; 4] = match d {
                            Dir::Down => [[0., 0., 1.], [1., 0., 1.], [1., 0., 0.], [0., 0., 0.]],
                            Dir::Up => [[0., h, 0.], [1., h, 0.], [1., h, 1.], [0., h, 1.]],
                            Dir::North => [[1., h, 0.], [0., h, 0.], [0., 0., 0.], [1., 0., 0.]],
                            Dir::South => [[0., h, 1.], [1., h, 1.], [1., 0., 1.], [0., 0., 1.]],
                            Dir::West => [[0., h, 0.], [0., h, 1.], [0., 0., 1.], [0., 0., 0.]],
                            Dir::East => [[1., h, 1.], [1., h, 0.], [1., 0., 0.], [1., 0., 1.]],
                        };
                        let pos = [c[3], c[2], c[1], c[0]];
                        let uv = pos.map(|p| canonical_uv(d, p));
                        out.emit(ctx, x, y, z, pos, uv, Some(d), mat, group, false);
                    }
                }
            }
        }
    }
    out
}

fn greedy(key: &PlaneKey, cells: &[(i32, i32)], out: &mut Vec<OutQuad>) {
    let set: HashSet<(i32, i32)> = cells.iter().copied().collect();
    let mut used: HashSet<(i32, i32)> = HashSet::with_capacity(set.len());
    let mut sorted: Vec<(i32, i32)> = set.iter().copied().collect();
    sorted.sort_by_key(|&(a, b)| (b, a));
    let d = crate::registry::DIRS[key.dir as usize];
    let plane = key.plane as f32 / 16.0;
    for &(a, b) in &sorted {
        if used.contains(&(a, b)) {
            continue;
        }
        let mut w = 1;
        while set.contains(&(a + w, b)) && !used.contains(&(a + w, b)) {
            w += 1;
        }
        let mut h = 1;
        'grow: loop {
            for i in 0..w {
                let c = (a + i, b + h);
                if !set.contains(&c) || used.contains(&c) {
                    break 'grow;
                }
            }
            h += 1;
        }
        for j in 0..h {
            for i in 0..w {
                used.insert((a + i, b + j));
            }
        }
        let (a0, a1, b0, b1) = (a as f32, (a + w) as f32, b as f32, (b + h) as f32);
        let to3 = |u: f32, v: f32| -> [f32; 3] {
            match d {
                Dir::Down | Dir::Up => [u, plane, v],
                Dir::North | Dir::South => [u, v, plane],
                _ => [plane, v, u],
            }
        };
        let mut p = [to3(a0, b0), to3(a1, b0), to3(a1, b1), to3(a0, b1)];
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[3][0] - p[0][0], p[3][1] - p[0][1], p[3][2] - p[0][2]];
        let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        let dv = d.vec();
        if (n[0] * dv[0] as f32 + n[1] * dv[1] as f32 + n[2] * dv[2] as f32) < 0.0 {
            p = [p[0], p[3], p[2], p[1]];
        }
        let uv = p.map(|q| canonical_uv(d, q));
        out.push(OutQuad { pos: p, uv, mat: key.mat, group: key.group });
    }
}

pub struct Progress {
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    pub cancel: AtomicBool,
    pub stage: std::sync::Mutex<String>,
}

impl Progress {
    pub fn new() -> Self {
        Progress {
            done: AtomicUsize::new(0),
            total: AtomicUsize::new(1),
            cancel: AtomicBool::new(false),
            stage: std::sync::Mutex::new(String::new()),
        }
    }
    pub fn set_stage(&self, s: &str, total: usize) {
        *self.stage.lock().unwrap() = s.to_string();
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total.max(1), Ordering::Relaxed);
    }
    pub fn fraction(&self) -> f32 {
        self.done.load(Ordering::Relaxed) as f32 / self.total.load(Ordering::Relaxed).max(1) as f32
    }
}

pub fn build_mesh(reg: &Registry, source: &Source, sel: Selection, opts: &ExportOptions, prog: &Progress) -> anyhow::Result<Mesh> {
    let sel = sel.normalized();
    let (cx0, cz0, cx1, cz1) = (sel.x0 >> 4, sel.z0 >> 4, sel.x1 >> 4, sel.z1 >> 4);
    crate::skins::DOWNLOAD.store(opts.download_skins, Ordering::Relaxed);
    crate::mobs2::GHAST_GROUND.store(opts.ghast_on_ground, Ordering::Relaxed);
    prog.set_stage("Lecture des chunks", 1);
    let chunks = source.chunks(cx0 - 1, cz0 - 1, cx1 + 1, cz1 + 1, reg);
    if prog.cancel.load(Ordering::Relaxed) {
        anyhow::bail!("Export annulé");
    }

    // remplacement des minerais et blocs exclus
    let excluded: HashSet<&str> = opts.excluded.iter().map(|s| s.as_str()).collect();
    let stone = reg.state_id("minecraft:stone", Vec::new());
    let deepslate = reg.state_id("minecraft:deepslate", vec![("axis".into(), "y".into())]);
    let netherrack = reg.state_id("minecraft:netherrack", Vec::new());
    reg.state_id("minecraft:water", vec![("level".into(), "0".into())]);
    let n = reg.state_count();
    let remap: Vec<u32> = (0..n as u32)
        .map(|id| {
            let k = reg.state(id);
            if excluded.contains(k.name.as_str()) {
                return 0;
            }
            if opts.ore_to_stone && k.name.ends_with("_ore") {
                let s = k.name.as_str();
                return if s.contains("deepslate_") {
                    deepslate
                } else if s.contains("nether_") {
                    netherrack
                } else {
                    stone
                };
            }
            id
        })
        .collect();
    prog.set_stage("Préparation des modèles", n);
    let geoms: Vec<Arc<BlockGeom>> = (0..n as u32)
        .into_par_iter()
        .map(|i| {
            prog.done.fetch_add(1, Ordering::Relaxed);
            reg.geom(i)
        })
        .collect();

    let ctx = Ctx {
        chunks: &chunks,
        geoms: &geoms,
        remap: &remap,
        sel,
        opts,
        reg,
        water_tex: reg.tex_id("minecraft:block/water_still"),
        lava_tex: reg.tex_id("minecraft:block/lava_still"),
    };
    let mut cols = Vec::new();
    for cx in cx0..=cx1 {
        for cz in cz0..=cz1 {
            cols.push((cx, cz));
        }
    }
    prog.set_stage("Génération du maillage", cols.len());
    let parts: Vec<Partial> = cols
        .par_iter()
        .map(|&(cx, cz)| {
            if prog.cancel.load(Ordering::Relaxed) {
                return Partial::default();
            }
            let p = mesh_column(&ctx, cx, cz);
            prog.done.fetch_add(1, Ordering::Relaxed);
            p
        })
        .collect();
    if prog.cancel.load(Ordering::Relaxed) {
        anyhow::bail!("Export annulé");
    }
    let mut quads = Vec::new();
    let mut cells: HashMap<PlaneKey, Vec<(i32, i32)>> = HashMap::new();
    for p in parts {
        quads.extend(p.quads);
        for (k, v) in p.cells {
            cells.entry(k).or_default().extend(v);
        }
    }
    // entités
    if opts.render_entities {
        prog.set_stage("Entités", 1);
        let ents = source.entities(cx0, cz0, cx1, cz1);
        let eo = crate::entity::EntOpts { sign_text: opts.sign_text, download_skins: opts.download_skins, world_dir: source.world_dir() };
        let plains = reg.biome_colors(0);
        let eq: Vec<OutQuad> = ents
            .par_iter()
            .filter(|e| {
                crate::entity::entity_pos(e)
                    .map(|p| {
                        p[0] >= sel.x0 as f64 && p[0] < (sel.x1 + 1) as f64 && p[2] >= sel.z0 as f64 && p[2] < (sel.z1 + 1) as f64
                            && p[1] >= sel.y0 as f64 - 1.0 && p[1] < (sel.y1 + 1) as f64
                    })
                    .unwrap_or(false)
            })
            .flat_map_iter(|e| {
                let p = crate::entity::entity_pos(e).unwrap();
                let group = if opts.obj_per_chunk { ((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4) } else { (0, 0) };
                crate::entity::entity_quads(reg, reg.font(), e, &eo).into_iter().map(move |q| {
                    let tint = q.tint.color(&plains);
                    OutQuad {
                        pos: q.pos.map(|v| [v[0] - sel.x0 as f32, v[1], v[2] - sel.z0 as f32]),
                        uv: q.uv,
                        mat: MatKey { tex: q.tex, tint: if q.tint == Tint::None { None } else { tint } },
                        group,
                    }
                })
            })
            .collect();
        quads.extend(eq);
    }
    prog.set_stage("Optimisation du maillage", 1);
    let merged: Vec<Vec<OutQuad>> = cells
        .par_iter()
        .map(|(k, v)| {
            let mut o = Vec::new();
            greedy(k, v, &mut o);
            o
        })
        .collect();
    for m in merged {
        quads.extend(m);
    }
    Ok(Mesh { quads, sel, chunks_loaded: chunks.len() })
}
