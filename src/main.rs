#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod assets;
mod bent;
mod deco;
mod entity;
mod font;
mod pose;
mod export;
mod map;
mod mesh;
mod mobs;
mod mobs2;
mod registry;
mod schem;
mod skins;
mod world;

use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let dir = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("mc-exporter");
        let _ = std::fs::create_dir_all(&dir);
        let bt = std::backtrace::Backtrace::force_capture();
        let _ = std::fs::write(dir.join("crash.log"), format!("{info}\n\n{bt}"));
        eprintln!("{info}");
    }));
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 3 && args[1] == "--schem" {
        let r = (|| -> anyhow::Result<()> {
            let t0 = std::time::Instant::now();
            let jars = assets::list_jars(&assets::minecraft_dir());
            let jar = match args.get(4) { Some(p) => assets::JarInfo { path: PathBuf::from(p), game_version: String::new(), id: String::new() }, None => assets::pick_jar(&jars, None).ok_or_else(|| anyhow::anyhow!("aucun jar"))? };
            let reg = registry::Registry::new(Arc::new(assets::Assets::load(&jar.path, &[])?));
            let s = schem::load(std::path::Path::new(&args[2]), &reg)?;
            println!("{} : {} — {:?} — {} blocs", s.name, s.format, s.size, s.blocks);
            println!("{} blocs spéciaux, {} entités", s.block_entities, s.entities);
            let source = world::Source::Mem(Arc::new(s.world));
            let sel = mesh::Selection { x0: 0, z0: 0, x1: s.size[0] - 1, z1: s.size[2] - 1, y0: 0, y1: s.size[1] - 1 };
            let opts = mesh::ExportOptions::default();
            let prog = mesh::Progress::new();
            let m = mesh::build_mesh(&reg, &source, sel, &opts, &prog)?;
            let r = export::write_obj(&reg, &m, &opts, &PathBuf::from(&args[3]), &s.name, &prog)?;
            let unknown: Vec<String> = (0..reg.state_count() as u32).filter(|&i| reg.geom(i).unknown).map(|i| reg.state(i).name).collect();
            println!("OBJ : {} quads, {} matériaux en {:?} ; inconnus : {:?}", r.quads, r.materials, t0.elapsed(), unknown);
            Ok(())
        })();
        if let Err(e) = r {
            eprintln!("Erreur : {e:#}");
        }
        return;
    }
    if args.len() > 1 && (args[1] == "--export" || args[1] == "--map") {
        if let Err(e) = cli(&args[1..]) {
            eprintln!("Erreur : {e:#}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(e) = app::run() {
        eprintln!("Erreur : {e}");
    }
}

/// Command-line mode:
/// --export <world> <dimension> x0 z0 x1 z1 y0 y1 <output.obj>
/// --map <world> <dimension> rx rz <output.png>
fn cli(a: &[String]) -> anyhow::Result<()> {
    let t0 = std::time::Instant::now();
    let wpath = PathBuf::from(&a[1]);
    let info = world::WorldInfo::open(&wpath)?;
    let jars = assets::list_jars(&assets::minecraft_dir());
    let jar = assets::pick_jar(&jars, info.version_name.as_deref()).ok_or_else(|| anyhow::anyhow!("aucun jar Minecraft trouvé"))?;
    println!("Monde « {} » version {:?}, jar {}", info.name, info.version_name, jar.path.display());
    let assets = Arc::new(assets::Assets::load(&jar.path, &[])?);
    let reg = registry::Registry::new(assets);
    let dim = info.dims.iter().find(|d| d.id == a[2]).ok_or_else(|| anyhow::anyhow!("dimension inconnue {}", a[2]))?;
    let n = |i: usize| -> i32 { a[i].parse().unwrap() };
    if a[0] == "--map" {
        let m = map::render_region(&reg, &dim.source, n(3), n(4), -64, 320);
        image::save_buffer(&a[5], &m.rgba, 512, 512, image::ColorType::Rgba8)?;
        println!("Carte écrite en {:?}", t0.elapsed());
        return Ok(());
    }
    let sel = mesh::Selection { x0: n(3), z0: n(4), x1: n(5), z1: n(6), y0: n(7), y1: n(8) };
    let opts = mesh::ExportOptions::default();
    let prog = mesh::Progress::new();
    let m = mesh::build_mesh(&reg, &dim.source, sel, &opts, &prog)?;
    println!("{} chunks, {} quads en {:?}", m.chunks_loaded, m.quads.len(), t0.elapsed());
    let s = export::write_obj(&reg, &m, &opts, &PathBuf::from(&a[9]), &info.name, &prog)?;
    println!(
        "OBJ : {} quads, {} sommets, {} matériaux, {:.1} Mo en {:?}",
        s.quads,
        s.vertices,
        s.materials,
        s.obj_bytes as f64 / 1e6,
        t0.elapsed()
    );
    Ok(())
}
