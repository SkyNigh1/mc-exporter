//! Game bitmap font (ascii, accented, nonlatin…) and text components (JSON or NBT).

use crate::assets::{Assets, Texture};
use fastnbt::Value;
use std::collections::HashMap;
use std::sync::Arc;

struct Glyph {
    tex: Arc<Texture>,
    x: u32,
    y: u32,
    cw: u32,
    ch: u32,
    /// display height (in font pixels) and vertical offset
    height: f32,
    ascent: f32,
    width: f32,
}

pub struct Font {
    glyphs: HashMap<char, Glyph>,
}

/// Colored text segment
pub type Span = (String, [u8; 3]);

impl Font {
    pub fn load(assets: &Assets) -> Font {
        let mut f = Font { glyphs: HashMap::new() };
        let mut seen = std::collections::HashSet::new();
        f.load_def(assets, "minecraft:default", &mut seen, 0);
        f
    }

    fn load_def(&mut self, assets: &Assets, id: &str, seen: &mut std::collections::HashSet<String>, depth: u32) {
        if depth > 6 || !seen.insert(id.to_string()) {
            return;
        }
        let (ns, p) = id.split_once(':').unwrap_or(("minecraft", id));
        let Some(j) = assets.json(&format!("assets/{ns}/font/{p}.json")) else { return };
        let Some(provs) = j.get("providers").and_then(|v| v.as_array()) else { return };
        for pr in provs {
            match pr.get("type").and_then(|v| v.as_str()) {
                Some("reference") => {
                    if let Some(r) = pr.get("id").and_then(|v| v.as_str()) {
                        self.load_def(assets, r, seen, depth + 1);
                    }
                }
                Some("bitmap") => self.load_bitmap(assets, pr),
                _ => {}
            }
        }
    }

    fn load_bitmap(&mut self, assets: &Assets, pr: &serde_json::Value) {
        let Some(file) = pr.get("file").and_then(|v| v.as_str()) else { return };
        let file = file.trim_end_matches(".png");
        let (ns, p) = file.split_once(':').unwrap_or(("minecraft", file));
        let Some(tex) = assets.texture(&format!("{ns}:textures_raw/{p}")).or_else(|| assets.texture(&format!("{ns}:{p}"))) else { return };
        let height = pr.get("height").and_then(|v| v.as_f64()).unwrap_or(8.0) as f32;
        let ascent = pr.get("ascent").and_then(|v| v.as_f64()).unwrap_or(7.0) as f32;
        let Some(rows) = pr.get("chars").and_then(|v| v.as_array()) else { return };
        let rows: Vec<Vec<char>> = rows.iter().filter_map(|r| r.as_str()).map(|s| s.chars().collect()).collect();
        if rows.is_empty() {
            return;
        }
        let ncols = rows.iter().map(|r| r.len()).max().unwrap_or(1) as u32;
        let cw = tex.w / ncols.max(1);
        let ch = tex.h / rows.len() as u32;
        for (ry, row) in rows.iter().enumerate() {
            for (rx, c) in row.iter().enumerate() {
                if *c == '\0' || *c == ' ' || self.glyphs.contains_key(c) {
                    continue;
                }
                let (x0, y0) = (rx as u32 * cw, ry as u32 * ch);
                // actual width: last non-transparent column
                let mut last: i32 = -1;
                for x in 0..cw {
                    for y in 0..ch {
                        let i = (((y0 + y) * tex.w + x0 + x) * 4 + 3) as usize;
                        if tex.rgba.get(i).copied().unwrap_or(0) > 0 {
                            last = last.max(x as i32);
                        }
                    }
                }
                let scale = height / ch as f32;
                let width = ((last + 1) as f32 * scale).ceil();
                self.glyphs.insert(*c, Glyph { tex: tex.clone(), x: x0, y: y0, cw, ch, height, ascent, width });
            }
        }
    }

    pub fn advance(&self, c: char) -> f32 {
        if c == ' ' {
            return 4.0;
        }
        self.glyphs.get(&c).map(|g| g.width + 1.0).unwrap_or(6.0)
    }

    pub fn width(&self, spans: &[Span]) -> f32 {
        spans.iter().flat_map(|(s, _)| s.chars()).map(|c| self.advance(c)).sum()
    }

    /// Draws lines (centered or aligned) into an RGBA image. `line_h` = line height in pixels.
    /// Returns (width, height, pixels).
    pub fn render(&self, lines: &[Vec<Span>], line_h: u32, align: Align, bg: [u8; 4], pad: u32) -> (u32, u32, Vec<u8>) {
        let widths: Vec<f32> = lines.iter().map(|l| self.width(l)).collect();
        let maxw = widths.iter().cloned().fold(0.0, f32::max).ceil() as u32;
        let w = (maxw + pad * 2).max(1);
        let h = (lines.len() as u32 * line_h + pad * 2).max(1);
        let mut img = vec![0u8; (w * h * 4) as usize];
        for p in img.chunks_exact_mut(4) {
            p.copy_from_slice(&bg);
        }
        for (li, line) in lines.iter().enumerate() {
            let lw = widths[li];
            let mut x = pad as f32
                + match align {
                    Align::Center => ((maxw as f32 - lw) / 2.0).floor(),
                    Align::Left => 0.0,
                    Align::Right => maxw as f32 - lw,
                };
            let base_y = pad as f32 + li as f32 * line_h as f32;
            for (s, col) in line {
                for c in s.chars() {
                    if let Some(g) = self.glyphs.get(&c) {
                        let scale = g.height / g.ch as f32;
                        let gy0 = base_y + 7.0 - g.ascent;
                        let gw = (g.width / scale).ceil() as u32;
                        for yy in 0..g.ch {
                            for xx in 0..gw.min(g.cw) {
                                let si = (((g.y + yy) * g.tex.w + g.x + xx) * 4) as usize;
                                let a = g.tex.rgba.get(si + 3).copied().unwrap_or(0);
                                if a == 0 {
                                    continue;
                                }
                                // nearest-neighbor sampling when the glyph is larger than 8 px
                                let px = (x + xx as f32 * scale) as i32;
                                let py = (gy0 + yy as f32 * scale) as i32;
                                let reps = scale.ceil().max(1.0) as i32;
                                for dy in 0..reps {
                                    for dx in 0..reps {
                                        let (tx, ty) = (px + dx, py + dy);
                                        if tx < 0 || ty < 0 || tx >= w as i32 || ty >= h as i32 {
                                            continue;
                                        }
                                        let di = ((ty as u32 * w + tx as u32) * 4) as usize;
                                        let sr = g.tex.rgba[si] as u32;
                                        img[di] = (sr * col[0] as u32 / 255) as u8;
                                        img[di + 1] = (g.tex.rgba[si + 1] as u32 * col[1] as u32 / 255) as u8;
                                        img[di + 2] = (g.tex.rgba[si + 2] as u32 * col[2] as u32 / 255) as u8;
                                        img[di + 3] = 255;
                                    }
                                }
                            }
                        }
                    }
                    x += self.advance(c);
                }
            }
        }
        (w, h, img)
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

pub fn named_color(s: &str) -> Option<[u8; 3]> {
    if let Some(h) = s.strip_prefix('#') {
        let n = u32::from_str_radix(h, 16).ok()?;
        return Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
    }
    let n: u32 = match s {
        "black" => 0x000000,
        "dark_blue" => 0x0000AA,
        "dark_green" => 0x00AA00,
        "dark_aqua" => 0x00AAAA,
        "dark_red" => 0xAA0000,
        "dark_purple" => 0xAA00AA,
        "gold" => 0xFFAA00,
        "gray" => 0xAAAAAA,
        "dark_gray" => 0x555555,
        "blue" => 0x5555FF,
        "green" => 0x55FF55,
        "aqua" => 0x55FFFF,
        "red" => 0xFF5555,
        "light_purple" => 0xFF55FF,
        "yellow" => 0xFFFF55,
        "white" => 0xFFFFFF,
        _ => return None,
    };
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// Dye text color (signs)
pub fn dye_text_color(s: &str) -> [u8; 3] {
    let n: u32 = match s {
        "white" => 0xFFFFFF,
        "orange" => 0xFF681F,
        "magenta" => 0xFF00FF,
        "light_blue" => 0x9AC0CD,
        "yellow" => 0xFFFF00,
        "lime" => 0xBFFF00,
        "pink" => 0xFF69B4,
        "gray" => 0x808080,
        "light_gray" => 0xD3D3D3,
        "cyan" => 0x00FFFF,
        "purple" => 0xA020F0,
        "blue" => 0x0000FF,
        "brown" => 0x8B4513,
        "green" => 0x00FF00,
        "red" => 0xFF0000,
        _ => 0x000000,
    };
    [(n >> 16) as u8, (n >> 8) as u8, n as u8]
}

fn flatten_json(v: &serde_json::Value, color: [u8; 3], out: &mut Vec<Span>) {
    match v {
        serde_json::Value::String(s) => out.push((s.clone(), color)),
        serde_json::Value::Array(a) => {
            for x in a {
                flatten_json(x, color, out);
            }
        }
        serde_json::Value::Object(o) => {
            let c = o.get("color").and_then(|c| c.as_str()).and_then(named_color).unwrap_or(color);
            if let Some(t) = o.get("text").and_then(|t| t.as_str()) {
                out.push((t.to_string(), c));
            } else if let Some(t) = o.get("translate").and_then(|t| t.as_str()) {
                let fb = o.get("fallback").and_then(|t| t.as_str()).unwrap_or(t);
                out.push((fb.to_string(), c));
            }
            if let Some(e) = o.get("extra") {
                flatten_json(e, c, out);
            }
        }
        serde_json::Value::Number(n) => out.push((n.to_string(), color)),
        serde_json::Value::Bool(b) => out.push((b.to_string(), color)),
        _ => {}
    }
}

fn flatten_nbt(v: &Value, color: [u8; 3], out: &mut Vec<Span>) {
    match v {
        Value::String(s) => {
            let t = s.trim_start();
            if t.starts_with('{') || t.starts_with('[') || t.starts_with('"') {
                if let Ok(j) = serde_json::from_str::<serde_json::Value>(s) {
                    flatten_json(&j, color, out);
                    return;
                }
            }
            out.push((s.clone(), color));
        }
        Value::List(l) => {
            for x in l {
                flatten_nbt(x, color, out);
            }
        }
        Value::Compound(m) => {
            let c = match m.get("color") {
                Some(Value::String(s)) => named_color(s).unwrap_or(color),
                _ => color,
            };
            match (m.get("text"), m.get("translate")) {
                (Some(Value::String(t)), _) => out.push((t.clone(), c)),
                (_, Some(Value::String(t))) => {
                    let fb = match m.get("fallback") {
                        Some(Value::String(f)) => f.clone(),
                        _ => t.clone(),
                    };
                    out.push((fb, c));
                }
                _ => {}
            }
            if let Some(e) = m.get("extra") {
                flatten_nbt(e, c, out);
            }
        }
        _ => {}
    }
}

/// Text component (NBT or JSON string) → lines of colored segments
pub fn component_lines(v: &Value, default: [u8; 3]) -> Vec<Vec<Span>> {
    let mut spans = Vec::new();
    flatten_nbt(v, default, &mut spans);
    let mut lines: Vec<Vec<Span>> = vec![Vec::new()];
    for (s, c) in spans {
        let mut first = true;
        for part in s.split('\n') {
            if !first {
                lines.push(Vec::new());
            }
            first = false;
            if !part.is_empty() {
                lines.last_mut().unwrap().push((part.to_string(), c));
            }
        }
    }
    lines
}
