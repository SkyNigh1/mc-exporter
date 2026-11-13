//! Binary glTF 2.0 (.glb) writer: a single file with geometry and embedded PNG textures.
//! Materials use alpha masking for cut-out textures (leaves, plants, glass panes) and
//! blending for translucent ones (water, stained glass, clouds).

use crate::export::{ExportSummary, material_image, material_names, placement};
use crate::mesh::{ExportOptions, MatKey, Mesh, Progress};
use crate::registry::Registry;
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const FLOAT: u32 = 5126;
const UINT: u32 = 5125;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;

#[derive(Default)]
struct Bin {
    data: Vec<u8>,
    views: Vec<Value>,
}

impl Bin {
    /// Appends bytes as a new buffer view (4-byte aligned) and returns its index
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while self.data.len() % 4 != 0 {
            self.data.push(0);
        }
        let mut v = json!({ "buffer": 0, "byteOffset": self.data.len(), "byteLength": bytes.len() });
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.data.extend_from_slice(bytes);
        self.views.push(v);
        self.views.len() - 1
    }
}

fn f32_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn png_bytes(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out).write_image(rgba, w, h, image::ExtendedColorType::Rgba8)?;
    Ok(out)
}

pub fn write_glb(reg: &Registry, mesh: &Mesh, opts: &ExportOptions, path: &Path, world_name: &str, prog: &Progress) -> Result<ExportSummary> {
    let (shift, scale) = placement(mesh, opts);
    let mut mats: Vec<MatKey> = mesh.quads.iter().map(|q| q.mat).collect();
    mats.sort();
    mats.dedup();
    let names = material_names(reg, &mats);
    let mut bin = Bin::default();

    // materials and textures
    prog.set_stage(&crate::i18n::t("Writing textures"), mats.len());
    let mut materials = Vec::new();
    let mut textures = Vec::new();
    let mut images = Vec::new();
    let mut mat_index: HashMap<MatKey, usize> = HashMap::new();
    for m in &mats {
        let img = material_image(reg, m);
        let mut mat = json!({
            "name": names[m],
            "pbrMetallicRoughness": { "metallicFactor": 0.0, "roughnessFactor": 1.0 },
        });
        if img.translucent {
            mat["alphaMode"] = json!("BLEND");
        } else if img.has_alpha {
            mat["alphaMode"] = json!("MASK");
            mat["alphaCutoff"] = json!(0.5);
        }
        if img.has_alpha {
            mat["doubleSided"] = json!(true);
        }
        if opts.export_textures {
            let img = img.upscaled(opts.prescale.max(1));
            let png = png_bytes(&img.rgba, img.w, img.h)?;
            let view = bin.view(&png, None);
            images.push(json!({ "name": names[m], "bufferView": view, "mimeType": "image/png" }));
            textures.push(json!({ "sampler": 0, "source": images.len() - 1 }));
            mat["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": textures.len() - 1 });
        } else {
            let a = img.average();
            mat["pbrMetallicRoughness"]["baseColorFactor"] = json!([a[0], a[1], a[2], 1.0]);
        }
        mat_index.insert(*m, materials.len());
        materials.push(mat);
        prog.done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    // geometry: one node per chunk (if requested), one primitive per material
    prog.set_stage(&crate::i18n::t("Writing geometry"), mesh.quads.len());
    let mut groups: BTreeMap<(i32, i32), BTreeMap<usize, Vec<usize>>> = BTreeMap::new();
    for (i, q) in mesh.quads.iter().enumerate() {
        let g = if opts.obj_per_chunk { q.group } else { (0, 0) };
        groups.entry(g).or_default().entry(mat_index[&q.mat]).or_default().push(i);
    }
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    let mut total_vertices = 0usize;
    for (g, per_mat) in &groups {
        let mut prims = Vec::new();
        for (&mi, quads) in per_mat {
            let mut pos: Vec<f32> = Vec::new();
            let mut nrm: Vec<f32> = Vec::new();
            let mut uv: Vec<f32> = Vec::new();
            let mut idx: Vec<u32> = Vec::new();
            let mut seen: HashMap<[i32; 8], u32> = HashMap::new();
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for &qi in quads {
                let q = &mesh.quads[qi];
                let p = q.pos;
                let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
                let e2 = [p[3][0] - p[0][0], p[3][1] - p[0][1], p[3][2] - p[0][2]];
                let mut n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if len < 1e-12 {
                    continue;
                }
                n = n.map(|c| c / len);
                let mut corner = [0u32; 4];
                for k in 0..4 {
                    let wp = [(p[k][0] + shift[0]) * scale, (p[k][1] + shift[1]) * scale, (p[k][2] + shift[2]) * scale];
                    // glTF texture coordinates start at the top of the image
                    let t = [q.uv[k][0], 1.0 - q.uv[k][1]];
                    let key = [
                        (wp[0] * 4096.0).round() as i32,
                        (wp[1] * 4096.0).round() as i32,
                        (wp[2] * 4096.0).round() as i32,
                        (t[0] * 4096.0).round() as i32,
                        (t[1] * 4096.0).round() as i32,
                        (n[0] * 100.0).round() as i32,
                        (n[1] * 100.0).round() as i32,
                        (n[2] * 100.0).round() as i32,
                    ];
                    let next = (pos.len() / 3) as u32;
                    corner[k] = *seen.entry(key).or_insert_with(|| {
                        pos.extend_from_slice(&wp);
                        nrm.extend_from_slice(&n);
                        uv.extend_from_slice(&t);
                        for a in 0..3 {
                            lo[a] = lo[a].min(wp[a]);
                            hi[a] = hi[a].max(wp[a]);
                        }
                        next
                    });
                }
                idx.extend_from_slice(&[corner[0], corner[1], corner[2], corner[0], corner[2], corner[3]]);
            }
            if idx.is_empty() {
                continue;
            }
            let nv = pos.len() / 3;
            total_vertices += nv;
            let vp = bin.view(&f32_bytes(&pos), Some(ARRAY_BUFFER));
            accessors.push(json!({ "bufferView": vp, "componentType": FLOAT, "count": nv, "type": "VEC3", "min": lo, "max": hi }));
            let a_pos = accessors.len() - 1;
            let vn = bin.view(&f32_bytes(&nrm), Some(ARRAY_BUFFER));
            accessors.push(json!({ "bufferView": vn, "componentType": FLOAT, "count": nv, "type": "VEC3" }));
            let a_nrm = accessors.len() - 1;
            let vt = bin.view(&f32_bytes(&uv), Some(ARRAY_BUFFER));
            accessors.push(json!({ "bufferView": vt, "componentType": FLOAT, "count": nv, "type": "VEC2" }));
            let a_uv = accessors.len() - 1;
            let ib: Vec<u8> = idx.iter().flat_map(|x| x.to_le_bytes()).collect();
            let vi = bin.view(&ib, Some(ELEMENT_ARRAY_BUFFER));
            accessors.push(json!({ "bufferView": vi, "componentType": UINT, "count": idx.len(), "type": "SCALAR" }));
            let a_idx = accessors.len() - 1;
            prims.push(json!({
                "attributes": { "POSITION": a_pos, "NORMAL": a_nrm, "TEXCOORD_0": a_uv },
                "indices": a_idx,
                "material": mi,
            }));
            prog.done.fetch_add(quads.len(), std::sync::atomic::Ordering::Relaxed);
        }
        if prims.is_empty() {
            continue;
        }
        let name = if opts.obj_per_chunk { format!("chunk_{}_{}", g.0, g.1) } else { "minecraft".to_string() };
        meshes.push(json!({ "name": name, "primitives": prims }));
        nodes.push(json!({ "name": name, "mesh": meshes.len() - 1 }));
    }

    let sel = mesh.sel;
    let mut doc = json!({
        "asset": {
            "version": "2.0",
            "generator": "MC Exporter",
            "extras": {
                "world": world_name,
                "selection": format!("x {}..{}  y {}..{}  z {}..{}", sel.x0, sel.x1, sel.y0, sel.y1, sel.z0, sel.z1),
            }
        },
        "scene": 0,
        "scenes": [{ "name": world_name, "nodes": (0..nodes.len()).collect::<Vec<_>>() }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": bin.views,
        "buffers": [{ "byteLength": 0 }],
    });
    if !textures.is_empty() {
        // NEAREST filtering keeps the pixel-art look; REPEAT is needed by merged faces
        doc["samplers"] = json!([{ "magFilter": 9728, "minFilter": 9984, "wrapS": 10497, "wrapT": 10497 }]);
        doc["textures"] = json!(textures);
        doc["images"] = json!(images);
    }
    while bin.data.len() % 4 != 0 {
        bin.data.push(0);
    }
    doc["buffers"][0]["byteLength"] = json!(bin.data.len());
    let mut js = serde_json::to_vec(&doc)?;
    while js.len() % 4 != 0 {
        js.push(b' ');
    }

    let total = 12 + 8 + js.len() + 8 + bin.data.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(js.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin.data);
    std::fs::write(path, &out).with_context(|| format!("creating {}", path.display()))?;
    Ok(ExportSummary { quads: mesh.quads.len(), vertices: total_vertices, materials: mats.len(), obj_bytes: out.len() as u64 })
}
