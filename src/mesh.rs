//! Mesh building for a selection: culling, fluids, biomes, face merging.

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
    /// blocks excluded from the export (full names)
    pub excluded: Vec<String>,
    /// chests, banners, heads, signs…
    pub render_block_entities: bool,
    /// sign text
    pub sign_text: bool,
    /// entities (displays, item frames, paintings, armor stands…)
    pub render_entities: bool,
    /// download player head skins missing from the game cache
    pub download_skins: bool,
    /// lift ghasts so that their tentacles touch the ground
    pub ghast_on_ground: bool,
    /// export the cloud layer above the selection
    pub clouds: bool,
    /// height of the cloud layer (192 since 1.18, 128 before)
    pub cloud_height: i32,
    /// drop what cannot be seen from outside the selection (closed caves and rooms)
    pub hide_cavities: bool,
    /// output format
    pub format: ExportFormat,
    /// 3D printing: size of one block in millimetres
    pub print_block_mm: f32,
    /// 3D printing: subdivisions per block edge (1, 2 or 4)
    pub print_detail: u32,
    /// 3D printing: wall thickness in blocks when hollowing (0 = solid)
    pub print_hollow: u32,
    /// 3D printing: remove floating pieces smaller than this many blocks (0 = keep all)
    pub print_min_piece: u32,
    /// darken the corners between blocks like the game's smooth lighting (vertex colors)
    pub ambient_occlusion: bool,
    /// normal / roughness / metal / emission maps from PBR resource packs (LabPBR)
    pub pbr: bool,
    /// pack all textures into a few atlas images
    pub atlas: bool,
    /// entity categories left out of the export
    pub excluded_entities: Vec<crate::entity::EntityKind>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum ExportFormat {
    Obj,
    Glb,
    Stl,
    ThreeMf,
    Schem,
}

impl ExportFormat {
    pub fn ext(self) -> &'static str {
        match self {
            ExportFormat::Obj => "obj",
            ExportFormat::Glb => "glb",
            ExportFormat::Stl => "stl",
            ExportFormat::ThreeMf => "3mf",
            ExportFormat::Schem => "schem",
        }
    }
    pub fn from_ext(e: &str) -> Option<ExportFormat> {
        match e.to_ascii_lowercase().as_str() {
            "obj" => Some(ExportFormat::Obj),
            "glb" => Some(ExportFormat::Glb),
            "stl" => Some(ExportFormat::Stl),
            "3mf" => Some(ExportFormat::ThreeMf),
            "schem" => Some(ExportFormat::Schem),
            _ => None,
        }
    }
    pub fn is_print(self) -> bool {
        matches!(self, ExportFormat::Stl | ExportFormat::ThreeMf)
    }
    pub fn is_mesh(self) -> bool {
        matches!(self, ExportFormat::Obj | ExportFormat::Glb)
    }
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
            clouds: true,
            cloud_height: 192,
            hide_cavities: false,
            format: ExportFormat::Obj,
            print_block_mm: 1.0,
            print_detail: 2,
            print_hollow: 0,
            print_min_piece: 8,
            ambient_occlusion: false,
            pbr: true,
            atlas: false,
            excluded_entities: Vec::new(),
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
    pub fn contains(&self, x: i32, y: i32, z: i32) -> bool {
        x >= self.x0 && x <= self.x1 && z >= self.z0 && z <= self.z1 && y >= self.y0 && y <= self.y1
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct MatKey {
    pub tex: u32,
    pub tint: Option<[u8; 3]>,
}

pub struct OutQuad {
    /// coordinates relative to the selection's (x0, 0, z0)
    pub pos: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
    pub mat: MatKey,
    pub group: (i32, i32),
    /// per-vertex brightness (ambient occlusion), 255 = fully lit
    pub light: [u8; 4],
}

pub struct Mesh {
    pub quads: Vec<OutQuad>,
    pub sel: Selection,
    pub chunks_loaded: usize,
    /// blocks and entities this version cannot draw
    pub unknown: Vec<String>,
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
    /// cells reachable from outside the selection (None: everything is visible)
    reach: Option<&'a Reach>,
}

/// Cells of the selection that can be seen from outside it.
pub struct Reach {
    sel: Selection,
    sx: usize,
    sz: usize,
    bits: Vec<u64>,
}

impl Reach {
    #[inline]
    fn index(&self, x: i32, y: i32, z: i32) -> usize {
        ((y - self.sel.y0) as usize * self.sz + (z - self.sel.z0) as usize) * self.sx + (x - self.sel.x0) as usize
    }
    /// true outside the selection
    #[inline]
    pub fn seen(&self, x: i32, y: i32, z: i32) -> bool {
        if !self.sel.contains(x, y, z) {
            return true;
        }
        let i = self.index(x, y, z);
        self.bits[i >> 6] >> (i & 63) & 1 != 0
    }
    /// the cell or one of its neighbours is seen
    pub fn near(&self, x: i32, y: i32, z: i32) -> bool {
        self.seen(x, y, z)
            || crate::registry::DIRS.iter().any(|d| {
                let v = d.vec();
                self.seen(x + v[0], y + v[1], z + v[2])
            })
    }
}

/// Flood fill from the selection boundary through every cell that is not a full
/// opaque cube (air, glass, plants, slabs…).
fn compute_reach(ctx: &Ctx) -> Option<Reach> {
    let sel = ctx.sel;
    let (sx, sy, sz) = ((sel.x1 - sel.x0 + 1) as usize, (sel.y1 - sel.y0 + 1) as usize, (sel.z1 - sel.z0 + 1) as usize);
    let n = sx * sy * sz;
    if n > 200_000_000 {
        return None;
    }
    let mut r = Reach { sel, sx, sz, bits: vec![0u64; n.div_ceil(64)] };
    let passable = |x: i32, y: i32, z: i32| -> bool {
        let g = &ctx.geoms[ctx.get(x, y, z) as usize];
        !g.occludes.iter().all(|o| *o) || (g.unknown && !ctx.opts.render_unknown)
    };
    let mut stack: Vec<(i32, i32, i32)> = Vec::new();
    let mut visit = |r: &mut Reach, stack: &mut Vec<(i32, i32, i32)>, x: i32, y: i32, z: i32| {
        if !sel.contains(x, y, z) {
            return;
        }
        let i = r.index(x, y, z);
        if r.bits[i >> 6] >> (i & 63) & 1 != 0 || !passable(x, y, z) {
            return;
        }
        r.bits[i >> 6] |= 1 << (i & 63);
        stack.push((x, y, z));
    };
    for y in sel.y0..=sel.y1 {
        for z in sel.z0..=sel.z1 {
            for x in sel.x0..=sel.x1 {
                let edge = x == sel.x0 || x == sel.x1 || y == sel.y0 || y == sel.y1 || z == sel.z0 || z == sel.z1;
                if edge {
                    visit(&mut r, &mut stack, x, y, z);
                }
            }
        }
    }
    while let Some((x, y, z)) = stack.pop() {
        for d in crate::registry::DIRS {
            let v = d.vec();
            visit(&mut r, &mut stack, x + v[0], y + v[1], z + v[2]);
        }
    }
    Some(r)
}

impl Ctx<'_> {
    #[inline]
    fn get(&self, x: i32, y: i32, z: i32) -> u32 {
        match self.chunks.get(&(x >> 4, z >> 4)) {
            Some(c) => self.remap[c.block((x & 15) as usize, y, (z & 15) as usize) as usize],
            None => 0,
        }
    }
    /// neighbor as seen from the selection (air if outside the selection and "world sides" is enabled)
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
    unknown: HashSet<u32>,
}

/// Brightness of the 4 corners of a face lying on the block boundary, from the
/// opaque cubes around it (same rule as the game's smooth lighting).
fn ambient_occlusion(ctx: &Ctx, x: i32, y: i32, z: i32, pos: &[[f32; 3]; 4], dir: Option<Dir>) -> [u8; 4] {
    const LEVELS: [u8; 4] = [128, 166, 209, 255];
    let Some(d) = dir else { return [255; 4] };
    let v = d.vec();
    let axis = if v[0] != 0 { 0 } else if v[1] != 0 { 1 } else { 2 };
    let plane = if v[axis] > 0 { 1.0 } else { 0.0 };
    if pos.iter().any(|p| (p[axis] - plane).abs() > 1e-4) {
        return [255; 4];
    }
    let solid = |dx: i32, dy: i32, dz: i32| -> u8 {
        let g = &ctx.geoms[ctx.neighbor(x + v[0] + dx, y + v[1] + dy, z + v[2] + dz) as usize];
        // like the game: any full block casts shade, leaves and glass included
        ((g.full || g.occludes.iter().all(|o| *o)) && !g.unknown) as u8
    };
    let (t1, t2) = match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let mut out = [255u8; 4];
    for (k, p) in pos.iter().enumerate() {
        let mut a = [0i32; 3];
        let mut b = [0i32; 3];
        a[t1] = if p[t1] >= 0.5 { 1 } else { -1 };
        b[t2] = if p[t2] >= 0.5 { 1 } else { -1 };
        let s1 = solid(a[0], a[1], a[2]);
        let s2 = solid(b[0], b[1], b[2]);
        let c = solid(a[0] + b[0], a[1] + b[1], a[2] + b[2]);
        let level = if s1 == 1 && s2 == 1 { 0 } else { 3 - (s1 + s2 + c) as usize };
        out[k] = LEVELS[level];
    }
    out
}

impl Partial {
    #[allow(clippy::too_many_arguments)]
    fn emit(&mut self, ctx: &Ctx, bx: i32, by: i32, bz: i32, pos: [[f32; 3]; 4], uv: [[f32; 2]; 4], dir: Option<Dir>, mat: MatKey, group: (i32, i32), double: bool, light: [u8; 4]) {
        let lx = (bx - ctx.sel.x0) as f32;
        let lz = (bz - ctx.sel.z0) as f32;
        let ly = by as f32;
        if ctx.opts.optimize && !double && light == [255; 4] && !ctx.opts.atlas {
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
            self.quads.push(OutQuad { pos: [p[3], p[2], p[1], p[0]], uv: [uv[3], uv[2], uv[1], uv[0]], mat, group, light: [light[3], light[2], light[1], light[0]] });
        } else {
            self.quads.push(OutQuad { pos: p, uv, mat, group, light });
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
                if g.unknown {
                    out.unknown.insert(id);
                    if !ctx.opts.render_unknown {
                        continue;
                    }
                }
                if let Some(r) = ctx.reach {
                    if !r.near(x, y, z) {
                        continue;
                    }
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
                            if let Some(r) = ctx.reach {
                                if !r.seen(x + v[0], y + v[1], z + v[2]) {
                                    continue;
                                }
                            }
                        }
                        let tint = q.tint.color(&biome);
                        let mat = MatKey { tex: q.tex, tint: if q.tint == Tint::None { None } else { tint } };
                        let light = if ctx.opts.ambient_occlusion { ambient_occlusion(ctx, x, y, z, &q.pos, q.dir) } else { [255; 4] };
                        out.emit(ctx, x, y, z, q.pos, q.uv, q.dir, mat, group, false, light);
                        if ctx.opts.double_sided && q.flat {
                            out.emit(ctx, x, y, z, q.pos, q.uv, q.dir, mat, group, true, light);
                        }
                    }
                }
                // specially rendered blocks (chests, banners, signs…)
                if g.be && ctx.opts.render_block_entities {
                    let key = ctx.reg.state(id);
                    let nbt = chunk.block_entities.get(&(x, y, z));
                    let bo = crate::bent::BeOpts { sign_text: ctx.opts.sign_text, download_skins: ctx.opts.download_skins };
                    for q in crate::bent::quads(ctx.reg, ctx.reg.font(), &key, nbt, g.has_model, &bo) {
                        let tint = q.tint.color(&biome);
                        let mat = MatKey { tex: q.tex, tint: if q.tint == Tint::None { None } else { tint } };
                        out.emit(ctx, x, y, z, q.pos, q.uv, None, mat, group, false, [255; 4]);
                    }
                }
                // fluids
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
                        if let Some(r) = ctx.reach {
                            if !r.seen(x + v[0], y + v[1], z + v[2]) {
                                continue;
                            }
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
                        let light = if ctx.opts.ambient_occlusion { ambient_occlusion(ctx, x, y, z, &pos, Some(d)) } else { [255; 4] };
                        out.emit(ctx, x, y, z, pos, uv, Some(d), mat, group, false, light);
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
        out.push(OutQuad { pos: p, uv, mat: key.mat, group: key.group, light: [255; 4] });
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
    prog.set_stage(&crate::i18n::t("Reading chunks"), 1);
    let chunks = source.chunks(cx0 - 1, cz0 - 1, cx1 + 1, cz1 + 1, reg);
    if prog.cancel.load(Ordering::Relaxed) {
        anyhow::bail!("{}", crate::i18n::t("Export cancelled"));
    }

    // ore replacement and excluded blocks
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
    prog.set_stage(&crate::i18n::t("Preparing models"), n);
    let geoms: Vec<Arc<BlockGeom>> = (0..n as u32)
        .into_par_iter()
        .map(|i| {
            prog.done.fetch_add(1, Ordering::Relaxed);
            reg.geom(i)
        })
        .collect();

    let mut ctx = Ctx {
        chunks: &chunks,
        geoms: &geoms,
        remap: &remap,
        sel,
        opts,
        reg,
        water_tex: reg.tex_id("minecraft:block/water_still"),
        lava_tex: reg.tex_id("minecraft:block/lava_still"),
        reach: None,
    };
    let reach = if opts.hide_cavities {
        prog.set_stage(&crate::i18n::t("Finding hidden cavities"), 1);
        compute_reach(&ctx)
    } else {
        None
    };
    ctx.reach = reach.as_ref();
    let mut cols = Vec::new();
    for cx in cx0..=cx1 {
        for cz in cz0..=cz1 {
            cols.push((cx, cz));
        }
    }
    prog.set_stage(&crate::i18n::t("Building mesh"), cols.len());
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
        anyhow::bail!("{}", crate::i18n::t("Export cancelled"));
    }
    let mut quads = Vec::new();
    let mut cells: HashMap<PlaneKey, Vec<(i32, i32)>> = HashMap::new();
    let mut unknown: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for p in parts {
        for id in &p.unknown {
            unknown.insert(reg.state(*id).name);
        }
        quads.extend(p.quads);
        for (k, v) in p.cells {
            cells.entry(k).or_default().extend(v);
        }
    }
    // entities
    let unknown_ents: std::sync::Mutex<std::collections::BTreeSet<String>> = Default::default();
    if opts.render_entities {
        prog.set_stage(&crate::i18n::t("Entities"), 1);
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
                            && reach.as_ref().map(|r| r.near(p[0].floor() as i32, p[1].floor() as i32, p[2].floor() as i32)).unwrap_or(true)
                    })
                    .unwrap_or(false)
                    && !opts.excluded_entities.contains(&crate::entity::kind(e))
            })
            .flat_map_iter(|e| {
                let p = crate::entity::entity_pos(e).unwrap();
                let group = if opts.obj_per_chunk { ((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4) } else { (0, 0) };
                let (eq, known) = crate::entity::entity_quads_checked(reg, reg.font(), e, &eo);
                if !known {
                    if let Some(id) = crate::entity::entity_id(e) {
                        unknown_ents.lock().unwrap().insert(id);
                    }
                }
                eq.into_iter().map(move |q| {
                    let tint = q.tint.color(&plains);
                    OutQuad {
                        pos: q.pos.map(|v| [v[0] - sel.x0 as f32, v[1], v[2] - sel.z0 as f32]),
                        uv: q.uv,
                        mat: MatKey { tex: q.tex, tint: if q.tint == Tint::None { None } else { tint } },
                        group,
                        light: [255; 4],
                    }
                })
            })
            .collect();
        quads.extend(eq);
    }
    // clouds
    if opts.clouds {
        prog.set_stage(&crate::i18n::t("Clouds"), 1);
        quads.extend(crate::clouds::cloud_quads(reg, sel, opts.cloud_height as f32, opts.obj_per_chunk));
    }
    prog.set_stage(&crate::i18n::t("Optimizing mesh"), 1);
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
    unknown.extend(unknown_ents.into_inner().unwrap());
    Ok(Mesh { quads, sel, chunks_loaded: chunks.len(), unknown: unknown.into_iter().collect() })
}
