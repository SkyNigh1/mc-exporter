//! Clouds, like the game's "fancy" clouds: one 12×4×12 box per opaque pixel of
//! `environment/clouds.png` (256×256, repeating), cut to the selection bounds.

use crate::mesh::{MatKey, OutQuad, Selection};
use crate::registry::Registry;

const CELL: i32 = 12;
const THICKNESS: f32 = 4.0;

/// Cloud quads over the selection, in selection-relative coordinates (like `OutQuad`).
pub fn cloud_quads(reg: &Registry, sel: Selection, height: f32, per_chunk: bool) -> Vec<OutQuad> {
    let Some(tex) = reg.assets.texture("minecraft:environment/clouds") else { return Vec::new() };
    // plain, slightly transparent white material (the game draws clouds at 80 % opacity)
    let name = "gen:cloud";
    if !reg.assets.has_texture(name) {
        let px = [255u8, 255, 255, 204];
        reg.assets.put_texture(name, 4, 4, px.iter().copied().cycle().take(4 * 4 * 4).collect());
    }
    let mat = MatKey { tex: reg.tex_id(name), tint: None };
    let is_cloud = |i: i32, j: i32| -> bool {
        let (u, v) = (i.rem_euclid(tex.w as i32) as u32, j.rem_euclid(tex.h as i32) as u32);
        tex.rgba[((v * tex.w + u) * 4 + 3) as usize] > 0
    };
    let (x0, x1, z0, z1) = (sel.x0, sel.x1 + 1, sel.z0, sel.z1 + 1);
    let (y0, y1) = (height, height + THICKNESS);
    let mut out = Vec::new();
    for i in x0.div_euclid(CELL)..=(x1 - 1).div_euclid(CELL) {
        for j in z0.div_euclid(CELL)..=(z1 - 1).div_euclid(CELL) {
            if !is_cloud(i, j) {
                continue;
            }
            let (xa, xb) = ((i * CELL).max(x0), ((i + 1) * CELL).min(x1));
            let (za, zb) = ((j * CELL).max(z0), ((j + 1) * CELL).min(z1));
            let group = if per_chunk { (((xa + xb) / 2) >> 4, ((za + zb) / 2) >> 4) } else { (0, 0) };
            let (fxa, fxb, fza, fzb) = ((xa - x0) as f32, (xb - x0) as f32, (za - z0) as f32, (zb - z0) as f32);
            let mut face = |p: [[f32; 3]; 4], n: [f32; 3]| {
                // vertices in counter-clockwise order seen from outside
                let e1 = [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]];
                let e2 = [p[3][0] - p[0][0], p[3][1] - p[0][1], p[3][2] - p[0][2]];
                let c = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let p = if c[0] * n[0] + c[1] * n[1] + c[2] * n[2] < 0.0 { [p[0], p[3], p[2], p[1]] } else { p };
                out.push(OutQuad { pos: p, uv: [[0., 0.], [1., 0.], [1., 1.], [0., 1.]], mat, group });
            };
            face([[fxa, y1, fza], [fxb, y1, fza], [fxb, y1, fzb], [fxa, y1, fzb]], [0., 1., 0.]);
            face([[fxa, y0, fza], [fxb, y0, fza], [fxb, y0, fzb], [fxa, y0, fzb]], [0., -1., 0.]);
            // sides: hidden between two touching clouds, kept at the selection edge
            if xa == x0 || !is_cloud(i - 1, j) {
                face([[fxa, y0, fza], [fxa, y0, fzb], [fxa, y1, fzb], [fxa, y1, fza]], [-1., 0., 0.]);
            }
            if xb == x1 || !is_cloud(i + 1, j) {
                face([[fxb, y0, fza], [fxb, y0, fzb], [fxb, y1, fzb], [fxb, y1, fza]], [1., 0., 0.]);
            }
            if za == z0 || !is_cloud(i, j - 1) {
                face([[fxa, y0, fza], [fxb, y0, fza], [fxb, y1, fza], [fxa, y1, fza]], [0., 0., -1.]);
            }
            if zb == z1 || !is_cloud(i, j + 1) {
                face([[fxa, y0, fzb], [fxb, y0, fzb], [fxb, y1, fzb], [fxa, y1, fzb]], [0., 0., 1.]);
            }
        }
    }
    out
}
