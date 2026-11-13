//! Mobs (dimensions lues dans les classes de modèles du jeu 26.x) et armures portées.

use crate::bent::{COLORS, dye_diffuse, e, skin};
use crate::pose::{ALL_FACES, Pose, Skin, bx, cube};
use crate::registry::{Quad, Registry, Tint};
use fastnbt::Value;
use std::f32::consts::PI;
use std::hash::{Hash, Hasher};

pub(crate) fn get<'a>(v: Option<&'a Value>, k: &str) -> Option<&'a Value> {
    match v? {
        Value::Compound(m) => m.get(k),
        _ => None,
    }
}
pub(crate) fn int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Byte(x) => Some(*x as i64),
        Value::Short(x) => Some(*x as i64),
        Value::Int(x) => Some(*x as i64),
        Value::Long(x) => Some(*x),
        _ => None,
    }
}
pub(crate) fn string(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.trim_start_matches("minecraft:").to_string()),
        _ => None,
    }
}

/// Superpose plusieurs textures de même taille (villageois : peau + biome + métier…)
pub fn composite(reg: &Registry, layers: &[Vec<String>]) -> Option<Skin> {
    let mut found: Vec<String> = Vec::new();
    for cands in layers {
        if let Some((id, _)) = reg.assets.first_texture(cands) {
            found.push(id);
        }
    }
    let first = found.first()?.clone();
    if found.len() == 1 {
        return skin(reg, &[first], Tint::None);
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    found.hash(&mut h);
    let name = format!("gen:layers_{:016x}", h.finish());
    let base = reg.assets.texture(&first)?;
    if !reg.assets.has_texture(&name) {
        let mut img = base.rgba.clone();
        for l in &found[1..] {
            let Some(t) = reg.assets.texture(l) else { continue };
            if t.w != base.w || t.h != base.h {
                continue;
            }
            for (d, s) in img.chunks_exact_mut(4).zip(t.rgba.chunks_exact(4)) {
                let a = s[3] as f32 / 255.0;
                for k in 0..3 {
                    d[k] = (s[k] as f32 * a + d[k] as f32 * (1.0 - a)) as u8;
                }
                d[3] = d[3].max(s[3]);
            }
        }
        reg.assets.put_texture(&name, base.w, base.h, img);
    }
    Some(Skin { tex: reg.tex_id(&name), w: base.w as f32, h: base.h as f32, tint: Tint::None })
}

/// Poses des membres d'un modèle humanoïde (décalage en pixels, rotation en radians)
#[derive(Clone, Copy)]
pub struct Limbs {
    pub head: ([f32; 3], [f32; 3]),
    pub body: ([f32; 3], [f32; 3]),
    pub rarm: ([f32; 3], [f32; 3]),
    pub larm: ([f32; 3], [f32; 3]),
    pub rleg: ([f32; 3], [f32; 3]),
    pub lleg: ([f32; 3], [f32; 3]),
}

impl Limbs {
    pub fn humanoid() -> Limbs {
        Limbs {
            head: ([0.; 3], [0.; 3]),
            body: ([0.; 3], [0.; 3]),
            rarm: ([-5., 2., 0.], [0.; 3]),
            larm: ([5., 2., 0.], [0.; 3]),
            rleg: ([-1.9, 12., 0.], [0.; 3]),
            lleg: ([1.9, 12., 0.], [0.; 3]),
        }
    }
}

fn item_id(v: &Value) -> Option<String> {
    string(get(Some(v), "id")).filter(|s| s != "air")
}

/// Objet équipé dans un emplacement (1.21.5+ : `equipment`, avant : ArmorItems / HandItems)
pub fn equipment<'a>(ent: &'a Value, slot: &str) -> Option<&'a Value> {
    if let Some(v) = get(get(Some(ent), "equipment"), slot) {
        return Some(v);
    }
    let (list, idx) = match slot {
        "feet" => ("ArmorItems", 0),
        "legs" => ("ArmorItems", 1),
        "chest" => ("ArmorItems", 2),
        "head" => ("ArmorItems", 3),
        "mainhand" => ("HandItems", 0),
        "offhand" => ("HandItems", 1),
        _ => return None,
    };
    match get(Some(ent), list) {
        Some(Value::List(l)) => l.get(idx).filter(|v| item_id(v).is_some()),
        _ => None,
    }
}

fn dyed_color(item: &Value) -> Option<[u8; 3]> {
    let c = get(get(Some(item), "components"), "minecraft:dyed_color");
    let n = int(c).or_else(|| int(get(c, "rgb"))).or_else(|| int(get(get(get(Some(item), "tag"), "display"), "color")))?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// Calques de texture d'une armure : (texture, teinte)
fn armor_layers_raw(reg: &Registry, item: &Value, leggings: bool) -> Vec<(Skin, ())> {
    let Some(id) = item_id(item) else { return Vec::new() };
    let mat = if id == "turtle_helmet" {
        "turtle_scute".to_string()
    } else {
        let m = id.rsplit_once('_').map(|x| x.0).unwrap_or(&id);
        if m == "golden" { "gold".to_string() } else { m.to_string() }
    };
    let kind = if leggings { "humanoid_leggings" } else { "humanoid" };
    let mut out = Vec::new();
    // format 1.21.2+ : assets/minecraft/equipment/<matériau>.json
    if let Some(j) = reg.assets.json(&format!("assets/minecraft/equipment/{mat}.json")) {
        if let Some(layers) = j.get("layers").and_then(|l| l.get(kind)).and_then(|l| l.as_array()) {
            for l in layers {
                let Some(t) = l.get("texture").and_then(|t| t.as_str()) else { continue };
                let t = t.trim_start_matches("minecraft:");
                let tint = if let Some(d) = l.get("dyeable") {
                    let undyed = d.get("color_when_undyed").and_then(|c| c.as_i64()).map(|n| [(n >> 16) as u8, (n >> 8) as u8, n as u8]);
                    match dyed_color(item).or(undyed) {
                        Some(c) => Tint::Fixed(c),
                        None => Tint::None,
                    }
                } else {
                    Tint::None
                };
                if let Some(sk) = skin(reg, &[e(&format!("equipment/{kind}/{t}"))], tint) {
                    out.push((sk, ()));
                }
            }
            return out;
        }
    }
    // anciennes versions : textures/models/armor/<matériau>_layer_1.png
    let layer = if leggings { 2 } else { 1 };
    let name = if mat == "turtle_scute" { "turtle".to_string() } else { mat.clone() };
    let tint = if mat == "leather" { Tint::Fixed(dyed_color(item).unwrap_or([0xA0, 0x65, 0x40])) } else { Tint::None };
    if let Some(sk) = skin(reg, &[format!("minecraft:models/armor/{name}_layer_{layer}")], tint) {
        out.push((sk, ()));
    }
    if mat == "leather" {
        if let Some(sk) = skin(reg, &[format!("minecraft:models/armor/leather_layer_{layer}_overlay")], Tint::None) {
            out.push((sk, ()));
        }
    }
    out
}

/// Armure portée sur un modèle humanoïde. `stand` : variante porte-armure (tête plus haute, jambes plus courtes).
pub fn armor(reg: &Registry, ent: &Value, base: &Pose, l: &Limbs, stand: bool, out: &mut Vec<Quad>) {
    for slot in ["head", "chest", "legs", "feet"] {
        let Some(it) = equipment(ent, slot) else { continue };
        let leggings = slot == "legs";
        let g = if leggings { 0.5 } else { 1.0 };
        for sk in armor_texture(reg, it, leggings) {
            let part = |(o, r): ([f32; 3], [f32; 3])| base.part(o, r);
            match slot {
                "head" => {
                    let off = if stand { [0., 1., 0.] } else { l.head.0 };
                    let h = base.part(off, l.head.1);
                    cube(out, &h, &sk, 0., 0., [-4., -8., -4.], [8., 8., 8.], g, false, ALL_FACES);
                    cube(out, &h, &sk, 32., 0., [-4., -8., -4.], [8., 8., 8.], g + 0.5, false, ALL_FACES);
                }
                "chest" => {
                    cube(out, &part(l.body), &sk, 16., 16., [-4., 0., -2.], [8., 12., 4.], g, false, ALL_FACES);
                    cube(out, &part(l.rarm), &sk, 40., 16., [-3., -2., -2.], [4., 12., 4.], g, false, ALL_FACES);
                    cube(out, &part(l.larm), &sk, 40., 16., [-1., -2., -2.], [4., 12., 4.], g, true, ALL_FACES);
                }
                _ => {
                    if leggings {
                        cube(out, &part(l.body), &sk, 16., 16., [-4., 0., -2.], [8., 12., 4.], g, false, ALL_FACES);
                    }
                    let (rl, ll) = if stand {
                        ((([-1.9, 11., 0.]), l.rleg.1), (([1.9, 11., 0.]), l.lleg.1))
                    } else {
                        (l.rleg, l.lleg)
                    };
                    let gl = if stand { g - 0.1 } else { g };
                    cube(out, &part(rl), &sk, 0., 16., [-2., 0., -2.], [4., 12., 4.], gl, false, ALL_FACES);
                    cube(out, &part(ll), &sk, 0., 16., [-2., 0., -2.], [4., 12., 4.], gl, true, ALL_FACES);
                }
            }
        }
    }
}

pub(crate) fn living(pos: [f64; 3], yaw: f32, scale: f32) -> Pose {
    Pose::new()
        .t(pos[0] as f32, pos[1] as f32, pos[2] as f32)
        .ry(180.0 - yaw)
        .s(-scale, -scale, scale)
        .t(0.0, -1.501, 0.0)
}

fn quad_legs(out: &mut Vec<Quad>, base: &Pose, sk: &Skin, uv: (f32, f32), at: [[f32; 3]; 4], from: [f32; 3], size: [f32; 3]) {
    for (i, a) in at.iter().enumerate() {
        cube(out, &base.part(*a, [0.; 3]), sk, uv.0, uv.1, from, size, 0.0, i % 2 == 1, ALL_FACES);
    }
}

pub(crate) fn humanoid_body(out: &mut Vec<Quad>, base: &Pose, sk: &Skin, l: &Limbs, thin: bool, hat: bool) {
    let h = base.part(l.head.0, l.head.1);
    bx(out, &h, sk, (0., 0.), [-4., -8., -4.], [8., 8., 8.]);
    if hat {
        cube(out, &h, sk, 32., 0., [-4., -8., -4.], [8., 8., 8.], 0.5, false, ALL_FACES);
    }
    bx(out, &base.part(l.body.0, l.body.1), sk, (16., 16.), [-4., 0., -2.], [8., 12., 4.]);
    if thin {
        bx(out, &base.part(l.rarm.0, l.rarm.1), sk, (40., 16.), [-1., -2., -1.], [2., 12., 2.]);
        cube(out, &base.part(l.larm.0, l.larm.1), sk, 40., 16., [-1., -2., -1.], [2., 12., 2.], 0.0, true, ALL_FACES);
        bx(out, &base.part(l.rleg.0, l.rleg.1), sk, (0., 16.), [-1., 0., -1.], [2., 12., 2.]);
        cube(out, &base.part(l.lleg.0, l.lleg.1), sk, 0., 16., [-1., 0., -1.], [2., 12., 2.], 0.0, true, ALL_FACES);
    } else {
        bx(out, &base.part(l.rarm.0, l.rarm.1), sk, (40., 16.), [-3., -2., -2.], [4., 12., 4.]);
        cube(out, &base.part(l.larm.0, l.larm.1), sk, 40., 16., [-1., -2., -2.], [4., 12., 4.], 0.0, true, ALL_FACES);
        bx(out, &base.part(l.rleg.0, l.rleg.1), sk, (0., 16.), [-2., 0., -2.], [4., 12., 4.]);
        cube(out, &base.part(l.lleg.0, l.lleg.1), sk, 0., 16., [-2., 0., -2.], [4., 12., 4.], 0.0, true, ALL_FACES);
    }
}

fn villager_tex(ent: &Value, root: &str) -> Vec<Vec<String>> {
    let vd = get(Some(ent), "VillagerData").or_else(|| get(Some(ent), "villager_data"));
    let t = string(get(vd, "type")).unwrap_or("plains".into());
    let p = string(get(vd, "profession")).unwrap_or("none".into());
    let mut v = vec![vec![e(&format!("{root}/{root}"))], vec![e(&format!("{root}/type/{t}"))]];
    if p != "none" {
        v.push(vec![e(&format!("{root}/profession/{p}"))]);
    }
    v
}

/// Mobs pris en charge ici ; renvoie false si l'identifiant n'est pas connu
pub fn mob(reg: &Registry, id: &str, ent: &Value, pos: [f64; 3], yaw: f32, out: &mut Vec<Quad>) -> bool {
    let h = PI / 2.0;
    let baby = int(get(Some(ent), "Age")).unwrap_or(0) < 0 || int(get(Some(ent), "IsBaby")).unwrap_or(0) != 0;
    let sc = if baby { 0.5 } else { 1.0 };
    match id {
        "zombie" | "husk" | "drowned" | "skeleton" | "stray" | "wither_skeleton" | "bogged" | "zombified_piglin" | "piglin" | "piglin_brute" => {
            let thin = matches!(id, "skeleton" | "stray" | "wither_skeleton" | "bogged");
            let tex = match id {
                "zombie" => vec![e("zombie/zombie")],
                "husk" => vec![e("zombie/husk")],
                "drowned" => vec![e("zombie/drowned")],
                "skeleton" => vec![e("skeleton/skeleton")],
                "stray" => vec![e("skeleton/stray")],
                "wither_skeleton" => vec![e("skeleton/wither_skeleton")],
                "bogged" => vec![e("skeleton/bogged")],
                "zombified_piglin" => vec![e("piglin/zombified_piglin")],
                "piglin" => vec![e("piglin/piglin")],
                _ => vec![e("piglin/piglin_brute")],
            };
            let Some(sk) = skin(reg, &tex, Tint::None) else { return true };
            let s = if id == "wither_skeleton" { 1.2 } else { sc };
            let base = living(pos, yaw, s);
            let mut l = Limbs::humanoid();
            if thin {
                l.rleg.0 = [-2., 12., 0.];
                l.lleg.0 = [2., 12., 0.];
            }
            if id.contains("zombie") || id == "husk" || id == "drowned" {
                // bras tendus des zombies (AnimationUtils.animateZombieArms, au repos)
                l.rarm.1 = [-PI / 2.25, 0.0, 0.0];
                l.larm.1 = [-PI / 2.25, 0.0, 0.0];
            }
            if id.contains("piglin") {
                let hp = base.part(l.head.0, l.head.1);
                bx(out, &hp, &sk, (0., 0.), [-5., -8., -4.], [10., 8., 8.]);
                bx(out, &hp, &sk, (31., 1.), [-2., -4., -5.], [4., 4., 1.]);
                bx(out, &hp, &sk, (2., 4.), [2., -2., -5.], [1., 2., 1.]);
                bx(out, &hp, &sk, (2., 0.), [-3., -2., -5.], [1., 2., 1.]);
                bx(out, &hp.part([4.5, -6., 0.], [0., 0., -PI / 6.]), &sk, (51., 6.), [0., 0., -2.], [1., 5., 4.]);
                bx(out, &hp.part([-4.5, -6., 0.], [0., 0., PI / 6.]), &sk, (39., 6.), [-1., 0., -2.], [1., 5., 4.]);
                let mut l2 = l;
                l2.head = ([0., -100., 0.], [0.; 3]);
                // corps sans la tête humanoïde
                bx(out, &base.part(l.body.0, l.body.1), &sk, (16., 16.), [-4., 0., -2.], [8., 12., 4.]);
                bx(out, &base.part(l.rarm.0, l.rarm.1), &sk, (40., 16.), [-3., -2., -2.], [4., 12., 4.]);
                cube(out, &base.part(l.larm.0, l.larm.1), &sk, 40., 16., [-1., -2., -2.], [4., 12., 4.], 0.0, true, ALL_FACES);
                bx(out, &base.part(l.rleg.0, l.rleg.1), &sk, (0., 16.), [-2., 0., -2.], [4., 12., 4.]);
                cube(out, &base.part(l.lleg.0, l.lleg.1), &sk, 0., 16., [-2., 0., -2.], [4., 12., 4.], 0.0, true, ALL_FACES);
            } else {
                humanoid_body(out, &base, &sk, &l, thin, !thin);
            }
            if id == "stray" {
                if let Some(o) = skin(reg, &[e("skeleton/stray_overlay")], Tint::None) {
                    humanoid_overlay(out, &base, &o, &l, 0.25);
                }
            } else if id == "drowned" {
                if let Some(o) = skin(reg, &[e("zombie/drowned_outer_layer")], Tint::None) {
                    humanoid_overlay(out, &base, &o, &l, 0.25);
                }
            }
            armor(reg, ent, &base, &l, false, out);
            held_items(reg, ent, &base, &l, thin, out);
            crate::deco::head_item(reg, ent, &base.part(l.head.0, l.head.1), out);
        }
        "creeper" => {
            let Some(sk) = skin(reg, &[e("creeper/creeper")], Tint::None) else { return true };
            let b = living(pos, yaw, 1.0);
            bx(out, &b.part([0., 6., 0.], [0.; 3]), &sk, (0., 0.), [-4., -8., -4.], [8., 8., 8.]);
            bx(out, &b.part([0., 6., 0.], [0.; 3]), &sk, (16., 16.), [-4., 0., -2.], [8., 12., 4.]);
            quad_legs(out, &b, &sk, (0., 16.), [[-2., 18., 4.], [2., 18., 4.], [-2., 18., -4.], [2., 18., -4.]], [-2., 0., -2.], [4., 6., 4.]);
        }
        "spider" | "cave_spider" => {
            let Some(sk) = skin(reg, &[e(&format!("spider/{id}")), e("spider/spider")], Tint::None) else { return true };
            let b = living(pos, yaw, if id == "cave_spider" { 0.7 } else { 1.0 });
            bx(out, &b.part([0., 15., -3.], [0.; 3]), &sk, (32., 4.), [-4., -4., -8.], [8., 8., 8.]);
            bx(out, &b.part([0., 15., 0.], [0.; 3]), &sk, (0., 0.), [-3., -3., -3.], [6., 6., 6.]);
            bx(out, &b.part([0., 15., 9.], [0.; 3]), &sk, (0., 12.), [-5., -4., -6.], [10., 8., 12.]);
            let (q, m) = (0.7853982, 0.58119464);
            let e8 = 0.3926991;
            let legs: [([f32; 3], [f32; 3], bool); 8] = [
                ([-4., 15., 2.], [0., q, -q], false),
                ([4., 15., 2.], [0., -q, q], true),
                ([-4., 15., 1.], [0., e8, -m], false),
                ([4., 15., 1.], [0., -e8, m], true),
                ([-4., 15., 0.], [0., -e8, -m], false),
                ([4., 15., 0.], [0., e8, m], true),
                ([-4., 15., -1.], [0., -q, -q], false),
                ([4., 15., -1.], [0., q, q], true),
            ];
            for (o, r, left) in legs {
                let p = b.part(o, r);
                if left {
                    cube(out, &p, &sk, 18., 0., [-1., -1., -1.], [16., 2., 2.], 0.0, true, ALL_FACES);
                } else {
                    bx(out, &p, &sk, (18., 0.), [-15., -1., -1.], [16., 2., 2.]);
                }
            }
        }
        "enderman" => {
            let Some(sk) = skin(reg, &[e("enderman/enderman")], Tint::None) else { return true };
            let b = living(pos, yaw, 1.0);
            let hp = b.part([0., -13., 0.], [0.; 3]);
            bx(out, &hp, &sk, (0., 0.), [-4., -8., -4.], [8., 8., 8.]);
            cube(out, &hp, &sk, 0., 16., [-4., -8., -4.], [8., 8., 8.], -0.5, false, ALL_FACES);
            bx(out, &b.part([0., -14., 0.], [0.; 3]), &sk, (32., 16.), [-4., 0., -2.], [8., 12., 4.]);
            bx(out, &b.part([-5., -12., 0.], [0.; 3]), &sk, (56., 0.), [-1., -2., -1.], [2., 30., 2.]);
            cube(out, &b.part([5., -12., 0.], [0.; 3]), &sk, 56., 0., [-1., -2., -1.], [2., 30., 2.], 0.0, true, ALL_FACES);
            bx(out, &b.part([-2., -5., 0.], [0.; 3]), &sk, (56., 0.), [-1., 0., -1.], [2., 30., 2.]);
            cube(out, &b.part([2., -5., 0.], [0.; 3]), &sk, 56., 0., [-1., 0., -1.], [2., 30., 2.], 0.0, true, ALL_FACES);
        }
        "villager" | "wandering_trader" | "zombie_villager" | "witch" => {
            let layers = match id {
                "villager" => villager_tex(ent, "villager"),
                "zombie_villager" => villager_tex(ent, "zombie_villager"),
                "witch" => vec![vec![e("witch/witch"), e("witch")]],
                _ => vec![vec![e("wandering_trader/wandering_trader"), e("wandering_trader")]],
            };
            let Some(sk) = composite(reg, &layers) else { return true };
            let b = living(pos, yaw, sc);
            let head = b.part([0.; 3], [0.; 3]);
            bx(out, &head, &sk, (0., 0.), [-4., -10., -4.], [8., 10., 8.]);
            bx(out, &head.part([0., -2., 0.], [0.; 3]), &sk, (24., 0.), [-1., -1., -6.], [2., 4., 2.]);
            if id == "witch" {
                let h1 = head.part([-5., -10.03125, -5.], [0.; 3]);
                bx(out, &h1, &sk, (0., 64.), [0., 0., 0.], [10., 2., 10.]);
                let h2 = h1.part([1.75, -4., 2.], [-0.05235988, 0., 0.02617994]);
                bx(out, &h2, &sk, (0., 76.), [0., 0., 0.], [7., 4., 7.]);
                let h3 = h2.part([1.75, -4., 2.], [-0.10471976, 0., 0.05235988]);
                bx(out, &h3, &sk, (0., 87.), [0., 0., 0.], [4., 4., 4.]);
                let h4 = h3.part([1.75, -2., 2.], [-0.20943952, 0., 0.10471976]);
                cube(out, &h4, &sk, 0., 95., [0., 0., 0.], [1., 2., 1.], 0.25, false, ALL_FACES);
            } else {
                cube(out, &head, &sk, 32., 0., [-4., -10., -4.], [8., 10., 8.], 0.51, false, ALL_FACES);
                if id != "zombie_villager" {
                    bx(out, &head.part([0.; 3], [-h, 0., 0.]), &sk, (30., 47.), [-8., -8., -6.], [16., 16., 1.]);
                }
            }
            let body = b.part([0.; 3], [0.; 3]);
            bx(out, &body, &sk, (16., 20.), [-4., 0., -3.], [8., 12., 6.]);
            cube(out, &body, &sk, 0., 38., [-4., 0., -3.], [8., 20., 6.], 0.5, false, ALL_FACES);
            if id == "zombie_villager" {
                let mut l = Limbs::humanoid();
                l.rarm.1 = [-PI / 2.25, 0., 0.];
                l.larm.1 = [-PI / 2.25, 0., 0.];
                bx(out, &b.part(l.rarm.0, l.rarm.1), &sk, (44., 22.), [-3., -2., -2.], [4., 12., 4.]);
                cube(out, &b.part(l.larm.0, l.larm.1), &sk, 44., 22., [-1., -2., -2.], [4., 12., 4.], 0.0, true, ALL_FACES);
            } else {
                let arms = b.part([0., 3., -1.], [-0.75, 0., 0.]);
                bx(out, &arms, &sk, (44., 22.), [-8., -2., -2.], [4., 8., 4.]);
                cube(out, &arms, &sk, 44., 22., [4., -2., -2.], [4., 8., 4.], 0.0, true, ALL_FACES);
                bx(out, &arms, &sk, (40., 38.), [-4., 2., -2.], [8., 4., 4.]);
            }
            bx(out, &b.part([-2., 12., 0.], [0.; 3]), &sk, (0., 22.), [-2., 0., -2.], [4., 12., 4.]);
            cube(out, &b.part([2., 12., 0.], [0.; 3]), &sk, 0., 22., [-2., 0., -2.], [4., 12., 4.], 0.0, true, ALL_FACES);
            if id == "zombie_villager" {
                let mut l = Limbs::humanoid();
                l.rarm.1 = [-PI / 2.25, 0., 0.];
                l.larm.1 = [-PI / 2.25, 0., 0.];
                armor(reg, ent, &b, &l, false, out);
                held_items(reg, ent, &b, &l, false, out);
            } else {
                crossed_arms_item(reg, ent, &b, out);
            }
        }
        "iron_golem" => {
            let Some(sk) = skin(reg, &[e("iron_golem/iron_golem"), e("iron_golem")], Tint::None) else { return true };
            let b = living(pos, yaw, 1.0);
            let hp = b.part([0., -7., -2.], [0.; 3]);
            bx(out, &hp, &sk, (0., 0.), [-4., -12., -5.5], [8., 10., 8.]);
            bx(out, &hp, &sk, (24., 0.), [-1., -5., -7.5], [2., 4., 2.]);
            let bp = b.part([0., -7., 0.], [0.; 3]);
            bx(out, &bp, &sk, (0., 40.), [-9., -2., -6.], [18., 12., 11.]);
            cube(out, &bp, &sk, 0., 70., [-4.5, 10., -3.], [9., 5., 6.], 0.5, false, ALL_FACES);
            bx(out, &bp, &sk, (60., 21.), [-13., -2.5, -3.], [4., 30., 6.]);
            bx(out, &bp, &sk, (60., 58.), [9., -2.5, -3.], [4., 30., 6.]);
            bx(out, &b.part([-4., 11., 0.], [0.; 3]), &sk, (37., 0.), [-3.5, -3., -3.], [6., 16., 5.]);
            cube(out, &b.part([5., 11., 0.], [0.; 3]), &sk, 60., 0., [-3.5, -3., -3.], [6., 16., 5.], 0.0, true, ALL_FACES);
        }
        "snow_golem" => {
            let Some(sk) = skin(reg, &[e("snow_golem/snow_golem"), e("snowman")], Tint::None) else { return true };
            let b = living(pos, yaw, 1.0);
            let hp = b.part([0., 4., 0.], [0.; 3]);
            cube(out, &hp, &sk, 0., 0., [-4., -8., -4.], [8., 8., 8.], -0.5, false, ALL_FACES);
            cube(out, &b.part([5., 6., 1.], [0., 0., 1.]), &sk, 32., 0., [-1., 0., -1.], [12., 2., 2.], -0.5, false, ALL_FACES);
            cube(out, &b.part([-5., 6., -1.], [0., PI, -1.]), &sk, 32., 0., [-1., 0., -1.], [12., 2., 2.], -0.5, false, ALL_FACES);
            cube(out, &b.part([0., 13., 0.], [0.; 3]), &sk, 0., 16., [-5., -10., -5.], [10., 10., 10.], -0.5, false, ALL_FACES);
            cube(out, &b.part([0., 24., 0.], [0.; 3]), &sk, 0., 36., [-6., -12., -6.], [12., 12., 12.], -0.5, false, ALL_FACES);
            if int(get(Some(ent), "Pumpkin")).unwrap_or(1) != 0 {
                let sid = reg.state_id("minecraft:carved_pumpkin", vec![("facing".into(), "north".into())]);
                let p = hp.t(0.0, -0.34375, 0.0).ry(180.0).s(0.625, -0.625, -0.625).t(-0.5, -0.5, -0.5);
                out.extend(reg.state_quads(sid).iter().map(|q| p.quad(q)));
            }
        }
        "wolf" => {
            let v = string(get(Some(ent), "variant")).unwrap_or("pale".into());
            let tame = get(Some(ent), "Owner").is_some();
            let suffix = if tame { "_tame" } else { "" };
            let mut tex = vec![e(&format!("wolf/wolf_{v}{suffix}")), e(&format!("wolf/wolf{suffix}")), e("wolf/wolf")];
            if v == "pale" {
                tex.insert(0, e(&format!("wolf/wolf{suffix}")));
            }
            let Some(sk) = skin(reg, &tex, Tint::None) else { return true };
            let b = living(pos, yaw, sc);
            let hp = b.part([-1., 13.5, -7.], [0.; 3]);
            bx(out, &hp, &sk, (0., 0.), [-2., -3., -2.], [6., 6., 4.]);
            bx(out, &hp, &sk, (16., 14.), [-2., -5., 0.], [2., 2., 1.]);
            bx(out, &hp, &sk, (16., 14.), [2., -5., 0.], [2., 2., 1.]);
            bx(out, &hp, &sk, (0., 10.), [-0.5, -0.001, -5.], [3., 3., 4.]);
            bx(out, &b.part([0., 14., 2.], [h, 0., 0.]), &sk, (18., 14.), [-3., -2., -3.], [6., 9., 6.]);
            bx(out, &b.part([-1., 14., -3.], [h, 0., 0.]), &sk, (21., 0.), [-3., -3., -3.], [8., 6., 7.]);
            quad_legs(out, &b, &sk, (0., 18.), [[-2.5, 16., 7.], [0.5, 16., 7.], [-2.5, 16., -4.], [0.5, 16., -4.]], [0., 0., -1.], [2., 8., 2.]);
            bx(out, &b.part([-1., 12., 8.], [0.62831855, 0., 0.]), &sk, (9., 18.), [0., 0., -1.], [2., 8., 2.]);
            crate::deco::wolf(reg, ent, &b, out);
        }
        "fox" => {
            let t = if string(get(Some(ent), "Type")).as_deref() == Some("snow") { "fox/fox_snow" } else { "fox/fox" };
            let Some(sk) = skin(reg, &[e(t), e("fox/fox")], Tint::None) else { return true };
            let b = living(pos, yaw, sc);
            let hp = b.part([-1., 16.5, -3.], [0.; 3]);
            bx(out, &hp, &sk, (1., 5.), [-3., -2., -5.], [8., 6., 6.]);
            bx(out, &hp, &sk, (8., 1.), [-3., -4., -4.], [2., 2., 1.]);
            bx(out, &hp, &sk, (15., 1.), [3., -4., -4.], [2., 2., 1.]);
            bx(out, &hp, &sk, (6., 18.), [-1., 2.01, -8.], [4., 2., 3.]);
            let body = b.part([0., 16., -6.], [h, 0., 0.]);
            bx(out, &body, &sk, (24., 15.), [-3., 3.999, -3.5], [6., 11., 6.]);
            for (o, u) in [([-5., 17.5, 7.], 4.), ([-1., 17.5, 7.], 13.), ([-5., 17.5, 0.], 4.), ([-1., 17.5, 0.], 13.)] {
                bx(out, &b.part(o, [0.; 3]), &sk, (u, 24.), [2., 0.5, -1.], [2., 6., 2.]);
            }
            bx(out, &body.part([-4., 15., -1.], [-0.05235988, 0., 0.]), &sk, (30., 0.), [2., 0., -1.], [4., 9., 5.]);
        }
        "cat" | "ocelot" => {
            let v = string(get(Some(ent), "variant")).unwrap_or("tabby".into());
            let tex = if id == "ocelot" { vec![e("cat/ocelot"), e("ocelot/ocelot")] } else { vec![e(&format!("cat/{v}")), e(&format!("cat/cat_{v}")), e("cat/tabby")] };
            let Some(sk) = skin(reg, &tex, Tint::None) else { return true };
            let b = living(pos, yaw, if id == "cat" { 0.8 * sc } else { sc });
            let hp = b.part([0., 15., -9.], [0.; 3]);
            bx(out, &hp, &sk, (0., 0.), [-2.5, -2., -3.], [5., 4., 5.]);
            bx(out, &hp, &sk, (0., 24.), [-1.5, -0.001, -4.], [3., 2., 2.]);
            bx(out, &hp, &sk, (0., 10.), [-2., -3., 0.], [1., 1., 2.]);
            bx(out, &hp, &sk, (6., 10.), [1., -3., 0.], [1., 1., 2.]);
            bx(out, &b.part([0., 12., -10.], [h, 0., 0.]), &sk, (20., 0.), [-2., 3., -8.], [4., 16., 6.]);
            bx(out, &b.part([0., 15., 8.], [0.9, 0., 0.]), &sk, (0., 15.), [-0.5, 0., 0.], [1., 8., 1.]);
            bx(out, &b.part([0., 20., 14.], [0.; 3]), &sk, (4., 15.), [-0.5, 0., 0.], [1., 8., 1.]);
            bx(out, &b.part([1.1, 18., 5.], [0.; 3]), &sk, (8., 13.), [-1., 0., 1.], [2., 6., 2.]);
            bx(out, &b.part([-1.1, 18., 5.], [0.; 3]), &sk, (8., 13.), [-1., 0., 1.], [2., 6., 2.]);
            bx(out, &b.part([1.2, 14.1, -5.], [0.; 3]), &sk, (40., 0.), [-1., 0., 0.], [2., 10., 2.]);
            bx(out, &b.part([-1.2, 14.1, -5.], [0.; 3]), &sk, (40., 0.), [-1., 0., 0.], [2., 10., 2.]);
        }
        "horse" | "donkey" | "mule" | "skeleton_horse" | "zombie_horse" => {
            let layers = if id == "horse" {
                let var = int(get(Some(ent), "Variant")).unwrap_or(0);
                let colors = ["white", "creamy", "chestnut", "brown", "black", "gray", "darkbrown"];
                let marks = ["", "white", "whitefield", "whitedots", "blackdots"];
                let c = colors[(var & 0xff).clamp(0, 6) as usize];
                let m = marks[((var >> 8) & 0xff).clamp(0, 4) as usize];
                let mut l = vec![vec![e(&format!("horse/horse_{c}"))]];
                if !m.is_empty() {
                    l.push(vec![e(&format!("horse/horse_markings_{m}"))]);
                }
                l
            } else {
                vec![vec![e(&format!("horse/{id}")), e(&format!("horse/horse_{}", id.replace("_horse", "")))]]
            };
            let Some(sk) = composite(reg, &layers) else { return true };
            let b = living(pos, yaw, if id == "horse" { 1.1 } else if id == "mule" { 0.92 } else if id == "donkey" { 0.87 } else { 1.0 });
            let body = b.part([0., 11., 5.], [0.; 3]);
            cube(out, &body, &sk, 0., 32., [-5., -8., -17.], [10., 10., 22.], 0.05, false, ALL_FACES);
            bx(out, &body.part([0., -5., 2.], [0.5235988, 0., 0.]), &sk, (42., 36.), [-1.5, 0., 0.], [3., 14., 4.]);
            let hp = b.part([0., 4., -12.], [0.5235988, 0., 0.]);
            bx(out, &hp, &sk, (0., 35.), [-2.05, -6., -2.], [4., 12., 7.]);
            bx(out, &hp, &sk, (0., 13.), [-3., -11., -2.], [6., 5., 7.]);
            bx(out, &hp, &sk, (56., 36.), [-1., -11., 5.01], [2., 16., 2.]);
            bx(out, &hp, &sk, (0., 25.), [-2., -11., -7.], [4., 5., 5.]);
            cube(out, &hp, &sk, 19., 16., [0.55, -13., 4.], [2., 3., 1.], -0.001, false, ALL_FACES);
            cube(out, &hp, &sk, 19., 16., [-2.55, -13., 4.], [2., 3., 1.], -0.001, false, ALL_FACES);
            cube(out, &b.part([4., 14., 7.], [0.; 3]), &sk, 48., 21., [-3., -1.01, -1.], [4., 11., 4.], 0.0, true, ALL_FACES);
            bx(out, &b.part([-4., 14., 7.], [0.; 3]), &sk, (48., 21.), [-1., -1.01, -1.], [4., 11., 4.]);
            cube(out, &b.part([4., 14., -10.], [0.; 3]), &sk, 48., 21., [-3., -1.01, -1.9], [4., 11., 4.], 0.0, true, ALL_FACES);
            bx(out, &b.part([-4., 14., -10.], [0.; 3]), &sk, (48., 21.), [-1., -1.01, -1.9], [4., 11., 4.]);
            crate::deco::horse(reg, ent, id, &b, &sk, out);
        }
        "polar_bear" => {
            let Some(sk) = skin(reg, &[e("bear/polarbear"), e("bear/polar_bear")], Tint::None) else { return true };
            let b = living(pos, yaw, sc);
            let hp = b.part([0., 10., -16.], [0.; 3]);
            bx(out, &hp, &sk, (0., 0.), [-3.5, -3., -3.], [7., 7., 7.]);
            bx(out, &hp, &sk, (0., 44.), [-2.5, 1., -6.], [5., 3., 3.]);
            bx(out, &hp, &sk, (26., 0.), [-4.5, -4., -1.], [2., 2., 1.]);
            cube(out, &hp, &sk, 26., 0., [2.5, -4., -1.], [2., 2., 1.], 0.0, true, ALL_FACES);
            let bp = b.part([-2., 9., 12.], [h, 0., 0.]);
            bx(out, &bp, &sk, (0., 19.), [-5., -13., -7.], [14., 14., 11.]);
            bx(out, &bp, &sk, (39., 0.), [-4., -25., -7.], [12., 12., 10.]);
            for o in [[-4.5, 14., 6.], [4.5, 14., 6.]] {
                bx(out, &b.part(o, [0.; 3]), &sk, (50., 22.), [-2., 0., -2.], [4., 10., 8.]);
            }
            for o in [[-3.5, 14., -8.], [3.5, 14., -8.]] {
                bx(out, &b.part(o, [0.; 3]), &sk, (50., 40.), [-2., 0., -2.], [4., 10., 6.]);
            }
        }
        "squid" | "glow_squid" => {
            let Some(sk) = skin(reg, &[e(&format!("squid/{id}")), e("squid/squid")], Tint::None) else { return true };
            let b = living(pos, yaw, sc);
            bx(out, &b.part([0., 8., 0.], [0.; 3]), &sk, (0., 0.), [-6., -8., -6.], [12., 16., 12.]);
            for i in 0..8 {
                let a = i as f32 * PI * 2.0 / 8.0;
                let (x, z) = (a.cos() * 5.0, a.sin() * 5.0);
                let yr = i as f32 * PI * -2.0 / 8.0 + PI / 2.0;
                bx(out, &b.part([x, 15., z], [0., yr, 0.]), &sk, (48., 0.), [-1., 0., -1.], [2., 18., 2.]);
            }
        }
        "copper_golem" => {
            let st = string(get(Some(ent), "weather_state")).unwrap_or("unaffected".into());
            let key = match st.as_str() {
                "exposed" => "minecraft:exposed_copper_golem_statue",
                "weathered" => "minecraft:weathered_copper_golem_statue",
                "oxidized" => "minecraft:oxidized_copper_golem_statue",
                _ => "minecraft:copper_golem_statue",
            };
            let fake = crate::registry::StateKey { name: key.into(), props: vec![("facing".into(), "south".into())] };
            let o = crate::bent::BeOpts { sign_text: false, download_skins: false };
            let p = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).ry(-yaw).t(-0.5, 0.0, -0.5);
            out.extend(crate::bent::quads(reg, reg.font(), &fake, None, false, &o).iter().map(|q| p.quad(q)));
        }
        _ => return rare_mob(reg, id, ent, pos, yaw, out),
    }
    true
}

fn humanoid_overlay(out: &mut Vec<Quad>, base: &Pose, sk: &Skin, l: &Limbs, g: f32) {
    let h = base.part(l.head.0, l.head.1);
    cube(out, &h, sk, 0., 0., [-4., -8., -4.], [8., 8., 8.], g + 0.25, false, ALL_FACES);
    cube(out, &base.part(l.body.0, l.body.1), sk, 16., 16., [-4., 0., -2.], [8., 12., 4.], g, false, ALL_FACES);
    cube(out, &base.part(l.rarm.0, l.rarm.1), sk, 40., 16., [-3., -2., -2.], [4., 12., 4.], g, false, ALL_FACES);
    cube(out, &base.part(l.larm.0, l.larm.1), sk, 40., 16., [-1., -2., -2.], [4., 12., 4.], g, true, ALL_FACES);
    cube(out, &base.part(l.rleg.0, l.rleg.1), sk, 0., 16., [-2., 0., -2.], [4., 12., 4.], g, false, ALL_FACES);
    cube(out, &base.part(l.lleg.0, l.lleg.1), sk, 0., 16., [-2., 0., -2.], [4., 12., 4.], g, true, ALL_FACES);
}

#[allow(dead_code)]
fn _unused() -> ([&'static str; 16], fn(&str) -> [u8; 3]) {
    (COLORS, dye_diffuse)
}

fn trim_of(item: &Value) -> Option<(String, String)> {
    let t = get(get(Some(item), "components"), "minecraft:trim").or_else(|| get(get(Some(item), "tag"), "Trim"))?;
    Some((string(get(Some(t), "material"))?, string(get(Some(t), "pattern"))?))
}

fn armor_material(id: &str) -> String {
    if id == "turtle_helmet" {
        return "turtle_scute".into();
    }
    let m = id.rsplit_once('_').map(|x| x.0).unwrap_or(id);
    if m == "golden" { "gold".into() } else { m.into() }
}

/// Image RGBA de la garniture (motif recoloré avec la palette du matériau)
fn trim_image(reg: &Registry, item: &Value, leggings: bool, w: u32, h: u32) -> Option<Vec<u8>> {
    let (mat, pat) = trim_of(item)?;
    let armor_mat = armor_material(&item_id(item)?);
    let mj = reg.assets.json(&format!("data/minecraft/trim_material/{mat}.json"));
    let mut asset = mj.as_ref().and_then(|j| j.get("asset_name")).and_then(|v| v.as_str()).unwrap_or(&mat).to_string();
    if let Some(j) = &mj {
        let o = j.get("override_armor_assets").or_else(|| j.get("override_armor_materials"));
        if let Some(x) = o.and_then(|o| o.get(format!("minecraft:{armor_mat}")).or_else(|| o.get(&armor_mat))).and_then(|v| v.as_str()) {
            asset = x.to_string();
        }
    }
    let pj = reg.assets.json(&format!("data/minecraft/trim_pattern/{pat}.json"));
    let passet = pj.as_ref().and_then(|j| j.get("asset_id")).and_then(|v| v.as_str()).unwrap_or(&pat).trim_start_matches("minecraft:").to_string();
    let kind = if leggings { "humanoid_leggings" } else { "humanoid" };
    let suffix = if leggings { "_leggings" } else { "" };
    let (_, patt) = reg.assets.first_texture(&[
        format!("minecraft:trims/entity/{kind}/{passet}"),
        format!("minecraft:trims/models/armor/{passet}{suffix}"),
    ])?;
    let key = reg.assets.texture("minecraft:trims/color_palettes/trim_palette")?;
    let pal = reg.assets.texture(&format!("minecraft:trims/color_palettes/{asset}"))?;
    if patt.w != w || patt.h != h {
        return None;
    }
    let keys: Vec<[u8; 3]> = key.rgba.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect();
    let cols: Vec<[u8; 3]> = pal.rgba.chunks_exact(4).map(|p| [p[0], p[1], p[2]]).collect();
    let mut img = patt.rgba.clone();
    for p in img.chunks_exact_mut(4) {
        if p[3] == 0 {
            continue;
        }
        let i = keys
            .iter()
            .enumerate()
            .min_by_key(|(_, k)| (0..3).map(|c| (k[c] as i32 - p[c] as i32).abs()).sum::<i32>())
            .map(|x| x.0)
            .unwrap_or(0);
        if let Some(c) = cols.get(i) {
            p[0] = c[0];
            p[1] = c[1];
            p[2] = c[2];
        }
    }
    Some(img)
}

/// Texture finale d'une pièce d'armure : calques (teintés) fusionnés + garniture
fn armor_texture(reg: &Registry, item: &Value, leggings: bool) -> Option<Skin> {
    let layers = armor_layers_raw(reg, item, leggings);
    let first = layers.first()?.0;
    let trim = trim_of(item);
    if layers.len() == 1 && trim.is_none() {
        return Some(first);
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (l, _) in &layers {
        l.tex.hash(&mut h);
        l.tint.hash(&mut h);
    }
    trim.hash(&mut h);
    leggings.hash(&mut h);
    let name = format!("gen:armor_{:016x}", h.finish());
    if !reg.assets.has_texture(&name) {
        let base = reg.assets.texture(&reg.tex_name(first.tex))?;
        let (w, hh) = (base.w, base.h);
        let mut img = vec![0u8; (w * hh * 4) as usize];
        let blend = |img: &mut Vec<u8>, src: &[u8], tint: Option<[u8; 3]>| {
            for (d, s) in img.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
                let a = s[3] as f32 / 255.0;
                for k in 0..3 {
                    let v = match tint {
                        Some(t) => s[k] as f32 * t[k] as f32 / 255.0,
                        None => s[k] as f32,
                    };
                    d[k] = (v * a + d[k] as f32 * (1.0 - a)) as u8;
                }
                d[3] = d[3].max(s[3]);
            }
        };
        for (l, _) in &layers {
            let Some(t) = reg.assets.texture(&reg.tex_name(l.tex)) else { continue };
            if t.w != w || t.h != hh {
                continue;
            }
            let tint = match l.tint {
                Tint::Fixed(c) => Some(c),
                _ => None,
            };
            blend(&mut img, &t.rgba, tint);
        }
        if let Some(t) = trim_image(reg, item, leggings, w, hh) {
            blend(&mut img, &t, None);
        }
        reg.assets.put_texture(&name, w, hh, img);
    }
    Some(Skin { tex: reg.tex_id(&name), w: first.w, h: first.h, tint: Tint::None })
}

/// Objets tenus par un mob humanoïde (ItemInHandLayer)
pub(crate) fn held_items(reg: &Registry, ent: &Value, base: &Pose, l: &Limbs, thin: bool, out: &mut Vec<Quad>) {
    for (slot, left) in [("mainhand", false), ("offhand", true)] {
        let Some(it) = equipment(ent, slot) else { continue };
        let (mut o, r) = if left { l.larm } else { l.rarm };
        if thin {
            o[0] += if left { -1.0 } else { 1.0 };
        }
        let p = base.part(o, r).rx(-90.0).ry(180.0).t(if left { -1.0 } else { 1.0 } / 16.0, 0.125, -0.625);
        let ctx = if left { "thirdperson_lefthand" } else { "thirdperson_righthand" };
        out.extend(crate::entity::item_quads(reg, reg.font(), it, ctx).iter().map(|q| p.quad(q)));
    }
}

/// Objet tenu bras croisés (villageois, sorcière : CrossedArmsItemLayer)
fn crossed_arms_item(reg: &Registry, ent: &Value, base: &Pose, out: &mut Vec<Quad>) {
    if let Some(it) = equipment(ent, "mainhand") {
        let p = base.t(0.0, 0.4, -0.4).rx(180.0);
        out.extend(crate::entity::item_quads(reg, reg.font(), it, "ground").iter().map(|q| p.quad(q)));
    }
}

fn variant_name(ent: &Value) -> Option<String> {
    string(get(Some(ent), "variant"))
}

/// Mobs plus rares : abeille, lama, chèvre, perroquet, axolotl, lapin, grenouille, tortue, panda, chauve-souris, dauphin
pub fn rare_mob(reg: &Registry, id: &str, ent: &Value, pos: [f64; 3], yaw: f32, out: &mut Vec<Quad>) -> bool {
    let h = PI / 2.0;
    let baby = int(get(Some(ent), "Age")).unwrap_or(0) < 0;
    let sc = if baby { 0.5 } else { 1.0 };
    let b = living(pos, yaw, sc);
    let flat = |o: &mut Vec<Quad>, p: &Pose, sk: &Skin, uv: (f32, f32), f: [f32; 3], sz: [f32; 3], mirror: bool| {
        cube(o, p, sk, uv.0, uv.1, f, sz, 0.0, mirror, ALL_FACES)
    };
    match id {
        "bee" => {
            let Some(sk) = skin(reg, &[e("bee/bee"), e("bee")], Tint::None) else { return true };
            let bone = b.part([0., 19., 0.], [0.; 3]);
            bx(out, &bone, &sk, (0., 0.), [-3.5, -4., -5.], [7., 7., 10.]);
            bx(out, &bone, &sk, (26., 7.), [0., -1., 5.], [0., 1., 2.]);
            let ant = bone.part([0., -2., -5.], [0.; 3]);
            bx(out, &ant, &sk, (2., 0.), [1.5, -2., -3.], [1., 2., 3.]);
            bx(out, &ant, &sk, (2., 3.), [-2.5, -2., -3.], [1., 2., 3.]);
            cube(out, &bone.part([-1.5, -4., -3.], [0., -0.2618, 0.]), &sk, 0., 18., [-9., 0., 0.], [9., 0., 6.], 0.001, false, ALL_FACES);
            cube(out, &bone.part([1.5, -4., -3.], [0., 0.2618, 0.]), &sk, 0., 18., [0., 0., 0.], [9., 0., 6.], 0.001, true, ALL_FACES);
            for (z, v) in [(-2., 1.), (0., 3.), (2., 5.)] {
                bx(out, &bone.part([1.5, 3., z], [0.; 3]), &sk, (26., v), [-5., 0., 0.], [7., 2., 0.]);
            }
        }
        "llama" | "trader_llama" => {
            let v = int(get(Some(ent), "Variant")).unwrap_or(0).clamp(0, 3) as usize;
            let c = ["creamy", "white", "brown", "gray"][v];
            let Some(sk) = skin(reg, &[e(&format!("llama/llama_{c}")), e(&format!("llama/{c}"))], Tint::None) else { return true };
            let hp = b.part([0., 7., -6.], [0.; 3]);
            bx(out, &hp, &sk, (0., 0.), [-2., -14., -10.], [4., 4., 9.]);
            bx(out, &hp, &sk, (0., 14.), [-4., -16., -6.], [8., 18., 6.]);
            bx(out, &hp, &sk, (17., 0.), [-4., -19., -4.], [3., 3., 2.]);
            bx(out, &hp, &sk, (17., 0.), [1., -19., -4.], [3., 3., 2.]);
            bx(out, &b.part([0., 5., 2.], [h, 0., 0.]), &sk, (29., 0.), [-6., -10., -7.], [12., 18., 10.]);
            for (i, o) in [[-3.5, 10., 6.], [3.5, 10., 6.], [-3.5, 10., -5.], [3.5, 10., -5.]].iter().enumerate() {
                flat(out, &b.part(*o, [0.; 3]), &sk, (29., 29.), [-2., 0., -2.], [4., 14., 4.], i % 2 == 1);
            }
            crate::deco::llama(reg, ent, id, &b, &sk, out);
        }
        "goat" => {
            let Some(sk) = skin(reg, &[e("goat/goat")], Tint::None) else { return true };
            let hp = b.part([1., 14., 0.], [0.; 3]);
            bx(out, &hp, &sk, (2., 61.), [-6., -11., -10.], [3., 2., 1.]);
            flat(out, &hp, &sk, (2., 61.), [2., -11., -10.], [3., 2., 1.], true);
            bx(out, &hp, &sk, (23., 52.), [-0.5, -3., -14.], [0., 7., 5.]);
            if int(get(Some(ent), "HasLeftHorn")).unwrap_or(1) != 0 {
                bx(out, &hp, &sk, (12., 55.), [-0.01, -16., -10.], [2., 7., 2.]);
            }
            if int(get(Some(ent), "HasRightHorn")).unwrap_or(1) != 0 {
                bx(out, &hp, &sk, (12., 55.), [-2.99, -16., -10.], [2., 7., 2.]);
            }
            bx(out, &hp.part([0., -8., -8.], [0.9599, 0., 0.]), &sk, (34., 46.), [-3., -4., -8.], [5., 7., 10.]);
            let bp = b.part([0., 24., 0.], [0.; 3]);
            bx(out, &bp, &sk, (1., 1.), [-4., -17., -7.], [9., 11., 16.]);
            bx(out, &bp, &sk, (0., 28.), [-5., -18., -8.], [11., 14., 11.]);
            bx(out, &b.part([1., 14., 4.], [0.; 3]), &sk, (36., 29.), [0., 4., 0.], [3., 6., 3.]);
            bx(out, &b.part([-3., 14., 4.], [0.; 3]), &sk, (49., 29.), [0., 4., 0.], [3., 6., 3.]);
            bx(out, &b.part([1., 14., -6.], [0.; 3]), &sk, (49., 2.), [0., 0., 0.], [3., 10., 3.]);
            bx(out, &b.part([-3., 14., -6.], [0.; 3]), &sk, (35., 2.), [0., 0., 0.], [3., 10., 3.]);
        }
        "parrot" => {
            let v = int(get(Some(ent), "Variant")).unwrap_or(0).clamp(0, 4) as usize;
            let c = ["red_blue", "blue", "green", "yellow_blue", "grey"][v];
            let Some(sk) = skin(reg, &[e(&format!("parrot/parrot_{c}"))], Tint::None) else { return true };
            bx(out, &b.part([0., 16.5, -3.], [0.4937, 0., 0.]), &sk, (2., 8.), [-1.5, 0., -1.5], [3., 6., 3.]);
            bx(out, &b.part([0., 21.07, 1.16], [1.015, 0., 0.]), &sk, (22., 1.), [-1.5, -1., -1.], [3., 4., 1.]);
            bx(out, &b.part([1.5, 16.94, -2.76], [-0.6981, -PI, 0.]), &sk, (19., 8.), [-0.5, 0., -1.5], [1., 5., 3.]);
            bx(out, &b.part([-1.5, 16.94, -2.76], [-0.6981, -PI, 0.]), &sk, (19., 8.), [-0.5, 0., -1.5], [1., 5., 3.]);
            let hp = b.part([0., 15.69, -2.76], [0.; 3]);
            bx(out, &hp, &sk, (2., 2.), [-1., -1.5, -1.], [2., 3., 2.]);
            bx(out, &hp.part([0., -2., -1.], [0.; 3]), &sk, (10., 0.), [-1., -0.5, -2.], [2., 1., 4.]);
            bx(out, &hp.part([0., -0.5, -1.5], [0.; 3]), &sk, (11., 7.), [-0.5, -1., -0.5], [1., 2., 1.]);
            bx(out, &hp.part([0., -1.75, -2.45], [0.; 3]), &sk, (16., 7.), [-0.5, 0., -0.5], [1., 2., 1.]);
            bx(out, &hp.part([0., -2.15, 0.15], [-0.2214, 0., 0.]), &sk, (2., 18.), [0., -4., -2.], [0., 5., 4.]);
            for x in [1., -1.] {
                bx(out, &b.part([x, 22., -1.05], [-0.0299, 0., 0.]), &sk, (14., 18.), [-0.5, 0., -0.5], [1., 2., 1.]);
            }
        }
        "axolotl" => {
            let v = int(get(Some(ent), "Variant")).unwrap_or(0).clamp(0, 4) as usize;
            let c = ["lucy", "wild", "gold", "cyan", "blue"][v];
            let Some(sk) = skin(reg, &[e(&format!("axolotl/axolotl_{c}"))], Tint::None) else { return true };
            let body = b.part([0., 19.5, 5.], [0.; 3]);
            bx(out, &body, &sk, (0., 11.), [-4., -2., -9.], [8., 4., 10.]);
            bx(out, &body, &sk, (2., 17.), [0., -3., -8.], [0., 5., 9.]);
            let head = body.part([0., 0., -9.], [0.; 3]);
            bx(out, &head, &sk, (0., 1.), [-4., -3., -5.], [8., 5., 5.]);
            bx(out, &head.part([0., -3., -1.], [0.; 3]), &sk, (3., 37.), [-4., -3., 0.], [8., 3., 0.]);
            bx(out, &head.part([-4., 0., -1.], [0.; 3]), &sk, (0., 40.), [-3., -5., 0.], [3., 7., 0.]);
            bx(out, &head.part([4., 0., -1.], [0.; 3]), &sk, (11., 40.), [0., -5., 0.], [3., 7., 0.]);
            bx(out, &body.part([-3.5, 1., -1.], [0.; 3]), &sk, (2., 13.), [-1., 0., 0.], [3., 5., 0.]);
            bx(out, &body.part([3.5, 1., -1.], [0.; 3]), &sk, (2., 13.), [-2., 0., 0.], [3., 5., 0.]);
            bx(out, &body.part([-3.5, 1., -8.], [0.; 3]), &sk, (2., 13.), [-1., 0., 0.], [3., 5., 0.]);
            bx(out, &body.part([3.5, 1., -8.], [0.; 3]), &sk, (2., 13.), [-2., 0., 0.], [3., 5., 0.]);
            bx(out, &body.part([0., 0., 1.], [0.; 3]), &sk, (2., 19.), [0., -3., 0.], [0., 5., 12.]);
        }
        "rabbit" => {
            let t = int(get(Some(ent), "RabbitType")).unwrap_or(0);
            let c = match t {
                1 => "white",
                2 => "black",
                3 => "white_splotched",
                4 => "gold",
                5 => "salt",
                99 => "caerbannog",
                _ => "brown",
            };
            let Some(sk) = skin(reg, &[e(&format!("rabbit/{c}")), e(&format!("rabbit/rabbit_{c}"))], Tint::None) else { return true };
            let body = b.part([0., 23., 4.], [-0.3927, 0., 0.]);
            bx(out, &body, &sk, (0., 0.), [-4., -6., -9.], [8., 6., 10.]);
            bx(out, &body.part([0., -4.9916, 0.0125], [0.; 3]), &sk, (20., 16.), [-2., -3.0084, -1.0125], [4., 4., 4.]);
            let head = body.part([0., -5.2929, -8.1213], [0.3927, 0., 0.]);
            bx(out, &head, &sk, (0., 16.), [-2.5, -3., -4.], [5., 5., 5.]);
            bx(out, &head.part([1.5, -3.7071, -0.8787], [0.; 3]), &sk, (32., 0.), [-1., -4.2929, -0.1213], [2., 5., 1.]);
            bx(out, &head.part([-1.5, -3.7071, -0.8787], [0.; 3]), &sk, (26., 0.), [-1., -4.2929, -0.1213], [2., 5., 1.]);
            let fl = body.part([0., -1.5349, -6.3108], [0.; 3]);
            bx(out, &fl.part([-2., 1.9239, 0.3827], [0.3927, 0., 0.]), &sk, (36., 18.), [-0.9, -1., -0.9], [2., 4., 2.]);
            bx(out, &fl.part([2., 1.9239, 0.4827], [0.3927, 0., 0.]), &sk, (44., 18.), [-1., -1., -1.], [2., 4., 2.]);
            let bl = b.part([0., 23., 4.], [0.; 3]);
            bx(out, &bl.part([-3., 0.5, 0.], [0.; 3]).part([0., -0.5, 0.], [0., 0.3927, 0.]), &sk, (20., 24.), [-1., 0., -5.], [2., 1., 6.]);
            bx(out, &bl.part([3., 0.5, 0.], [0.; 3]).part([0., -0.5, 0.], [0., -0.3927, 0.]), &sk, (36., 24.), [-1., 0., -5.], [2., 1., 6.]);
        }
        "frog" => {
            let v = variant_name(ent).unwrap_or("temperate".into());
            let Some(sk) = skin(reg, &[e(&format!("frog/frog_{v}")), e(&format!("frog/{v}_frog")), e("frog/frog_temperate"), e("frog/temperate_frog")], Tint::None) else { return true };
            let root = b.part([0., 24., 0.], [0.; 3]);
            let body = root.part([0., -2., 4.], [0.; 3]);
            bx(out, &body, &sk, (3., 1.), [-3.5, -2., -8.], [7., 3., 9.]);
            bx(out, &body, &sk, (23., 22.), [-3.5, -1., -8.], [7., 0., 9.]);
            let head = body.part([0., -2., -1.], [0.; 3]);
            bx(out, &head, &sk, (23., 13.), [-3.5, -1., -7.], [7., 0., 9.]);
            bx(out, &head, &sk, (0., 13.), [-3.5, -2., -7.], [7., 3., 9.]);
            let eyes = head.part([-0.5, 0., 2.], [0.; 3]);
            bx(out, &eyes.part([-1.5, -3., -6.5], [0.; 3]), &sk, (0., 0.), [-1.5, -1., -1.5], [3., 2., 3.]);
            bx(out, &eyes.part([2.5, -3., -6.5], [0.; 3]), &sk, (0., 5.), [-1.5, -1., -1.5], [3., 2., 3.]);
            let la = body.part([4., -1., -6.5], [0.; 3]);
            bx(out, &la, &sk, (0., 32.), [-1., 0., -1.], [2., 3., 3.]);
            bx(out, &la.part([0., 3., -1.], [0.; 3]), &sk, (18., 40.), [-4., 0.01, -4.], [8., 0., 8.]);
            let ra = body.part([-4., -1., -6.5], [0.; 3]);
            bx(out, &ra, &sk, (0., 38.), [-1., 0., -1.], [2., 3., 3.]);
            bx(out, &ra.part([0., 3., 0.], [0.; 3]), &sk, (2., 40.), [-4., 0.01, -5.], [8., 0., 8.]);
            let ll = root.part([3.5, -3., 4.], [0.; 3]);
            bx(out, &ll, &sk, (14., 25.), [-1., 0., -2.], [3., 3., 4.]);
            bx(out, &ll.part([2., 3., 0.], [0.; 3]), &sk, (2., 32.), [-4., 0.01, -4.], [8., 0., 8.]);
            let rl = root.part([-3.5, -3., 4.], [0.; 3]);
            bx(out, &rl, &sk, (0., 25.), [-2., 0., -2.], [3., 3., 4.]);
            bx(out, &rl.part([-2., 3., 0.], [0.; 3]), &sk, (18., 32.), [-4., 0.01, -4.], [8., 0., 8.]);
        }
        "turtle" => {
            let Some(sk) = skin(reg, &[e("turtle/big_sea_turtle"), e("turtle/turtle")], Tint::None) else { return true };
            bx(out, &b.part([0., 19., -10.], [0.; 3]), &sk, (3., 0.), [-3., -1., -3.], [6., 5., 6.]);
            let bp = b.part([0., 11., -10.], [h, 0., 0.]);
            bx(out, &bp, &sk, (7., 37.), [-9.5, 3., -10.], [19., 20., 6.]);
            bx(out, &bp, &sk, (31., 1.), [-5.5, 3., -13.], [11., 18., 3.]);
            bx(out, &b.part([-3.5, 22., 11.], [0.; 3]), &sk, (1., 23.), [-2., 0., 0.], [4., 1., 10.]);
            bx(out, &b.part([3.5, 22., 11.], [0.; 3]), &sk, (1., 12.), [-2., 0., 0.], [4., 1., 10.]);
            bx(out, &b.part([-5., 21., -4.], [0.; 3]), &sk, (27., 30.), [-13., 0., -2.], [13., 1., 5.]);
            bx(out, &b.part([5., 21., -4.], [0.; 3]), &sk, (27., 24.), [0., 0., -2.], [13., 1., 5.]);
        }
        "panda" => {
            let g = string(get(Some(ent), "MainGene")).unwrap_or("normal".into());
            let tex = if g == "normal" { vec![e("panda/panda")] } else { vec![e(&format!("panda/{g}_panda")), e("panda/panda")] };
            let Some(sk) = skin(reg, &tex, Tint::None) else { return true };
            let hp = b.part([0., 11.5, -17.], [0.; 3]);
            bx(out, &hp, &sk, (0., 6.), [-6.5, -5., -4.], [13., 10., 9.]);
            bx(out, &hp, &sk, (45., 16.), [-3.5, 0., -6.], [7., 5., 2.]);
            bx(out, &hp, &sk, (52., 25.), [3.5, -8., -1.], [5., 4., 1.]);
            bx(out, &hp, &sk, (52., 25.), [-8.5, -8., -1.], [5., 4., 1.]);
            bx(out, &b.part([0., 10., 0.], [h, 0., 0.]), &sk, (0., 25.), [-9.5, -13., -6.5], [19., 26., 13.]);
            for o in [[-5.5, 15., 9.], [5.5, 15., 9.], [-5.5, 15., -9.], [5.5, 15., -9.]] {
                bx(out, &b.part(o, [0.; 3]), &sk, (40., 0.), [-3., 0., -3.], [6., 9., 6.]);
            }
        }
        "bat" => {
            let Some(sk) = skin(reg, &[e("bat/bat"), e("bat")], Tint::None) else { return true };
            let body = b.part([0., 17., 0.], [0.; 3]);
            bx(out, &body, &sk, (0., 0.), [-1.5, 0., -1.], [3., 5., 2.]);
            let head = b.part([0., 17., 0.], [0.; 3]);
            bx(out, &head, &sk, (0., 7.), [-2., -3., -1.], [4., 3., 2.]);
            bx(out, &head.part([-1.5, -2., 0.], [0.; 3]), &sk, (1., 15.), [-2.5, -4., 0.], [3., 5., 0.]);
            bx(out, &head.part([1.1, -3., 0.], [0.; 3]), &sk, (8., 15.), [-0.1, -3., 0.], [3., 5., 0.]);
            let rw = body.part([-1.5, 0., 0.], [0.; 3]);
            bx(out, &rw, &sk, (12., 0.), [-2., -2., 0.], [2., 7., 0.]);
            bx(out, &rw.part([-2., 0., 0.], [0.; 3]), &sk, (16., 0.), [-6., -2., 0.], [6., 8., 0.]);
            let lw = body.part([1.5, 0., 0.], [0.; 3]);
            bx(out, &lw, &sk, (12., 7.), [0., -2., 0.], [2., 7., 0.]);
            bx(out, &lw.part([2., 0., 0.], [0.; 3]), &sk, (16., 8.), [0., -2., 0.], [6., 8., 0.]);
            bx(out, &body.part([0., 5., 0.], [0.; 3]), &sk, (16., 16.), [-1.5, 0., 0.], [3., 2., 0.]);
        }
        "dolphin" => {
            let Some(sk) = skin(reg, &[e("dolphin/dolphin"), e("dolphin")], Tint::None) else { return true };
            let body = b.part([0., 22., -5.], [0.; 3]);
            bx(out, &body, &sk, (22., 0.), [-4., -7., 0.], [8., 7., 13.]);
            bx(out, &body.part([0.; 3], [1.0471976, 0., 0.]), &sk, (51., 0.), [-0.5, 0., 8.], [1., 4., 5.]);
            flat(out, &body.part([2., -2., 4.], [1.0471976, 0., 2.0943952]), &sk, (48., 20.), [-0.5, -4., 0.], [1., 4., 7.], true);
            bx(out, &body.part([-2., -2., 4.], [1.0471976, 0., -2.0943952]), &sk, (48., 20.), [-0.5, -4., 0.], [1., 4., 7.]);
            let tail = body.part([0., -2.5, 11.], [-0.10471976, 0., 0.]);
            bx(out, &tail, &sk, (0., 19.), [-2., -2.5, 0.], [4., 5., 11.]);
            bx(out, &tail.part([0., 0., 9.], [0.; 3]), &sk, (19., 20.), [-5., -0.5, 0.], [10., 1., 6.]);
            let head = body.part([0., -4., -3.], [0.; 3]);
            bx(out, &head, &sk, (0., 0.), [-4., -3., -3.], [8., 7., 6.]);
            bx(out, &head, &sk, (0., 13.), [-1., 2., -7.], [2., 2., 4.]);
        }
        _ => return crate::mobs2::more_mob(reg, id, ent, pos, yaw, out),
    }
    true
}
