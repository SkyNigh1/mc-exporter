//! Mount decorations and worn items: saddles, armor (horse, wolf, nautilus),
//! llama carpets and chests, donkey chests, happy ghast harness, items worn on mob heads.

use crate::bent::e;
use crate::mobs::{get, int, string};
use crate::pose::{ALL_FACES, Pose, Skin, bx, cube};
use crate::registry::{Quad, Registry, Tint};
use fastnbt::Value;
use std::f32::consts::PI;
use std::hash::{Hash, Hasher};

/// Item worn on the body (horse armor, llama carpet, wolf armor, harness)
pub fn body_item(ent: &Value) -> Option<&Value> {
    get(get(Some(ent), "equipment"), "body")
        .or_else(|| get(Some(ent), "body_armor_item"))
        .or_else(|| get(Some(ent), "ArmorItem"))
        .or_else(|| get(Some(ent), "DecorItem"))
        .filter(|v| string(get(Some(v), "id")).map(|s| s != "air").unwrap_or(false))
}

pub fn has_saddle(ent: &Value) -> bool {
    get(get(Some(ent), "equipment"), "saddle").is_some()
        || get(Some(ent), "SaddleItem").is_some()
        || int(get(Some(ent), "Saddle")).unwrap_or(0) != 0
}

pub fn chested(ent: &Value) -> bool {
    int(get(Some(ent), "ChestedHorse")).unwrap_or(0) != 0
}

/// Equipment asset id of an item (iron_horse_armor → iron, golden → gold…)
fn asset_of(item_id: &str) -> String {
    let s = item_id
        .trim_end_matches("_horse_armor")
        .trim_end_matches("_nautilus_armor")

        .to_string();
    match s.as_str() {
        "golden" => "gold".into(),
        "wolf_armor" => "armadillo_scute".into(),
        _ => s,
    }
}

fn dyed(item: Option<&Value>) -> Option<[u8; 3]> {
    let it = item?;
    let c = get(get(Some(it), "components"), "minecraft:dyed_color");
    let n = int(c).or_else(|| int(get(c, "rgb"))).or_else(|| int(get(get(get(Some(it), "tag"), "display"), "color")))?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// Texture of an equipment layer (1.21.2+ format: equipment/<asset>.json), dyed layers merged
pub fn equip_skin(reg: &Registry, asset: &str, kind: &str, item: Option<&Value>, fallback: &[String]) -> Option<Skin> {
    let mut layers: Vec<(String, Option<[u8; 3]>)> = Vec::new();
    if let Some(j) = reg.assets.json(&format!("assets/minecraft/equipment/{asset}.json")) {
        if let Some(ls) = j.get("layers").and_then(|l| l.get(kind)).and_then(|l| l.as_array()) {
            for l in ls {
                let Some(t) = l.get("texture").and_then(|t| t.as_str()) else { continue };
                let tint = l.get("dyeable").map(|d| {
                    let undyed = d.get("color_when_undyed").and_then(|c| c.as_i64()).map(|n| [(n >> 16) as u8, (n >> 8) as u8, n as u8]);
                    dyed(item).or(undyed)
                });
                let id = e(&format!("equipment/{kind}/{}", t.trim_start_matches("minecraft:")));
                if reg.assets.texture(&id).is_some() {
                    if !(matches!(tint, Some(None))) {
                        layers.push((id, tint.flatten()));
                    }
                }
            }
        }
    }
    if layers.is_empty() {
        let (id, _) = reg.assets.first_texture(fallback)?;
        let tint = if asset == "leather" { Some(dyed(item).unwrap_or([0xA0, 0x65, 0x40])) } else { None };
        layers.push((id, tint));
    }
    if layers.len() == 1 && layers[0].1.is_none() {
        let t = reg.assets.texture(&layers[0].0)?;
        return Some(Skin { tex: reg.tex_id(&layers[0].0), w: t.w as f32, h: t.h as f32, tint: Tint::None });
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    layers.hash(&mut h);
    let name = format!("gen:equip_{:016x}", h.finish());
    let base = reg.assets.texture(&layers[0].0)?;
    if !reg.assets.has_texture(&name) {
        let mut img = vec![0u8; (base.w * base.h * 4) as usize];
        for (id, tint) in &layers {
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

fn saddle_skin(reg: &Registry, kind: &str, fallback: &[String]) -> Option<Skin> {
    equip_skin(reg, "saddle", kind, None, fallback)
}

fn body_skin(reg: &Registry, ent: &Value, kind: &str, fallback: impl Fn(&str) -> Vec<String>) -> Option<Skin> {
    let it = body_item(ent)?;
    let id = string(get(Some(it), "id"))?;
    let asset = asset_of(&id);
    equip_skin(reg, &asset, kind, Some(it), &fallback(&asset))
}

// ───────────── pig ─────────────

pub fn pig(reg: &Registry, ent: &Value, base: &Pose, out: &mut Vec<Quad>) {
    if !has_saddle(ent) {
        return;
    }
    let Some(sk) = saddle_skin(reg, "pig_saddle", &[e("pig/pig_saddle")]) else { return };
    let g = 0.5;
    let hp = base.part([0., 12., -6.], [0.; 3]);
    cube(out, &hp, &sk, 0., 0., [-4., -4., -8.], [8., 8., 8.], g, false, ALL_FACES);
    cube(out, &hp, &sk, 16., 16., [-2., 0., -9.], [4., 3., 1.], g, false, ALL_FACES);
    cube(out, &base.part([0., 11., 2.], [PI / 2.0, 0., 0.]), &sk, 28., 8., [-5., -10., -7.], [10., 16., 8.], g, false, ALL_FACES);
    for (x, z) in [(-3., 7.), (3., 7.), (-3., -5.), (3., -5.)] {
        cube(out, &base.part([x, 18., z], [0.; 3]), &sk, 0., 16., [-2., 0., -2.], [4., 6., 4.], g, false, ALL_FACES);
    }
}

// ───────────── horses, donkeys, mules ─────────────

fn horse_geo(out: &mut Vec<Quad>, b: &Pose, sk: &Skin, g: f32) {
    let body = b.part([0., 11., 5.], [0.; 3]);
    cube(out, &body, sk, 0., 32., [-5., -8., -17.], [10., 10., 22.], 0.05 + g, false, ALL_FACES);
    cube(out, &body.part([0., -5., 2.], [0.5235988, 0., 0.]), sk, 42., 36., [-1.5, 0., 0.], [3., 14., 4.], g, false, ALL_FACES);
    let hp = b.part([0., 4., -12.], [0.5235988, 0., 0.]);
    cube(out, &hp, sk, 0., 35., [-2.05, -6., -2.], [4., 12., 7.], g, false, ALL_FACES);
    cube(out, &hp, sk, 0., 13., [-3., -11., -2.], [6., 5., 7.], g, false, ALL_FACES);
    cube(out, &hp, sk, 56., 36., [-1., -11., 5.01], [2., 16., 2.], g, false, ALL_FACES);
    cube(out, &hp, sk, 0., 25., [-2., -11., -7.], [4., 5., 5.], g, false, ALL_FACES);
    cube(out, &b.part([4., 14., 7.], [0.; 3]), sk, 48., 21., [-3., -1.01, -1.], [4., 11., 4.], g, true, ALL_FACES);
    cube(out, &b.part([-4., 14., 7.], [0.; 3]), sk, 48., 21., [-1., -1.01, -1.], [4., 11., 4.], g, false, ALL_FACES);
    cube(out, &b.part([4., 14., -10.], [0.; 3]), sk, 48., 21., [-3., -1.01, -1.9], [4., 11., 4.], g, true, ALL_FACES);
    cube(out, &b.part([-4., 14., -10.], [0.; 3]), sk, 48., 21., [-1., -1.01, -1.9], [4., 11., 4.], g, false, ALL_FACES);
}

/// Saddle, armor and chests of equines. `own`: the horse texture (older versions draw the saddle in it)
pub fn horse(reg: &Registry, ent: &Value, id: &str, b: &Pose, own: &Skin, out: &mut Vec<Quad>) {
    if id == "horse" {
        if let Some(sk) = body_skin(reg, ent, "horse_body", |a| vec![e(&format!("horse/armor/horse_armor_{a}"))]) {
            horse_geo(out, b, &sk, 0.1);
        }
    }
    if (id == "donkey" || id == "mule") && chested(ent) {
        let body = b.part([0., 11., 5.], [0.; 3]);
        bx(out, &body.part([6., -8., 0.], [0., -PI / 2.0, 0.]), own, (26., 21.), [-4., 0., -2.], [8., 8., 3.]);
        bx(out, &body.part([-6., -8., 0.], [0., PI / 2.0, 0.]), own, (26., 21.), [-4., 0., -2.], [8., 8., 3.]);
    }
    if has_saddle(ent) {
        let kind = format!("{id}_saddle");
        let sk = saddle_skin(reg, &kind, &[]).unwrap_or(*own);
        let body = b.part([0., 11., 5.], [0.; 3]);
        cube(out, &body, &sk, 26., 0., [-5., -8., -9.], [10., 9., 9.], 0.5, false, ALL_FACES);
        let hp = b.part([0., 4., -12.], [0.5235988, 0., 0.]);
        bx(out, &hp, &sk, (29., 5.), [2., -9., -6.], [1., 2., 2.]);
        bx(out, &hp, &sk, (29., 5.), [-3., -9., -6.], [1., 2., 2.]);
        bx(out, &hp.part([0.; 3], [-0.5235988, 0., 0.]), &sk, (32., 2.), [3.1, -6., -8.], [0., 3., 16.]);
        bx(out, &hp.part([0.; 3], [-0.5235988, 0., 0.]), &sk, (32., 2.), [-3.1, -6., -8.], [0., 3., 16.]);
        cube(out, &hp, &sk, 1., 1., [-3., -11., -1.9], [6., 5., 6.], 0.22, false, ALL_FACES);
        cube(out, &hp, &sk, 19., 0., [-2., -11., -4.], [4., 5., 2.], 0.2, false, ALL_FACES);
    }
}

// ───────────── llamas ─────────────

pub fn llama(reg: &Registry, ent: &Value, id: &str, b: &Pose, own: &Skin, out: &mut Vec<Quad>) {
    if chested(ent) {
        bx(out, &b.part([-8.5, 3., 3.], [0., PI / 2.0, 0.]), own, (45., 28.), [-3., 0., 0.], [8., 8., 3.]);
        bx(out, &b.part([5.5, 3., 3.], [0., PI / 2.0, 0.]), own, (45., 41.), [-3., 0., 0.], [8., 8., 3.]);
    }
    let sk = body_skin(reg, ent, "llama_body", |a| vec![e(&format!("llama/decor/{}", a.trim_end_matches("_carpet")))])
        .or_else(|| if id == "trader_llama" { equip_skin(reg, "trader_llama", "llama_body", None, &[e("llama/decor/trader_llama")]) } else { None });
    let Some(sk) = sk else { return };
    let g = 0.5;
    let hp = b.part([0., 7., -6.], [0.; 3]);
    cube(out, &hp, &sk, 0., 0., [-2., -14., -10.], [4., 4., 9.], g, false, ALL_FACES);
    cube(out, &hp, &sk, 0., 14., [-4., -16., -6.], [8., 18., 6.], g, false, ALL_FACES);
    cube(out, &hp, &sk, 17., 0., [-4., -19., -4.], [3., 3., 2.], g, false, ALL_FACES);
    cube(out, &hp, &sk, 17., 0., [1., -19., -4.], [3., 3., 2.], g, false, ALL_FACES);
    cube(out, &b.part([0., 5., 2.], [PI / 2.0, 0., 0.]), &sk, 29., 0., [-6., -10., -7.], [12., 18., 10.], g, false, ALL_FACES);
    for (i, o) in [[-3.5, 10., 6.], [3.5, 10., 6.], [-3.5, 10., -5.], [3.5, 10., -5.]].iter().enumerate() {
        cube(out, &b.part(*o, [0.; 3]), &sk, 29., 29., [-2., 0., -2.], [4., 14., 4.], g, i % 2 == 1, ALL_FACES);
    }
}

// ───────────── wolf ─────────────

pub fn wolf(reg: &Registry, ent: &Value, b: &Pose, out: &mut Vec<Quad>) {
    let Some(sk) = body_skin(reg, ent, "wolf_body", |_| vec![e("wolf/wolf_armor")]) else { return };
    let g = 0.2;
    let hp = b.part([-1., 13.5, -7.], [0.; 3]);
    cube(out, &hp, &sk, 0., 0., [-2., -3., -2.], [6., 6., 4.], g, false, ALL_FACES);
    cube(out, &hp, &sk, 16., 14., [-2., -5., 0.], [2., 2., 1.], g, false, ALL_FACES);
    cube(out, &hp, &sk, 16., 14., [2., -5., 0.], [2., 2., 1.], g, false, ALL_FACES);
    cube(out, &hp, &sk, 0., 10., [-0.5, -0.001, -5.], [3., 3., 4.], g, false, ALL_FACES);
    cube(out, &b.part([0., 14., 2.], [PI / 2.0, 0., 0.]), &sk, 18., 14., [-3., -2., -3.], [6., 9., 6.], g, false, ALL_FACES);
    cube(out, &b.part([-1., 14., -3.], [PI / 2.0, 0., 0.]), &sk, 21., 0., [-3., -3., -3.], [8., 6., 7.], g, false, ALL_FACES);
    for o in [[-2.5, 16., 7.], [0.5, 16., 7.], [-2.5, 16., -4.], [0.5, 16., -4.]] {
        cube(out, &b.part(o, [0.; 3]), &sk, 0., 18., [0., 0., -1.], [2., 8., 2.], g, false, ALL_FACES);
    }
    cube(out, &b.part([-1., 12., 8.], [0.62831855, 0., 0.]), &sk, 9., 18., [0., 0., -1.], [2., 8., 2.], g, false, ALL_FACES);
}

// ───────────── strider, camel, nautilus, happy ghast ─────────────

pub fn strider(reg: &Registry, ent: &Value, b: &Pose, out: &mut Vec<Quad>) {
    if !has_saddle(ent) {
        return;
    }
    let Some(sk) = saddle_skin(reg, "strider_saddle", &[e("strider/strider_saddle")]) else { return };
    // same geometry as the body: slight inflation to avoid coplanar faces
    cube(out, &b.part([0., 1., 0.], [0.; 3]), &sk, 0., 0., [-8., -6., -8.], [16., 14., 16.], 0.02, false, ALL_FACES);
}

pub fn camel(reg: &Registry, ent: &Value, b: &Pose, out: &mut Vec<Quad>) {
    if !has_saddle(ent) {
        return;
    }
    let id = string(get(Some(ent), "id")).unwrap_or_default();
    let kind = if id == "camel_husk" { "camel_husk_saddle" } else { "camel_saddle" };
    let Some(sk) = saddle_skin(reg, kind, &[]) else { return };
    let g = 0.05;
    let body = b.part([0., 4., 9.5], [0.; 3]);
    cube(out, &body, &sk, 74., 64., [-4.5, -17., -15.5], [9., 5., 11.], g, false, ALL_FACES);
    cube(out, &body, &sk, 92., 114., [-3.5, -20., -15.5], [7., 3., 11.], g, false, ALL_FACES);
    cube(out, &body, &sk, 0., 89., [-7.5, -12., -23.5], [15., 12., 27.], g, false, ALL_FACES);
    let head = body.part([0., -3., -19.5], [0.; 3]);
    bx(out, &head, &sk, (98., 42.), [3.51, -18., -17.], [0., 7., 15.]);
    bx(out, &head, &sk, (84., 57.), [-3.5, -18., -2.], [7., 7., 0.]);
    bx(out, &head, &sk, (98., 42.), [-3.51, -18., -17.], [0., 7., 15.]);
    cube(out, &head, &sk, 60., 87., [-3.5, -7., -15.], [7., 8., 19.], g, false, ALL_FACES);
    cube(out, &head, &sk, 21., 64., [-3.5, -21., -15.], [7., 14., 7.], g, false, ALL_FACES);
    cube(out, &head, &sk, 50., 64., [-2.5, -21., -21.], [5., 5., 6.], g, false, ALL_FACES);
    bx(out, &head, &sk, (74., 70.), [2.5, -19., -18.], [1., 2., 2.]);
    cube(out, &head, &sk, 74., 70., [-3.5, -19., -18.], [1., 2., 2.], 0.0, true, ALL_FACES);
}

pub fn nautilus(reg: &Registry, ent: &Value, root: &Pose, out: &mut Vec<Quad>) {
    let shell = root.part([0., -13., 5.], [0.; 3]);
    if let Some(sk) = body_skin(reg, ent, "nautilus_body", |_| Vec::new()) {
        cube(out, &shell, &sk, 0., 0., [-7., -10., -7.], [14., 10., 16.], 0.03, false, ALL_FACES);
        cube(out, &shell, &sk, 0., 26., [-7., 0., -7.], [14., 8., 20.], 0.03, false, ALL_FACES);
    }
    if has_saddle(ent) {
        if let Some(sk) = saddle_skin(reg, "nautilus_saddle", &[]) {
            cube(out, &shell, &sk, 0., 0., [-7., -10., -7.], [14., 10., 16.], 0.2, false, ALL_FACES);
        }
    }
}

pub fn happy_ghast(reg: &Registry, ent: &Value, b: &Pose, out: &mut Vec<Quad>) {
    let Some(sk) = body_skin(reg, ent, "happy_ghast_body", |_| Vec::new()) else { return };
    cube(out, &b.part([0., 24., 0.], [0.; 3]), &sk, 0., 0., [-8., -16., -8.], [16., 16., 16.], 0.02, false, ALL_FACES);
    cube(out, &b.part([0., 14., -5.5], [0.; 3]), &sk, 0., 32., [-8., -2.5, -2.5], [16., 5., 5.], 0.15, false, ALL_FACES);
}

// ───────────── item worn on the head (CustomHeadLayer) ─────────────

pub fn head_item(reg: &Registry, ent: &Value, head: &Pose, out: &mut Vec<Quad>) {
    let Some(it) = crate::mobs::equipment(ent, "head") else { return };
    let id = string(get(Some(it), "id")).unwrap_or_default();
    if id.ends_with("_helmet") {
        return;
    }
    let p = head.t(0.0, -0.25, 0.0).ry(180.0).s(0.625, -0.625, -0.625);
    out.extend(crate::entity::item_quads(reg, reg.font(), it, "head").iter().map(|q| p.quad(q)));
}
