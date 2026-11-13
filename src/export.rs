//! OBJ + MTL + PNG texture writer.

use crate::mesh::{ExportOptions, MatKey, Mesh, OffsetMode, Progress};
use crate::registry::{Registry, UNKNOWN_TEX};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

pub struct ExportSummary {
    pub quads: usize,
    pub vertices: usize,
    pub materials: usize,
    pub obj_bytes: u64,
}

fn fmt(v: f32) -> String {
    let r = (v * 100000.0).round() / 100000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

fn tex_short(name: &str) -> String {
    let (ns, p) = name.split_once(':').unwrap_or(("minecraft", name));
    let p = p.strip_prefix("block/").unwrap_or(p).replace('/', "_");
    if ns == "minecraft" { p } else { format!("{}_{}", ns.replace(':', "_"), p) }
}

struct TexImage {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    has_alpha: bool,
}

fn unknown_tex() -> TexImage {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            let on = ((x / 4) + (y / 4)) % 2 == 0;
            rgba.extend_from_slice(if on { &[255, 0, 220, 255] } else { &[20, 20, 20, 255] });
        }
    }
    TexImage { w: 16, h: 16, rgba, has_alpha: false }
}

pub fn write_obj(reg: &Registry, mesh: &Mesh, opts: &ExportOptions, obj_path: &Path, world_name: &str, prog: &Progress) -> Result<ExportSummary> {
    let dir = obj_path.parent().unwrap_or(Path::new("."));
    let stem = obj_path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("export".into());
    let tex_dir_name = format!("{stem}_tex");
    let tex_dir = dir.join(&tex_dir_name);

    // used materials and their names
    let mut mats: Vec<MatKey> = mesh.quads.iter().map(|q| q.mat).collect();
    mats.sort();
    mats.dedup();
    let mut per_tex: HashMap<u32, usize> = HashMap::new();
    for m in &mats {
        *per_tex.entry(m.tex).or_default() += 1;
    }
    let mut names: HashMap<MatKey, String> = HashMap::new();
    for m in &mats {
        let base = tex_short(&reg.tex_name(m.tex));
        let name = match m.tint {
            Some(c) if per_tex[&m.tex] > 1 => format!("{base}_{:02x}{:02x}{:02x}", c[0], c[1], c[2]),
            _ => base,
        };
        names.insert(*m, name);
    }

    // textures
    prog.set_stage("Écriture des textures", mats.len());
    let mut has_alpha: HashMap<MatKey, bool> = HashMap::new();
    let mut avg: HashMap<MatKey, [f32; 3]> = HashMap::new();
    if opts.export_textures {
        fs::create_dir_all(&tex_dir).with_context(|| format!("création de {}", tex_dir.display()))?;
    }
    for m in &mats {
        let tname = reg.tex_name(m.tex);
        let mut img = if tname == UNKNOWN_TEX {
            unknown_tex()
        } else {
            match reg.assets.texture(&tname) {
                Some(t) => TexImage { w: t.w, h: t.h, rgba: t.rgba.clone(), has_alpha: t.has_alpha },
                None => unknown_tex(),
            }
        };
        if let Some(c) = m.tint {
            for p in img.rgba.chunks_exact_mut(4) {
                p[0] = (p[0] as u32 * c[0] as u32 / 255) as u8;
                p[1] = (p[1] as u32 * c[1] as u32 / 255) as u8;
                p[2] = (p[2] as u32 * c[2] as u32 / 255) as u8;
            }
        }
        let (mut s, mut n) = ([0f32; 3], 0f32);
        for p in img.rgba.chunks_exact(4) {
            if p[3] > 0 {
                s[0] += p[0] as f32;
                s[1] += p[1] as f32;
                s[2] += p[2] as f32;
                n += 1.0;
            }
        }
        avg.insert(*m, if n > 0.0 { [s[0] / n / 255.0, s[1] / n / 255.0, s[2] / n / 255.0] } else { [1.0; 3] });
        has_alpha.insert(*m, img.has_alpha);
        if opts.export_textures {
            let f = opts.prescale.max(1);
            if f > 1 {
                let (w2, h2) = (img.w * f, img.h * f);
                let mut big = vec![0u8; (w2 * h2 * 4) as usize];
                for y in 0..h2 {
                    for x in 0..w2 {
                        let si = (((y / f) * img.w + x / f) * 4) as usize;
                        let di = ((y * w2 + x) * 4) as usize;
                        big[di..di + 4].copy_from_slice(&img.rgba[si..si + 4]);
                    }
                }
                img = TexImage { w: w2, h: h2, rgba: big, has_alpha: img.has_alpha };
            }
            let name = &names[m];
            image::save_buffer(tex_dir.join(format!("{name}.png")), &img.rgba, img.w, img.h, image::ColorType::Rgba8)?;
            if opts.alpha_separate && img.has_alpha {
                let a: Vec<u8> = img.rgba.chunks_exact(4).map(|p| p[3]).collect();
                image::save_buffer(tex_dir.join(format!("{name}_a.png")), &a, img.w, img.h, image::ColorType::L8)?;
            }
        }
        prog.done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    // MTL
    let mtl_name = format!("{stem}.mtl");
    let mut mtl = String::new();
    let _ = writeln!(mtl, "# Exporté par MC Exporter\n");
    for m in &mats {
        let name = &names[m];
        let a = avg[m];
        let _ = writeln!(mtl, "newmtl {name}");
        if opts.export_textures {
            let _ = writeln!(mtl, "Kd 1.000 1.000 1.000");
        } else {
            let _ = writeln!(mtl, "Kd {:.3} {:.3} {:.3}", a[0], a[1], a[2]);
        }
        let _ = writeln!(mtl, "Ka 0.000 0.000 0.000\nKs 0.000 0.000 0.000\nNs 0\nillum 1\nd 1.0");
        if opts.export_textures {
            let _ = writeln!(mtl, "map_Kd {tex_dir_name}/{name}.png");
            if has_alpha[m] {
                if opts.alpha_separate {
                    let _ = writeln!(mtl, "map_d {tex_dir_name}/{name}_a.png");
                } else {
                    let _ = writeln!(mtl, "map_d {tex_dir_name}/{name}.png");
                }
            }
        }
        let _ = writeln!(mtl);
    }
    fs::write(dir.join(&mtl_name), mtl)?;

    // OBJ
    let sel = mesh.sel;
    let (ox, oz) = match opts.offset_mode {
        OffsetMode::None => (0.0, 0.0),
        OffsetMode::Center => ((sel.x0 + sel.x1 + 1) as f32 / 2.0, (sel.z0 + sel.z1 + 1) as f32 / 2.0),
        OffsetMode::Custom => (opts.offset_x as f32, opts.offset_z as f32),
    };
    let shift = [sel.x0 as f32 - ox, 0.0, sel.z0 as f32 - oz];
    let scale = if opts.scale > 0.0 { opts.scale } else { 1.0 };

    let mut order: Vec<usize> = (0..mesh.quads.len()).collect();
    order.sort_by_key(|&i| {
        let q = &mesh.quads[i];
        if opts.obj_per_chunk { (q.group, names[&q.mat].clone()) } else { ((0, 0), names[&q.mat].clone()) }
    });

    prog.set_stage("Écriture de l'OBJ", order.len());
    let f = fs::File::create(obj_path).with_context(|| format!("création de {}", obj_path.display()))?;
    let mut w = BufWriter::with_capacity(1 << 20, f);
    writeln!(w, "# Exporté par MC Exporter")?;
    writeln!(w, "# Monde : {world_name}")?;
    writeln!(w, "# Sélection : x {}..{}  y {}..{}  z {}..{}", sel.x0, sel.x1, sel.y0, sel.y1, sel.z0, sel.z1)?;
    writeln!(w, "mtllib {mtl_name}")?;

    let mut vmap: HashMap<[i32; 3], usize> = HashMap::new();
    let mut tmap: HashMap<[i32; 2], usize> = HashMap::new();
    let mut nmap: HashMap<[i32; 3], usize> = HashMap::new();
    let mut body = String::with_capacity(1 << 20);
    let mut cur_obj: Option<String> = None;
    let mut cur_mat: Option<String> = None;
    let kw = if opts.use_groups { "g" } else { "o" };
    for (n, &i) in order.iter().enumerate() {
        let q = &mesh.quads[i];
        let mname = &names[&q.mat];
        let obj = match (opts.obj_per_chunk, opts.obj_per_material) {
            (true, true) => format!("chunk_{}_{}_{}", q.group.0, q.group.1, mname),
            (true, false) => format!("chunk_{}_{}", q.group.0, q.group.1),
            (false, true) => mname.clone(),
            (false, false) => "minecraft".to_string(),
        };
        if cur_obj.as_ref() != Some(&obj) {
            let _ = writeln!(body, "{kw} {obj}");
            cur_obj = Some(obj);
            cur_mat = None;
        }
        if cur_mat.as_ref() != Some(mname) {
            let _ = writeln!(body, "usemtl {mname}");
            cur_mat = Some(mname.clone());
        }
        let p = q.pos;
        let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
        let e2 = [p[3][0] - p[0][0], p[3][1] - p[0][1], p[3][2] - p[0][2]];
        let mut nv = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        let len = (nv[0] * nv[0] + nv[1] * nv[1] + nv[2] * nv[2]).sqrt();
        if len > 0.0 {
            nv = nv.map(|c| c / len);
        }
        let nk = nv.map(|c| (c * 1000.0).round() as i32);
        let nl = nmap.len() + 1;
        let ni = *nmap.entry(nk).or_insert_with(|| {
            let _ = writeln!(w, "vn {} {} {}", fmt(nv[0]), fmt(nv[1]), fmt(nv[2]));
            nl
        });
        let _ = write!(body, "f");
        for k in 0..4 {
            let wp = [(p[k][0] + shift[0]) * scale, (p[k][1] + shift[1]) * scale, (p[k][2] + shift[2]) * scale];
            let vk = wp.map(|c| (c * 65536.0).round() as i32);
            let vl = vmap.len() + 1;
            let vi = *vmap.entry(vk).or_insert_with(|| {
                let _ = writeln!(w, "v {} {} {}", fmt(wp[0]), fmt(wp[1]), fmt(wp[2]));
                vl
            });
            let uv = q.uv[k];
            let tk = [(uv[0] * 65536.0).round() as i32, (uv[1] * 65536.0).round() as i32];
            let tl = tmap.len() + 1;
            let ti = *tmap.entry(tk).or_insert_with(|| {
                let _ = writeln!(w, "vt {} {}", fmt(uv[0]), fmt(uv[1]));
                tl
            });
            let _ = write!(body, " {vi}/{ti}/{ni}");
        }
        let _ = writeln!(body);
        if n % 4096 == 0 {
            prog.done.store(n, std::sync::atomic::Ordering::Relaxed);
        }
    }
    w.write_all(body.as_bytes())?;
    w.flush()?;
    drop(w);
    let obj_bytes = fs::metadata(obj_path).map(|m| m.len()).unwrap_or(0);
    Ok(ExportSummary { quads: mesh.quads.len(), vertices: vmap.len(), materials: mats.len(), obj_bytes })
}
