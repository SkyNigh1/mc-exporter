//! Texture atlas: every material of a mesh is packed into three images (solid, cut-out,
//! translucent), so the export has three materials instead of hundreds.
//! PBR maps (`_n`, `_s`) are packed with the same layout.

use crate::export::{TexImage, material_image};
use crate::mesh::{MatKey, Mesh};
use crate::registry::Registry;
use std::collections::HashMap;

const PAD: u32 = 2;

struct Placed {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Shelf packing of (w, h) boxes; returns positions and the atlas size
fn pack(sizes: &[(u32, u32)]) -> (Vec<(u32, u32)>, u32, u32) {
    let area: u64 = sizes.iter().map(|(w, h)| ((w + 2 * PAD) * (h + 2 * PAD)) as u64).sum();
    let widest = sizes.iter().map(|(w, _)| w + 2 * PAD).max().unwrap_or(1);
    let mut width = ((area as f64).sqrt() * 1.1) as u32;
    width = width.max(widest).next_power_of_two();
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(sizes[i].1));
    let mut pos = vec![(0, 0); sizes.len()];
    let (mut x, mut y, mut shelf) = (0u32, 0u32, 0u32);
    for i in order {
        let (w, h) = (sizes[i].0 + 2 * PAD, sizes[i].1 + 2 * PAD);
        if x + w > width {
            x = 0;
            y += shelf;
            shelf = 0;
        }
        pos[i] = (x + PAD, y + PAD);
        x += w;
        shelf = shelf.max(h);
    }
    (pos, width, y + shelf)
}

fn resized(img: &TexImage, w: u32, h: u32) -> Vec<u8> {
    if img.w == w && img.h == h {
        return img.rgba.clone();
    }
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let si = (((y * img.h / h) * img.w + x * img.w / w) * 4) as usize;
            let di = ((y * w + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&img.rgba[si..si + 4]);
        }
    }
    out
}

/// Copies an image into the atlas, extruding its border into the padding
fn blit(atlas: &mut [u8], aw: u32, p: &Placed, src: &[u8]) {
    for y in 0..p.h + 2 * PAD {
        for x in 0..p.w + 2 * PAD {
            let sx = (x as i64 - PAD as i64).clamp(0, p.w as i64 - 1) as u32;
            let sy = (y as i64 - PAD as i64).clamp(0, p.h as i64 - 1) as u32;
            let si = ((sy * p.w + sx) * 4) as usize;
            let (dx, dy) = (p.x + x - PAD, p.y + y - PAD);
            let di = ((dy * aw + dx) * 4) as usize;
            atlas[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
}

/// Raw LabPBR map of a material texture (`_n` or `_s`), if a pack provides it
fn raw_map(reg: &Registry, m: &MatKey, suffix: &str) -> Option<TexImage> {
    let name = reg.tex_name(m.tex);
    let t = reg.assets.texture(&format!("{name}{suffix}"))?;
    Some(TexImage { w: t.w, h: t.h, rgba: t.rgba.clone(), has_alpha: t.has_alpha, translucent: t.translucent })
}

pub fn apply(reg: &Registry, mesh: &mut Mesh, pbr: bool) {
    let mut mats: Vec<MatKey> = mesh.quads.iter().map(|q| q.mat).collect();
    mats.sort();
    mats.dedup();
    let images: Vec<TexImage> = mats.iter().map(|m| material_image(reg, m)).collect();
    let class_of = |img: &TexImage| if img.translucent { 2 } else if img.has_alpha { 1 } else { 0 };
    let mut remap: HashMap<MatKey, (MatKey, Placed, u32, u32)> = HashMap::new();
    for (class, cname) in ["solid", "cutout", "translucent"].iter().enumerate() {
        let members: Vec<usize> = (0..mats.len()).filter(|&i| class_of(&images[i]) == class).collect();
        if members.is_empty() {
            continue;
        }
        let sizes: Vec<(u32, u32)> = members.iter().map(|&i| (images[i].w, images[i].h)).collect();
        let (pos, aw, ah) = pack(&sizes);
        // solid atlas: opaque background so that it is not taken for a cut-out texture
        let bg: [u8; 4] = if class == 0 { [0, 0, 0, 255] } else { [0, 0, 0, 0] };
        let mut atlas: Vec<u8> = bg.iter().copied().cycle().take((aw * ah * 4) as usize).collect();
        let name = format!("gen:atlas_{cname}");
        let key = MatKey { tex: reg.tex_id(&name), tint: None };
        let mut placed = Vec::new();
        for (k, &i) in members.iter().enumerate() {
            let p = Placed { x: pos[k].0, y: pos[k].1, w: images[i].w, h: images[i].h };
            blit(&mut atlas, aw, &p, &images[i].rgba);
            placed.push(p);
        }
        // PBR maps with the same layout (neutral values where a texture has none)
        if pbr {
            for (suffix, neutral) in [("_n", [128u8, 128, 255, 255]), ("_s", [0u8, 0, 0, 255])] {
                let maps: Vec<Option<TexImage>> = members.iter().map(|&i| raw_map(reg, &mats[i], suffix)).collect();
                if maps.iter().all(|m| m.is_none()) {
                    continue;
                }
                let mut a: Vec<u8> = neutral.iter().copied().cycle().take((aw * ah * 4) as usize).collect();
                for (k, m) in maps.iter().enumerate() {
                    let p = &placed[k];
                    let src = match m {
                        Some(img) => resized(img, p.w, p.h),
                        None => neutral.iter().copied().cycle().take((p.w * p.h * 4) as usize).collect(),
                    };
                    blit(&mut a, aw, p, &src);
                }
                reg.assets.put_texture(&format!("{name}{suffix}"), aw, ah, a);
            }
        }
        reg.assets.put_texture(&name, aw, ah, atlas);
        for (k, &i) in members.iter().enumerate() {
            let p = Placed { x: placed[k].x, y: placed[k].y, w: placed[k].w, h: placed[k].h };
            remap.insert(mats[i], (key, p, aw, ah));
        }
    }
    for q in mesh.quads.iter_mut() {
        let Some((key, p, aw, ah)) = remap.get(&q.mat) else { continue };
        for uv in q.uv.iter_mut() {
            let (u, v) = (uv[0].clamp(0.0, 1.0), uv[1].clamp(0.0, 1.0));
            // UVs follow the OBJ convention (v up); the atlas is stored top-down
            let px = p.x as f32 + u * p.w as f32;
            let py = p.y as f32 + (1.0 - v) * p.h as f32;
            *uv = [px / *aw as f32, 1.0 - py / *ah as f32];
        }
        q.mat = *key;
    }
}
