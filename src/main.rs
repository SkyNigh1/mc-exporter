#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod atlas;
mod bent;
mod clouds;
mod deco;
mod entity;
mod font;
mod gltf;
mod i18n;
mod legacy;
mod pose;
mod preview;
mod print;
mod export;
mod map;
mod mesh;
mod mobs;
mod mobs2;
mod paths;
mod registry;
mod schem;
mod schem_out;
mod skins;
mod update;
mod world;

use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let dir = paths::app_dir();
        let _ = std::fs::create_dir_all(&dir);
        let bt = std::backtrace::Backtrace::force_capture();
        let _ = std::fs::write(dir.join("crash.log"), format!("{info}\n\n{bt}"));
        eprintln!("{info}");
    }));
    let raw: Vec<String> = std::env::args().collect();
    // options: --pack <path> (repeatable), --hide-cavities, --detail <1|2|4>, --block-mm <mm>, --hollow <blocks>, --no-clouds
    let mut packs: Vec<PathBuf> = Vec::new();
    let mut mods_dir: Option<PathBuf> = None;
    let mut no_mods = false;
    let mut opts = mesh::ExportOptions::default();
    let mut args: Vec<String> = Vec::new();
    let mut it = raw.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--pack" => packs.extend(it.next().map(PathBuf::from)),
            "--hide-cavities" => opts.hide_cavities = true,
            "--no-clouds" => opts.clouds = false,
            "--detail" => opts.print_detail = it.next().and_then(|v| v.parse().ok()).unwrap_or(2),
            "--block-mm" => opts.print_block_mm = it.next().and_then(|v| v.parse().ok()).unwrap_or(1.0),
            "--hollow" => opts.print_hollow = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--mods" => mods_dir = it.next().map(PathBuf::from),
            "--no-mods" => no_mods = true,
            "--ao" => opts.ambient_occlusion = true,
            "--atlas" => opts.atlas = true,
            _ => args.push(a),
        }
    }
    // mods: --mods <folder>, else the mods folder next to the world's saves folder
    let world_arg = if args.len() > 2 && args[1] == "--export" { Some(PathBuf::from(&args[2])) } else { None };
    if !no_mods {
        let found = match (&mods_dir, &world_arg) {
            (Some(d), _) => assets::find_mods(d),
            (None, Some(w)) => assets::find_mods(w),
            _ => Vec::new(),
        };
        packs.extend(found);
    }
    if args.len() > 1 && args[1] == "--version" {
        println!("MC Exporter {}", env!("CARGO_PKG_VERSION"));
        match update::check() {
            Some(u) => println!("version {} is available: {}", u.version, u.url),
            None => println!("no newer version found"),
        }
        return;
    }
    if args.len() > 3 && args[1] == "--schem" {
        let r = (|| -> anyhow::Result<()> {
            let t0 = std::time::Instant::now();
            let jars = assets::list_jars(&assets::minecraft_dir());
            let jar = match args.get(4) { Some(p) => assets::JarInfo { path: PathBuf::from(p), game_version: String::new(), id: String::new() }, None => assets::pick_jar(&jars, None).ok_or_else(|| anyhow::anyhow!("no Minecraft jar found"))? };
            let reg = registry::Registry::new(Arc::new(assets::Assets::load(&jar.path, &packs)?));
            let s = schem::load(std::path::Path::new(&args[2]), &reg)?;
            println!("{}: {} — {:?} — {} blocks", s.name, s.format, s.size, s.blocks);
            println!("{} block entities, {} entities", s.block_entities, s.entities);
            let source = world::Source::Mem(Arc::new(s.world));
            let sel = mesh::Selection { x0: 0, z0: 0, x1: s.size[0] - 1, z1: s.size[2] - 1, y0: 0, y1: s.size[1] - 1 };
            let out = PathBuf::from(&args[3]);
            let mut opts = opts.clone();
            opts.format = out.extension().and_then(|e| mesh::ExportFormat::from_ext(&e.to_string_lossy())).unwrap_or(mesh::ExportFormat::Obj);
            let prog = mesh::Progress::new();
            let r = export::run_export(&reg, &source, sel, &opts, &out, &s.name, &prog)?;
            println!("{}: {} faces, {} materials, {:.1} MB in {:?}", opts.format.ext(), r.quads, r.materials, r.obj_bytes as f64 / 1e6, t0.elapsed());
            if !r.unknown.is_empty() {
                println!("not drawn by this version: {}", r.unknown.join(", "));
            }
            Ok(())
        })();
        if let Err(e) = r {
            eprintln!("Error: {e:#}");
        }
        return;
    }
    if args.len() > 1 && (args[1] == "--export" || args[1] == "--map") {
        if let Err(e) = cli(&args[1..], &packs, opts) {
            eprintln!("Error: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(e) = app::run() {
        eprintln!("Error: {e}");
    }
}

/// Command-line mode:
/// --export <world> <dimension> x0 z0 x1 z1 y0 y1 <output.obj|.glb|.stl|.3mf>
/// --map <world> <dimension> rx rz <output.png>
fn cli(a: &[String], packs: &[PathBuf], mut opts: mesh::ExportOptions) -> anyhow::Result<()> {
    let t0 = std::time::Instant::now();
    let wpath = PathBuf::from(&a[1]);
    let info = world::WorldInfo::open(&wpath)?;
    let jars = assets::list_jars(&assets::minecraft_dir());
    let jar = assets::pick_jar(&jars, info.version_name.as_deref()).ok_or_else(|| anyhow::anyhow!("no Minecraft jar found"))?;
    println!("World \"{}\" version {:?}, jar {}", info.name, info.version_name, jar.path.display());
    let assets = Arc::new(assets::Assets::load(&jar.path, packs)?);
    let reg = registry::Registry::new(assets);
    let dim = info.dims.iter().find(|d| d.id == a[2]).ok_or_else(|| anyhow::anyhow!("unknown dimension {}", a[2]))?;
    let n = |i: usize| -> i32 { a[i].parse().unwrap() };
    if a[0] == "--map" {
        let m = map::render_region(&reg, &dim.source, n(3), n(4), -64, 320);
        image::save_buffer(&a[5], &m.rgba, 512, 512, image::ColorType::Rgba8)?;
        println!("Map written in {:?}", t0.elapsed());
        return Ok(());
    }
    let sel = mesh::Selection { x0: n(3), z0: n(4), x1: n(5), z1: n(6), y0: n(7), y1: n(8) };
    let out = PathBuf::from(&a[9]);
    opts.format = out.extension().and_then(|e| mesh::ExportFormat::from_ext(&e.to_string_lossy())).unwrap_or(mesh::ExportFormat::Obj);
    let prog = mesh::Progress::new();
    let s = export::run_export(&reg, &dim.source, sel, &opts, &out, &info.name, &prog)?;
    println!(
        "{}: {} faces, {} vertices, {} materials, {:.1} MB in {:?}",
        opts.format.ext(),
        s.quads,
        s.vertices,
        s.materials,
        s.obj_bytes as f64 / 1e6,
        t0.elapsed()
    );
    if !s.unknown.is_empty() {
        println!("not drawn by this version: {}", s.unknown.join(", "));
    }
    Ok(())
}
