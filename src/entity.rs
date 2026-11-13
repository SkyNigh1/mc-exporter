//! Entities: block/item/text displays, item frames, paintings, armor stands, minecarts, boats,
//! falling blocks and a few animals. Models use the game's dimensions.

use crate::bent::{self, BeOpts, COLORS, dye_diffuse, e, skin};
use crate::font::{self, Align, Font};
use crate::pose::{self, ALL_FACES, M4, Pose, Skin, bx, cube, quat};
use crate::registry::{Quad, Registry, StateKey, Tint};
use fastnbt::Value;
use std::f32::consts::PI;

fn get<'a>(v: Option<&'a Value>, k: &str) -> Option<&'a Value> {
    match v? {
        Value::Compound(m) => m.get(k),
        _ => None,
    }
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Byte(x) => Some(*x as f64),
        Value::Short(x) => Some(*x as f64),
        Value::Int(x) => Some(*x as f64),
        Value::Long(x) => Some(*x as f64),
        Value::Float(x) => Some(*x as f64),
        Value::Double(x) => Some(*x),
        _ => None,
    }
}

fn nums(v: Option<&Value>) -> Vec<f32> {
    match v {
        Some(Value::List(l)) => l.iter().filter_map(num).map(|x| x as f32).collect(),
        Some(Value::IntArray(a)) => a.iter().map(|x| *x as f32).collect(),
        Some(Value::ByteArray(a)) => a.iter().map(|x| *x as f32).collect(),
        Some(Value::LongArray(a)) => a.iter().map(|x| *x as f32).collect(),
        _ => Vec::new(),
    }
}

fn int(v: Option<&Value>) -> Option<i64> {
    v.and_then(num).map(|x| x as i64)
}

fn string(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn short(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

/// Position of an entity
pub fn entity_pos(e: &Value) -> Option<[f64; 3]> {
    match get(Some(e), "Pos") {
        Some(Value::List(l)) if l.len() >= 3 => Some([num(&l[0])?, num(&l[1])?, num(&l[2])?]),
        _ => None,
    }
}

pub struct EntOpts {
    pub sign_text: bool,
    pub download_skins: bool,
    /// world folder (for maps in item frames)
    pub world_dir: Option<std::path::PathBuf>,
}

// ───────────────────────────── items ─────────────────────────────

enum ItemModel {
    Model(String),
    Special(String, String), // (base, type)
}

fn find_item_model(v: &serde_json::Value) -> Option<ItemModel> {
    let o = v.as_object()?;
    let t = o.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if t.ends_with("model") {
        if let Some(m) = o.get("model").and_then(|m| m.as_str()) {
            return Some(ItemModel::Model(m.to_string()));
        }
    }
    if t.ends_with("special") {
        let base = o.get("base").and_then(|b| b.as_str()).unwrap_or("").to_string();
        let st = o.get("model").and_then(|m| m.get("type")).and_then(|t| t.as_str()).unwrap_or("").to_string();
        return Some(ItemModel::Special(base, st));
    }
    for k in ["fallback", "on_false", "model"] {
        if let Some(x) = o.get(k).and_then(find_item_model) {
            return Some(x);
        }
    }
    for k in ["cases", "entries", "models"] {
        if let Some(a) = o.get(k).and_then(|a| a.as_array()) {
            for c in a {
                if let Some(x) = find_item_model(c).or_else(|| c.get("model").and_then(find_item_model)) {
                    return Some(x);
                }
            }
        }
    }
    o.get("on_true").and_then(find_item_model)
}

/// Extruded flat sprite (item/generated), in [0,1]³ space
fn sprite_quads(reg: &Registry, layers: &[String]) -> Vec<Quad> {
    let mut out = Vec::new();
    for (li, tname) in layers.iter().enumerate() {
        let Some(t) = reg.assets.texture(tname) else { continue };
        let tex = reg.tex_id(tname);
        let (w, h) = (t.w as f32, t.h as f32);
        let z0 = 7.5 / 16.0 - li as f32 * 0.0005;
        let z1 = 8.5 / 16.0 + li as f32 * 0.0005;
        let q = |pos: [[f32; 3]; 4], uv: [[f32; 2]; 4]| Quad { pos, uv, tex, tint: Tint::None, cull: None, dir: None, flat: false };
        out.push(q([[0., 0., z1], [1., 0., z1], [1., 1., z1], [0., 1., z1]], [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]));
        out.push(q([[1., 0., z0], [0., 0., z0], [0., 1., z0], [1., 1., z0]], [[1., 0.], [0., 0.], [0., 1.], [1., 1.]]));
        let opaque = |x: i32, y: i32| -> bool {
            x >= 0 && y >= 0 && x < t.w as i32 && y < t.h as i32 && t.rgba[((y as u32 * t.w + x as u32) * 4 + 3) as usize] > 0
        };
        for py in 0..t.h as i32 {
            for px in 0..t.w as i32 {
                if !opaque(px, py) {
                    continue;
                }
                let (x0, x1) = (px as f32 / w, (px + 1) as f32 / w);
                let (y1, y0) = (1.0 - py as f32 / h, 1.0 - (py + 1) as f32 / h);
                let uc = [(px as f32 + 0.5) / w, 1.0 - (py as f32 + 0.5) / h];
                let uv = [uc; 4];
                if !opaque(px - 1, py) {
                    out.push(q([[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]], uv));
                }
                if !opaque(px + 1, py) {
                    out.push(q([[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]], uv));
                }
                if !opaque(px, py - 1) {
                    out.push(q([[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]], uv));
                }
                if !opaque(px, py + 1) {
                    out.push(q([[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]], uv));
                }
            }
        }
    }
    out
}

fn item_id(item: &Value) -> Option<String> {
    let id = string(get(Some(item), "id"))?;
    let id = if id.contains(':') { id } else { format!("minecraft:{id}") };
    if id == "minecraft:air" { None } else { Some(id) }
}

/// Quads of an item, transformed for the display context (fixed, head, ground…, or "none")
pub fn item_quads(reg: &Registry, font: &Font, item: &Value, ctx: &str) -> Vec<Quad> {
    let Some(id) = item_id(item) else { return Vec::new() };
    let (ns, p) = id.split_once(':').unwrap();
    let def = reg
        .assets
        .json(&format!("assets/{ns}/items/{p}.json"))
        .and_then(|j| j.get("model").and_then(find_item_model))
        .or_else(|| {
            reg.assets
                .get(&format!("assets/{ns}/models/item/{p}.json"))
                .map(|_| ItemModel::Model(format!("{ns}:item/{p}")))
        });
    let Some(def) = def else { return Vec::new() };
    let (model_id, geom) = match def {
        ItemModel::Model(m) => {
            let g = match reg.model_layers(&m) {
                Some(layers) => sprite_quads(reg, &layers),
                None => reg.item_geometry(&m, &id),
            };
            (m, g)
        }
        ItemModel::Special(base, st) => {
            let st = short(&st).to_string();
            let fake = |name: &str, props: Vec<(&str, &str)>| StateKey {
                name: name.to_string(),
                props: props.into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
            };
            let o = BeOpts { sign_text: false, download_skins: crate::skins::DOWNLOAD.load(std::sync::atomic::Ordering::Relaxed) };
            let g = match st.as_str() {
                "chest" => bent::quads(reg, font, &fake("minecraft:chest", vec![("facing", "south")]), None, false, &o),
                "head" | "player_head" => {
                    let n = p.to_string();
                    bent::quads(reg, font, &fake(&format!("minecraft:{n}"), vec![("rotation", "8")]), Some(item), false, &o)
                }
                "shulker_box" => bent::quads(reg, font, &fake(&id, vec![("facing", "up")]), None, false, &o),
                "banner" => bent::quads(reg, font, &fake(&id, vec![("rotation", "0")]), get(get(Some(item), "components"), "minecraft:banner_patterns").map(|_| item), false, &o),
                "decorated_pot" => bent::quads(reg, font, &fake("minecraft:decorated_pot", vec![("facing", "south")]), None, false, &o),
                "bell" => bent::quads(reg, font, &fake("minecraft:bell", vec![]), None, false, &o),
                _ => reg.model_geometry(&base),
            };
            (base, g)
        }
    };
    let mut pose = Pose::new();
    if ctx != "none" {
        if let Some([r, t, s]) = reg.model_display(&model_id, ctx) {
            pose = pose.t(t[0], t[1], t[2]).rx(r[0]).ry(r[1]).rz(r[2]).s(s[0], s[1], s[2]);
        }
    }
    let pose = pose.t(-0.5, -0.5, -0.5);
    geom.iter().map(|q| pose.quad(q)).collect()
}

// ───────────────────────────── entities ─────────────────────────────

fn transformation(v: Option<&Value>) -> M4 {
    match v {
        Some(Value::List(l)) if l.len() == 16 => {
            let f: Vec<f32> = l.iter().filter_map(num).map(|x| x as f32).collect();
            if f.len() == 16 {
                return [[f[0], f[1], f[2], f[3]], [f[4], f[5], f[6], f[7]], [f[8], f[9], f[10], f[11]], [f[12], f[13], f[14], f[15]]];
            }
            pose::IDENT
        }
        Some(c @ Value::Compound(_)) => {
            let t = nums(get(Some(c), "translation"));
            let s = nums(get(Some(c), "scale"));
            let rotm = |k: &str| -> M4 {
                let r = get(Some(c), k);
                let q = nums(r);
                if q.len() == 4 {
                    return quat([q[0], q[1], q[2], q[3]]);
                }
                if let Some(a) = get(r, "angle").and_then(num) {
                    let ax = nums(get(r, "axis"));
                    if ax.len() == 3 {
                        let (sn, cs) = ((a as f32) / 2.0).sin_cos();
                        let n = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt().max(1e-9);
                        return quat([ax[0] / n * sn, ax[1] / n * sn, ax[2] / n * sn, cs]);
                    }
                }
                pose::IDENT
            };
            let mut m = pose::IDENT;
            if t.len() == 3 {
                m = pose::mul(&m, &pose::translation(t[0], t[1], t[2]));
            }
            m = pose::mul(&m, &rotm("left_rotation"));
            if s.len() == 3 {
                m = pose::mul(&m, &pose::scaling(s[0], s[1], s[2]));
            }
            pose::mul(&m, &rotm("right_rotation"))
        }
        _ => pose::IDENT,
    }
}

fn living_base(pos: [f64; 3], yaw: f32) -> Pose {
    Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).ry(180.0 - yaw).s(-1.0, -1.0, 1.0).t(0.0, -1.501, 0.0)
}

/// Quads of an entity, in world coordinates
pub fn entity_quads(reg: &Registry, font: &Font, ent: &Value, o: &EntOpts) -> Vec<Quad> {
    let mut out = Vec::new();
    let Some(id) = string(get(Some(ent), "id")) else { return out };
    let id = short(&id).to_string();
    let Some(pos) = entity_pos(ent) else { return out };
    let rot = nums(get(Some(ent), "Rotation"));
    let (yaw, pitch) = (rot.first().copied().unwrap_or(0.0), rot.get(1).copied().unwrap_or(0.0));
    let invisible = int(get(Some(ent), "Invisible")).unwrap_or(0) != 0;
    let p0 = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32);
    let be = BeOpts { sign_text: o.sign_text, download_skins: o.download_skins };
    match id.as_str() {
        "block_display" | "item_display" | "text_display" => {
            let orient = Pose::new().ry(-yaw).rx(pitch);
            let p = p0.m(&orient.0).m(&transformation(get(Some(ent), "transformation")));
            match id.as_str() {
                "block_display" => {
                    let Some(bs) = get(Some(ent), "block_state") else { return out };
                    let name = string(get(Some(bs), "Name")).unwrap_or_default();
                    let mut props: Vec<(String, String)> = match get(Some(bs), "Properties") {
                        Some(Value::Compound(m)) => m.iter().filter_map(|(k, v)| string(Some(v)).map(|s| (k.clone(), s))).collect(),
                        _ => Vec::new(),
                    };
                    props.sort();
                    let sid = reg.state_id(&name, props);
                    let g = reg.geom(sid);
                    let mut q = reg.state_quads(sid);
                    if g.be {
                        q.extend(bent::quads(reg, font, &reg.state(sid), None, g.has_model, &be));
                    }
                    out.extend(q.iter().map(|x| p.quad(x)));
                }
                "item_display" => {
                    let Some(item) = get(Some(ent), "item") else { return out };
                    let ctx = string(get(Some(ent), "item_display")).unwrap_or("none".into()).to_lowercase();
                    let p = p.ry(180.0);
                    out.extend(item_quads(reg, font, item, &ctx).iter().map(|x| p.quad(x)));
                }
                _ => {
                    let Some(text) = get(Some(ent), "text") else { return out };
                    let width = int(get(Some(ent), "line_width")).unwrap_or(200) as f32;
                    let lines = wrap(font, font::component_lines(text, [255, 255, 255]), width);
                    let bg = int(get(Some(ent), "background")).map(|b| b as u32).unwrap_or(0x40000000);
                    let bg = [(bg >> 16) as u8, (bg >> 8) as u8, bg as u8, (bg >> 24) as u8];
                    let align = match string(get(Some(ent), "alignment")).as_deref() {
                        Some("left") => Align::Left,
                        Some("right") => Align::Right,
                        _ => Align::Center,
                    };
                    let (tex, w, h) = bent::text_texture(reg, font, &lines, 10, align, bg, 1);
                    let p = p.s(0.025, 0.025, 0.025).t(-w / 2.0, 0.0, 0.0);
                    out.push(pose::flat_quad(&p, tex, w, h, [0.0, 1.0, 1.0, 0.0]));
                }
            }
        }
        "item_frame" | "glow_item_frame" => {
            let facing = int(get(Some(ent), "Facing")).or_else(|| int(get(Some(ent), "facing"))).unwrap_or(3);
            let (dname, step) = match facing {
                0 => ("down", [0., -1., 0.]),
                1 => ("up", [0., 1., 0.]),
                2 => ("north", [0., 0., -1.]),
                4 => ("west", [-1., 0., 0.]),
                5 => ("east", [1., 0., 0.]),
                _ => ("south", [0., 0., 1.]),
            };
            let (xr, yr) = match dname {
                "up" => (-90.0, 180.0),
                "down" => (90.0, 180.0),
                d => (0.0, 180.0 - bent::to_y_rot(d)),
            };
            let p = p0.t(step[0] * 0.46875, step[1] * 0.46875, step[2] * 0.46875).rx(xr).ry(yr);
            let item = get(Some(ent), "Item");
            let is_map = item.and_then(item_id).map(|i| i.ends_with("filled_map")).unwrap_or(false);
            if !invisible {
                let base = if id == "glow_item_frame" { "glow_item_frame" } else { "item_frame" };
                let model = format!("minecraft:block/{base}{}", if is_map { "_map" } else { "" });
                let pf = p.t(-0.5, -0.5, -0.5);
                out.extend(reg.model_geometry(&model).iter().map(|x| pf.quad(x)));
            }
            if let Some(item) = item {
                if is_map {
                    let r = (int(get(Some(ent), "ItemRotation")).unwrap_or(0) % 4 * 2) as f32;
                    if let Some(tex) = map_texture(reg, o.world_dir.as_deref(), item) {
                        let pm = p
                            .t(0.0, 0.0, if invisible { 0.5 } else { 0.4375 })
                            .rz(r * 45.0)
                            .rz(180.0)
                            .s(1.0 / 128.0, 1.0 / 128.0, 1.0 / 128.0)
                            .t(-64.0, -64.0, -1.0);
                        let q = Quad {
                            pos: [[0., 128., -0.01], [128., 128., -0.01], [128., 0., -0.01], [0., 0., -0.01]],
                            uv: [[0., 0.], [1., 0.], [1., 1.], [0., 1.]],
                            tex,
                            tint: Tint::None,
                            cull: None,
                            dir: None,
                            flat: true,
                        };
                        out.push(pm.quad(&q));
                    }
                } else {
                    let r = int(get(Some(ent), "ItemRotation")).unwrap_or(0) as f32;
                    let pi = p.t(0.0, 0.0, if invisible { 0.5 } else { 0.4375 }).rz(r * 45.0).s(0.5, 0.5, 0.5);
                    out.extend(item_quads(reg, font, item, "fixed").iter().map(|x| pi.quad(x)));
                }
            }
        }
        "painting" => painting(reg, ent, pos, &mut out),
        "armor_stand" => armor_stand(reg, font, ent, pos, yaw, invisible, &mut out),
        "falling_block" => {
            let Some(bs) = get(Some(ent), "BlockState") else { return out };
            let name = string(get(Some(bs), "Name")).unwrap_or_default();
            let mut props: Vec<(String, String)> = match get(Some(bs), "Properties") {
                Some(Value::Compound(m)) => m.iter().filter_map(|(k, v)| string(Some(v)).map(|s| (k.clone(), s))).collect(),
                _ => Vec::new(),
            };
            props.sort();
            let sid = reg.state_id(&name, props);
            let p = p0.t(-0.5, 0.0, -0.5);
            out.extend(reg.state_quads(sid).iter().map(|x| p.quad(x)));
        }
        s if s.ends_with("minecart") => minecart(reg, font, &id, ent, pos, yaw, pitch, &be, &mut out),
        s if s.ends_with("boat") => boat(reg, ent, &id, pos, yaw, &mut out),
        _ if !invisible => {
            if !crate::mobs::mob(reg, &id, ent, pos, yaw, &mut out) {
                animal(reg, &id, ent, pos, yaw, &mut out);
            }
        }
        _ => {}
    }
    out
}

fn painting(reg: &Registry, ent: &Value, pos: [f64; 3], out: &mut Vec<Quad>) {
    let variant = string(get(Some(ent), "variant")).or_else(|| string(get(Some(ent), "Motive"))).unwrap_or("minecraft:kebab".into());
    let (ns, p) = variant.split_once(':').unwrap_or(("minecraft", &variant));
    let (asset, w, h) = match reg.assets.json(&format!("data/{ns}/painting_variant/{p}.json")) {
        Some(j) => (
            j.get("asset_id").and_then(|a| a.as_str()).unwrap_or(&variant).to_string(),
            j.get("width").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
            j.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
        ),
        None => {
            let (w, h) = match p {
                "pool" | "courbet" | "sea" | "sunset" | "creebet" => (2., 1.),
                "wanderer" | "graham" => (1., 2.),
                "match" | "bust" | "stage" | "void" | "skull_and_roses" | "wither" | "earth" | "wind" | "water" | "fire" => (2., 2.),
                "fighters" => (4., 2.),
                "pointer" | "pigscene" | "burning_skull" => (4., 4.),
                "skeleton" | "donkey_kong" => (4., 3.),
                _ => (1., 1.),
            };
            (variant.clone(), w, h)
        }
    };
    let (ans, ap) = asset.split_once(':').unwrap_or(("minecraft", &asset));
    let front_name = format!("{ans}:painting/{ap}");
    if reg.assets.texture(&front_name).is_none() {
        return;
    }
    let front = reg.tex_id(&front_name);
    let back = reg.assets.texture("minecraft:painting/back").map(|_| reg.tex_id("minecraft:painting/back"));
    let facing = int(get(Some(ent), "facing")).or_else(|| int(get(Some(ent), "Facing"))).unwrap_or(0);
    let angle = match facing {
        1 => -90.0, // west
        2 => 180.0, // north
        3 => 90.0,  // east
        _ => 0.0,   // south
    };
    let base = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).ry(angle);
    let d = 1.0 / 32.0;
    let pf = base.t(-w / 2.0, -h / 2.0, d);
    out.push(pose::flat_quad(&pf, front, w, h, [0.0, 1.0, 1.0, 0.0]));
    if let Some(b) = back {
        let pb = base.ry(180.0).t(-w / 2.0, -h / 2.0, d);
        out.push(pose::flat_quad(&pb, b, w, h, [0.0, h, w, 0.0]));
        // edges
        let edge = |p: Pose, l: f32| pose::flat_quad(&p, b, l, 2.0 * d, [0.0, 1.0 / 16.0, l, 0.0]);
        out.push(edge(base.t(-w / 2.0, h / 2.0, d).rx(-90.0), w));
        out.push(edge(base.t(w / 2.0, -h / 2.0, d).rz(180.0).rx(-90.0), w));
        out.push(edge(base.t(-w / 2.0, -h / 2.0, d).rz(90.0).rx(-90.0), h));
        out.push(edge(base.t(w / 2.0, h / 2.0, d).rz(-90.0).rx(-90.0), h));
    }
}

fn equipment<'a>(ent: &'a Value, slot: &str) -> Option<&'a Value> {
    if let Some(v) = get(get(Some(ent), "equipment"), slot) {
        return Some(v);
    }
    let (list, idx) = match slot {
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

fn armor_stand(reg: &Registry, font: &Font, ent: &Value, pos: [f64; 3], yaw: f32, invisible: bool, out: &mut Vec<Quad>) {
    let small = int(get(Some(ent), "Small")).unwrap_or(0) != 0;
    let mut base = living_base(pos, yaw);
    if small {
        base = Pose::new()
            .t(pos[0] as f32, pos[1] as f32, pos[2] as f32)
            .ry(180.0 - yaw)
            .s(-0.5, -0.5, 0.5)
            .t(0.0, -1.501, 0.0);
    }
    let pose_nbt = get(Some(ent), "Pose");
    let ang = |k: &str, def: [f32; 3]| -> [f32; 3] {
        let v = nums(get(pose_nbt, k));
        if v.len() == 3 { [v[0].to_radians(), v[1].to_radians(), v[2].to_radians()] } else { def.map(|d: f32| d.to_radians()) }
    };
    let head = ang("Head", [0., 0., 0.]);
    let body = ang("Body", [0., 0., 0.]);
    let larm = ang("LeftArm", [-10., 0., -10.]);
    let rarm = ang("RightArm", [-15., 0., 10.]);
    let lleg = ang("LeftLeg", [-1., 0., -1.]);
    let rleg = ang("RightLeg", [1., 0., 1.]);
    let show_arms = int(get(Some(ent), "ShowArms")).unwrap_or(0) != 0;
    let plate = int(get(Some(ent), "NoBasePlate")).unwrap_or(0) == 0;
    let head_p = base.part([0., 1., 0.], head);
    let rarm_p = base.part([-5., 2., 0.], rarm);
    let larm_p = base.part([5., 2., 0.], larm);
    if !invisible {
        if let Some(sk) = skin(reg, &[e("armorstand/armorstand"), e("armorstand/wood")], Tint::None) {
            bx(out, &head_p, &sk, (0., 0.), [-1., -7., -1.], [2., 7., 2.]);
            let bp = base.part([0., 0., 0.], body);
            bx(out, &bp, &sk, (0., 26.), [-6., 0., -1.5], [12., 3., 3.]);
            bx(out, &bp, &sk, (16., 0.), [-3., 3., -1.], [2., 7., 2.]);
            bx(out, &bp, &sk, (48., 16.), [1., 3., -1.], [2., 7., 2.]);
            bx(out, &bp, &sk, (0., 48.), [-4., 10., -1.], [8., 2., 2.]);
            if show_arms {
                bx(out, &rarm_p, &sk, (24., 0.), [-2., -2., -1.], [2., 12., 2.]);
                cube(out, &larm_p, &sk, 32., 16., [0., -2., -1.], [2., 12., 2.], 0.0, true, ALL_FACES);
            }
            bx(out, &base.part([-1.9, 12., 0.], rleg), &sk, (8., 0.), [-1., 0., -1.], [2., 11., 2.]);
            cube(out, &base.part([1.9, 12., 0.], lleg), &sk, 40., 16., [-1., 0., -1.], [2., 11., 2.], 0.0, true, ALL_FACES);
            if plate {
                bx(out, &base.part([0., 12., 0.], [0.; 3]), &sk, (0., 32.), [-6., 11., -6.], [12., 1., 12.]);
            }
        }
    }
    // armor
    let limbs = crate::mobs::Limbs {
        head: ([0., 1., 0.], head),
        body: ([0.; 3], body),
        rarm: ([-5., 2., 0.], rarm),
        larm: ([5., 2., 0.], larm),
        rleg: ([-1.9, 12., 0.], rleg),
        lleg: ([1.9, 12., 0.], lleg),
    };
    crate::mobs::armor(reg, ent, &base, &limbs, true, out);
    // item on the head (block, player head…)
    if let Some(it) = equipment(ent, "head").filter(|v| !item_id(v).map(|i| i.ends_with("_helmet")).unwrap_or(false)) {
        let p = head_p.t(0.0, -0.25, 0.0).ry(180.0).s(0.625, -0.625, -0.625);
        out.extend(item_quads(reg, font, it, "head").iter().map(|q| p.quad(q)));
    }
    // held items
    for (slot, arm, left) in [("mainhand", rarm_p, false), ("offhand", larm_p, true)] {
        if let Some(it) = equipment(ent, slot) {
            let p = arm.rx(-90.0).ry(180.0).t(if left { -1.0 } else { 1.0 } / 16.0, 0.125, -0.625);
            let ctx = if left { "thirdperson_lefthand" } else { "thirdperson_righthand" };
            out.extend(item_quads(reg, font, it, ctx).iter().map(|q| p.quad(q)));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn minecart(reg: &Registry, font: &Font, id: &str, ent: &Value, pos: [f64; 3], yaw: f32, pitch: f32, be: &BeOpts, out: &mut Vec<Quad>) {
    let p = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).t(0.0, 0.375, 0.0).ry(180.0 - yaw).rz(-pitch);
    // contents
    let content = match id {
        "chest_minecart" => Some("minecraft:chest"),
        "furnace_minecart" => Some("minecraft:furnace"),
        "tnt_minecart" => Some("minecraft:tnt"),
        "hopper_minecart" => Some("minecraft:hopper"),
        "spawner_minecart" => Some("minecraft:spawner"),
        "command_block_minecart" => Some("minecraft:command_block"),
        _ => None,
    };
    let custom = get(Some(ent), "DisplayState").or_else(|| get(Some(ent), "display_state"));
    let sid = match (custom, content) {
        (Some(bs), _) => {
            let name = string(get(Some(bs), "Name")).unwrap_or_default();
            let mut props: Vec<(String, String)> = match get(Some(bs), "Properties") {
                Some(Value::Compound(m)) => m.iter().filter_map(|(k, v)| string(Some(v)).map(|s| (k.clone(), s))).collect(),
                _ => Vec::new(),
            };
            props.sort();
            Some(reg.state_id(&name, props))
        }
        (None, Some(c)) => Some(reg.state_id(c, if c.ends_with("chest") { vec![("facing".into(), "south".into())] } else { vec![] })),
        _ => None,
    };
    if let Some(sid) = sid {
        let off = int(get(Some(ent), "DisplayOffset")).unwrap_or(if id == "hopper_minecart" { 1 } else { 6 }) as f32;
        let pc = p.s(0.75, 0.75, 0.75).t(-0.5, (off - 8.0) / 16.0, 0.5).ry(90.0);
        let g = reg.geom(sid);
        let mut q = reg.state_quads(sid);
        if g.be {
            q.extend(bent::quads(reg, font, &reg.state(sid), None, g.has_model, be));
        }
        out.extend(q.iter().map(|x| pc.quad(x)));
    }
    let Some(sk) = skin(reg, &[e("minecart/minecart"), e("minecart")], Tint::None) else { return };
    let m = p.s(-1.0, -1.0, 1.0);
    bx(out, &m.part([0., 4., 0.], [PI / 2.0, 0., 0.]), &sk, (0., 10.), [-10., -8., -1.], [20., 16., 2.]);
    let side = |o: &mut Vec<Quad>, off: [f32; 3], ry: f32| bx(o, &m.part(off, [0., ry, 0.]), &sk, (0., 0.), [-8., -9., -1.], [16., 8., 2.]);
    side(out, [-9., 4., 0.], PI * 1.5);
    side(out, [9., 4., 0.], PI / 2.0);
    side(out, [0., 4., -7.], PI);
    side(out, [0., 4., 7.], 0.0);
}

fn boat(reg: &Registry, ent: &Value, id: &str, pos: [f64; 3], yaw: f32, out: &mut Vec<Quad>) {
    let chest = id.contains("chest");
    let wood = match id.strip_suffix("_chest_boat").or_else(|| id.strip_suffix("_boat")) {
        Some(w) => w.to_string(),
        None => string(get(Some(ent), "Type")).unwrap_or("oak".into()),
    };
    let dir = if chest { "chest_boat" } else { "boat" };
    let Some(sk) = skin(reg, &[e(&format!("{dir}/{wood}"))], Tint::None) else { return };
    let m = Pose::new().t(pos[0] as f32, pos[1] as f32, pos[2] as f32).t(0.0, 0.375, 0.0).ry(180.0 - yaw).s(-1.0, -1.0, 1.0).ry(90.0);
    bx(out, &m.part([0., 3., 1.], [PI / 2.0, 0., 0.]), &sk, (0., 0.), [-14., -9., -3.], [28., 16., 3.]);
    bx(out, &m.part([-15., 4., 4.], [0., PI * 1.5, 0.]), &sk, (0., 19.), [-13., -7., -1.], [18., 6., 2.]);
    bx(out, &m.part([15., 4., 0.], [0., PI / 2.0, 0.]), &sk, (0., 27.), [-8., -7., -1.], [16., 6., 2.]);
    bx(out, &m.part([0., 4., -9.], [0., PI, 0.]), &sk, (0., 35.), [-14., -7., -1.], [28., 6., 2.]);
    bx(out, &m.part([0., 4., 9.], [0.; 3]), &sk, (0., 43.), [-14., -7., -1.], [28., 6., 2.]);
    let lp = m.part([3., -5., 9.], [0., 0., 0.19634955]);
    bx(out, &lp, &sk, (62., 0.), [-1., 0., -5.], [2., 2., 18.]);
    bx(out, &lp, &sk, (62., 0.), [-1.001, -3., 8.], [1., 6., 7.]);
    let rp = m.part([3., -5., -9.], [0., PI, 0.19634955]);
    bx(out, &rp, &sk, (62., 20.), [-1., 0., -5.], [2., 2., 18.]);
    bx(out, &rp, &sk, (62., 20.), [0.001, -3., 8.], [1., 6., 7.]);
    if chest {
        bx(out, &m.part([-2., -5., -6.], [0., -PI / 2.0, 0.]), &sk, (0., 76.), [0., 0., 0.], [12., 8., 12.]);
        bx(out, &m.part([-2., -9., -6.], [0., -PI / 2.0, 0.]), &sk, (0., 59.), [0., 0., 0.], [12., 4., 12.]);
        bx(out, &m.part([-1., -6., -1.], [0., -PI / 2.0, 0.]), &sk, (0., 59.), [0., 0., 0.], [2., 4., 1.]);
    }
}

fn variant_tex(ent: &Value, kind: &str) -> Vec<String> {
    let v = string(get(Some(ent), "variant")).map(|s| short(&s).to_string()).unwrap_or("temperate".into());
    vec![
        e(&format!("{kind}/{kind}_{v}")),
        e(&format!("{kind}/{v}_{kind}")),
        e(&format!("{kind}/{kind}")),
        e(kind),
    ]
}

fn legs(out: &mut Vec<Quad>, base: &Pose, sk: &Skin, y: f32, h: f32, xs: f32, zs: [f32; 2], grow: f32) {
    for (x, z) in [(-xs, zs[0]), (xs, zs[0]), (-xs, zs[1]), (xs, zs[1])] {
        cube(out, &base.part([x, y, z], [0.; 3]), sk, 0., 16., [-2., 0., -2.], [4., h, 4.], grow, false, ALL_FACES);
    }
}

fn animal(reg: &Registry, id: &str, ent: &Value, pos: [f64; 3], yaw: f32, out: &mut Vec<Quad>) {
    let base = living_base(pos, yaw);
    let h = PI / 2.0;
    match id {
        "pig" => {
            let Some(sk) = skin(reg, &variant_tex(ent, "pig"), Tint::None) else { return };
            let hp = base.part([0., 12., -6.], [0.; 3]);
            bx(out, &hp, &sk, (0., 0.), [-4., -4., -8.], [8., 8., 8.]);
            bx(out, &hp, &sk, (16., 16.), [-2., 0., -9.], [4., 3., 1.]);
            bx(out, &base.part([0., 11., 2.], [h, 0., 0.]), &sk, (28., 8.), [-5., -10., -7.], [10., 16., 8.]);
            legs(out, &base, &sk, 18., 6., 3., [7., -5.], 0.0);
            crate::deco::pig(reg, ent, &base, out);
        }
        "cow" | "mooshroom" => {
            let tex = if id == "mooshroom" {
                let t = string(get(Some(ent), "Type")).unwrap_or("red".into());
                vec![e(&format!("cow/mooshroom_{t}")), e(&format!("cow/{t}_mooshroom"))]
            } else {
                variant_tex(ent, "cow")
            };
            let Some(sk) = skin(reg, &tex, Tint::None) else { return };
            let hp = base.part([0., 4., -8.], [0.; 3]);
            bx(out, &hp, &sk, (0., 0.), [-4., -4., -6.], [8., 8., 6.]);
            bx(out, &hp, &sk, (1., 33.), [-3., 1., -7.], [6., 3., 1.]);
            bx(out, &hp, &sk, (22., 0.), [-5., -5., -5.], [1., 3., 1.]);
            bx(out, &hp, &sk, (22., 0.), [4., -5., -5.], [1., 3., 1.]);
            let bp = base.part([0., 5., 2.], [h, 0., 0.]);
            bx(out, &bp, &sk, (18., 4.), [-6., -10., -7.], [12., 18., 10.]);
            bx(out, &bp, &sk, (52., 0.), [-2., 2., -8.], [4., 6., 1.]);
            legs(out, &base, &sk, 12., 12., 4., [7., -5.], 0.0);
        }
        "sheep" => {
            let Some(sk) = skin(reg, &[e("sheep/sheep")], Tint::None) else { return };
            let hp = base.part([0., 6., -8.], [0.; 3]);
            bx(out, &hp, &sk, (0., 0.), [-3., -4., -6.], [6., 6., 8.]);
            let bp = base.part([0., 5., 2.], [h, 0., 0.]);
            bx(out, &bp, &sk, (28., 8.), [-4., -10., -7.], [8., 16., 6.]);
            legs(out, &base, &sk, 12., 12., 3., [7., -5.], 0.0);
            if int(get(Some(ent), "Sheared")).unwrap_or(0) == 0 {
                let c = int(get(Some(ent), "Color")).unwrap_or(0).clamp(0, 15) as usize;
                if let Some(w) = skin(reg, &[e("sheep/sheep_wool"), e("sheep/sheep_fur")], Tint::Fixed(dye_diffuse(COLORS[c]))) {
                    cube(out, &hp, &w, 0., 0., [-3., -4., -4.], [6., 6., 6.], 0.6, false, ALL_FACES);
                    cube(out, &bp, &w, 28., 8., [-4., -10., -7.], [8., 16., 6.], 1.75, false, ALL_FACES);
                    legs(out, &base, &w, 12., 6., 3., [7., -5.], 0.5);
                }
            }
        }
        "chicken" => {
            let Some(sk) = skin(reg, &variant_tex(ent, "chicken"), Tint::None) else { return };
            let hp = base.part([0., 15., -4.], [0.; 3]);
            bx(out, &hp, &sk, (0., 0.), [-2., -6., -2.], [4., 6., 3.]);
            bx(out, &hp, &sk, (14., 0.), [-2., -4., -4.], [4., 2., 2.]);
            bx(out, &hp, &sk, (14., 4.), [-1., -2., -3.], [2., 2., 2.]);
            bx(out, &base.part([0., 16., 0.], [h, 0., 0.]), &sk, (0., 9.), [-3., -4., -3.], [6., 8., 6.]);
            bx(out, &base.part([-2., 19., 1.], [0.; 3]), &sk, (26., 0.), [-1., 0., -3.], [3., 5., 3.]);
            bx(out, &base.part([1., 19., 1.], [0.; 3]), &sk, (26., 0.), [-1., 0., -3.], [3., 5., 3.]);
            bx(out, &base.part([-4., 13., 0.], [0.; 3]), &sk, (24., 13.), [0., 0., -3.], [1., 4., 6.]);
            bx(out, &base.part([4., 13., 0.], [0.; 3]), &sk, (24., 13.), [-1., 0., -3.], [1., 4., 6.]);
        }
        "slime" | "magma_cube" => {
            if id == "magma_cube" {
                return;
            }
            let Some(sk) = skin(reg, &[e("slime/slime"), e("slime")], Tint::None) else { return };
            let size = (int(get(Some(ent), "Size")).unwrap_or(0) + 1) as f32;
            let p = Pose::new()
                .t(pos[0] as f32, pos[1] as f32, pos[2] as f32)
                .ry(180.0 - yaw)
                .s(-0.999 * size, -0.999 * size, 0.999 * size)
                .t(0.0, -1.501, 0.0);
            bx(out, &p, &sk, (0., 16.), [-3., 17., -3.], [6., 6., 6.]);
            bx(out, &p, &sk, (32., 0.), [-3.25, 18., -3.5], [2., 2., 2.]);
            bx(out, &p, &sk, (32., 4.), [1.25, 18., -3.5], [2., 2., 2.]);
            bx(out, &p, &sk, (32., 8.), [0., 21., -3.5], [1., 1., 1.]);
            bx(out, &p, &sk, (0., 0.), [-4., 16., -4.], [8., 8., 8.]);
        }
        _ => {}
    }
}

/// Wraps lines that are too long (text displays: line_width, 200 px by default)
fn wrap(font: &Font, lines: Vec<Vec<font::Span>>, width: f32) -> Vec<Vec<font::Span>> {
    let mut out = Vec::new();
    for line in lines {
        let chars: Vec<(char, [u8; 3])> = line.iter().flat_map(|(s, c)| s.chars().map(move |ch| (ch, *c))).collect();
        let mut cur: Vec<(char, [u8; 3])> = Vec::new();
        let mut w = 0.0;
        for ch in chars {
            let a = font.advance(ch.0);
            if w + a > width && !cur.is_empty() {
                // break at the last space when possible
                let cut = cur.iter().rposition(|(c, _)| *c == ' ').filter(|&i| i > 0);
                let rest: Vec<(char, [u8; 3])> = match cut {
                    Some(i) => {
                        let r = cur.split_off(i + 1);
                        cur.pop();
                        r
                    }
                    None => Vec::new(),
                };
                out.push(regroup(&cur));
                cur = rest;
                w = cur.iter().map(|(c, _)| font.advance(*c)).sum();
            }
            cur.push(ch);
            w += a;
        }
        out.push(regroup(&cur));
    }
    out
}

fn regroup(chars: &[(char, [u8; 3])]) -> Vec<font::Span> {
    let mut v: Vec<font::Span> = Vec::new();
    for (c, col) in chars {
        match v.last_mut() {
            Some((s, cc)) if cc == col => s.push(*c),
            _ => v.push((c.to_string(), *col)),
        }
    }
    v
}

/// Base map colors (MapColor), in id order
const MAP_COLORS: [u32; 62] = [
    0, 8368696, 16247203, 13092807, 16711680, 10526975, 10987431, 31744, 16777215, 10791096, 9923917, 7368816, 4210943,
    9402184, 16776437, 14188339, 11685080, 6724056, 15066419, 8375321, 15892389, 5000268, 10066329, 5013401, 8339378,
    3361970, 6704179, 6717235, 10040115, 1644825, 16445005, 6085589, 4882687, 55610, 8476209, 7340544, 13742497, 10441252,
    9787244, 7367818, 12223780, 6780213, 10505550, 3746083, 8874850, 5725276, 8014168, 4996700, 4993571, 5001770, 9321518,
    2430480, 12398641, 9715553, 6035741, 1474182, 3837580, 5647422, 1356933, 6579300, 14200723, 8365974,
];

fn find_map_file(world: &std::path::Path, id: i64) -> Option<std::path::PathBuf> {
    let name = format!("map_{id}.dat");
    for c in [world.join("data").join(&name), world.join("data").join("minecraft").join(&name), world.join("data").join("minecraft").join("maps").join(&name)] {
        if c.is_file() {
            return Some(c);
        }
    }
    fn walk(d: &std::path::Path, name: &str, depth: u32) -> Option<std::path::PathBuf> {
        if depth > 4 {
            return None;
        }
        for e in std::fs::read_dir(d).ok()?.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(r) = walk(&p, name, depth + 1) {
                    return Some(r);
                }
            } else if e.file_name().to_string_lossy() == name {
                return Some(p);
            }
        }
        None
    }
    walk(&world.join("data"), &name, 0)
}

/// 128×128 texture of a map (map_<id>.dat), cached
fn map_texture(reg: &Registry, world: Option<&std::path::Path>, item: &Value) -> Option<u32> {
    let id = int(get(get(Some(item), "components"), "minecraft:map_id"))
        .or_else(|| int(get(get(Some(item), "tag"), "map")))?;
    let name = format!("gen:map_{id}");
    if !reg.assets.has_texture(&name) {
        let env = std::env::var_os("MCEXPORT_WORLD").map(std::path::PathBuf::from);
        let file = find_map_file(world.map(|w| w.to_path_buf()).or(env)?.as_path(), id)?;
        let raw = std::fs::read(file).ok()?;
        let mut data = Vec::new();
        if raw.starts_with(&[0x1f, 0x8b]) {
            std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&raw[..]), &mut data).ok()?;
        } else {
            data = raw;
        }
        let v: Value = fastnbt::from_bytes(&data).ok()?;
        let colors = match get(get(Some(&v), "data"), "colors").or_else(|| get(Some(&v), "colors")) {
            Some(Value::ByteArray(b)) => b.iter().map(|x| *x as u8).collect::<Vec<u8>>(),
            _ => return None,
        };
        let mult = [180u32, 220, 255, 135];
        let mut img = vec![0u8; 128 * 128 * 4];
        for (i, c) in colors.iter().take(128 * 128).enumerate() {
            let base = (*c >> 2) as usize;
            if base == 0 || base >= MAP_COLORS.len() {
                continue;
            }
            let col = MAP_COLORS[base];
            let m = mult[(*c & 3) as usize];
            img[i * 4] = (((col >> 16) & 255) * m / 255) as u8;
            img[i * 4 + 1] = (((col >> 8) & 255) * m / 255) as u8;
            img[i * 4 + 2] = ((col & 255) * m / 255) as u8;
            img[i * 4 + 3] = 255;
        }
        reg.assets.put_texture(&name, 128, 128, img);
    }
    Some(reg.tex_id(&name))
}
