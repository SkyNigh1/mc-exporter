//! Saves the selection as a Sponge v3 schematic (.schem), readable by WorldEdit,
//! FAWE, Litematica (import) and Amulet: blocks, block entities and entities.

use crate::export::ExportSummary;
use crate::mesh::{ExportOptions, Progress, Selection};
use crate::registry::Registry;
use crate::world::Source;
use anyhow::{Context, Result};
use fastnbt::Value;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;

fn varint(out: &mut Vec<i8>, mut v: u32) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b as i8);
            break;
        }
        out.push((b | 0x80) as i8);
    }
}

fn compound(pairs: Vec<(&str, Value)>) -> Value {
    Value::Compound(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

/// Data version of the world (level.dat), else of the game jar
fn data_version(reg: &Registry, source: &Source) -> i32 {
    let from_level = source.world_dir().and_then(|d| {
        let raw = std::fs::read(d.join("level.dat")).ok()?;
        let mut dec = flate2::read::GzDecoder::new(&raw[..]);
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut dec, &mut buf).ok()?;
        let v: Value = fastnbt::from_bytes(&buf).ok()?;
        match v {
            Value::Compound(m) => match m.get("Data")? {
                Value::Compound(d) => match d.get("DataVersion")? {
                    Value::Int(i) => Some(*i),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    });
    from_level.or_else(|| reg.assets.jar_data_version()).unwrap_or(4189)
}

pub fn write(reg: &Registry, source: &Source, sel: Selection, opts: &ExportOptions, path: &Path, prog: &Progress) -> Result<ExportSummary> {
    let sel = sel.normalized();
    let (w, h, l) = (sel.x1 - sel.x0 + 1, sel.y1 - sel.y0 + 1, sel.z1 - sel.z0 + 1);
    if w > 32767 || h > 32767 || l > 32767 {
        anyhow::bail!("selection too large for a schematic");
    }
    prog.set_stage(&crate::i18n::t("Reading chunks"), 1);
    let chunks = source.chunks(sel.x0 >> 4, sel.z0 >> 4, sel.x1 >> 4, sel.z1 >> 4, reg);
    let excluded: HashSet<&str> = opts.excluded.iter().map(|s| s.as_str()).collect();

    // palette and block data (index = x + z * width + y * width * length)
    prog.set_stage(&crate::i18n::t("Writing blocks"), h as usize);
    let mut palette: HashMap<String, i32> = HashMap::new();
    palette.insert("minecraft:air".into(), 0);
    let mut by_state: HashMap<u32, u32> = HashMap::new();
    let mut data: Vec<i8> = Vec::with_capacity((w * h * l) as usize);
    let mut bpalette: HashMap<String, i32> = HashMap::new();
    let mut bdata: Vec<i8> = Vec::with_capacity((w * h * l) as usize);
    let mut blocks = 0usize;
    for y in sel.y0..=sel.y1 {
        for z in sel.z0..=sel.z1 {
            for x in sel.x0..=sel.x1 {
                let id = chunks.get(&(x >> 4, z >> 4)).map(|c| c.block((x & 15) as usize, y, (z & 15) as usize)).unwrap_or(0);
                let idx = *by_state.entry(id).or_insert_with(|| {
                    let k = reg.state(id);
                    if id == 0 || excluded.contains(k.name.as_str()) {
                        return 0;
                    }
                    let name = if k.props.is_empty() {
                        k.name.clone()
                    } else {
                        let p: Vec<String> = k.props.iter().map(|(a, b)| format!("{a}={b}")).collect();
                        format!("{}[{}]", k.name, p.join(","))
                    };
                    let n = palette.len() as i32;
                    *palette.entry(name).or_insert(n) as u32
                });
                if idx != 0 {
                    blocks += 1;
                }
                varint(&mut data, idx);
                let b = chunks.get(&(x >> 4, z >> 4)).map(|c| c.biome((x & 15) as usize, y, (z & 15) as usize)).unwrap_or(0);
                let bn = reg.biome_name(b);
                let n = bpalette.len() as i32;
                let bi = *bpalette.entry(bn).or_insert(n);
                varint(&mut bdata, bi as u32);
            }
        }
        prog.done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    // block entities
    let mut bes = Vec::new();
    for c in chunks.values() {
        for (&(x, y, z), v) in &c.block_entities {
            if !sel.contains(x, y, z) {
                continue;
            }
            let Value::Compound(m) = v else { continue };
            let id = match m.get("id") {
                Some(Value::String(s)) => s.clone(),
                _ => continue,
            };
            let state = c.block((x & 15) as usize, y, (z & 15) as usize);
            if state == 0 || excluded.contains(reg.state(state).name.as_str()) {
                continue;
            }
            let mut d = m.clone();
            for k in ["x", "y", "z", "id", "keepPacked"] {
                d.remove(k);
            }
            bes.push(compound(vec![
                ("Pos", Value::IntArray(fastnbt::IntArray::new(vec![x - sel.x0, y - sel.y0, z - sel.z0]))),
                ("Id", Value::String(id)),
                ("Data", Value::Compound(d)),
            ]));
        }
    }

    // entities
    let mut ents = Vec::new();
    if opts.render_entities {
        for e in source.entities(sel.x0 >> 4, sel.z0 >> 4, sel.x1 >> 4, sel.z1 >> 4) {
            let Some(p) = crate::entity::entity_pos(&e) else { continue };
            let inside = p[0] >= sel.x0 as f64
                && p[0] < (sel.x1 + 1) as f64
                && p[1] >= sel.y0 as f64
                && p[1] < (sel.y1 + 1) as f64
                && p[2] >= sel.z0 as f64
                && p[2] < (sel.z1 + 1) as f64;
            if !inside || opts.excluded_entities.contains(&crate::entity::kind(&e)) {
                continue;
            }
            let Some(id) = crate::entity::entity_id(&e) else { continue };
            let rel = vec![p[0] - sel.x0 as f64, p[1] - sel.y0 as f64, p[2] - sel.z0 as f64];
            let mut d = match e {
                Value::Compound(m) => m,
                _ => continue,
            };
            d.insert("Pos".into(), Value::List(rel.iter().map(|v| Value::Double(*v)).collect()));
            d.insert("id".into(), Value::String(id.clone()));
            ents.push(compound(vec![
                ("Pos", Value::List(rel.into_iter().map(Value::Double).collect())),
                ("Id", Value::String(id)),
                ("Data", Value::Compound(d)),
            ]));
        }
    }

    let n_ents = ents.len();
    let n_palette = palette.len();
    let schem = compound(vec![
        ("Version", Value::Int(3)),
        ("DataVersion", Value::Int(data_version(reg, source))),
        ("Width", Value::Short(w as i16)),
        ("Height", Value::Short(h as i16)),
        ("Length", Value::Short(l as i16)),
        ("Offset", Value::IntArray(fastnbt::IntArray::new(vec![0, 0, 0]))),
        (
            "Metadata",
            compound(vec![
                ("Name", Value::String(path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())),
                ("Generator", Value::String("MC Exporter".into())),
                ("WEOriginX", Value::Int(sel.x0)),
                ("WEOriginY", Value::Int(sel.y0)),
                ("WEOriginZ", Value::Int(sel.z0)),
            ]),
        ),
        (
            "Blocks",
            compound(vec![
                ("Palette", Value::Compound(palette.into_iter().map(|(k, v)| (k, Value::Int(v))).collect())),
                ("Data", Value::ByteArray(fastnbt::ByteArray::new(data))),
                ("BlockEntities", Value::List(bes)),
            ]),
        ),
        (
            "Biomes",
            compound(vec![
                ("Palette", Value::Compound(bpalette.into_iter().map(|(k, v)| (k, Value::Int(v))).collect())),
                ("Data", Value::ByteArray(fastnbt::ByteArray::new(bdata))),
            ]),
        ),
        ("Entities", Value::List(ents)),
    ]);
    let root = compound(vec![("Schematic", schem)]);
    let bytes = fastnbt::to_bytes(&root)?;
    let f = std::fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut gz = flate2::write::GzEncoder::new(f, flate2::Compression::default());
    gz.write_all(&bytes)?;
    gz.finish()?;
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(ExportSummary { quads: blocks, vertices: n_ents, materials: n_palette, obj_bytes: size, unknown: Vec::new() })
}
