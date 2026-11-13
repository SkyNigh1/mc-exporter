//! Player head skins: read from the game cache (.minecraft/assets/skins),
//! otherwise downloaded from Mojang's servers (when the option is enabled), then cached.

use crate::registry::Registry;
use fastnbt::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// download allowed for the current export
pub static DOWNLOAD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

static CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

fn get<'a>(v: Option<&'a Value>, k: &str) -> Option<&'a Value> {
    match v? {
        Value::Compound(m) => m.get(k),
        _ => None,
    }
}

fn s(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(x) => Some(x.clone()),
        _ => None,
    }
}

pub fn b64_decode(input: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;
    for c in input.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    out
}

/// Base64 "textures" value of a profile (1.20.5+ and older formats)
fn textures_value(p: &Value) -> Option<String> {
    if let Some(Value::List(props)) = get(Some(p), "properties") {
        for pr in props {
            if s(get(Some(pr), "name")).as_deref() == Some("textures") {
                return s(get(Some(pr), "value"));
            }
        }
    }
    if let Some(Value::List(t)) = get(get(Some(p), "Properties"), "textures") {
        return t.first().and_then(|x| s(get(Some(x), "Value")));
    }
    None
}

fn profile_name(p: &Value) -> Option<String> {
    match p {
        Value::String(n) => Some(n.clone()),
        _ => s(get(Some(p), "name")).or_else(|| s(get(Some(p), "Name"))),
    }
}

fn http_get(url: &str) -> Option<Vec<u8>> {
    let agent = ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(8)).user_agent("mc-exporter").build();
    let r = agent.get(url).call().ok()?;
    let mut v = Vec::new();
    std::io::Read::read_to_end(&mut r.into_reader(), &mut v).ok()?;
    Some(v)
}

fn skin_url_from_b64(v: &str) -> Option<String> {
    let j: serde_json::Value = serde_json::from_slice(&b64_decode(v)).ok()?;
    j.get("textures")?.get("SKIN")?.get("url")?.as_str().map(|s| s.to_string())
}

fn our_cache() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("mc-exporter").join("skins")
}

fn png_for_hash(hash: &str, url: &str, download: bool) -> Option<Vec<u8>> {
    let game = crate::assets::minecraft_dir().join("assets").join("skins").join(&hash[..2.min(hash.len())]).join(hash);
    if let Ok(b) = std::fs::read(&game) {
        return Some(b);
    }
    let mine = our_cache().join(format!("{hash}.png"));
    if let Ok(b) = std::fs::read(&mine) {
        return Some(b);
    }
    if !download {
        return None;
    }
    let url = url.replacen("http://", "https://", 1);
    let b = http_get(&url)?;
    let _ = std::fs::create_dir_all(our_cache());
    let _ = std::fs::write(&mine, &b);
    Some(b)
}

/// Texture (internal name) of a player head profile's skin
pub fn skin_for_profile(reg: &Registry, profile: &Value, download: bool) -> Option<String> {
    let key = textures_value(profile).or_else(|| profile_name(profile).map(|n| format!("name:{}", n.to_lowercase())))?;
    if let Some(c) = CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(&key) {
        return c.clone();
    }
    let result = (|| {
        let b64 = match textures_value(profile) {
            Some(v) => v,
            None => {
                if !download {
                    return None;
                }
                let name = profile_name(profile)?;
                let j: serde_json::Value = serde_json::from_slice(&http_get(&format!("https://api.mojang.com/users/profiles/minecraft/{name}"))?).ok()?;
                let id = j.get("id")?.as_str()?.to_string();
                let p: serde_json::Value =
                    serde_json::from_slice(&http_get(&format!("https://sessionserver.mojang.com/session/minecraft/profile/{id}"))?).ok()?;
                p.get("properties")?.as_array()?.iter().find(|x| x.get("name").and_then(|n| n.as_str()) == Some("textures"))?.get("value")?.as_str()?.to_string()
            }
        };
        let url = skin_url_from_b64(&b64)?;
        let hash = url.rsplit('/').next()?.to_string();
        let name = format!("gen:skin_{hash}");
        if !reg.assets.has_texture(&name) {
            let png = png_for_hash(&hash, &url, download)?;
            let img = image::load_from_memory(&png).ok()?.to_rgba8();
            let (w, h) = img.dimensions();
            let mut px = img.into_raw();
            // like the game: base areas (head, body, limbs) are always opaque
            let s = w as f32 / 64.0;
            let mut opaque = |x0: u32, y0: u32, x1: u32, y1: u32| {
                let (x0, y0, x1, y1) = ((x0 as f32 * s) as u32, (y0 as f32 * s) as u32, (x1 as f32 * s) as u32, (y1 as f32 * s) as u32);
                for y in y0..y1.min(h) {
                    for x in x0..x1.min(w) {
                        px[((y * w + x) * 4 + 3) as usize] = 255;
                    }
                }
            };
            opaque(0, 0, 32, 16);
            opaque(0, 16, 64, 32);
            if h as f32 >= 64.0 * s {
                opaque(16, 48, 48, 64);
            }
            reg.assets.put_texture(&name, w, h, px);
        }
        Some(name)
    })();
    CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, result.clone());
    result
}

/// Profile from the data of a head block or head item
pub fn profile_of(nbt: Option<&Value>) -> Option<&Value> {
    get(nbt, "profile")
        .or_else(|| get(nbt, "SkullOwner"))
        .or_else(|| get(get(nbt, "components"), "minecraft:profile"))
        .or_else(|| get(get(nbt, "tag"), "SkullOwner"))
}
