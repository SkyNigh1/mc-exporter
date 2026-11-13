//! More mobs: monsters, illagers, aquatic creatures, bosses… (dimensions read from
//! the game's 26.x model classes), plus dropped items, primed TNT and end crystals.

use crate::bent::{COLORS, dye_diffuse, e, skin};
use crate::mobs::{Limbs, equipment, get, held_items, int, living, string};
use crate::pose::{ALL_FACES, Pose, Skin, bx, cube};
use crate::registry::{Quad, Registry, Tint};
use fastnbt::Value;
use std::f32::consts::PI;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};

/// Lift ghasts so that their tentacles touch the ground (option)
pub static GHAST_GROUND: AtomicBool = AtomicBool::new(false);

/// Tinted layers merged into one texture (tropical fish…)
fn tinted(reg: &Registry, layers: &[(Vec<String>, Option<[u8; 3]>)]) -> Option<Skin> {
    let mut found = Vec::new();
    for (c, t) in layers {
        if let Some((id, _)) = reg.assets.first_texture(c) {
            found.push((id, *t));
        }
    }
    let (first, _) = found.first()?.clone();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    found.hash(&mut h);
    let name = format!("gen:tinted_{:016x}", h.finish());
    let base = reg.assets.texture(&first)?;
    if !reg.assets.has_texture(&name) {
        let mut img = vec![0u8; (base.w * base.h * 4) as usize];
        for (id, tint) in &found {
            let Some(t) = reg.assets.texture(id) else { continue };
            if t.w != base.w || t.h != base.h {
                continue;
            }
            for (d, s) in img.chunks_exact_mut(4).zip(t.rgba.chunks_exact(4)) {
                let a = s[3] as f32 / 255.0;
                for k in 0..3 {
                    let v = s[k] as f32 * tint.map(|c| c[k] as f32 / 255.0).unwrap_or(1.0);
                    d[k] = (v * a + d[k] as f32 * (1.0 - a)) as u8;
                }
                d[3] = d[3].max(s[3]);
            }
        }
        reg.assets.put_texture(&name, base.w, base.h, img);
    }
    Some(Skin { tex: reg.tex_id(&name), w: base.w as f32, h: base.h as f32, tint: Tint::None })
}

fn c(out: &mut Vec<Quad>, p: &Pose, sk: &Skin, uv: (f32, f32), f: [f32; 3], s: [f32; 3], g: f32) {
    cube(out, p, sk, uv.0, uv.1, f, s, g, false, ALL_FACES);
}
fn m(out: &mut Vec<Quad>, p: &Pose, sk: &Skin, uv: (f32, f32), f: [f32; 3], s: [f32; 3]) {
    cube(out, p, sk, uv.0, uv.1, f, s, 0.0, true, ALL_FACES);
}

/// Aligned segments (silverfish, endermite): sizes, UVs; returns the z position of each segment
fn segments(out: &mut Vec<Quad>, b: &Pose, sk: &Skin, sizes: &[[f32; 3]], uvs: &[(f32, f32)]) -> Vec<f32> {
    let mut z = -3.5;
    let mut zs = Vec::new();
    for i in 0..sizes.len() {
        let s = sizes[i];
        bx(out, &b.part([0., 24. - s[1], z], [0.; 3]), sk, uvs[i], [s[0] * -0.5, 0., s[2] * -0.5], s);
        zs.push(z);
        if i + 1 < sizes.len() {
            z += (s[2] + sizes[i + 1][2]) * 0.5;
        }
    }
    zs
}

fn illager_body(out: &mut Vec<Quad>, b: &Pose, sk: &Skin, crossed: bool) {
    let head = b.part([0.; 3], [0.; 3]);
    bx(out, &head, sk, (0., 0.), [-4., -10., -4.], [8., 10., 8.]);
    bx(out, &head.part([0., -2., 0.], [0.; 3]), sk, (24., 0.), [-1., -1., -6.], [2., 4., 2.]);
    bx(out, b, sk, (16., 20.), [-4., 0., -3.], [8., 12., 6.]);
    c(out, b, sk, (0., 38.), [-4., 0., -3.], [8., 20., 6.], 0.5);
    if crossed {
        let arms = b.part([0., 3., -1.], [-0.75, 0., 0.]);
        bx(out, &arms, sk, (44., 22.), [-8., -2., -2.], [4., 8., 4.]);
        bx(out, &arms, sk, (40., 38.), [-4., 2., -2.], [8., 4., 4.]);
        m(out, &arms, sk, (44., 22.), [4., -2., -2.], [4., 8., 4.]);
    } else {
        bx(out, &b.part([-5., 2., 0.], [0.; 3]), sk, (40., 46.), [-3., -2., -2.], [4., 12., 4.]);
        m(out, &b.part([5., 2., 0.], [0.; 3]), sk, (40., 46.), [-1., -2., -2.], [4., 12., 4.]);
    }
    bx(out, &b.part([-2., 12., 0.], [0.; 3]), sk, (0., 22.), [-2., 0., -2.], [4., 12., 4.]);
    m(out, &b.part([2., 12., 0.], [0.; 3]), sk, (0., 22.), [-2., 0., -2.], [4., 12., 4.]);
}

pub fn more_mob(reg: &Registry, id: &str, ent: &Value, pos: [f64; 3], yaw: f32, out: &mut Vec<Quad>) -> bool {
    let h = PI / 2.0;
    let baby = int(get(Some(ent), "Age")).unwrap_or(0) < 0 || int(get(Some(ent), "IsBaby")).unwrap_or(0) != 0;
    let sc = if baby { 0.5 } else { 1.0 };
    let tex = |c: &[&str]| skin(reg, &c.iter().map(|x| e(x)).collect::<Vec<_>>(), Tint::None);
    match id {
        "blaze" => {
            let Some(sk) = tex(&["blaze/blaze", "blaze"]) else { return true };
            let b = living(pos, yaw, 1.0);
            bx(out, &b, &sk, (0., 0.), [-4., -4., -4.], [8., 8., 8.]);
            // three rings of rods (BlazeModel.setupAnim at rest)
            let mut f: f32 = 0.0;
            for i in 0..4 {
                let y = -2.0 + ((i * 2) as f32 * 0.25).cos();
                bx(out, &b.part([f.cos() * 9., y, f.sin() * 9.], [0.; 3]), &sk, (0., 16.), [0., 0., 0.], [2., 8., 2.]);
                f += 1.0;
            }
            f = 0.7853982;
            for j in 4..8 {
                let y = 2.0 + ((j * 2) as f32 * 0.25).cos();
                bx(out, &b.part([f.cos() * 7., y, f.sin() * 7.], [0.; 3]), &sk, (0., 16.), [0., 0., 0.], [2., 8., 2.]);
                f += 1.0;
            }
            f = 0.47123894;
            for k in 8..12 {
                let y = 11.0 + ((k as f32 * 1.5) * 0.5).cos();
                bx(out, &b.part([f.cos() * 5., y, f.sin() * 5.], [0.; 3]), &sk, (0., 16.), [0., 0., 0.], [2., 8., 2.]);
                f += 1.0;
            }
        }
        "silverfish" => {
            let Some(sk) = tex(&["silverfish/silverfish", "silverfish"]) else { return true };
            let b = living(pos, yaw, 1.0);
            let sizes = [[3., 2., 2.], [4., 3., 2.], [6., 4., 3.], [3., 3., 3.], [2., 2., 3.], [2., 1., 2.], [1., 1., 2.]];
            let uvs = [(0., 0.), (0., 4.), (0., 9.), (0., 16.), (0., 22.), (11., 0.), (13., 4.)];
            let zs = segments(out, &b, &sk, &sizes, &uvs);
            bx(out, &b.part([0., 16., zs[2]], [0.; 3]), &sk, (20., 0.), [-5., 0., sizes[2][2] * -0.5], [10., 8., sizes[2][2]]);
            bx(out, &b.part([0., 20., zs[4]], [0.; 3]), &sk, (20., 11.), [-3., 0., sizes[4][2] * -0.5], [6., 4., sizes[4][2]]);
            bx(out, &b.part([0., 19., zs[1]], [0.; 3]), &sk, (20., 18.), [-3., 0., sizes[4][2] * -0.5], [6., 5., sizes[1][2]]);
        }
        "endermite" => {
            let Some(sk) = tex(&["endermite/endermite", "endermite"]) else { return true };
            let b = living(pos, yaw, 1.0);
            segments(out, &b, &sk, &[[4., 3., 2.], [6., 4., 5.], [3., 3., 1.], [1., 2., 1.]], &[(0., 0.), (0., 5.), (0., 14.), (0., 18.)]);
        }
        "magma_cube" => {
            let Some(sk) = tex(&["slime/magmacube", "magmacube"]) else { return true };
            let size = (int(get(Some(ent), "Size")).unwrap_or(0) + 1) as f32;
            let b = living(pos, yaw, 0.999 * size);
            for i in 0..8 {
                let (u, v) = match i {
                    2 => (24., 10.),
                    3 => (24., 19.),
                    _ => (0., i as f32),
                };
                bx(out, &b, &sk, (u, v), [-4., 16. + i as f32, -4.], [8., 1., 8.]);
            }
            bx(out, &b, &sk, (24., 40.), [-2., 18., -2.], [4., 4., 4.]);
        }
        "sulfur_cube" => {
            let size = (int(get(Some(ent), "Size")).unwrap_or(0) + 1) as f32;
            let b = living(pos, yaw, 0.5 * size).part([0., 15., 0.], [0.; 3]);
            if let Some(sk) = tex(&["sulfur_cube/sulfur_cube_inner"]) {
                bx(out, &b, &sk, (0., 36.), [-8., -8., -8.], [16., 16., 16.]);
            }
            if let Some(sk) = tex(&["sulfur_cube/sulfur_cube_outer"]) {
                bx(out, &b, &sk, (0., 0.), [-9., -9., -9.], [18., 18., 18.]);
            }
        }
        "vex" | "allay" => {
            let Some(sk) = tex(&[if id == "vex" { "illager/vex" } else { "allay/allay" }]) else { return true };
            let b = living(pos, yaw, 1.0);
            let (root, hy, by) = if id == "vex" { (b.part([0., -2.5, 0.], [0.; 3]), 20., 20.) } else { (b.part([0., 23.5, 0.], [0.; 3]), -3.99, -4.) };
            bx(out, &root.part([0., hy, 0.], [0.; 3]), &sk, (0., 0.), [-2.5, -5., -2.5], [5., 5., 5.]);
            let body = root.part([0., by, 0.], [0.; 3]);
            bx(out, &body, &sk, (0., 10.), [-1.5, 0., -1.], [3., 4., 2.]);
            c(out, &body, &sk, (0., 16.), [-1.5, if id == "vex" { 1. } else { 0. }, -1.], [3., 5., 2.], -0.2);
            if id == "vex" {
                c(out, &body.part([-1.75, 0.25, 0.], [0.; 3]), &sk, (23., 0.), [-1.25, -0.5, -1.], [2., 4., 2.], -0.1);
                c(out, &body.part([1.75, 0.25, 0.], [0.; 3]), &sk, (23., 6.), [-0.75, -0.5, -1.], [2., 4., 2.], -0.1);
                m(out, &body.part([0.5, 1., 1.], [0., 0.47, 0.]), &sk, (16., 14.), [0., 0., 0.], [0., 5., 8.]);
                bx(out, &body.part([-0.5, 1., 1.], [0., -0.47, 0.]), &sk, (16., 14.), [0., 0., 0.], [0., 5., 8.]);
            } else {
                c(out, &body.part([-1.75, 0.5, 0.], [0.; 3]), &sk, (23., 0.), [-0.75, -0.5, -1.], [1., 4., 2.], -0.01);
                c(out, &body.part([1.75, 0.5, 0.], [0.; 3]), &sk, (23., 6.), [-0.25, -0.5, -1.], [1., 4., 2.], -0.01);
                bx(out, &body.part([-0.5, 0., 0.6], [0., -0.47, 0.]), &sk, (16., 14.), [0., 1., 0.], [0., 5., 8.]);
                bx(out, &body.part([0.5, 0., 0.6], [0., 0.47, 0.]), &sk, (16., 14.), [0., 1., 0.], [0., 5., 8.]);
            }
        }
        "evoker" | "vindicator" | "pillager" | "illusioner" => {
            let Some(sk) = tex(&[&format!("illager/{id}")]) else { return true };
            let b = living(pos, yaw, 0.9375);
            let crossed = id != "pillager";
            illager_body(out, &b, &sk, crossed);
            crate::deco::head_item(reg, ent, &b, out);
            if !crossed {
                let l = Limbs { rarm: ([-5., 2., 0.], [0.; 3]), larm: ([5., 2., 0.], [0.; 3]), ..Limbs::humanoid() };
                held_items(reg, ent, &b, &l, false, out);
            }
        }
        "ghast" | "happy_ghast" => {
            let happy = id == "happy_ghast";
            let Some(sk) = tex(&[if happy { "ghast/happy_ghast" } else { "ghast/ghast" }]) else { return true };
            let lift = if GHAST_GROUND.load(Ordering::Relaxed) { if happy { (31.0 - 24.0) / 16.0 * 4.0 * sc } else { (38.6 - 24.0) / 16.0 * 4.5 } } else { 0.0 };
            let pos = [pos[0], pos[1] + lift as f64, pos[2]];
            let b = living(pos, yaw, if happy { 4.0 * sc } else { 4.5 });
            if happy {
                crate::deco::happy_ghast(reg, ent, &b, out);
            }
            if happy {
                let body = b.part([0., 16., 0.], [0.; 3]);
                bx(out, &body, &sk, (0., 0.), [-8., -8., -8.], [16., 16., 16.]);
                let t = [(-3.75, -5., 5.), (1.25, -5., 7.), (6.25, -5., 4.), (-6.25, 0., 5.), (-1.25, 0., 5.), (3.75, 0., 7.), (-3.75, 5., 8.), (1.25, 5., 8.), (6.25, 5., 5.)];
                for (x, z, l) in t {
                    bx(out, &body.part([x, 7., z], [0.; 3]), &sk, (0., 0.), [-1., 0., -1.], [2., l, 2.]);
                }
            } else {
                bx(out, &b.part([0., 17.6, 0.], [0.; 3]), &sk, (0., 0.), [-8., -8., -8.], [16., 16., 16.]);
                let lens = [9., 12., 10., 14., 8., 11., 13., 9., 12.];
                for i in 0..9 {
                    let a = (i % 3) as f32 - ((i / 3) % 2) as f32 * 0.5 + 0.25;
                    let (x, z) = ((a - 1.0) * 5.0, ((i / 3) as f32 - 1.0) * 5.0);
                    bx(out, &b.part([x, 24.6, z], [0.; 3]), &sk, (0., 0.), [-1., 0., -1.], [2., lens[i], 2.]);
                }
            }
        }
        "phantom" => {
            let Some(sk) = tex(&["phantom/phantom", "phantom"]) else { return true };
            let size = int(get(Some(ent), "size")).or_else(|| int(get(Some(ent), "Size"))).unwrap_or(0) as f32;
            let s = 1.0 + 0.15 * size;
            let b = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).ry(180.0 - yaw).s(-s, -s, s).t(0.0, -1.501, 0.0).t(0.0, 1.3125, 0.1875);
            let body = b.part([0.; 3], [-0.1, 0., 0.]);
            bx(out, &body, &sk, (0., 8.), [-3., -2., -8.], [5., 3., 9.]);
            let tb = body.part([0., -2., 1.], [0.; 3]);
            bx(out, &tb, &sk, (3., 20.), [-2., 0., 0.], [3., 2., 6.]);
            bx(out, &tb.part([0., 0.5, 6.], [0.; 3]), &sk, (4., 29.), [-1., 0., 0.], [1., 1., 6.]);
            let lw = body.part([2., -2., -8.], [0., 0., 0.1]);
            bx(out, &lw, &sk, (23., 12.), [0., 0., 0.], [6., 2., 9.]);
            bx(out, &lw.part([6., 0., 0.], [0., 0., 0.1]), &sk, (16., 24.), [0., 0., 0.], [13., 1., 9.]);
            let rw = body.part([-3., -2., -8.], [0., 0., -0.1]);
            m(out, &rw, &sk, (23., 12.), [-6., 0., 0.], [6., 2., 9.]);
            m(out, &rw.part([-6., 0., 0.], [0., 0., -0.1]), &sk, (16., 24.), [-13., 0., 0.], [13., 1., 9.]);
            bx(out, &body.part([0., 1., -7.], [0.2, 0., 0.]), &sk, (0., 0.), [-4., -2., -5.], [7., 3., 5.]);
        }
        "guardian" | "elder_guardian" => {
            let elder = id == "elder_guardian";
            let Some(sk) = tex(&[if elder { "guardian/guardian_elder" } else { "guardian/guardian" }]) else { return true };
            let b = living(pos, yaw, if elder { 2.35 } else { 1.0 });
            bx(out, &b, &sk, (0., 0.), [-6., 10., -8.], [12., 12., 16.]);
            bx(out, &b, &sk, (0., 28.), [-8., 10., -6.], [2., 12., 12.]);
            m(out, &b, &sk, (0., 28.), [6., 10., -6.], [2., 12., 12.]);
            bx(out, &b, &sk, (16., 40.), [-6., 8., -6.], [12., 2., 12.]);
            bx(out, &b, &sk, (16., 40.), [-6., 22., -6.], [12., 2., 12.]);
            const XR: [f32; 12] = [1.75, 0.25, 0., 0., 0.5, 0.5, 0.5, 0.5, 1.25, 0.75, 0., 0.];
            const YR: [f32; 12] = [0., 0., 0., 0., 0.25, 1.75, 1.25, 0.75, 0., 0., 0., 0.];
            const ZR: [f32; 12] = [0., 0., 0.25, 1.75, 0., 0., 0., 0., 0., 0., 0.75, 1.25];
            const SX: [f32; 12] = [0., 0., 8., -8., -8., 8., 8., -8., 0., 0., 8., -8.];
            const SY: [f32; 12] = [-8., -8., -8., -8., 0., 0., 0., 0., 8., 8., 8., 8.];
            const SZ: [f32; 12] = [8., -8., 0., 0., -8., -8., 8., 8., 8., -8., 0., 0.];
            for i in 0..12 {
                let o = 1.0 + (i as f32).cos() * 0.01;
                let p = b.part([SX[i] * o, 16.0 + SY[i] * o, SZ[i] * o], [PI * XR[i], PI * YR[i], PI * ZR[i]]);
                bx(out, &p, &sk, (0., 0.), [-1., -4.5, -1.], [2., 9., 2.]);
            }
            bx(out, &b.part([0., 0., -8.25], [0.; 3]), &sk, (8., 0.), [-1., 15., 0.], [2., 2., 1.]);
            bx(out, &b, &sk, (40., 0.), [-2., 14., 7.], [4., 4., 8.]);
            let t1 = b.part([-1.5, 0.5, 14.], [0.; 3]);
            bx(out, &t1, &sk, (0., 54.), [0., 14., 0.], [3., 3., 7.]);
            let t2 = t1.part([0.5, 0.5, 6.], [0.; 3]);
            bx(out, &t2, &sk, (41., 32.), [0., 14., 0.], [2., 2., 6.]);
            bx(out, &t2, &sk, (25., 19.), [1., 10.5, 3.], [1., 9., 9.]);
        }
        "hoglin" | "zoglin" => {
            let Some(sk) = tex(&[&format!("hoglin/{id}")]) else { return true };
            let b = living(pos, yaw, sc);
            let body = b.part([0., 7., 0.], [0.; 3]);
            bx(out, &body, &sk, (1., 1.), [-8., -7., -13.], [16., 14., 26.]);
            c(out, &body.part([0., -14., -7.], [0.; 3]), &sk, (90., 33.), [0., 0., -9.], [0., 10., 19.], 0.001);
            let head = b.part([0., 2., -12.], [0.87266463, 0., 0.]);
            bx(out, &head, &sk, (61., 1.), [-7., -3., -19.], [14., 6., 19.]);
            bx(out, &head.part([-6., -2., -3.], [0., 0., -0.6981317]), &sk, (1., 1.), [-6., -1., -2.], [6., 1., 4.]);
            bx(out, &head.part([6., -2., -3.], [0., 0., 0.6981317]), &sk, (1., 6.), [0., -1., -2.], [6., 1., 4.]);
            bx(out, &head.part([-7., 2., -12.], [0.; 3]), &sk, (10., 13.), [-1., -11., -1.], [2., 11., 2.]);
            bx(out, &head.part([7., 2., -12.], [0.; 3]), &sk, (1., 13.), [-1., -11., -1.], [2., 11., 2.]);
            bx(out, &b.part([-4., 10., -8.5], [0.; 3]), &sk, (66., 42.), [-3., 0., -3.], [6., 14., 6.]);
            bx(out, &b.part([4., 10., -8.5], [0.; 3]), &sk, (41., 42.), [-3., 0., -3.], [6., 14., 6.]);
            bx(out, &b.part([-5., 13., 10.], [0.; 3]), &sk, (21., 45.), [-2.5, 0., -2.5], [5., 11., 5.]);
            bx(out, &b.part([5., 13., 10.], [0.; 3]), &sk, (0., 45.), [-2.5, 0., -2.5], [5., 11., 5.]);
        }
        "ravager" => {
            let Some(sk) = tex(&["illager/ravager"]) else { return true };
            let b = living(pos, yaw, 1.0);
            let neck = b.part([0., -7., 5.5], [0.; 3]);
            bx(out, &neck, &sk, (68., 73.), [-5., -1., -18.], [10., 10., 18.]);
            let head = neck.part([0., 16., -17.], [0.; 3]);
            bx(out, &head, &sk, (0., 0.), [-8., -20., -14.], [16., 20., 16.]);
            bx(out, &head, &sk, (0., 0.), [-2., -6., -18.], [4., 8., 4.]);
            bx(out, &head.part([-10., -14., -8.], [1.0995574, 0., 0.]), &sk, (74., 55.), [0., -14., -2.], [2., 14., 4.]);
            m(out, &head.part([8., -14., -8.], [1.0995574, 0., 0.]), &sk, (74., 55.), [0., -14., -2.], [2., 14., 4.]);
            bx(out, &head.part([0., -2., 2.], [0.; 3]), &sk, (0., 36.), [-8., 0., -16.], [16., 3., 16.]);
            let body = b.part([0., 1., 2.], [h, 0., 0.]);
            bx(out, &body, &sk, (0., 55.), [-7., -10., -7.], [14., 16., 20.]);
            bx(out, &body, &sk, (0., 91.), [-6., 6., -7.], [12., 13., 18.]);
            bx(out, &b.part([-8., -13., 18.], [0.; 3]), &sk, (96., 0.), [-4., 0., -4.], [8., 37., 8.]);
            m(out, &b.part([8., -13., 18.], [0.; 3]), &sk, (96., 0.), [-4., 0., -4.], [8., 37., 8.]);
            bx(out, &b.part([-8., -13., -5.], [0.; 3]), &sk, (64., 0.), [-4., 0., -4.], [8., 37., 8.]);
            m(out, &b.part([8., -13., -5.], [0.; 3]), &sk, (64., 0.), [-4., 0., -4.], [8., 37., 8.]);
        }
        "strider" => {
            let Some(sk) = tex(&["strider/strider", "strider"]) else { return true };
            let b = living(pos, yaw, sc);
            bx(out, &b.part([-4., 8., 0.], [0.; 3]), &sk, (0., 32.), [-2., 0., -2.], [4., 16., 4.]);
            bx(out, &b.part([4., 8., 0.], [0.; 3]), &sk, (0., 55.), [-2., 0., -2.], [4., 16., 4.]);
            let body = b.part([0., 1., 0.], [0.; 3]);
            bx(out, &body, &sk, (0., 0.), [-8., -6., -8.], [16., 14., 16.]);
            for (o, z, v, mi) in [([-8., 4., -8.], -1.2217305, 65., true), ([-8., -1., -8.], -1.134464, 49., true), ([-8., -5., -8.], -0.87266463, 33., true)] {
                cube(out, &body.part(o, [0., 0., z]), &sk, 16., v, [-12., 0., 0.], [12., 0., 16.], 0.0, mi, ALL_FACES);
            }
            for (o, z, v) in [([8., -6., -8.], 0.87266463, 33.), ([8., -2., -8.], 1.134464, 49.), ([8., 3., -8.], 1.2217305, 65.)] {
                bx(out, &body.part(o, [0., 0., z]), &sk, (16., v), [0., 0., 0.], [12., 0., 16.]);
            }
            crate::deco::strider(reg, ent, &b, out);
        }
        "breeze" => {
            let b = living(pos, yaw, 1.0);
            if let Some(sk) = tex(&["breeze/breeze"]) {
                let rods = b.part([0., 8., 0.], [0.; 3]);
                for (o, r) in [([2.5981, -3., 1.5], [-2.7489, -1.0472, 3.1416]), ([-2.5981, -3., 1.5], [-2.7489, 1.0472, 3.1416]), ([0., -3., -3.], [0.3927, 0., 0.])] {
                    bx(out, &rods.part(o, r), &sk, (0., 17.), [-1., 0., -3.], [2., 8., 2.]);
                }
                let head = b.part([0., 4., 0.], [0.; 3]);
                bx(out, &head, &sk, (4., 24.), [-5., -5., -4.2], [10., 3., 4.]);
                bx(out, &head, &sk, (0., 0.), [-4., -8., -4.], [8., 8., 8.]);
            }
            if let Some(w) = tex(&["breeze/breeze_wind"]) {
                let bot = b.part([0., 24., 0.], [0.; 3]);
                bx(out, &bot, &w, (1., 83.), [-2.5, -7., -2.5], [5., 7., 5.]);
                let mid = bot.part([0., -7., 0.], [0.; 3]);
                bx(out, &mid, &w, (74., 28.), [-6., -6., -6.], [12., 6., 12.]);
                bx(out, &mid, &w, (78., 32.), [-4., -6., -4.], [8., 6., 8.]);
                bx(out, &mid, &w, (49., 71.), [-2.5, -6., -2.5], [5., 6., 5.]);
                let top = mid.part([0., -6., 0.], [0.; 3]);
                bx(out, &top, &w, (0., 0.), [-9., -8., -9.], [18., 8., 18.]);
                bx(out, &top, &w, (6., 6.), [-6., -8., -6.], [12., 8., 12.]);
                bx(out, &top, &w, (105., 57.), [-2.5, -8., -2.5], [5., 8., 5.]);
            }
        }
        "creaking" => {
            let Some(sk) = tex(&["creaking/creaking"]) else { return true };
            let root = living(pos, yaw, 1.0).part([0., 24., 0.], [0.; 3]);
            let up = root.part([-1., -19., 0.], [0.; 3]);
            let head = up.part([-3., -11., 0.], [0.; 3]);
            bx(out, &head, &sk, (0., 0.), [-3., -10., -3.], [6., 10., 6.]);
            bx(out, &head, &sk, (28., 31.), [-3., -13., -3.], [6., 3., 6.]);
            bx(out, &head, &sk, (12., 40.), [3., -13., 0.], [9., 14., 0.]);
            bx(out, &head, &sk, (34., 12.), [-12., -14., 0.], [9., 14., 0.]);
            let body = up.part([0., -7., 1.], [0.; 3]);
            bx(out, &body, &sk, (0., 16.), [0., -3., -3.], [6., 13., 5.]);
            bx(out, &body, &sk, (24., 0.), [-6., -4., -3.], [6., 7., 5.]);
            let ra = up.part([-7., -9.5, 1.5], [0.; 3]);
            bx(out, &ra, &sk, (22., 13.), [-2., -1.5, -1.5], [3., 21., 3.]);
            bx(out, &ra, &sk, (46., 0.), [-2., 19.5, -1.5], [3., 4., 3.]);
            let la = up.part([6., -9., 0.5], [0.; 3]);
            bx(out, &la, &sk, (30., 40.), [0., -1., -1.5], [3., 16., 3.]);
            bx(out, &la, &sk, (52., 12.), [0., -5., -1.5], [3., 4., 3.]);
            bx(out, &la, &sk, (52., 19.), [0., 15., -1.5], [3., 4., 3.]);
            let ll = root.part([1.5, -16., 0.5], [0.; 3]);
            bx(out, &ll, &sk, (42., 40.), [-1.5, 0., -1.5], [3., 16., 3.]);
            bx(out, &ll, &sk, (45., 55.), [-1.5, 15.7, -4.5], [5., 0., 9.]);
            let rl = root.part([-1., -17.5, 0.5], [0.; 3]);
            bx(out, &rl, &sk, (0., 34.), [-3., -1.5, -1.5], [3., 19., 3.]);
            bx(out, &rl, &sk, (45., 46.), [-5., 17.2, -4.5], [5., 0., 9.]);
            bx(out, &rl, &sk, (12., 34.), [-3., -4.5, -1.5], [3., 3., 3.]);
        }
        "warden" => {
            let Some(sk) = tex(&["warden/warden"]) else { return true };
            let bone = living(pos, yaw, 1.0).part([0., 24., 0.], [0.; 3]);
            let body = bone.part([0., -21., 0.], [0.; 3]);
            bx(out, &body, &sk, (0., 0.), [-9., -13., -4.], [18., 21., 11.]);
            bx(out, &body.part([-7., -2., -4.], [0.; 3]), &sk, (90., 11.), [-2., -11., -0.1], [9., 21., 0.]);
            m(out, &body.part([7., -2., -4.], [0.; 3]), &sk, (90., 11.), [-7., -11., -0.1], [9., 21., 0.]);
            let head = body.part([0., -13., 0.], [0.; 3]);
            bx(out, &head, &sk, (0., 32.), [-8., -16., -5.], [16., 16., 10.]);
            bx(out, &head.part([-8., -12., 0.], [0.; 3]), &sk, (52., 32.), [-16., -13., 0.], [16., 16., 0.]);
            bx(out, &head.part([8., -12., 0.], [0.; 3]), &sk, (58., 0.), [0., -13., 0.], [16., 16., 0.]);
            bx(out, &body.part([-13., -13., 1.], [0.; 3]), &sk, (44., 50.), [-4., 0., -4.], [8., 28., 8.]);
            bx(out, &body.part([13., -13., 1.], [0.; 3]), &sk, (0., 58.), [-4., 0., -4.], [8., 28., 8.]);
            bx(out, &bone.part([-5.9, -13., 0.], [0.; 3]), &sk, (76., 48.), [-3.1, 0., -3.], [6., 13., 6.]);
            bx(out, &bone.part([5.9, -13., 0.], [0.; 3]), &sk, (76., 76.), [-2.9, 0., -3.], [6., 13., 6.]);
        }
        "wither" => {
            let Some(sk) = tex(&["wither/wither"]) else { return true };
            let b = living(pos, yaw, 2.0);
            bx(out, &b, &sk, (0., 16.), [-10., 3.9, -0.5], [20., 3., 3.]);
            let rib = b.part([-2., 6.9, -0.5], [0.20420352, 0., 0.]);
            bx(out, &rib, &sk, (0., 22.), [0., 0., 0.], [3., 10., 3.]);
            for y in [1.5, 4.0, 6.5] {
                bx(out, &rib, &sk, (24., 22.), [-4., y, 0.5], [11., 2., 2.]);
            }
            let a = 0.20420352f32;
            bx(out, &b.part([-2., 6.9 + a.cos() * 10., -0.5 + a.sin() * 10.], [0.83252203, 0., 0.]), &sk, (12., 22.), [0., 0., 0.], [3., 6., 3.]);
            bx(out, &b, &sk, (0., 0.), [-4., -4., -4.], [8., 8., 8.]);
            bx(out, &b.part([-8., 4., 0.], [0.; 3]), &sk, (32., 0.), [-4., -4., -4.], [6., 6., 6.]);
            bx(out, &b.part([10., 4., 0.], [0.; 3]), &sk, (32., 0.), [-4., -4., -4.], [6., 6., 6.]);
        }
        "ender_dragon" => {
            let Some(sk) = tex(&["enderdragon/dragon"]) else { return true };
            let b = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).ry(-yaw).t(0.0, 0.0, 1.0).s(-1.0, -1.0, 1.0).t(0.0, -1.501, 0.0);
            let head = b.part([0., 20., -62.], [0.; 3]);
            bx(out, &head, &sk, (176., 44.), [-6., -1., -24.], [12., 5., 16.]);
            bx(out, &head, &sk, (112., 30.), [-8., -8., -10.], [16., 16., 16.]);
            m(out, &head, &sk, (0., 0.), [-5., -12., -4.], [2., 4., 6.]);
            m(out, &head, &sk, (112., 0.), [-5., -3., -22.], [2., 2., 4.]);
            bx(out, &head, &sk, (0., 0.), [3., -12., -4.], [2., 4., 6.]);
            bx(out, &head, &sk, (112., 0.), [3., -3., -22.], [2., 2., 4.]);
            bx(out, &head.part([0., 4., -8.], [0.; 3]), &sk, (176., 65.), [-6., 0., -16.], [12., 4., 16.]);
            let seg = |o: &mut Vec<Quad>, p: Pose| {
                bx(o, &p, &sk, (192., 104.), [-5., -5., -5.], [10., 10., 10.]);
                bx(o, &p, &sk, (48., 0.), [-1., -9., -3.], [2., 4., 6.]);
            };
            for i in 0..5 {
                seg(out, b.part([0., 20., -12. - 10. * i as f32], [0.; 3]));
            }
            for i in 0..12 {
                seg(out, b.part([0., 10., 60. + 10. * i as f32], [0.; 3]));
            }
            let body = b.part([0., 3., 8.], [0.; 3]);
            bx(out, &body, &sk, (0., 0.), [-12., 1., -16.], [24., 24., 64.]);
            for z in [-10., 10., 30.] {
                bx(out, &body, &sk, (220., 53.), [-1., -5., z], [2., 6., 12.]);
            }
            for (side, sx) in [(1.0f32, 12.0f32), (-1.0, -12.0)] {
                let w = b.part([sx, 2., -6.], [-0.075, 0.25 * side, 0.1 * side]);
                let x0 = if side > 0.0 { 0.0 } else { -56.0 };
                cube(out, &w, &sk, 112., 88., [x0, -4., -4.], [56., 8., 8.], 0.0, side > 0.0, ALL_FACES);
                cube(out, &w, &sk, -56., 88., [x0, 0., 2.], [56., 0., 56.], 0.0, side > 0.0, ALL_FACES);
                let t = w.part([56. * side, 0., 0.], [0., 0., -1.057 * side]);
                cube(out, &t, &sk, 112., 136., [x0, -2., -2.], [56., 4., 4.], 0.0, side > 0.0, ALL_FACES);
                cube(out, &t, &sk, -56., 144., [x0, 0., 2.], [56., 0., 56.], 0.0, side > 0.0, ALL_FACES);
                let fl = b.part([12. * side, 17., -6.], [1.3, 0., 0.]);
                bx(out, &fl, &sk, (112., 104.), [-4., -4., -4.], [8., 24., 8.]);
                let ft = fl.part([0., 20., -1.], [-0.5, 0., 0.]);
                bx(out, &ft, &sk, (226., 138.), [-3., -1., -3.], [6., 24., 6.]);
                bx(out, &ft.part([0., 23., 0.], [0.75, 0., 0.]), &sk, (144., 104.), [-4., 0., -12.], [8., 4., 16.]);
                let hl = b.part([16. * side, 13., 34.], [1.0, 0., 0.]);
                bx(out, &hl, &sk, (0., 0.), [-8., -4., -8.], [16., 32., 16.]);
                let ht = hl.part([0., 32., -4.], [0.5, 0., 0.]);
                bx(out, &ht, &sk, (196., 0.), [-6., -2., 0.], [12., 32., 12.]);
                bx(out, &ht.part([0., 31., 4.], [0.75, 0., 0.]), &sk, (112., 0.), [-9., 0., -20.], [18., 6., 24.]);
            }
        }
        "camel" | "camel_husk" => {
            let Some(sk) = tex(&[&format!("camel/{id}"), "camel/camel"]) else { return true };
            let b = living(pos, yaw, sc);
            let body = b.part([0., 4., 9.5], [0.; 3]);
            bx(out, &body, &sk, (0., 25.), [-7.5, -12., -23.5], [15., 12., 27.]);
            bx(out, &body.part([0., -12., -10.], [0.; 3]), &sk, (74., 0.), [-4.5, -5., -5.5], [9., 5., 11.]);
            bx(out, &body.part([0., -9., 3.5], [0.; 3]), &sk, (122., 0.), [-1.5, 0., 0.], [3., 14., 0.]);
            let head = body.part([0., -3., -19.5], [0.; 3]);
            bx(out, &head, &sk, (60., 24.), [-3.5, -7., -15.], [7., 8., 19.]);
            bx(out, &head, &sk, (21., 0.), [-3.5, -21., -15.], [7., 14., 7.]);
            bx(out, &head, &sk, (50., 0.), [-2.5, -21., -21.], [5., 5., 6.]);
            bx(out, &head.part([2.5, -21., -9.5], [0.; 3]), &sk, (45., 0.), [-0.5, 0.5, -1.], [3., 1., 2.]);
            bx(out, &head.part([-2.5, -21., -9.5], [0.; 3]), &sk, (67., 0.), [-2.5, 0.5, -1.], [3., 1., 2.]);
            for (o, uv) in [([4.9, 1., 9.5], (58., 16.)), ([-4.9, 1., 9.5], (94., 16.)), ([4.9, 1., -10.5], (0., 0.)), ([-4.9, 1., -10.5], (0., 26.))] {
                bx(out, &b.part(o, [0.; 3]), &sk, uv, [-2.5, 2., -2.5], [5., 21., 5.]);
            }
            crate::deco::camel(reg, ent, &b, out);
        }
        "sniffer" => {
            let Some(sk) = tex(&["sniffer/sniffer"]) else { return true };
            let bone = living(pos, yaw, sc).part([0., 5., 0.], [0.; 3]);
            bx(out, &bone, &sk, (62., 68.), [-12.5, -14., -20.], [25., 29., 40.]);
            c(out, &bone, &sk, (62., 0.), [-12.5, -14., -20.], [25., 24., 40.], 0.5);
            bx(out, &bone, &sk, (87., 68.), [-12.5, 12., -20.], [25., 0., 40.]);
            for (o, uv) in [([-7.5, 10., -15.], (32., 87.)), ([-7.5, 10., 0.], (32., 105.)), ([-7.5, 10., 15.], (32., 123.)), ([7.5, 10., -15.], (0., 87.)), ([7.5, 10., 0.], (0., 105.)), ([7.5, 10., 15.], (0., 123.))] {
                bx(out, &bone.part(o, [0.; 3]), &sk, uv, [-3.5, -1., -4.], [7., 10., 8.]);
            }
            let head = bone.part([0., 6.5, -19.48], [0.; 3]);
            bx(out, &head, &sk, (8., 15.), [-6.5, -7.5, -11.5], [13., 18., 11.]);
            bx(out, &head, &sk, (8., 4.), [-6.5, 7.5, -11.5], [13., 0., 11.]);
            bx(out, &head.part([6.51, -7.5, -4.51], [0.; 3]), &sk, (2., 0.), [0., 0., -3.], [1., 19., 7.]);
            bx(out, &head.part([-6.51, -7.5, -4.51], [0.; 3]), &sk, (48., 0.), [-1., 0., -3.], [1., 19., 7.]);
            bx(out, &head.part([0., -4.5, -11.5], [0.; 3]), &sk, (10., 45.), [-6.5, -2., -9.], [13., 2., 9.]);
            bx(out, &head.part([0., 2.5, -12.5], [0.; 3]), &sk, (10., 57.), [-6.5, -7., -8.], [13., 12., 9.]);
        }
        "armadillo" => {
            let Some(sk) = tex(&["armadillo/armadillo"]) else { return true };
            let b = living(pos, yaw, sc);
            let body = b.part([0., 21., 4.], [0.; 3]);
            c(out, &body, &sk, (0., 20.), [-4., -7., -10.], [8., 8., 12.], 0.3);
            bx(out, &body, &sk, (0., 40.), [-4., -7., -10.], [8., 8., 12.]);
            bx(out, &body.part([0., -3., 1.], [0.5061, 0., 0.]), &sk, (44., 53.), [-0.5, -0.0865, 0.0933], [1., 6., 1.]);
            let head = body.part([0., -2., -11.], [0.; 3]);
            bx(out, &head.part([0.; 3], [-0.3927, 0., 0.]), &sk, (43., 15.), [-1.5, -1., -1.], [3., 5., 2.]);
            bx(out, &head.part([-1., -1., 0.], [0.; 3]).part([-0.5, 0., -0.6], [0.1886, -0.3864, -0.0718]), &sk, (43., 10.), [-2., -3., 0.], [2., 5., 0.]);
            bx(out, &head.part([1., -2., 0.], [0.; 3]).part([0.5, 1., -0.6], [0.1886, 0.3864, 0.0718]), &sk, (47., 10.), [0., -3., 0.], [2., 5., 0.]);
            for (o, uv) in [([-2., 21., 4.], (51., 31.)), ([2., 21., 4.], (42., 31.)), ([-2., 21., -4.], (51., 43.)), ([2., 21., -4.], (42., 43.))] {
                bx(out, &b.part(o, [0.; 3]), &sk, uv, [-1., 0., -1.], [2., 3., 2.]);
            }
        }
        "nautilus" | "zombie_nautilus" => {
            let Some(sk) = tex(&[&format!("nautilus/{id}"), "nautilus/nautilus"]) else { return true };
            let root = living(pos, yaw, sc).part([0., 29., -6.], [0.; 3]);
            let shell = root.part([0., -13., 5.], [0.; 3]);
            bx(out, &shell, &sk, (0., 0.), [-7., -10., -7.], [14., 10., 16.]);
            bx(out, &shell, &sk, (0., 26.), [-7., 0., -7.], [14., 8., 20.]);
            bx(out, &shell, &sk, (48., 26.), [-7., 0., 6.], [14., 8., 0.]);
            let body = root.part([0., -8.5, 12.3], [0.; 3]);
            bx(out, &body, &sk, (0., 54.), [-5., -4.51, -3.], [10., 8., 14.]);
            bx(out, &body, &sk, (0., 76.), [-5., -4.51, 7.], [10., 8., 0.]);
            c(out, &body.part([0., -2.51, 7.], [0.; 3]), &sk, (54., 54.), [-5., -2., 0.], [10., 4., 4.], -0.001);
            bx(out, &body.part([0., -0.51, 7.5], [0.; 3]), &sk, (54., 70.), [-3., -2., -0.5], [6., 4., 4.]);
            c(out, &body.part([0., 1.49, 7.], [0.; 3]), &sk, (54., 62.), [-5., -1.98, 0.], [10., 4., 4.], -0.001);
            crate::deco::nautilus(reg, ent, &root, out);
        }
        "cod" | "salmon" | "tadpole" => {
            let b = living(pos, yaw, 1.0);
            match id {
                "cod" => {
                    let Some(sk) = tex(&["fish/cod"]) else { return true };
                    bx(out, &b.part([0., 22., 0.], [0.; 3]), &sk, (0., 0.), [-1., -2., 0.], [2., 4., 7.]);
                    bx(out, &b.part([0., 22., 0.], [0.; 3]), &sk, (11., 0.), [-1., -2., -3.], [2., 4., 3.]);
                    bx(out, &b.part([0., 22., -3.], [0.; 3]), &sk, (0., 0.), [-1., -2., -1.], [2., 3., 1.]);
                    bx(out, &b.part([-1., 23., 0.], [0., 0., -PI / 4.]), &sk, (22., 1.), [-2., 0., -1.], [2., 0., 2.]);
                    bx(out, &b.part([1., 23., 0.], [0., 0., PI / 4.]), &sk, (22., 4.), [0., 0., -1.], [2., 0., 2.]);
                    bx(out, &b.part([0., 22., 7.], [0.; 3]), &sk, (22., 3.), [0., -2., 0.], [0., 4., 4.]);
                    bx(out, &b.part([0., 20., 0.], [0.; 3]), &sk, (20., -6.), [0., -1., -1.], [0., 1., 6.]);
                }
                "salmon" => {
                    let Some(sk) = tex(&["fish/salmon"]) else { return true };
                    let front = b.part([0., 20., -7.2], [0.; 3]);
                    bx(out, &front, &sk, (0., 0.), [-1.5, -2.5, 0.], [3., 5., 8.]);
                    let back = b.part([0., 20., 0.8], [0.; 3]);
                    bx(out, &back, &sk, (0., 13.), [-1.5, -2.5, 0.], [3., 5., 8.]);
                    bx(out, &b.part([0., 20., -7.2], [0.; 3]), &sk, (22., 0.), [-1., -2., -3.], [2., 4., 3.]);
                    bx(out, &back.part([0., 0., 8.], [0.; 3]), &sk, (20., 10.), [0., -2.5, 0.], [0., 5., 6.]);
                    bx(out, &front.part([0., -4.5, 5.], [0.; 3]), &sk, (2., 1.), [0., 0., 0.], [0., 2., 3.]);
                    bx(out, &back.part([0., -4.5, -1.], [0.; 3]), &sk, (0., 2.), [0., 0., 0.], [0., 2., 4.]);
                    bx(out, &b.part([-1.5, 21.5, -7.2], [0., 0., -PI / 4.]), &sk, (-4., 0.), [-2., 0., 0.], [2., 0., 2.]);
                    bx(out, &b.part([1.5, 21.5, -7.2], [0., 0., PI / 4.]), &sk, (0., 0.), [0., 0., 0.], [2., 0., 2.]);
                }
                _ => {
                    let Some(sk) = tex(&["tadpole/tadpole", "tadpole"]) else { return true };
                    bx(out, &b.part([0., 22., -3.], [0.; 3]), &sk, (0., 0.), [-1.5, -1., 0.], [3., 2., 3.]);
                    bx(out, &b.part([0., 22., 0.], [0.; 3]), &sk, (0., 0.), [0., -1., 0.], [0., 2., 7.]);
                }
            }
        }
        "pufferfish" => {
            let Some(sk) = tex(&["fish/pufferfish"]) else { return true };
            let b = living(pos, yaw, 1.0);
            let q = PI / 4.0;
            match int(get(Some(ent), "PuffState")).unwrap_or(0) {
                0 => {
                    bx(out, &b.part([0., 23., 0.], [0.; 3]), &sk, (0., 27.), [-1.5, -2., -1.5], [3., 2., 3.]);
                    bx(out, &b.part([0., 20., 0.], [0.; 3]), &sk, (24., 6.), [-1.5, 0., -1.5], [1., 1., 1.]);
                    bx(out, &b.part([0., 20., 0.], [0.; 3]), &sk, (28., 6.), [0.5, 0., -1.5], [1., 1., 1.]);
                    bx(out, &b.part([0., 22., 1.5], [0.; 3]), &sk, (-3., 0.), [-1.5, 0., 0.], [3., 0., 3.]);
                    bx(out, &b.part([-1.5, 22., -1.5], [0.; 3]), &sk, (25., 0.), [-1., 0., 0.], [1., 0., 2.]);
                    bx(out, &b.part([1.5, 22., -1.5], [0.; 3]), &sk, (25., 0.), [0., 0., 0.], [1., 0., 2.]);
                }
                1 => {
                    bx(out, &b.part([0., 22., 0.], [0.; 3]), &sk, (12., 22.), [-2.5, -5., -2.5], [5., 5., 5.]);
                    bx(out, &b.part([-2.5, 18., -1.5], [0.; 3]), &sk, (24., 0.), [-2., 0., 0.], [2., 0., 2.]);
                    bx(out, &b.part([2.5, 18., -1.5], [0.; 3]), &sk, (24., 3.), [0., 0., 0.], [2., 0., 2.]);
                    bx(out, &b.part([0., 17., -2.5], [q, 0., 0.]), &sk, (19., 17.), [-2.5, -1., 0.], [5., 1., 0.]);
                    bx(out, &b.part([0., 17., 2.5], [-q, 0., 0.]), &sk, (11., 17.), [-2.5, -1., 0.], [5., 1., 0.]);
                    bx(out, &b.part([-2.5, 22., -2.5], [0., -q, 0.]), &sk, (5., 17.), [-1., -5., 0.], [1., 5., 0.]);
                    bx(out, &b.part([-2.5, 22., 2.5], [0., q, 0.]), &sk, (9., 17.), [-1., -5., 0.], [1., 5., 0.]);
                    bx(out, &b.part([2.5, 22., 2.5], [0., -q, 0.]), &sk, (1., 17.), [0., -5., 0.], [1., 5., 0.]);
                    bx(out, &b.part([2.5, 22., -2.5], [0., q, 0.]), &sk, (1., 17.), [0., -5., 0.], [1., 5., 0.]);
                    bx(out, &b.part([-2.5, 22., 2.5], [q, 0., 0.]), &sk, (18., 20.), [0., 0., 0.], [5., 1., 0.]);
                    bx(out, &b.part([0., 22., -2.5], [-q, 0., 0.]), &sk, (17., 19.), [-2.5, 0., 0.], [5., 1., 1.]);
                }
                _ => {
                    bx(out, &b.part([0., 22., 0.], [0.; 3]), &sk, (0., 0.), [-4., -8., -4.], [8., 8., 8.]);
                    bx(out, &b.part([-4., 15., -2.], [0.; 3]), &sk, (24., 0.), [-2., 0., -1.], [2., 1., 2.]);
                    bx(out, &b.part([4., 15., -2.], [0.; 3]), &sk, (24., 3.), [0., 0., -1.], [2., 1., 2.]);
                    bx(out, &b.part([0., 14., -4.], [q, 0., 0.]), &sk, (15., 17.), [-4., -1., 0.], [8., 1., 0.]);
                    bx(out, &b.part([0., 14., 0.], [0.; 3]), &sk, (14., 16.), [-4., -1., 0.], [8., 1., 1.]);
                    bx(out, &b.part([0., 14., 4.], [-q, 0., 0.]), &sk, (23., 18.), [-4., -1., 0.], [8., 1., 0.]);
                    bx(out, &b.part([-4., 22., -4.], [0., -q, 0.]), &sk, (5., 17.), [-1., -8., 0.], [1., 8., 0.]);
                    bx(out, &b.part([4., 22., -4.], [0., q, 0.]), &sk, (1., 17.), [0., -8., 0.], [1., 8., 0.]);
                    bx(out, &b.part([0., 22., -4.], [-q, 0., 0.]), &sk, (15., 20.), [-4., 0., 0.], [8., 1., 0.]);
                    bx(out, &b.part([0., 22., 0.], [0.; 3]), &sk, (15., 20.), [-4., 0., 0.], [8., 1., 0.]);
                    bx(out, &b.part([0., 22., 4.], [q, 0., 0.]), &sk, (15., 20.), [-4., 0., 0.], [8., 1., 0.]);
                    bx(out, &b.part([-4., 22., 4.], [0., q, 0.]), &sk, (9., 17.), [-1., -8., 0.], [1., 8., 0.]);
                    bx(out, &b.part([4., 22., 4.], [0., -q, 0.]), &sk, (9., 17.), [0., -8., 0.], [1., 8., 0.]);
                }
            }
        }
        "tropical_fish" => {
            let v = int(get(Some(ent), "Variant")).or_else(|| int(get(Some(ent), "variant"))).unwrap_or(0);
            let large = v & 0xff == 1;
            let pat = ((v >> 8) & 0xff).clamp(0, 5) + 1;
            let base = dye_diffuse(COLORS[((v >> 16) & 15) as usize]);
            let patc = dye_diffuse(COLORS[((v >> 24) & 15) as usize]);
            let ab = if large { "b" } else { "a" };
            let Some(sk) = tinted(reg, &[(vec![e(&format!("fish/tropical_{ab}"))], Some(base)), (vec![e(&format!("fish/tropical_{ab}_pattern_{pat}"))], Some(patc))]) else { return true };
            let b = living(pos, yaw, 1.0);
            let q = PI / 4.0;
            if large {
                bx(out, &b.part([0., 19., 0.], [0.; 3]), &sk, (0., 20.), [-1., -3., -3.], [2., 6., 6.]);
                bx(out, &b.part([0., 19., 3.], [0.; 3]), &sk, (21., 16.), [0., -3., 0.], [0., 6., 5.]);
                bx(out, &b.part([-1., 20., 0.], [0., q, 0.]), &sk, (2., 16.), [-2., 0., 0.], [2., 2., 0.]);
                bx(out, &b.part([1., 20., 0.], [0., -q, 0.]), &sk, (2., 12.), [0., 0., 0.], [2., 2., 0.]);
                bx(out, &b.part([0., 16., -3.], [0.; 3]), &sk, (20., 11.), [0., -4., 0.], [0., 4., 6.]);
                bx(out, &b.part([0., 22., -3.], [0.; 3]), &sk, (20., 21.), [0., 0., 0.], [0., 4., 6.]);
            } else {
                bx(out, &b.part([0., 22., 0.], [0.; 3]), &sk, (0., 0.), [-1., -1.5, -3.], [2., 3., 6.]);
                bx(out, &b.part([0., 22., 3.], [0.; 3]), &sk, (22., -6.), [0., -1.5, 0.], [0., 3., 6.]);
                bx(out, &b.part([-1., 22.5, 0.], [0., q, 0.]), &sk, (2., 16.), [-2., -1., 0.], [2., 2., 0.]);
                bx(out, &b.part([1., 22.5, 0.], [0., -q, 0.]), &sk, (2., 12.), [0., -1., 0.], [2., 2., 0.]);
                bx(out, &b.part([0., 20.5, -3.], [0.; 3]), &sk, (10., -5.), [0., -3., 0.], [0., 3., 6.]);
            }
        }
        "shulker" => {
            let color = int(get(Some(ent), "Color")).unwrap_or(16);
            let mut t = Vec::new();
            if (0..16).contains(&color) {
                t.push(e(&format!("shulker/shulker_{}", COLORS[color as usize])));
            }
            t.push(e("shulker/shulker"));
            let Some(sk) = skin(reg, &t, Tint::None) else { return true };
            let face = ["down", "up", "north", "south", "west", "east"][int(get(Some(ent), "AttachFace")).unwrap_or(0).clamp(0, 5) as usize];
            let opp = match face {
                "down" => "up",
                "up" => "down",
                "north" => "south",
                "south" => "north",
                "west" => "east",
                _ => "west",
            };
            let rot = crate::bent::dir_rotation_pub(opp);
            let b = Pose::new()
                .t(pos[0] as f32, pos[1] as f32, pos[2] as f32)
                .t(0.0, 0.5, 0.0)
                .m(&rot)
                .t(0.0, -0.5, 0.0)
                .ry(180.0)
                .s(-1.0, -1.0, 1.0)
                .t(0.0, -1.501, 0.0);
            let p = b.part([0., 24., 0.], [0.; 3]);
            bx(out, &p, &sk, (0., 0.), [-8., -16., -8.], [16., 12., 16.]);
            bx(out, &p, &sk, (0., 28.), [-8., -8., -8.], [16., 8., 16.]);
            bx(out, &b.part([0., 12., 0.], [0.; 3]), &sk, (0., 52.), [-3., 0., -3.], [6., 6., 6.]);
        }
        "giant" => {
            let Some(sk) = tex(&["zombie/zombie"]) else { return true };
            let b = living(pos, yaw, 6.0);
            let mut l = Limbs::humanoid();
            l.rarm.1 = [-PI / 2.25, 0., 0.];
            l.larm.1 = [-PI / 2.25, 0., 0.];
            crate::mobs::humanoid_body(out, &b, &sk, &l, false, true);
        }
        "parched" => {
            let Some(sk) = tex(&["skeleton/parched", "skeleton/skeleton"]) else { return true };
            let b = living(pos, yaw, 1.0);
            let mut l = Limbs::humanoid();
            l.rleg.0 = [-2., 12., 0.];
            l.lleg.0 = [2., 12., 0.];
            crate::mobs::humanoid_body(out, &b, &sk, &l, true, false);
            crate::mobs::armor(reg, ent, &b, &l, false, out);
            held_items(reg, ent, &b, &l, true, out);
        }
        // dropped items, primed TNT, end crystal
        "item" => {
            let Some(it) = get(Some(ent), "Item") else { return true };
            let p = Pose::new().t(pos[0] as f32, pos[1] as f32 + 0.1, pos[2] as f32).ry(-yaw);
            out.extend(crate::entity::item_quads(reg, reg.font(), it, "ground").iter().map(|q| p.quad(q)));
        }
        "arrow" | "spectral_arrow" => {
            let t = if id == "spectral_arrow" { "projectiles/arrow_spectral" } else { "projectiles/arrow" };
            let Some(sk) = tex(&[t, &t.replace("projectiles/", "")]) else { return true };
            let rot: Vec<f32> = match get(Some(ent), "Rotation") {
                Some(Value::List(l)) => l.iter().filter_map(|v| if let Value::Float(f) = v { Some(*f) } else { None }).collect(),
                _ => Vec::new(),
            };
            let pitch = rot.get(1).copied().unwrap_or(0.0);
            let p = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).ry(yaw - 90.0).rz(pitch).s(0.9, 0.9, 0.9);
            crate::pose::cube_uv(out, &p.part_s([-11., 0., 0.], [PI / 4.0, 0., 0.], 0.8), &sk, 0., 0., [0., -2.5, -2.5], [0., 5., 5.], [0., 5., 5.], 0.0, false, ALL_FACES);
            for r in [PI / 4.0, PI * 0.75] {
                crate::pose::cube_uv(out, &p.part([0.; 3], [r, 0., 0.]), &sk, 0., 0., [-12., -2., 0.], [16., 4., 0.], [16., 5., 0.], 0.0, false, ALL_FACES);
            }
        }
        "trident" => {
            let Some(sk) = tex(&["trident/trident", "trident"]) else { return true };
            let rot: Vec<f32> = match get(Some(ent), "Rotation") {
                Some(Value::List(l)) => l.iter().filter_map(|v| if let Value::Float(f) = v { Some(*f) } else { None }).collect(),
                _ => Vec::new(),
            };
            let pitch = rot.get(1).copied().unwrap_or(0.0);
            let p = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).ry(yaw - 90.0).rz(pitch + 90.0);
            bx(out, &p, &sk, (0., 6.), [-0.5, 2., -0.5], [1., 25., 1.]);
            bx(out, &p, &sk, (4., 0.), [-1.5, 0., -0.5], [3., 2., 1.]);
            bx(out, &p, &sk, (4., 3.), [-2.5, -3., -0.5], [1., 4., 1.]);
            bx(out, &p, &sk, (0., 0.), [-0.5, -4., -0.5], [1., 4., 1.]);
            m(out, &p, &sk, (4., 3.), [1.5, -3., -0.5], [1., 4., 1.]);
        }
        "experience_orb" => {
            let Some((tid, t)) = reg.assets.first_texture(&[e("experience/experience_orb"), e("experience_orb")]) else { return true };
            let v = int(get(Some(ent), "Value")).unwrap_or(1);
            let i = [2477, 1237, 617, 307, 149, 73, 37, 17, 7, 3].iter().position(|&th| v >= th).map(|p| 10 - p).unwrap_or(0) as f32;
            let (u0, v0) = ((i % 4.0) * 16.0 / t.w as f32, (i / 4.0).floor() * 16.0 / t.h as f32);
            let (u1, v1) = (u0 + 16.0 / t.w as f32, v0 + 16.0 / t.h as f32);
            let tex_id = reg.tex_id(&tid);
            for a in [0.0, 90.0] {
                let p = Pose::new().t(pos[0] as f32, pos[1] as f32 + 0.1, pos[2] as f32).ry(a).s(0.3, 0.3, 0.3).t(-0.5, 0.0, 0.0);
                let mut q = crate::pose::flat_quad(&p, tex_id, 1.0, 1.0, [u0, 1.0 - v0, u1, 1.0 - v1]);
                q.tint = Tint::Fixed([150, 255, 60]);
                out.push(q.clone());
                q.pos = [q.pos[3], q.pos[2], q.pos[1], q.pos[0]];
                q.uv = [q.uv[3], q.uv[2], q.uv[1], q.uv[0]];
                out.push(q);
            }
        }
        "leash_knot" => {
            let Some(sk) = tex(&["lead_knot/lead_knot", "lead_knot"]) else { return true };
            let p = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).s(-1.0, -1.0, 1.0);
            bx(out, &p, &sk, (0., 0.), [-3., -8., -3.], [6., 8., 6.]);
        }
        "snowball" | "egg" | "ender_pearl" | "eye_of_ender" | "potion" | "splash_potion" | "lingering_potion" | "experience_bottle"
        | "fireball" | "small_fireball" | "dragon_fireball" | "firework_rocket" | "wind_charge" | "breeze_wind_charge" => {
            let default = match id {
                "eye_of_ender" => "ender_eye",
                "fireball" | "small_fireball" | "dragon_fireball" => "fire_charge",
                "potion" | "splash_potion" => "splash_potion",
                "breeze_wind_charge" => "wind_charge",
                x => x,
            };
            let owned;
            let it = match get(Some(ent), "Item").or_else(|| get(Some(ent), "FireworksItem")) {
                Some(v) => v,
                None => {
                    let mut mm = std::collections::HashMap::new();
                    mm.insert("id".to_string(), Value::String(format!("minecraft:{default}")));
                    owned = Value::Compound(mm);
                    &owned
                }
            };
            let p = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).ry(-yaw);
            out.extend(crate::entity::item_quads(reg, reg.font(), it, "ground").iter().map(|q| p.quad(q)));
        }
        "wither_skull" => {
            let fake = crate::registry::StateKey { name: "minecraft:wither_skeleton_skull".into(), props: vec![("rotation".into(), "0".into())] };
            let o = crate::bent::BeOpts { sign_text: false, download_skins: false };
            let p = Pose::new().t(pos[0] as f32 - 0.5, pos[1] as f32, pos[2] as f32 - 0.5);
            out.extend(crate::bent::quads(reg, reg.font(), &fake, None, false, &o).iter().map(|q| p.quad(q)));
        }
        "tnt" => {
            let sid = reg.state_id("minecraft:tnt", vec![("unstable".into(), "false".into())]);
            let p = Pose::new().t(pos[0] as f32 - 0.5, pos[1] as f32, pos[2] as f32 - 0.5);
            out.extend(reg.state_quads(sid).iter().map(|q| p.quad(q)));
        }
        "end_crystal" => {
            let Some(sk) = tex(&["end_crystal/end_crystal", "endercrystal/endercrystal"]) else { return true };
            let b = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).s(2.0, 2.0, 2.0).t(0.0, -0.5, 0.0);
            if int(get(Some(ent), "ShowBottom")).unwrap_or(1) != 0 {
                bx(out, &b, &sk, (0., 16.), [-6., 0., -6.], [12., 4., 12.]);
            }
            let core = b.t(0.0, 1.2, 0.0).rx(45.0).rz(35.26439);
            bx(out, &core.s(0.875, 0.875, 0.875), &sk, (0., 0.), [-4., -4., -4.], [8., 8., 8.]);
            bx(out, &core.s(0.765625, 0.765625, 0.765625).rx(45.0).rz(35.26439), &sk, (32., 0.), [-4., -4., -4.], [8., 8., 8.]);
            bx(out, &core.s(0.669922, 0.669922, 0.669922).rx(45.0).rz(35.26439).rx(45.0).rz(35.26439), &sk, (32., 0.), [-4., -4., -4.], [8., 8., 8.]);
        }
        _ => return false,
    }
    // equipment held by illagers (vindicator axes) is not shown when arms are crossed
    let _ = equipment(ent, "mainhand");
    true
}
