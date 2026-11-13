//! Export for 3D printing (STL / 3MF): the selection is voxelized (1, 2 or 4 cells per
//! block edge, from the block models), then made printable:
//! small floating pieces removed, edge-only contacts joined, enclosed air filled,
//! optional hollowing, and a closed surface written in millimetres (Z up).

use crate::export::ExportSummary;
use crate::mesh::{ExportFormat, ExportOptions, Progress, Selection};
use crate::registry::{Fluid, Registry};
use crate::world::Source;
use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::Ordering;

const MAX_CELLS: usize = 400_000_000;

struct Grid {
    nx: usize,
    ny: usize,
    nz: usize,
    /// 0 = empty, otherwise color index + 1
    c: Vec<u16>,
}

impl Grid {
    #[inline]
    fn idx(&self, x: usize, y: usize, z: usize) -> usize {
        (y * self.nz + z) * self.nx + x
    }
    #[inline]
    fn get(&self, x: isize, y: isize, z: isize) -> u16 {
        if x < 0 || y < 0 || z < 0 || x >= self.nx as isize || y >= self.ny as isize || z >= self.nz as isize {
            return 0;
        }
        self.c[self.idx(x as usize, y as usize, z as usize)]
    }
    fn coords(&self, i: usize) -> (usize, usize, usize) {
        let x = i % self.nx;
        let z = (i / self.nx) % self.nz;
        let y = i / (self.nx * self.nz);
        (x, y, z)
    }
    fn neighbors(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let (x, y, z) = self.coords(i);
        let (nx, ny, nz) = (self.nx, self.ny, self.nz);
        let mut v = [usize::MAX; 6];
        if x > 0 {
            v[0] = i - 1;
        }
        if x + 1 < nx {
            v[1] = i + 1;
        }
        if z > 0 {
            v[2] = i - nx;
        }
        if z + 1 < nz {
            v[3] = i + nx;
        }
        if y > 0 {
            v[4] = i - nx * nz;
        }
        if y + 1 < ny {
            v[5] = i + nx * nz;
        }
        v.into_iter().filter(|&j| j != usize::MAX)
    }
}

/// Fallback shapes for blocks drawn without a model (chests, beds, heads…)
fn special_boxes(name: &str) -> Option<Vec<([f32; 3], [f32; 3])>> {
    let n = name.trim_start_matches("minecraft:");
    let b = |a: [f32; 3], c: [f32; 3]| (a.map(|v| v / 16.0), c.map(|v| v / 16.0));
    Some(if n.ends_with("chest") {
        vec![b([1., 0., 1.], [15., 14., 15.])]
    } else if n.ends_with("_bed") {
        vec![b([0., 3., 0.], [16., 9., 16.])]
    } else if n.ends_with("shulker_box") {
        vec![b([0., 0., 0.], [16., 16., 16.])]
    } else if n.ends_with("_skull") || n.ends_with("_head") {
        if n.contains("wall") { vec![b([4., 4., 8.], [12., 12., 16.])] } else { vec![b([4., 0., 4.], [12., 8., 12.])] }
    } else if n == "decorated_pot" {
        vec![b([1., 0., 1.], [15., 16., 15.])]
    } else if n == "conduit" {
        vec![b([5., 5., 5.], [11., 11., 11.])]
    } else if n == "bell" {
        vec![b([4., 4., 4.], [12., 13., 12.])]
    } else {
        return None;
    })
}

/// Occupancy of one block at resolution r (r³ cells, index (y*r+z)*r+x)
fn block_mask(reg: &Registry, id: u32, r: usize) -> Vec<bool> {
    let g = reg.geom(id);
    if g.occludes.iter().all(|o| *o) {
        return vec![true; r * r * r];
    }
    let boxes = if g.has_model {
        reg.state_boxes(id)
    } else {
        match special_boxes(&g.name) {
            Some(b) => b,
            None => return vec![false; r * r * r],
        }
    };
    let s = 1.0 / r as f32;
    let mut m = vec![false; r * r * r];
    for y in 0..r {
        for z in 0..r {
            for x in 0..r {
                let c0 = [x as f32 * s, y as f32 * s, z as f32 * s];
                let mut vol = 0.0;
                for (lo, hi) in &boxes {
                    let mut v = 1.0;
                    for a in 0..3 {
                        let o = (hi[a].min(c0[a] + s) - lo[a].max(c0[a])).max(0.0);
                        v *= o;
                    }
                    vol += v;
                }
                m[(y * r + z) * r + x] = vol >= 0.5 * s * s * s;
            }
        }
    }
    m
}

pub fn export_print(reg: &Registry, source: &Source, sel: Selection, opts: &ExportOptions, path: &Path, prog: &Progress) -> Result<ExportSummary> {
    let sel = sel.normalized();
    let r = opts.print_detail.clamp(1, 4) as usize;
    let (sx, sy, sz) = ((sel.x1 - sel.x0 + 1) as usize, (sel.y1 - sel.y0 + 1) as usize, (sel.z1 - sel.z0 + 1) as usize);
    let (nx, ny, nz) = (sx * r, sy * r, sz * r);
    if nx * ny * nz > MAX_CELLS {
        bail!("{}", crate::i18n::t("Selection too large for 3D printing at this detail level: reduce the area or the detail."));
    }
    prog.set_stage(&crate::i18n::t("Reading chunks"), 1);
    let chunks = source.chunks(sel.x0 >> 4, sel.z0 >> 4, sel.x1 >> 4, sel.z1 >> 4, reg);
    let excluded: HashSet<&str> = opts.excluded.iter().map(|s| s.as_str()).collect();

    // voxelization
    prog.set_stage(&crate::i18n::t("Voxelizing"), sy);
    let mut grid = Grid { nx, ny, nz, c: vec![0u16; nx * ny * nz] };
    let mut masks: HashMap<u32, Option<Vec<bool>>> = HashMap::new();
    let mut colors: Vec<[u8; 3]> = Vec::new();
    let mut color_of: HashMap<(u32, u16), u16> = HashMap::new();
    let mut color_ids: HashMap<[u8; 3], u16> = HashMap::new();
    for y in sel.y0..=sel.y1 {
        if prog.cancel.load(Ordering::Relaxed) {
            bail!("{}", crate::i18n::t("Export cancelled"));
        }
        for z in sel.z0..=sel.z1 {
            for x in sel.x0..=sel.x1 {
                let Some(ch) = chunks.get(&(x >> 4, z >> 4)) else { continue };
                let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
                let id = ch.block(lx, y, lz);
                if id == 0 {
                    continue;
                }
                let mask = masks.entry(id).or_insert_with(|| {
                    let g = reg.geom(id);
                    let skip = g.air || excluded.contains(g.name.as_str()) || (g.unknown && !opts.render_unknown) || g.fluid != Fluid::None;
                    if skip { None } else { Some(block_mask(reg, id, r)) }
                });
                let Some(mask) = mask else { continue };
                let biome = if opts.render_biomes { ch.biome(lx, y, lz) } else { 0 };
                let ci = *color_of.entry((id, biome)).or_insert_with(|| {
                    let g = reg.geom(id);
                    let mut col = [128u8, 128, 128];
                    if let Some(t) = g.map_tex.and_then(|t| reg.assets.texture(&reg.tex_name(t))) {
                        col = [t.avg[0], t.avg[1], t.avg[2]].map(|c| c.clamp(0.0, 255.0) as u8);
                        if let Some(tc) = g.map_tint.color(&reg.biome_colors(biome)) {
                            col = [0, 1, 2].map(|i| (col[i] as u32 * tc[i] as u32 / 255) as u8);
                        }
                    }
                    let n = color_ids.len();
                    *color_ids.entry(col).or_insert_with(|| {
                        colors.push(col);
                        (n + 1) as u16
                    })
                });
                let (bx, by, bz) = ((x - sel.x0) as usize * r, (y - sel.y0) as usize * r, (z - sel.z0) as usize * r);
                for j in 0..r {
                    for k in 0..r {
                        for i in 0..r {
                            if mask[(j * r + k) * r + i] {
                                let gi = grid.idx(bx + i, by + j, bz + k);
                                grid.c[gi] = ci;
                            }
                        }
                    }
                }
            }
        }
        prog.done.fetch_add(1, Ordering::Relaxed);
    }

    // small floating pieces
    if opts.print_min_piece > 0 {
        prog.set_stage(&crate::i18n::t("Removing floating pieces"), 1);
        let min_cells = opts.print_min_piece as usize * r * r * r;
        let mut seen = vec![false; grid.c.len()];
        let mut stack = Vec::new();
        let mut piece = Vec::new();
        for start in 0..grid.c.len() {
            if grid.c[start] == 0 || seen[start] {
                continue;
            }
            piece.clear();
            seen[start] = true;
            stack.push(start);
            while let Some(i) = stack.pop() {
                piece.push(i);
                for j in grid.neighbors(i) {
                    if grid.c[j] != 0 && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
            if piece.len() < min_cells {
                for &i in &piece {
                    grid.c[i] = 0;
                }
            }
        }
    }

    prog.set_stage(&crate::i18n::t("Making the model printable"), 1);
    fix_contacts(&mut grid);
    let outside = fill_enclosed(&mut grid);
    if opts.print_hollow > 0 {
        hollow(&mut grid, &outside, opts.print_hollow as usize * r);
        fix_contacts(&mut grid);
    }

    // surface
    prog.set_stage(&crate::i18n::t("Writing geometry"), 1);
    let cell = opts.print_block_mm.max(0.001) / r as f32;
    let mut verts: Vec<[f32; 3]> = Vec::new();
    let mut vmap: HashMap<(u32, u32, u32), u32> = HashMap::new();
    let mut tris: Vec<([u32; 3], u16)> = Vec::new();
    // Minecraft (x, y, z) → printer (x, -z, y), right-handed with Z up
    let mut vert = |x: usize, y: usize, z: usize| -> u32 {
        let k = (x as u32, y as u32, z as u32);
        let n = verts.len() as u32;
        *vmap.entry(k).or_insert_with(|| {
            verts.push([x as f32 * cell, (nz - z) as f32 * cell, y as f32 * cell]);
            n
        })
    };
    for y in 0..ny {
        for z in 0..nz {
            for x in 0..nx {
                let c = grid.c[grid.idx(x, y, z)];
                if c == 0 {
                    continue;
                }
                let (xi, yi, zi) = (x as isize, y as isize, z as isize);
                // faces: (neighbour offset, 4 corners in counter-clockwise order seen from outside)
                let faces: [([isize; 3], [[usize; 3]; 4]); 6] = [
                    ([0, 1, 0], [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]]),
                    ([0, -1, 0], [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]]),
                    ([1, 0, 0], [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]]),
                    ([-1, 0, 0], [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]]),
                    ([0, 0, 1], [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]]),
                    ([0, 0, -1], [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]]),
                ];
                for (d, corners) in faces {
                    if grid.get(xi + d[0], yi + d[1], zi + d[2]) != 0 {
                        continue;
                    }
                    let v: Vec<u32> = corners.iter().map(|c| vert(x + c[0], y + c[1], z + c[2])).collect();
                    tris.push(([v[0], v[1], v[2]], c));
                    tris.push(([v[0], v[2], v[3]], c));
                }
            }
        }
    }
    if tris.is_empty() {
        bail!("{}", crate::i18n::t("Nothing to print in this selection."));
    }
    // the winding above is counter-clockwise in Minecraft space; the axis swap keeps it
    // (determinant +1), so normals point outwards in printer space too

    let bytes = match opts.format {
        ExportFormat::ThreeMf => write_3mf(path, &verts, &tris, &colors)?,
        _ => write_stl(path, &verts, &tris)?,
    };
    Ok(ExportSummary { quads: tris.len(), vertices: verts.len(), materials: colors.len(), obj_bytes: bytes })
}

/// Joins cells that touch only by an edge or a corner (non-manifold for slicers)
fn fix_contacts(g: &mut Grid) {
    for _pass in 0..6 {
        let mut changed = false;
        let (nx, ny, nz) = (g.nx as isize, g.ny as isize, g.nz as isize);
        for y in -1..ny {
            for z in -1..nz {
                for x in -1..nx {
                    // the 2×2×2 window starting at (x, y, z)
                    let at = |g: &Grid, i: isize, j: isize, k: isize| g.get(x + i, y + j, z + k);
                    let mut set = |g: &mut Grid, i: isize, j: isize, k: isize, c: u16| {
                        let (a, b, d) = (x + i, y + j, z + k);
                        if a >= 0 && b >= 0 && d >= 0 && a < nx && b < ny && d < nz {
                            let id = g.idx(a as usize, b as usize, d as usize);
                            g.c[id] = c;
                        }
                    };
                    let mut filled = 0;
                    for k in 0..8 {
                        if at(g, k & 1, (k >> 1) & 1, (k >> 2) & 1) != 0 {
                            filled += 1;
                        }
                    }
                    if !(2..=6).contains(&filled) {
                        continue;
                    }
                    // edge contacts in the three planes of the window
                    for (pa, pb, fixed) in [(0usize, 2usize, 1usize), (0, 1, 2), (1, 2, 0)] {
                        for f in 0..2isize {
                            let cell = |u: isize, v: isize| {
                                let mut c = [0isize; 3];
                                c[pa] = u;
                                c[pb] = v;
                                c[fixed] = f;
                                c
                            };
                            let q = [cell(0, 0), cell(1, 1), cell(1, 0), cell(0, 1)];
                            let v: Vec<u16> = q.iter().map(|c| at(g, c[0], c[1], c[2])).collect();
                            if v[0] != 0 && v[1] != 0 && v[2] == 0 && v[3] == 0 {
                                set(g, q[2][0], q[2][1], q[2][2], v[0]);
                                changed = true;
                            } else if v[2] != 0 && v[3] != 0 && v[0] == 0 && v[1] == 0 {
                                set(g, q[0][0], q[0][1], q[0][2], v[2]);
                                changed = true;
                            }
                        }
                    }
                    // corner contacts: exactly two opposite corners filled
                    if filled == 2 {
                        for k in 0..4isize {
                            let a = [k & 1, (k >> 1) & 1, 0];
                            let b = [1 - a[0], 1 - a[1], 1];
                            let ca = at(g, a[0], a[1], a[2]);
                            if ca != 0 && at(g, b[0], b[1], b[2]) != 0 {
                                set(g, b[0], b[1], a[2], ca);
                                set(g, a[0], a[1], b[2], ca);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// Fills air pockets that cannot be reached from outside; returns the outside mask
fn fill_enclosed(g: &mut Grid) -> Vec<bool> {
    let mut outside = vec![false; g.c.len()];
    let mut stack = Vec::new();
    for y in 0..g.ny {
        for z in 0..g.nz {
            for x in 0..g.nx {
                let edge = x == 0 || y == 0 || z == 0 || x + 1 == g.nx || y + 1 == g.ny || z + 1 == g.nz;
                let i = g.idx(x, y, z);
                if edge && g.c[i] == 0 && !outside[i] {
                    outside[i] = true;
                    stack.push(i);
                }
            }
        }
    }
    while let Some(i) = stack.pop() {
        for j in g.neighbors(i) {
            if g.c[j] == 0 && !outside[j] {
                outside[j] = true;
                stack.push(j);
            }
        }
    }
    for i in 0..g.c.len() {
        if g.c[i] == 0 && !outside[i] {
            let c = g.neighbors(i).map(|j| g.c[j]).find(|&c| c != 0).unwrap_or(1);
            g.c[i] = c;
        }
    }
    outside
}

/// Keeps a shell of `t` cells under the outer surface
fn hollow(g: &mut Grid, outside: &[bool], t: usize) {
    let mut dist = vec![0u16; g.c.len()];
    let mut queue = std::collections::VecDeque::new();
    for i in 0..g.c.len() {
        if g.c[i] == 0 {
            continue;
        }
        let (x, y, z) = g.coords(i);
        let on_border = x == 0 || y == 0 || z == 0 || x + 1 == g.nx || y + 1 == g.ny || z + 1 == g.nz;
        if on_border || g.neighbors(i).any(|j| outside[j]) {
            dist[i] = 1;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let d = dist[i];
        if d as usize > t {
            continue;
        }
        let next: Vec<usize> = g.neighbors(i).filter(|&j| g.c[j] != 0 && dist[j] == 0).collect();
        for j in next {
            dist[j] = d + 1;
            queue.push_back(j);
        }
    }
    for i in 0..g.c.len() {
        if g.c[i] != 0 && (dist[i] == 0 || dist[i] as usize > t) {
            g.c[i] = 0;
        }
    }
}

fn normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-12);
    n.map(|x| x / l)
}

fn write_stl(path: &Path, verts: &[[f32; 3]], tris: &[([u32; 3], u16)]) -> Result<u64> {
    let mut out = Vec::with_capacity(84 + tris.len() * 50);
    let mut header = b"MC Exporter - binary STL, millimetres".to_vec();
    header.resize(80, b' ');
    out.extend_from_slice(&header);
    out.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    for (t, _) in tris {
        let (a, b, c) = (verts[t[0] as usize], verts[t[1] as usize], verts[t[2] as usize]);
        for v in [normal(a, b, c), a, b, c] {
            for x in v {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    std::fs::write(path, &out).with_context(|| format!("creating {}", path.display()))?;
    Ok(out.len() as u64)
}

fn write_3mf(path: &Path, verts: &[[f32; 3]], tris: &[([u32; 3], u16)], colors: &[[u8; 3]]) -> Result<u64> {
    use std::fmt::Write as _;
    let mut model = String::with_capacity(verts.len() * 48 + tris.len() * 48);
    model.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
 <metadata name="Application">MC Exporter</metadata>
 <resources>
  <basematerials id="1">
"#);
    for (i, c) in colors.iter().enumerate() {
        let _ = writeln!(model, "   <base name=\"color{i}\" displaycolor=\"#{:02X}{:02X}{:02X}\"/>", c[0], c[1], c[2]);
    }
    model.push_str("  </basematerials>\n  <object id=\"2\" type=\"model\" pid=\"1\" pindex=\"0\">\n   <mesh>\n    <vertices>\n");
    for v in verts {
        let _ = writeln!(model, "     <vertex x=\"{}\" y=\"{}\" z=\"{}\"/>", v[0], v[1], v[2]);
    }
    model.push_str("    </vertices>\n    <triangles>\n");
    for (t, c) in tris {
        let _ = writeln!(model, "     <triangle v1=\"{}\" v2=\"{}\" v3=\"{}\" pid=\"1\" p1=\"{}\"/>", t[0], t[1], t[2], c.saturating_sub(1));
    }
    model.push_str("    </triangles>\n   </mesh>\n  </object>\n </resources>\n <build>\n  <item objectid=\"2\"/>\n </build>\n</model>\n");

    let f = std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut z = zip::ZipWriter::new(f);
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file("[Content_Types].xml", o)?;
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/></Types>"#)?;
    z.start_file("_rels/.rels", o)?;
    z.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#)?;
    z.start_file("3D/3dmodel.model", o)?;
    z.write_all(model.as_bytes())?;
    z.finish()?;
    Ok(std::fs::metadata(path).map(|m| m.len()).unwrap_or(0))
}
