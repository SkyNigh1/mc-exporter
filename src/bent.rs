//! Blocs dessinés par du code dans le jeu (« block entities ») : coffres, shulkers, têtes,
//! bannières, lits et panneaux (anciennes versions), texte des panneaux, pot décoré, cloche…
//! Les dimensions viennent des modèles du jeu (ChestModel, ShulkerModel, BannerModel…).

use crate::font::{self, Align, Font, Span};
use crate::pose::{self, ALL_FACES, F_NORTH, Pose, Skin, bx, cube};
use crate::registry::{Quad, Registry, StateKey, Tint};
use fastnbt::Value;
use std::f32::consts::PI;
use std::hash::{Hash, Hasher};

pub const COLORS: [&str; 16] = [
    "white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan", "purple", "blue",
    "brown", "green", "red", "black",
];

/// Couleur « diffuse » des teintures (bannières, laine des moutons)
pub fn dye_diffuse(name: &str) -> [u8; 3] {
    let n: u32 = match name {
        "white" => 0xF9FFFE,
        "orange" => 0xF9801D,
        "magenta" => 0xC74EBD,
        "light_blue" => 0x3AB3DA,
        "yellow" => 0xFED83D,
        "lime" => 0x80C71F,
        "pink" => 0xF38BAA,
        "gray" => 0x474F52,
        "light_gray" => 0x9D9D97,
        "cyan" => 0x169C9C,
        "purple" => 0x8932B8,
        "blue" => 0x3C44AA,
        "brown" => 0x835432,
        "green" => 0x5E7C16,
        "red" => 0xB02E26,
        _ => 0x1D1D21,
    };
    [(n >> 16) as u8, (n >> 8) as u8, n as u8]
}

/// Ce bloc a-t-il besoin d'un rendu spécial ?
pub fn is_block_entity(short: &str) -> bool {
    short.ends_with("chest")
        || short.ends_with("shulker_box")
        || (short.ends_with("_head") && short != "piston_head")
        || short.ends_with("_skull")
        || short.ends_with("banner")
        || short.ends_with("_bed")
        || short.ends_with("sign")
        || short.ends_with("copper_golem_statue")
        || matches!(short, "decorated_pot" | "bell" | "conduit" | "end_portal" | "end_gateway")
}

pub fn to_y_rot(dir: &str) -> f32 {
    match dir {
        "west" => 90.0,
        "north" => 180.0,
        "east" => 270.0,
        _ => 0.0,
    }
}

fn dir_step(dir: &str) -> [f32; 3] {
    match dir {
        "north" => [0., 0., -1.],
        "south" => [0., 0., 1.],
        "west" => [-1., 0., 0.],
        "east" => [1., 0., 0.],
        "up" => [0., 1., 0.],
        "down" => [0., -1., 0.],
        _ => [0., 0., 0.],
    }
}

pub fn skin(reg: &Registry, candidates: &[String], tint: Tint) -> Option<Skin> {
    let (id, t) = reg.assets.first_texture(candidates)?;
    Some(Skin { tex: reg.tex_id(&id), w: t.w as f32, h: t.h as f32, tint })
}

pub fn e(p: &str) -> String {
    format!("minecraft:entity/{p}")
}

fn get<'a>(v: Option<&'a Value>, k: &str) -> Option<&'a Value> {
    match v? {
        Value::Compound(m) => m.get(k),
        _ => None,
    }
}

fn nbt_str(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn nbt_int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Byte(x) => Some(*x as i64),
        Value::Short(x) => Some(*x as i64),
        Value::Int(x) => Some(*x as i64),
        Value::Long(x) => Some(*x),
        _ => None,
    }
}

pub struct BeOpts {
    pub sign_text: bool,
    pub download_skins: bool,
}

/// Quads (coordonnées locales du bloc) pour un bloc à rendu spécial.
/// `has_model` : le jar fournit déjà la géométrie (lits et panneaux en 26.x).
pub fn quads(reg: &Registry, font: &Font, key: &StateKey, nbt: Option<&Value>, has_model: bool, o: &BeOpts) -> Vec<Quad> {
    let n = key.name.strip_prefix("minecraft:").unwrap_or(&key.name);
    let mut out = Vec::new();
    let facing = key.prop("facing").unwrap_or("south").to_string();
    let rotation: f32 = key.prop("rotation").and_then(|r| r.parse().ok()).unwrap_or(0.0);
    if n.ends_with("chest") {
        chest(reg, n, key.prop("type").unwrap_or("single"), &facing, &mut out);
    } else if n.ends_with("shulker_box") {
        shulker(reg, n, &facing, &mut out);
    } else if n.ends_with("copper_golem_statue") {
        copper_golem_statue(reg, n, &facing, key.prop("copper_golem_pose").unwrap_or("standing"), &mut out);
    } else if n.ends_with("_head") || n.ends_with("_skull") {
        skull(reg, n, &facing, rotation, nbt, o, &mut out);
    } else if n.ends_with("banner") {
        banner(reg, n, &facing, rotation, nbt, &mut out);
    } else if n.ends_with("_bed") {
        if !has_model {
            bed(reg, n, &facing, key.prop("part").unwrap_or("foot"), &mut out);
        }
    } else if n.ends_with("sign") {
        let hanging = n.contains("hanging");
        let wall = n.contains("_wall_");
        if !has_model {
            sign_model(reg, n, hanging, wall, &facing, rotation, &mut out);
        }
        if o.sign_text {
            sign_text(reg, font, hanging, wall, &facing, rotation, nbt, &mut out);
        }
    } else {
        match n {
            "decorated_pot" => decorated_pot(reg, &facing, nbt, &mut out),
            "bell" => {
                if let Some(sk) = skin(reg, &[e("bell/bell_body")], Tint::None) {
                    let p = Pose::new().part([8., 12., 8.], [0.; 3]);
                    bx(&mut out, &p, &sk, (0., 0.), [-3., -6., -3.], [6., 7., 6.]);
                    let b = p.part([-8., -12., -8.], [0.; 3]);
                    bx(&mut out, &b, &sk, (0., 13.), [4., 4., 4.], [8., 2., 8.]);
                }
            }
            "conduit" => {
                if let Some(sk) = skin(reg, &[e("conduit/base")], Tint::None) {
                    bx(&mut out, &Pose::new().t(0.5, 0.5, 0.5), &sk, (0., 0.), [-3., -3., -3.], [6., 6., 6.]);
                }
            }
            "end_portal" | "end_gateway" => {
                if let Some(sk) = skin(reg, &[e("end_portal/end_portal"), e("end_portal")], Tint::None) {
                    let uv = [0.0, 1.0, 0.25, 0.75];
                    if n == "end_portal" {
                        let p = Pose::new().t(0.0, 0.75, 1.0).rx(-90.0);
                        out.push(pose::flat_quad(&p, sk.tex, 1.0, 1.0, uv));
                    } else {
                        let c = Skin { w: sk.w / 4.0, h: sk.h / 4.0, ..sk };
                        cube(&mut out, &Pose::new(), &c, 0., 0., [0., 0., 0.], [16., 16., 16.], 0.0, false, ALL_FACES);
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn chest(reg: &Registry, n: &str, typ: &str, facing: &str, out: &mut Vec<Quad>) {
    let base = if n == "trapped_chest" {
        "trapped".to_string()
    } else if n == "ender_chest" {
        "ender".to_string()
    } else if n.contains("copper_chest") {
        let s = n.trim_start_matches("waxed_");
        match s.strip_suffix("_copper_chest") {
            Some(state) => format!("copper_{state}"),
            None => "copper".to_string(),
        }
    } else {
        "normal".to_string()
    };
    let typ = if n == "ender_chest" { "single" } else { typ };
    let suffix = match typ {
        "left" => "_left",
        "right" => "_right",
        _ => "",
    };
    let Some(sk) = skin(reg, &[e(&format!("chest/{base}{suffix}")), e(&format!("chest/normal{suffix}"))], Tint::None) else { return };
    // rotationAround(Y(-toYRot), 0.5, 0, 0.5)
    let p = Pose::new().t(0.5, 0.0, 0.5).ry(-to_y_rot(facing)).t(-0.5, 0.0, -0.5);
    let lid = p.part([0., 9., 1.], [0.; 3]);
    match typ {
        "left" => {
            bx(out, &p, &sk, (0., 19.), [0., 0., 1.], [15., 10., 14.]);
            bx(out, &lid, &sk, (0., 0.), [0., 0., 0.], [15., 5., 14.]);
            bx(out, &lid, &sk, (0., 0.), [0., -2., 14.], [1., 4., 1.]);
        }
        "right" => {
            bx(out, &p, &sk, (0., 19.), [1., 0., 1.], [15., 10., 14.]);
            bx(out, &lid, &sk, (0., 0.), [1., 0., 0.], [15., 5., 14.]);
            bx(out, &lid, &sk, (0., 0.), [15., -2., 14.], [1., 4., 1.]);
        }
        _ => {
            bx(out, &p, &sk, (0., 19.), [1., 0., 1.], [14., 10., 14.]);
            bx(out, &lid, &sk, (0., 0.), [1., 0., 0.], [14., 5., 14.]);
            bx(out, &lid, &sk, (0., 0.), [7., -2., 14.], [2., 4., 1.]);
        }
    }
}

/// Direction.getRotation()
fn dir_rotation(d: &str) -> pose::M4 {
    use pose::{mul, rot_x, rot_y, rot_z};
    let h = PI / 2.0;
    match d {
        "down" => rot_x(PI),
        "up" => pose::IDENT,
        "north" => mul(&rot_x(h), &mul(&rot_y(0.0), &rot_z(PI))),
        "south" => rot_x(h),
        "west" => mul(&rot_x(h), &rot_z(h)),
        "east" => mul(&rot_x(h), &rot_z(-h)),
        _ => pose::IDENT,
    }
}

fn shulker(reg: &Registry, n: &str, facing: &str, out: &mut Vec<Quad>) {
    let color = n.strip_suffix("_shulker_box").unwrap_or("");
    let mut c = vec![];
    if !color.is_empty() {
        c.push(e(&format!("shulker/shulker_{color}")));
    }
    c.push(e("shulker/shulker"));
    let Some(sk) = skin(reg, &c, Tint::None) else { return };
    let p = Pose::new().t(0.5, 0.5, 0.5).s(0.9995, 0.9995, 0.9995).m(&dir_rotation(facing)).s(1.0, -1.0, -1.0).t(0.0, -1.0, 0.0);
    let part = p.part([0., 24., 0.], [0.; 3]);
    bx(out, &part, &sk, (0., 0.), [-8., -16., -8.], [16., 12., 16.]);
    bx(out, &part, &sk, (0., 28.), [-8., -8., -8.], [16., 8., 16.]);
}

fn skull(reg: &Registry, n: &str, facing: &str, rotation: f32, nbt: Option<&Value>, o: &BeOpts, out: &mut Vec<Quad>) {
    let wall = n.contains("_wall_");
    let kind = n.replace("_wall", "");
    let tex: Vec<String> = match kind.as_str() {
        "skeleton_skull" => vec![e("skeleton/skeleton")],
        "wither_skeleton_skull" => vec![e("skeleton/wither_skeleton")],
        "zombie_head" => vec![e("zombie/zombie")],
        "creeper_head" => vec![e("creeper/creeper")],
        "piglin_head" => vec![e("piglin/piglin")],
        "dragon_head" => vec![e("enderdragon/dragon")],
        _ => {
            let mut v = Vec::new();
            if let Some(p) = crate::skins::profile_of(nbt) {
                if let Some(t) = crate::skins::skin_for_profile(reg, p, o.download_skins) {
                    v.push(t);
                }
            }
            v.push(e("player/wide/steve"));
            v.push(e("steve"));
            v
        }
    };
    let Some(sk) = skin(reg, &tex, Tint::None) else { return };
    let p = if wall {
        let s = dir_step(facing);
        let opp = match facing {
            "north" => "south",
            "south" => "north",
            "west" => "east",
            _ => "west",
        };
        Pose::new().t(0.5 - s[0] * 0.25, 0.25, 0.5 - s[2] * 0.25).ry(-to_y_rot(opp)).s(-1.0, -1.0, 1.0)
    } else {
        Pose::new().t(0.5, 0.0, 0.5).ry(-rotation * 22.5).s(-1.0, -1.0, 1.0)
    };
    if kind == "dragon_head" {
        // DragonHeadModel : pièce « head » réduite à 0,75, mâchoire enfant
        let h = p.part_s([0., -7.986666, 0.], [0.; 3], 0.75);
        bx(out, &h, &sk, (176., 44.), [-6., -1., -24.], [12., 5., 16.]);
        bx(out, &h, &sk, (112., 30.), [-8., -8., -10.], [16., 16., 16.]);
        cube(out, &h, &sk, 0., 0., [-5., -12., -4.], [2., 4., 6.], 0.0, true, ALL_FACES);
        cube(out, &h, &sk, 112., 0., [-5., -3., -22.], [2., 2., 4.], 0.0, true, ALL_FACES);
        bx(out, &h, &sk, (0., 0.), [3., -12., -4.], [2., 4., 6.]);
        bx(out, &h, &sk, (112., 0.), [3., -3., -22.], [2., 2., 4.]);
        let jaw = h.part([0., 4., -8.], [0.; 3]);
        bx(out, &jaw, &sk, (176., 65.), [-6., 0., -16.], [12., 4., 16.]);
        return;
    }
    if kind == "piglin_head" {
        bx(out, &p, &sk, (0., 0.), [-5., -8., -4.], [10., 8., 8.]);
        bx(out, &p, &sk, (31., 1.), [-2., -4., -5.], [4., 4., 1.]);
        bx(out, &p, &sk, (2., 4.), [2., -2., -5.], [1., 2., 1.]);
        bx(out, &p, &sk, (2., 0.), [-3., -2., -5.], [1., 2., 1.]);
        let le = p.part([4.5, -6., 0.], [0., 0., -PI / 6.]);
        bx(out, &le, &sk, (51., 6.), [0., 0., -2.], [1., 5., 4.]);
        let re = p.part([-4.5, -6., 0.], [0., 0., PI / 6.]);
        bx(out, &re, &sk, (39., 6.), [-1., 0., -2.], [1., 5., 4.]);
        return;
    }
    bx(out, &p, &sk, (0., 0.), [-4., -8., -4.], [8., 8., 8.]);
    if sk.h >= 64.0 && (kind == "player_head" || kind == "zombie_head") {
        // couche « chapeau » des têtes humanoïdes
        cube(out, &p, &sk, 32., 0., [-4., -8., -4.], [8., 8., 8.], 0.25, false, ALL_FACES);
    }
}

fn pattern_asset(reg: &Registry, id: &str) -> String {
    let (ns, p) = id.split_once(':').unwrap_or(("minecraft", id));
    reg.assets
        .json(&format!("data/{ns}/banner_pattern/{p}.json"))
        .and_then(|j| j.get("asset_id").and_then(|a| a.as_str()).map(|s| s.to_string()))
        .map(|a| a.split_once(':').map(|x| x.1.to_string()).unwrap_or(a))
        .unwrap_or_else(|| p.to_string())
}

fn legacy_pattern(code: &str) -> &'static str {
    match code {
        "b" => "base", "bl" => "square_bottom_left", "br" => "square_bottom_right", "tl" => "square_top_left",
        "tr" => "square_top_right", "bs" => "stripe_bottom", "ts" => "stripe_top", "ls" => "stripe_left",
        "rs" => "stripe_right", "cs" => "stripe_center", "ms" => "stripe_middle", "drs" => "stripe_downright",
        "dls" => "stripe_downleft", "ss" => "small_stripes", "cr" => "cross", "sc" => "straight_cross",
        "bt" => "triangle_bottom", "tt" => "triangle_top", "bts" => "triangles_bottom", "tts" => "triangles_top",
        "ld" => "diagonal_left", "rd" => "diagonal_up_right", "lud" => "diagonal_up_left", "rud" => "diagonal_right",
        "mc" => "circle", "mr" => "rhombus", "vh" => "half_vertical", "hh" => "half_horizontal",
        "vhr" => "half_vertical_right", "hhb" => "half_horizontal_bottom", "bo" => "border", "cbo" => "curly_border",
        "gra" => "gradient", "gru" => "gradient_up", "bri" => "bricks", "glb" => "globe", "cre" => "creeper",
        "sku" => "skull", "flo" => "flower", "moj" => "mojang", "pig" => "piglin", "flw" => "flow", "gus" => "guster",
        _ => "base",
    }
}

/// Liste (motif, couleur) d'une bannière, ancien et nouveau format
pub fn banner_layers(reg: &Registry, nbt: Option<&Value>) -> Vec<(String, [u8; 3])> {
    let mut v = Vec::new();
    let list = get(nbt, "patterns").or_else(|| get(nbt, "Patterns")).or_else(|| get(get(nbt, "components"), "minecraft:banner_patterns"));
    if let Some(Value::List(l)) = list {
        for p in l {
            let color = match get(Some(p), "color").or_else(|| get(Some(p), "Color")) {
                Some(Value::String(s)) => dye_diffuse(s),
                Some(x) => {
                    let i = nbt_int(Some(x)).unwrap_or(0).clamp(0, 15) as usize;
                    dye_diffuse(COLORS[i])
                }
                None => dye_diffuse("white"),
            };
            let pat = match get(Some(p), "pattern") {
                Some(Value::String(s)) => pattern_asset(reg, s),
                Some(c @ Value::Compound(_)) => nbt_str(get(Some(c), "asset_id")).map(|a| a.trim_start_matches("minecraft:").to_string()).unwrap_or("base".into()),
                _ => legacy_pattern(&nbt_str(get(Some(p), "Pattern")).unwrap_or_default()).to_string(),
            };
            v.push((pat, color));
        }
    }
    v
}

/// Texture composée d'une bannière (fond teint + motifs), mise en cache
pub fn banner_texture(reg: &Registry, base_color: &str, layers: &[(String, [u8; 3])]) -> Option<Skin> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    base_color.hash(&mut h);
    layers.hash(&mut h);
    let name = format!("gen:banner_{base_color}_{:08x}", h.finish() as u32);
    let base = reg.assets.first_texture(&[e("banner/banner_base"), e("banner_base"), e("banner/base")])?.1;
    if !reg.assets.has_texture(&name) {
        let mut img = base.rgba.clone();
        let mut all = vec![("base".to_string(), dye_diffuse(base_color))];
        all.extend(layers.iter().cloned());
        for (pat, col) in &all {
            let Some(t) = reg.assets.texture(&e(&format!("banner/{pat}"))) else { continue };
            if t.w != base.w || t.h != base.h {
                continue;
            }
            for (d, s) in img.chunks_exact_mut(4).zip(t.rgba.chunks_exact(4)) {
                let a = s[3] as f32 / 255.0;
                if a <= 0.0 {
                    continue;
                }
                for k in 0..3 {
                    let src = s[k] as f32 * col[k] as f32 / 255.0;
                    d[k] = (src * a + d[k] as f32 * (1.0 - a)) as u8;
                }
                d[3] = d[3].max(s[3]);
            }
        }
        reg.assets.put_texture(&name, base.w, base.h, img);
    }
    Some(Skin { tex: reg.tex_id(&name), w: base.w as f32, h: base.h as f32, tint: Tint::None })
}

fn banner(reg: &Registry, n: &str, facing: &str, rotation: f32, nbt: Option<&Value>, out: &mut Vec<Quad>) {
    let wall = n.ends_with("_wall_banner");
    let color = n.trim_end_matches("_wall_banner").trim_end_matches("_banner");
    let color = if color == "banner" || color.is_empty() {
        let b = nbt_int(get(nbt, "Base")).unwrap_or(0).clamp(0, 15) as usize;
        COLORS[15 - b]
    } else {
        color
    };
    let layers = banner_layers(reg, nbt);
    let Some(sk) = banner_texture(reg, color, &layers) else { return };
    let angle = if wall { to_y_rot(facing) } else { rotation * 22.5 };
    let p = Pose::new().t(0.5, 0.0, 0.5).ry(-angle).s(2.0 / 3.0, -2.0 / 3.0, -2.0 / 3.0);
    if wall {
        bx(out, &p, &sk, (0., 42.), [-10., -20.5, 9.5], [20., 2., 2.]);
        let f = p.part([0., -20.5, 10.5], [0.; 3]);
        bx(out, &f, &sk, (0., 0.), [-10., 0., -2.], [20., 40., 1.]);
    } else {
        bx(out, &p, &sk, (44., 0.), [-1., -42., -1.], [2., 42., 2.]);
        bx(out, &p, &sk, (0., 42.), [-10., -44., -1.], [20., 2., 2.]);
        let f = p.part([0., -44., 0.], [0.; 3]);
        bx(out, &f, &sk, (0., 0.), [-10., 0., -2.], [20., 40., 1.]);
    }
}

/// Lits (versions sans modèle de bloc)
fn bed(reg: &Registry, n: &str, facing: &str, part: &str, out: &mut Vec<Quad>) {
    let color = n.trim_end_matches("_bed");
    let Some(sk) = skin(reg, &[e(&format!("bed/{color}"))], Tint::None) else { return };
    let p = Pose::new().t(0.0, 0.5625, 0.0).rx(90.0).t(0.5, 0.5, 0.5).rz(180.0 + to_y_rot(facing)).t(-0.5, -0.5, -0.5);
    if part == "head" {
        bx(out, &p, &sk, (0., 0.), [0., 0., 0.], [16., 16., 6.]);
        let l = p.part([0., 0., 0.], [PI / 2.0, 0., PI / 2.0]);
        bx(out, &l, &sk, (50., 6.), [0., 6., 0.], [3., 3., 3.]);
        let r = p.part([0., 0., 0.], [PI / 2.0, 0., PI]);
        bx(out, &r, &sk, (50., 18.), [-16., 6., 0.], [3., 3., 3.]);
    } else {
        bx(out, &p, &sk, (0., 22.), [0., 0., 0.], [16., 16., 6.]);
        let l = p.part([0., 0., 0.], [PI / 2.0, 0., 0.]);
        bx(out, &l, &sk, (50., 0.), [0., 6., -16.], [3., 3., 3.]);
        let r = p.part([0., 0., 0.], [PI / 2.0, 0., PI * 1.5]);
        bx(out, &r, &sk, (50., 12.), [-16., 6., -16.], [3., 3., 3.]);
    }
}

fn wood_of(n: &str) -> String {
    n.trim_end_matches("_wall_hanging_sign")
        .trim_end_matches("_hanging_sign")
        .trim_end_matches("_wall_sign")
        .trim_end_matches("_sign")
        .to_string()
}

/// Panneaux (versions sans modèle de bloc : textures d'entité)
fn sign_model(reg: &Registry, n: &str, hanging: bool, wall: bool, facing: &str, rotation: f32, out: &mut Vec<Quad>) {
    let wood = wood_of(n);
    let angle = if wall { to_y_rot(facing) } else { rotation * 22.5 };
    if hanging {
        let Some(sk) = skin(reg, &[e(&format!("signs/hanging/{wood}"))], Tint::None) else { return };
        let p = Pose::new().t(0.5, 0.9375, 0.5).ry(-angle).t(0.0, -0.3125, 0.0).s(1.0, -1.0, -1.0);
        bx(out, &p, &sk, (0., 12.), [-7., 0., -1.], [14., 10., 2.]);
        if wall {
            bx(out, &p, &sk, (0., 0.), [-8., -6., -2.], [16., 2., 4.]);
        }
    } else {
        let Some(sk) = skin(reg, &[e(&format!("signs/{wood}"))], Tint::None) else { return };
        let mut p = Pose::new().t(0.5, 0.5, 0.5).ry(-angle);
        if wall {
            p = p.t(0.0, -0.3125, -0.4375);
        }
        let p = p.s(2.0 / 3.0, -2.0 / 3.0, -2.0 / 3.0);
        bx(out, &p, &sk, (0., 0.), [-12., -14., -1.], [24., 12., 2.]);
        if !wall {
            bx(out, &p, &sk, (0., 14.), [-1., -2., -1.], [2., 14., 2.]);
        }
    }
}

fn sign_side_lines(side: Option<&Value>, legacy: Option<&Value>) -> Option<(Vec<Vec<Span>>, [u8; 3])> {
    let (msgs, color, glow): (Vec<Value>, String, bool) = match side {
        Some(s) => {
            let m = match get(Some(s), "messages") {
                Some(Value::List(l)) => l.clone(),
                _ => return None,
            };
            let c = nbt_str(get(Some(s), "color")).unwrap_or("black".into());
            let g = nbt_int(get(Some(s), "has_glowing_text")).unwrap_or(0) != 0;
            (m, c, g)
        }
        None => {
            let l = legacy?;
            let m: Vec<Value> = (1..=4).filter_map(|i| get(Some(l), &format!("Text{i}")).cloned()).collect();
            if m.is_empty() {
                return None;
            }
            let c = nbt_str(get(Some(l), "Color")).unwrap_or("black".into());
            let g = nbt_int(get(Some(l), "GlowingText")).unwrap_or(0) != 0;
            (m, c, g)
        }
    };
    let base = font::dye_text_color(&color);
    let col = if glow { base } else { base.map(|c| (c as f32 * 0.4) as u8) };
    let lines: Vec<Vec<Span>> = msgs
        .iter()
        .map(|m| font::component_lines(m, col).into_iter().flatten().collect::<Vec<Span>>())
        .collect();
    if lines.iter().all(|l| l.iter().all(|(s, _)| s.trim().is_empty())) {
        return None;
    }
    Some((lines, col))
}

/// Texture de texte mise en cache ; renvoie (id, largeur, hauteur) en pixels de police
pub fn text_texture(reg: &Registry, font: &Font, lines: &[Vec<Span>], line_h: u32, align: Align, bg: [u8; 4], pad: u32) -> (u32, f32, f32) {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    lines.hash(&mut h);
    line_h.hash(&mut h);
    bg.hash(&mut h);
    pad.hash(&mut h);
    (align as u8).hash(&mut h);
    let name = format!("gen:text_{:016x}", h.finish());
    let (w, hh, img) = font.render(lines, line_h, align, bg, pad);
    if !reg.assets.has_texture(&name) {
        reg.assets.put_texture(&name, w, hh, img);
    }
    (reg.tex_id(&name), w as f32, hh as f32)
}

#[allow(clippy::too_many_arguments)]
fn sign_text(reg: &Registry, font: &Font, hanging: bool, wall: bool, facing: &str, rotation: f32, nbt: Option<&Value>, out: &mut Vec<Quad>) {
    let angle = if wall || (hanging && wall) { to_y_rot(facing) } else { rotation * 22.5 };
    let (line_h, max_lines) = if hanging { (9u32, 4) } else { (10u32, 4) };
    for front in [true, false] {
        let side = get(nbt, if front { "front_text" } else { "back_text" });
        let legacy = if front && side.is_none() { nbt } else { None };
        let Some((mut lines, _)) = sign_side_lines(side, legacy) else { continue };
        lines.truncate(max_lines);
        while lines.len() < 4 {
            lines.push(Vec::new());
        }
        let (tex, w, h) = text_texture(reg, font, &lines, line_h, Align::Center, [0, 0, 0, 0], 0);
        let mut p = if hanging {
            Pose::new().t(0.5, 0.9375, 0.5).ry(-angle).t(0.0, -0.3125, 0.0)
        } else {
            let mut p = Pose::new().t(0.5, 0.5, 0.5).ry(-angle);
            if wall {
                p = p.t(0.0, -0.3125, -0.4375);
            }
            p
        };
        if !front {
            p = p.ry(180.0);
        }
        let (off, s) = if hanging { ([0.0, -0.32, 0.073], 0.0140625) } else { ([0.0, 0.33333334, 0.046666667], 0.010416667) };
        // le texte est centré : x de -w/2 à w/2, y (vers le bas) de -2 lignes à +2 lignes
        let p = p.t(off[0], off[1], off[2] + 0.001).s(s, -s, s).t(-w / 2.0, -(line_h as f32) * 2.0 + h, 0.0).s(1.0, -1.0, 1.0);
        out.push(pose::flat_quad(&p, tex, w, h, [0.0, 1.0, 1.0, 0.0]));
    }
}

fn decorated_pot(reg: &Registry, facing: &str, nbt: Option<&Value>, out: &mut Vec<Quad>) {
    let Some(base) = skin(reg, &[e("decorated_pot/decorated_pot_base")], Tint::None) else { return };
    let p = Pose::new().t(0.5, 0.5, 0.5).ry(180.0 - to_y_rot(facing)).t(-0.5, -0.5, -0.5);
    let neck = p.part([0., 37., 16.], [PI, 0., 0.]);
    bx(out, &neck, &base, (0., 0.), [4., 17., 4.], [8., 3., 8.]);
    bx(out, &neck, &base, (0., 5.), [5., 20., 5.], [6., 1., 6.]);
    let top = p.part([1., 16., 1.], [0.; 3]);
    bx(out, &top, &base, (-14., 13.), [0., 0., 0.], [14., 0., 14.]);
    let bot = p.part([1., 0., 1.], [0.; 3]);
    bx(out, &bot, &base, (-14., 13.), [0., 0., 0.], [14., 0., 14.]);
    // côtés : arrière, gauche, droite, avant
    let sherds: Vec<String> = match get(nbt, "sherds") {
        Some(Value::List(l)) => l.iter().filter_map(|v| nbt_str(Some(v))).collect(),
        _ => Vec::new(),
    };
    let poses = [
        ([15., 16., 1.], [0., 0., PI]),
        ([1., 16., 1.], [0., -PI / 2.0, PI]),
        ([15., 16., 15.], [0., PI / 2.0, PI]),
        ([1., 16., 15.], [PI, 0., 0.]),
    ];
    for (i, (off, rot)) in poses.iter().enumerate() {
        let item = sherds.get(i).cloned().unwrap_or_else(|| "minecraft:brick".into());
        let short = item.trim_start_matches("minecraft:");
        let tex = match short.strip_suffix("_pottery_sherd") {
            Some(s) => vec![e(&format!("decorated_pot/{s}_pottery_pattern")), e("decorated_pot/decorated_pot_side")],
            None => vec![e("decorated_pot/decorated_pot_side")],
        };
        let Some(sk) = skin(reg, &tex, Tint::None) else { continue };
        let side = p.part(*off, *rot);
        cube(out, &side, &sk, 1., 0., [0., 0., 0.], [14., 16., 0.], 0.0, false, F_NORTH);
    }
}

/// Statue de golem de cuivre (pose debout), d'après CopperGolemModel
fn copper_golem_statue(reg: &Registry, n: &str, facing: &str, pose_name: &str, out: &mut Vec<Quad>) {
    let stage = ["exposed", "weathered", "oxidized"].iter().find(|s| n.contains(*s)).map(|s| format!("_{s}")).unwrap_or_default();
    let Some(sk) = skin(reg, &[e(&format!("copper_golem/copper_golem{stage}")), e("copper_golem/copper_golem")], Tint::None) else { return };
    let base = Pose::new().t(0.5, 0.0, 0.5).ry(180.0 - to_y_rot(facing)).s(-1.0, -1.0, 1.0).t(0.0, -1.501, 0.0).t(0.0, 1.5, 0.0);
    // (texOffs, origine, taille, gonflement) d'après CopperGolemModel
    let c = |o: &mut Vec<Quad>, p: &Pose, uv: (f32, f32), f: [f32; 3], sz: [f32; 3], g: f32| cube(o, p, &sk, uv.0, uv.1, f, sz, g, false, ALL_FACES);
    match pose_name {
        "running" => {
            let body = base.part([-1.064, -5., 0.], [0.; 3]);
            c(out, &body.part([1.1, 0.1, 0.7], [0.1204, -0.0064, -0.0779]), (0., 15.), [-4.02, -6.116, -3.5], [8., 6., 6.], 0.0);
            let head = body.part([0.7, -5.6, -1.8], [0.; 3]);
            c(out, &head, (0., 0.), [-4., -5.1, -5.], [8., 5., 10.], 0.0);
            c(out, &head, (56., 0.), [-1.02, -2.1, -6.], [2., 3., 2.], 0.0);
            c(out, &head, (37., 8.), [-1.02, -9.1, -1.], [2., 4., 2.], -0.015);
            c(out, &head, (37., 0.), [-2., -13.1, -2.], [4., 4., 4.], -0.015);
            c(out, &body.part([-4., -6., 0.], [0.; 3]).part([0.7, -0.248, -1.62], [1.0036, 0., 0.]), (36., 16.), [-3.052, -1.11, -2.036], [3., 10., 4.], 0.0);
            c(out, &body.part([4., -6., 0.], [0.; 3]).part([0.732, 0., 0.], [-0.8715, -0.0535, -0.0449]), (50., 16.), [0.032, -1.1, -2.], [3., 10., 4.], 0.0);
            c(out, &base.part([-3.064, -5., 0.], [0.; 3]).part([1.048, 0., -0.9], [-0.8727, 0., 0.]), (0., 27.), [-1.856, -0.1, -1.09], [4., 5., 4.], 0.0);
            c(out, &base.part([0.936, -5., 0.], [0.; 3]).part([1., 0., 0.], [0.7854, 0., 0.]), (16., 27.), [-2.088, -0.1, -2.], [4., 5., 4.], 0.0);
        }
        "sitting" => {
            let body = base.part([0., -3., 2.325], [0.; 3]);
            c(out, &body, (3., 19.), [-3., -4., -4.525], [6., 1., 6.], 0.0);
            c(out, &body, (0., 15.), [-4., -3., -3.525], [8., 6., 6.], 0.0);
            c(out, &body.part([0., -1., -4.325], [0., 0., -3.1416]), (3., 18.), [-4., -3., -2.2], [8., 6., 3.], 0.0);
            let head = body.part([0., -6., -0.2], [0.; 3]);
            c(out, &head, (37., 8.), [-1., -7., -3.3], [2., 4., 2.], -0.015);
            c(out, &head, (37., 0.), [-2., -11., -4.3], [4., 4., 4.], -0.015);
            c(out, &head, (0., 0.), [-4., -3., -7.325], [8., 5., 10.], 0.0);
            c(out, &head, (56., 0.), [-1., 0., -8.325], [2., 3., 2.], 0.0);
            c(out, &body.part([-4., -5.6, -1.8], [0.4363, 0., 0.]).part([0., 0.0893, 0.1198], [-1.0472, 0., 0.]), (36., 16.), [-3.075, -0.9733, -1.9966], [3., 10., 4.], 0.0);
            c(out, &body.part([4., -5.6, -1.7], [0.4363, 0., 0.]).part([0., -0.0015, -0.0808], [-1.0472, 0., 0.]), (50., 16.), [0.075, -1.0443, -1.8997], [3., 10., 4.], 0.0);
            c(out, &base.part([-2.1, -2.1, -2.075], [0.; 3]).part([0.05, -1.9, 1.075], [-1.5708, 0., 0.]), (0., 27.), [-2., 0.975, 0.], [4., 5., 4.], 0.0);
            c(out, &base.part([2., -2., -2.075], [0.; 3]).part([0.05, -2., 1.075], [-1.5708, 0., 0.]), (16., 27.), [-2., 0.975, 0.], [4., 5., 4.], 0.0);
        }
        _ => {
            let star = pose_name == "star";
            let body = base.part([0., -5., 0.], [0.; 3]);
            c(out, &body, (0., 15.), [-4., -6., -3.], [8., 6., 6.], 0.0);
            let head = body.part([0., -6., 0.], [0.; 3]);
            c(out, &head, (0., 0.), [-4., -5., -5.], [8., 5., 10.], if star { 0.0 } else { 0.015 });
            c(out, &head, (56., 0.), [-1., -2., -6.], [2., 3., 2.], 0.0);
            c(out, &head, (37., 8.), [-1., -9., -1.], [2., 4., 2.], -0.015);
            c(out, &head, (37., 0.), [-2., -13., -2.], [4., 4., 4.], -0.015);
            if star {
                c(out, &body.part([-4., -6., 0.], [0.; 3]).part([1., 1., 0.], [0., 0., 1.9199]), (36., 16.), [-1.5, -5., -2.], [3., 10., 4.], 0.0);
                c(out, &body.part([4., -6., 0.], [0.; 3]).part([-1., 1., 0.], [0., 0., -1.9199]), (50., 16.), [-1.5, -5., -2.], [3., 10., 4.], 0.0);
                c(out, &base.part([-3., -5., 0.], [0.; 3]).part([0.35, 2., 0.01], [0., 0., 0.2618]), (0., 27.), [-2., -2.5, -2.], [4., 5., 4.], 0.0);
                c(out, &base.part([1., -5., 0.], [0.; 3]).part([1.65, 2., 0.], [0., 0., -0.2618]), (16., 27.), [-2., -2.5, -2.], [4., 5., 4.], 0.0);
            } else {
                c(out, &body.part([-4., -6., 0.], [0.; 3]), (36., 16.), [-3., -1., -2.], [3., 10., 4.], 0.0);
                c(out, &body.part([4., -6., 0.], [0.; 3]), (50., 16.), [0., -1., -2.], [3., 10., 4.], 0.0);
                c(out, &base.part([0., -5., 0.], [0.; 3]), (0., 27.), [-4., 0., -2.], [4., 5., 4.], 0.0);
                c(out, &base.part([0., -5., 0.], [0.; 3]), (16., 27.), [0., 0., -2.], [4., 5., 4.], 0.0);
            }
        }
    }
}

/// Direction.getRotation() (pour les entités shulker)
pub fn dir_rotation_pub(d: &str) -> pose::M4 {
    dir_rotation(d)
}
