//! Graphical user interface (egui / eframe).

use crate::mesh::ExportFormat;
use crate::assets::{self, Assets, JarInfo};
use crate::export;
use crate::map::{RegionMap, render_region};
use crate::mesh::{ExportOptions, OffsetMode, Progress, Selection, build_mesh};
use crate::registry::Registry;
use crate::i18n::{self, Lang, t, tf};
use crate::schem;
use crate::world::{self, Source, WorldInfo};
use crossbeam_channel::{Receiver, unbounded};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Config {
    world_path: String,
    opts: ExportOptions,
    packs: Vec<PathBuf>,
    jar_override: Option<PathBuf>,
    floor: Option<i32>,
    ceil: Option<i32>,
    sel: Option<[i32; 4]>,
    last_export: Option<PathBuf>,
    lang: Lang,
    view_3d: bool,
    ignore_mods: bool,
}

fn config_path() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default().join("mc-exporter").join("config.json")
}

struct RegionTile {
    tex: egui::TextureHandle,
    top: Vec<u32>,
    height: Vec<i16>,
}

struct ExportJob {
    prog: Arc<Progress>,
    rx: Receiver<Result<String, String>>,
    started: Instant,
}

struct SchemInfo {
    format: String,
    size: [i32; 3],
    blocks: usize,
}

struct Loaded {
    info: WorldInfo,
    reg: Arc<Registry>,
    schem: Option<SchemInfo>,
    /// jar and layers (packs, then mods) the registry was built from
    key: (PathBuf, Vec<PathBuf>),
    mods: Vec<PathBuf>,
}

struct LoadJob {
    rx: Receiver<Result<Loaded, String>>,
}

enum LoadTarget {
    World(PathBuf),
    Schem(PathBuf),
}

pub struct App {
    mc_dir: PathBuf,
    saves: Vec<(String, PathBuf)>,
    world_path: String,
    world: Option<WorldInfo>,
    dim_idx: usize,
    jars: Vec<JarInfo>,
    jar_override: Option<PathBuf>,
    packs: Vec<PathBuf>,
    reg: Option<Arc<Registry>>,
    loaded_assets_key: Option<(PathBuf, Vec<PathBuf>)>,
    status: String,
    load_job: Option<LoadJob>,

    tiles: HashMap<(i32, i32), RegionTile>,
    map_rx: Option<Receiver<RegionMap>>,
    map_cancel: Arc<AtomicBool>,
    map_pending: usize,
    center: [f64; 2],
    zoom: f32,
    floor: i32,
    ceil: i32,
    sel: Option<[i32; 4]>,
    drag_start: Option<[i32; 2]>,
    show_chunks: bool,

    show_goto: bool,
    goto: [i32; 2],
    show_settings: bool,
    show_blocks: bool,
    block_filter: String,

    opts: ExportOptions,
    export: Option<ExportJob>,
    last_result: Option<Result<String, String>>,
    last_export: Option<PathBuf>,
    hover: Option<(i32, i32)>,

    /// 3D preview
    view_3d: bool,
    pv_ok: bool,
    pv_cam: crate::preview::Camera,
    pv_mesh: Option<Arc<crate::preview::PreviewMesh>>,
    pv_gen: u64,
    pv_key: String,
    pv_job: Option<(Receiver<Result<Arc<crate::preview::PreviewMesh>, String>>, Arc<Progress>)>,
    pv_status: String,
    pv_drag: Option<(usize, f32)>,
    /// read blocks from the mods of the world's game folder
    use_mods: bool,
    mods: Vec<PathBuf>,
}

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("MC Exporter"),
        // depth buffer for the 3D preview
        depth_buffer: 32,
        ..Default::default()
    };
    eframe::run_native("MC Exporter", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let pv_ok = match cc.wgpu_render_state.as_ref() {
            Some(rs) => {
                crate::preview::init(rs);
                true
            }
            None => false,
        };
        let cfg: Config = std::fs::read(config_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        i18n::set(cfg.lang);
        let mc_dir = assets::minecraft_dir();
        let saves = world::list_saves(&mc_dir);
        let world_path = if cfg.world_path.is_empty() {
            saves.first().map(|s| s.1.to_string_lossy().to_string()).unwrap_or_default()
        } else {
            cfg.world_path.clone()
        };
        let jars = assets::list_jars(&mc_dir);
        let mut app = App {
            mc_dir,
            saves,
            world_path,
            world: None,
            dim_idx: 0,
            jars,
            jar_override: cfg.jar_override,
            packs: cfg.packs,
            reg: None,
            loaded_assets_key: None,
            status: String::new(),
            load_job: None,
            tiles: HashMap::new(),
            map_rx: None,
            map_cancel: Arc::new(AtomicBool::new(false)),
            map_pending: 0,
            center: [0.0, 0.0],
            zoom: 2.0,
            floor: cfg.floor.unwrap_or(-64),
            ceil: cfg.ceil.unwrap_or(320),
            sel: cfg.sel,
            drag_start: None,
            show_chunks: false,
            show_goto: false,
            goto: [0, 0],
            show_settings: false,
            show_blocks: false,
            block_filter: String::new(),
            opts: cfg.opts,
            export: None,
            last_result: None,
            last_export: cfg.last_export,
            hover: None,
            view_3d: cfg.view_3d && pv_ok,
            pv_ok,
            pv_cam: crate::preview::Camera::new(16.0),
            pv_mesh: None,
            pv_gen: 0,
            pv_key: String::new(),
            pv_job: None,
            pv_status: String::new(),
            pv_drag: None,
            use_mods: !cfg.ignore_mods,
            mods: Vec::new(),
        };
        if app.jars.is_empty() {
            app.status = t("No Minecraft version found in .minecraft/versions: launch the game once or pick a jar in the settings.");
        } else if std::path::Path::new(app.world_path.trim()).join("level.dat").is_file() {
            app.start_load();
        }
        app
    }

    fn save_config(&self) {
        let cfg = Config {
            world_path: self.world_path.clone(),
            opts: self.opts.clone(),
            packs: self.packs.clone(),
            jar_override: self.jar_override.clone(),
            floor: Some(self.floor),
            ceil: Some(self.ceil),
            sel: self.sel,
            last_export: self.last_export.clone(),
            lang: i18n::current(),
            view_3d: self.view_3d,
            ignore_mods: !self.use_mods,
        };
        let p = config_path();
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, serde_json::to_vec_pretty(&cfg).unwrap_or_default());
    }

    fn start_load(&mut self) {
        let path = PathBuf::from(self.world_path.trim());
        self.spawn_load(LoadTarget::World(path));
    }

    fn start_import(&mut self, path: PathBuf) {
        self.spawn_load(LoadTarget::Schem(path));
    }

    fn pick_schematic(&mut self) {
        let mut dlg = rfd::FileDialog::new().add_filter("Schematic", schem::EXTENSIONS);
        let we = self.mc_dir.join("config").join("worldedit").join("schematics");
        let lite = self.mc_dir.join("schematics");
        if we.is_dir() {
            dlg = dlg.set_directory(we);
        } else if lite.is_dir() {
            dlg = dlg.set_directory(lite);
        }
        if let Some(p) = dlg.pick_file() {
            self.start_import(p);
        }
    }

    fn spawn_load(&mut self, target: LoadTarget) {
        let jars = self.jars.clone();
        let jar_override = self.jar_override.clone();
        let packs = self.packs.clone();
        let use_mods = self.use_mods;
        let mc_dir = self.mc_dir.clone();
        let reuse = self.reg.clone();
        let prev_key = self.loaded_assets_key.clone();
        let (tx, rx) = unbounded();
        self.status = match &target {
            LoadTarget::World(_) => t("Loading world…"),
            LoadTarget::Schem(p) => tf("Importing {0}…", &[&p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()]),
        };
        std::thread::spawn(move || {
            let r = (|| -> Result<Loaded, String> {
                let world = match &target {
                    LoadTarget::World(p) => Some(WorldInfo::open(p).map_err(|e| format!("{e:#}"))?),
                    LoadTarget::Schem(_) => None,
                };
                let version = world.as_ref().and_then(|w| w.version_name.clone());
                let jar = match jar_override {
                    Some(j) => j,
                    None => match (&world, &reuse) {
                        // for a schematic, keep the resources that are already loaded
                        (None, Some(r)) => r.assets.jar_path.clone(),
                        _ => {
                            assets::pick_jar(&jars, version.as_deref())
                                .ok_or_else(|| t("No Minecraft jar found (Settings → pick a jar)"))?
                                .path
                        }
                    },
                };
                // mods of the world's game folder (or of .minecraft for a schematic)
                let mods = if !use_mods {
                    Vec::new()
                } else {
                    match &target {
                        LoadTarget::World(p) => assets::find_mods(p),
                        LoadTarget::Schem(_) => assets::find_mods(&mc_dir),
                    }
                };
                let mut layers = packs.clone();
                layers.extend(mods.iter().cloned());
                let key = (jar.clone(), layers.clone());
                // keep the registry if the jar, the packs and the mods did not change
                let reg = match (reuse, prev_key) {
                    (Some(r), Some(k)) if k == key => r,
                    _ => {
                        let a = Assets::load(&jar, &layers).map_err(|e| format!("{e:#}"))?;
                        Arc::new(Registry::new(Arc::new(a)))
                    }
                };
                match target {
                    LoadTarget::World(_) => Ok(Loaded { info: world.unwrap(), reg, schem: None, key, mods }),
                    LoadTarget::Schem(p) => {
                        let s = schem::load(&p, &reg).map_err(|e| format!("{e:#}"))?;
                        let info = WorldInfo {
                            path: p.clone(),
                            name: s.name.clone(),
                            version_name: None,
                            spawn: (s.size[0] / 2, s.size[2] / 2),
                            dims: vec![world::DimInfo { id: "schematic".into(), source: Source::Mem(Arc::new(s.world)) }],
                        };
                        Ok(Loaded { info, reg, schem: Some(SchemInfo { format: s.format, size: s.size, blocks: s.blocks }), key, mods })
                    }
                }
            })();
            let _ = tx.send(r);
        });
        self.load_job = Some(LoadJob { rx });
    }

    fn is_schem(&self) -> bool {
        self.world.as_ref().map(|w| w.dims.iter().any(|d| matches!(d.source, Source::Mem(_)))).unwrap_or(false)
    }

    fn source(&self) -> Option<Source> {
        self.world.as_ref().and_then(|w| w.dims.get(self.dim_idx)).map(|d| d.source.clone())
    }

    fn start_map(&mut self) {
        self.map_cancel.store(true, Ordering::Relaxed);
        self.tiles.clear();
        let (Some(reg), Some(dir)) = (self.reg.clone(), self.source()) else { return };
        let mut regions = dir.regions();
        let c = [self.center[0] / 512.0, self.center[1] / 512.0];
        regions.sort_by(|a, b| {
            let da = (a.0 as f64 + 0.5 - c[0]).powi(2) + (a.1 as f64 + 0.5 - c[1]).powi(2);
            let db = (b.0 as f64 + 0.5 - c[0]).powi(2) + (b.1 as f64 + 0.5 - c[1]).powi(2);
            da.partial_cmp(&db).unwrap()
        });
        self.map_pending = regions.len();
        let cancel = Arc::new(AtomicBool::new(false));
        self.map_cancel = cancel.clone();
        let (tx, rx) = unbounded();
        self.map_rx = Some(rx);
        let (floor, ceil) = (self.floor, self.ceil);
        std::thread::spawn(move || {
            use rayon::prelude::*;
            regions.par_iter().for_each(|&(rx_, rz_)| {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let m = render_region(&reg, &dir, rx_, rz_, floor, ceil);
                if !cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(m);
                }
            });
        });
    }

    fn poll_jobs(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.load_job {
            if let Ok(r) = job.rx.try_recv() {
                self.load_job = None;
                match r {
                    Ok(Loaded { info, reg, schem: Some(si), key, mods }) => {
                        let jar = reg.assets.jar_path.clone();
                        self.loaded_assets_key = Some(key);
                        self.mods = mods;
                        self.status = tf(
                            "Schematic “{0}” — {1} — {2} × {3} × {4} — {5} blocks — resources: {6}",
                            &[
                                &info.name,
                                &si.format,
                                &si.size[0],
                                &si.size[1],
                                &si.size[2],
                                &si.blocks,
                                &jar.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                            ],
                        );
                        self.dim_idx = 0;
                        self.sel = Some([0, 0, si.size[0] - 1, si.size[2] - 1]);
                        self.floor = 0;
                        self.ceil = si.size[1] - 1;
                        self.center = [si.size[0] as f64 / 2.0, si.size[2] as f64 / 2.0];
                        let fit = (si.size[0].max(si.size[2]) as f32 * 1.3).max(1.0);
                        self.zoom = (700.0 / fit).clamp(0.05, 64.0);
                        self.world = Some(info);
                        self.reg = Some(reg);
                        self.start_map();
                    }
                    Ok(Loaded { info, reg, schem: None, key, mods }) => {
                        let jar = reg.assets.jar_path.clone();
                        self.loaded_assets_key = Some(key);
                        self.mods = mods;
                        self.status = tf(
                            "“{0}” — Minecraft {1} — resources: {2}",
                            &[
                                &info.name,
                                &info.version_name.clone().unwrap_or("?".into()),
                                &jar.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                            ],
                        );
                        let same_world = self.world.as_ref().map(|w| w.path == info.path).unwrap_or(false);
                        let was_schem = self.is_schem();
                        if was_schem {
                            self.floor = -64;
                            self.ceil = 320;
                        }
                        if !same_world {
                            self.dim_idx = 0;
                            match self.sel {
                                Some(s) if self.world.is_none() => {
                                    self.center = [(s[0] + s[2]) as f64 / 2.0, (s[1] + s[3]) as f64 / 2.0];
                                }
                                _ => {
                                    self.center = [info.spawn.0 as f64, info.spawn.1 as f64];
                                    if self.world.is_some() {
                                        self.sel = None;
                                    }
                                }
                            }
                        }
                        self.world = Some(info);
                        self.reg = Some(reg);
                        self.save_config();
                        self.start_map();
                    }
                    Err(e) => self.status = tf("Error: {0}", &[&e]),
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        if let Some(rx) = &self.map_rx {
            let mut n = 0;
            while let Ok(m) = rx.try_recv() {
                let img = egui::ColorImage::from_rgba_unmultiplied([512, 512], &m.rgba);
                let tex = ctx.load_texture(format!("r.{}.{}", m.rx, m.rz), img, egui::TextureOptions::NEAREST);
                self.tiles.insert((m.rx, m.rz), RegionTile { tex, top: m.top, height: m.height });
                self.map_pending = self.map_pending.saturating_sub(1);
                n += 1;
                if n > 8 {
                    break;
                }
            }
            if self.map_pending > 0 {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
        if let Some((rx, _)) = &self.pv_job {
            match rx.try_recv() {
                Ok(Ok(m)) => {
                    self.pv_status = tf("{0} faces", &[&m.quads]);
                    let first = self.pv_mesh.is_none();
                    if first || self.pv_cam.dist > m.radius * 20.0 {
                        self.pv_cam = crate::preview::Camera::new(m.radius);
                    }
                    self.pv_mesh = Some(m);
                    self.pv_gen += 1;
                    self.pv_job = None;
                }
                Ok(Err(e)) => {
                    self.pv_status = e;
                    self.pv_job = None;
                }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        if let Some(job) = &self.export {
            if let Ok(r) = job.rx.try_recv() {
                let r = r.map(|s| format!("{s} ({:.1} s)", job.started.elapsed().as_secs_f32()));
                self.last_result = Some(r);
                self.export = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }

    fn start_export(&mut self) {
        let (Some(reg), Some(dir), Some(s), Some(w)) = (self.reg.clone(), self.source(), self.sel, self.world.as_ref()) else {
            self.last_result = Some(Err(t("Load a world and select an area on the map (right-click + drag).")));
            return;
        };
        let fmt = self.opts.format;
        let ext = fmt.ext();
        let label = match fmt {
            ExportFormat::Obj => "Wavefront OBJ",
            ExportFormat::Glb => "glTF binary",
            ExportFormat::Stl => "STL",
            ExportFormat::ThreeMf => "3MF",
            ExportFormat::Schem => "Sponge schematic",
        };
        let mut dlg = rfd::FileDialog::new().add_filter(label, &[ext]);
        let stem = |p: &PathBuf| p.file_stem().map(|s| s.to_string_lossy().to_string());
        match &self.last_export {
            Some(p) => {
                if let Some(d) = p.parent() {
                    dlg = dlg.set_directory(d);
                }
                dlg = dlg.set_file_name(format!("{}.{ext}", stem(p).unwrap_or("export".into())));
            }
            None => dlg = dlg.set_file_name(format!("{}.{ext}", w.name.replace(|c: char| !c.is_alphanumeric(), "_"))),
        }
        let Some(mut out) = dlg.save_file() else { return };
        if out.extension().map(|e| !e.eq_ignore_ascii_case(ext)).unwrap_or(true) {
            let mut n = out.into_os_string();
            n.push(format!(".{ext}"));
            out = PathBuf::from(n);
        }
        self.last_export = Some(out.clone());
        self.save_config();
        let sel = Selection { x0: s[0], z0: s[1], x1: s[2], z1: s[3], y0: self.floor, y1: self.ceil };
        let opts = self.opts.clone();
        let prog = Arc::new(Progress::new());
        let p2 = prog.clone();
        let world_name = w.name.clone();
        let (tx, rx) = unbounded();
        std::thread::spawn(move || {
            let r = (|| -> anyhow::Result<String> {
                let s = export::run_export(&reg, &dir, sel, &opts, &out, &world_name, &p2)?;
                let mut msg = if opts.format == ExportFormat::Schem {
                    tf("Saved: {0} — {1} blocks, {2} entities, {3} block types", &[&out.display(), &s.quads, &s.vertices, &s.materials])
                } else {
                    tf(
                        "Exported: {0} — {1} faces, {2} vertices, {3} materials, {4} MB",
                        &[&out.display(), &s.quads, &s.vertices, &s.materials, &format!("{:.1}", s.obj_bytes as f64 / 1e6)],
                    )
                };
                if !s.unknown.is_empty() {
                    msg.push_str("\n\n");
                    msg.push_str(&tf("Not drawn by this version of MC Exporter ({0}): {1}", &[&s.unknown.len(), &s.unknown.join(", ")]));
                }
                Ok(msg)
            })();
            let _ = tx.send(r.map_err(|e| format!("{e:#}")));
        });
        self.last_result = None;
        self.export = Some(ExportJob { prog, rx, started: Instant::now() });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(t("World"));
            let current = self
                .saves
                .iter()
                .find(|s| s.1.to_string_lossy() == self.world_path)
                .map(|s| s.0.clone())
                .unwrap_or_else(|| t("(other folder)"));
            let mut picked = None;
            egui::ComboBox::from_id_salt("saves").selected_text(current).width(180.0).show_ui(ui, |ui| {
                for (n, p) in &self.saves {
                    if ui.selectable_label(p.to_string_lossy() == self.world_path, n).clicked() {
                        picked = Some(p.clone());
                    }
                }
            });
            if let Some(p) = picked {
                self.world_path = p.to_string_lossy().to_string();
                self.start_load();
            }
            ui.add(egui::TextEdit::singleline(&mut self.world_path).desired_width(360.0));
            if ui.button("…").on_hover_text(t("Choose a world folder")).clicked() {
                if let Some(p) = rfd::FileDialog::new().set_directory(self.mc_dir.join("saves")).pick_folder() {
                    self.world_path = p.to_string_lossy().to_string();
                    self.start_load();
                }
            }
            if ui
                .add_enabled(self.load_job.is_none(), egui::Button::new(t("Import a schematic…")))
                .on_hover_text(t(".schem (Sponge v1/v2/v3), .schematic (legacy MCEdit), .litematic, .nbt — you can also drop the file onto the window"))
                .clicked()
            {
                self.pick_schematic();
            }
            if ui.add_enabled(self.load_job.is_none(), egui::Button::new(t("Load"))).clicked() {
                self.saves = world::list_saves(&self.mc_dir);
                self.start_load();
            }
            if self.load_job.is_some() {
                ui.spinner();
            }
            ui.separator();
            ui.label(t("Dimension"));
            let dims: Vec<(String, bool)> = self
                .world
                .as_ref()
                .map(|w| w.dims.iter().map(|d| (d.id.clone(), !d.source.regions().is_empty())).collect())
                .unwrap_or_default();
            let sel_text = dims.get(self.dim_idx).map(|d| d.0.clone()).unwrap_or("—".into());
            let mut new_dim = None;
            egui::ComboBox::from_id_salt("dims").selected_text(sel_text).width(200.0).show_ui(ui, |ui| {
                for (i, (d, has)) in dims.iter().enumerate() {
                    let label = if *has { d.clone() } else { tf("{0} (empty)", &[d]) };
                    if ui.selectable_label(i == self.dim_idx, label).clicked() {
                        new_dim = Some(i);
                    }
                }
            });
            if let Some(i) = new_dim {
                if i != self.dim_idx {
                    self.dim_idx = i;
                    self.start_map();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut lang = i18n::current();
                egui::ComboBox::from_id_salt("lang").selected_text(lang.native_name()).show_ui(ui, |ui| {
                    for l in Lang::ALL {
                        ui.selectable_value(&mut lang, l, l.native_name());
                    }
                })
                .response
                .on_hover_text(t("Language"));
                if lang != i18n::current() {
                    i18n::set(lang);
                    self.save_config();
                }
                if ui.button(t("Settings")).clicked() {
                    self.show_settings = !self.show_settings;
                }
                if ui.button(t("Go to…")).clicked() {
                    self.goto = [self.center[0] as i32, self.center[1] as i32];
                    self.show_goto = !self.show_goto;
                }
            });
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(t("Floor"));
            let f = ui.add(egui::DragValue::new(&mut self.floor).range(-2048..=4096));
            ui.label(t("Ceiling"));
            let c = ui.add(egui::DragValue::new(&mut self.ceil).range(-2048..=4096));
            if (f.lost_focus() || f.drag_stopped() || c.lost_focus() || c.drag_stopped()) && self.reg.is_some() {
                self.start_map();
            }
            if ui.button(t("Refresh map")).clicked() {
                self.start_map();
            }
            ui.checkbox(&mut self.show_chunks, t("Show chunks"));
            ui.separator();
            let mut s = self.sel.unwrap_or([0, 0, 0, 0]);
            let before = s;
            ui.label(format!("{}  X", t("Position 1")));
            ui.add(egui::DragValue::new(&mut s[0]));
            ui.label("Z");
            ui.add(egui::DragValue::new(&mut s[1]));
            ui.label(format!("  {}  X", t("Position 2")));
            ui.add(egui::DragValue::new(&mut s[2]));
            ui.label("Z");
            ui.add(egui::DragValue::new(&mut s[3]));
            if s != before {
                self.sel = Some(s);
            }
            if let Some(s) = self.sel {
                let (w, d) = ((s[2] - s[0]).abs() + 1, (s[3] - s[1]).abs() + 1);
                ui.label(format!("  {}", tf("{0} × {1} × {2} blocks", &[&w, &d, &((self.ceil - self.floor).abs() + 1)])));
                if ui.small_button("✖").on_hover_text(t("Clear selection")).clicked() {
                    self.sel = None;
                }
            }
        });
        ui.add_space(4.0);
    }

    fn export_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading(t("Export"));
            ui.add_space(4.0);
            let o = &mut self.opts;
            let fname = |f: ExportFormat| match f {
                ExportFormat::Obj => t("OBJ (textured, all 3D software)"),
                ExportFormat::Glb => t("glTF .glb (single file, Blender, game engines)"),
                ExportFormat::Stl => t("STL (3D printing)"),
                ExportFormat::ThreeMf => t("3MF (3D printing, with colors)"),
                ExportFormat::Schem => t("Schematic .schem (WorldEdit, Litematica)"),
            };
            egui::ComboBox::from_id_salt("format").selected_text(fname(o.format)).width(ui.available_width() - 8.0).show_ui(ui, |ui| {
                for f in [ExportFormat::Obj, ExportFormat::Glb, ExportFormat::Stl, ExportFormat::ThreeMf, ExportFormat::Schem] {
                    ui.selectable_value(&mut o.format, f, fname(f));
                }
            });
            ui.add_space(4.0);
            if o.format.is_print() {
                self.print_panel(ui);
                return;
            }
            if o.format == ExportFormat::Schem {
                ui.label(egui::RichText::new(t("Saves the blocks, block entities and entities of the selection, to paste them with WorldEdit or share the build.")).weak());
                ui.separator();
                let n_ex = o.excluded.len();
                if ui
                    .button(if n_ex == 0 { t("Choose blocks to export…") } else { tf("Choose blocks to export… ({0} excluded)", &[&n_ex]) })
                    .clicked()
                {
                    self.show_blocks = true;
                }
                let o = &mut self.opts;
                ui.checkbox(&mut o.render_entities, t("Entities (mobs, displays, item frames, paintings…)"));
            entity_filter(ui, o);
            let o = &mut self.opts;
                entity_filter(ui, o);
                self.export_button(ui);
                return;
            }
            let o = &mut self.opts;
            egui::Grid::new("scale").num_columns(2).show(ui, |ui| {
                ui.label(t("Scale (size of one block)"));
                ui.add(egui::DragValue::new(&mut o.scale).speed(0.01).range(0.001..=1000.0));
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.label(egui::RichText::new(t("Offset")).strong());
            ui.horizontal(|ui| {
                ui.radio_value(&mut o.offset_mode, OffsetMode::None, t("None"));
                ui.radio_value(&mut o.offset_mode, OffsetMode::Center, t("Center"));
                ui.radio_value(&mut o.offset_mode, OffsetMode::Custom, t("Custom"));
            });
            if o.offset_mode == OffsetMode::Custom {
                ui.horizontal(|ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(&mut o.offset_x));
                    ui.label("Z");
                    ui.add(egui::DragValue::new(&mut o.offset_z));
                });
            }
            ui.separator();
            ui.label(egui::RichText::new(t("Content")).strong());
            let n_ex = o.excluded.len();
            if ui
                .button(if n_ex == 0 { t("Choose blocks to export…") } else { tf("Choose blocks to export… ({0} excluded)", &[&n_ex]) })
                .clicked()
            {
                self.show_blocks = true;
            }
            let o = &mut self.opts;
            ui.checkbox(&mut o.render_unknown, t("Show unknown blocks (mods)"));
            ui.checkbox(&mut o.render_sides, t("World sides and bottom"));
            ui.checkbox(&mut o.hide_cavities, t("Hide closed cavities"))
                .on_hover_text(t("Removes what cannot be seen from outside the selection: closed caves, sealed rooms and their contents."));
            ui.checkbox(&mut o.render_biomes, t("Biome colors"));
            ui.checkbox(&mut o.ore_to_stone, t("Convert ores to stone"));
            ui.checkbox(&mut o.render_block_entities, t("Chests, banners, heads, signs…"));
            ui.add_enabled_ui(o.render_block_entities || o.render_entities, |ui| {
                ui.checkbox(&mut o.sign_text, t("Sign text"));
            });
            ui.checkbox(&mut o.render_entities, t("Entities (mobs, displays, item frames, paintings…)"));
            ui.checkbox(&mut o.download_skins, t("Download player head skins"))
                .on_hover_text(t("Skins already seen in game are read from the Minecraft cache; the others are requested from Mojang's servers."));
            ui.horizontal(|ui| {
                ui.checkbox(&mut o.clouds, t("Clouds"));
                ui.add_enabled(o.clouds, egui::DragValue::new(&mut o.cloud_height).range(-2048..=4096))
                    .on_hover_text(t("Cloud height (192 since 1.18, 128 before)"));
            });
            ui.checkbox(&mut o.ghast_on_ground, t("Place ghasts on their tentacles"))
                .on_hover_text(t("In the game, ghast tentacles hang below their position. Check this to raise them to ground level."));
            ui.separator();
            ui.label(egui::RichText::new(t("Structure")).strong());
            ui.checkbox(&mut o.obj_per_material, t("One object per material"));
            ui.checkbox(&mut o.obj_per_chunk, t("One object per chunk"));
            ui.checkbox(&mut o.optimize, t("Optimize mesh (merge faces)"));
            ui.checkbox(&mut o.double_sided, t("Double-sided faces (plants, thin panes)"));
            ui.checkbox(&mut o.ambient_occlusion, t("Ambient occlusion (shaded corners)"))
                .on_hover_text(t("Darkens the corners between blocks like the game's smooth lighting, stored as vertex colors."));
            ui.checkbox(&mut o.randomize, t("Random model variants"));
            if o.format == ExportFormat::Obj {
                ui.checkbox(&mut o.use_groups, t("OBJ groups “g” (Maya compatible)"));
            }
            ui.separator();
            ui.label(egui::RichText::new(t("Textures")).strong());
            ui.checkbox(&mut o.export_textures, t("Export textures"));
            ui.add_enabled_ui(o.export_textures, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("Upscale"));
                    egui::ComboBox::from_id_salt("prescale").selected_text(format!("{}×", o.prescale)).show_ui(ui, |ui| {
                        for f in [1u32, 2, 4, 8, 16] {
                            ui.selectable_value(&mut o.prescale, f, format!("{f}×"));
                        }
                    });
                });
                if o.format == ExportFormat::Obj {
                    ui.checkbox(&mut o.alpha_separate, t("Alpha channel in separate files"));
                }
                ui.checkbox(&mut o.pbr, t("PBR maps from resource packs"))
                    .on_hover_text(t("Normal, roughness, metal and emission maps of LabPBR resource packs (_n and _s textures)."));
                ui.checkbox(&mut o.atlas, t("Texture atlas (3 materials only)"))
                    .on_hover_text(t("Packs every texture into one image per kind (solid, cut-out, translucent). Lighter for game engines; faces are not merged."));
            });
            self.export_button(ui);
        });
    }

    /// Options of the 3D printing formats
    fn print_panel(&mut self, ui: &mut egui::Ui) {
        let o = &mut self.opts;
        egui::Grid::new("print").num_columns(2).show(ui, |ui| {
            ui.label(t("Size of one block (mm)"));
            ui.add(egui::DragValue::new(&mut o.print_block_mm).speed(0.05).range(0.1..=100.0));
            ui.end_row();
            ui.label(t("Detail"));
            egui::ComboBox::from_id_salt("detail")
                .selected_text(match o.print_detail {
                    1 => t("Whole blocks"),
                    4 => t("Fine (¼ block)"),
                    _ => t("Half blocks"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut o.print_detail, 1, t("Whole blocks"));
                    ui.selectable_value(&mut o.print_detail, 2, t("Half blocks"));
                    ui.selectable_value(&mut o.print_detail, 4, t("Fine (¼ block)"));
                });
            ui.end_row();
            ui.label(t("Hollow (wall thickness)"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut o.print_hollow).range(0..=32).suffix(t(" blocks")));
                if o.print_hollow == 0 {
                    ui.label(egui::RichText::new(t("solid")).weak());
                }
            });
            ui.end_row();
            ui.label(t("Remove floating pieces under"));
            ui.add(egui::DragValue::new(&mut o.print_min_piece).range(0..=10000).suffix(t(" blocks")));
            ui.end_row();
        });
        if let Some(s) = self.sel {
            let mm = o.print_block_mm;
            let (w, d, h) = ((s[2] - s[0]).abs() + 1, (s[3] - s[1]).abs() + 1, self.ceil - self.floor + 1);
            ui.label(egui::RichText::new(tf("Maximum size: {0} × {1} × {2} mm", &[
                &format!("{:.0}", w as f32 * mm),
                &format!("{:.0}", d as f32 * mm),
                &format!("{:.0}", h as f32 * mm),
            ])).weak());
        }
        ui.label(egui::RichText::new(t("The model is closed and manifold: air pockets are filled, blocks touching only by an edge are joined, plants and other thin parts are left out. Z points up.")).weak());
        ui.separator();
        ui.label(egui::RichText::new(t("Content")).strong());
        let n_ex = o.excluded.len();
        if ui
            .button(if n_ex == 0 { t("Choose blocks to export…") } else { tf("Choose blocks to export… ({0} excluded)", &[&n_ex]) })
            .clicked()
        {
            self.show_blocks = true;
        }
        let o = &mut self.opts;
        ui.checkbox(&mut o.render_biomes, t("Biome colors"));
        ui.checkbox(&mut o.ore_to_stone, t("Convert ores to stone"));
        ui.checkbox(&mut o.render_unknown, t("Show unknown blocks (mods)"));
        self.export_button(ui);
    }

    fn export_button(&mut self, ui: &mut egui::Ui) {
        {
            ui.separator();
            ui.add_space(6.0);
            let busy = self.export.is_some();
            let can = self.sel.is_some() && self.reg.is_some() && !busy;
            let btn = egui::Button::new(egui::RichText::new(t("Export…")).size(18.0)).min_size(vec2(ui.available_width(), 36.0));
            if ui.add_enabled(can, btn).clicked() {
                self.save_config();
                self.start_export();
            }
            if self.sel.is_none() {
                ui.label(egui::RichText::new(t("Select an area: right-click + drag on the map.")).weak());
            }
            if let Some(job) = &self.export {
                let stage = job.prog.stage.lock().unwrap().clone();
                ui.add(egui::ProgressBar::new(job.prog.fraction()).text(stage).animate(true));
                if ui.button(t("Cancel")).clicked() {
                    job.prog.cancel.store(true, Ordering::Relaxed);
                }
            }
            match &self.last_result {
                Some(Ok(s)) => {
                    ui.label(egui::RichText::new(s).color(Color32::from_rgb(120, 200, 120)));
                    if let Some(p) = &self.last_export {
                        if ui.button(t("Open folder")).clicked() {
                            if let Some(d) = p.parent() {
                                let _ = std::process::Command::new("explorer").arg(d).spawn();
                            }
                        }
                    }
                }
                Some(Err(e)) => {
                    ui.label(egui::RichText::new(e).color(Color32::from_rgb(230, 110, 100)));
                }
                None => {}
            }
        }
    }

    fn world_to_screen(&self, rect: Rect, x: f64, z: f64) -> Pos2 {
        let c = rect.center();
        pos2(c.x + ((x - self.center[0]) * self.zoom as f64) as f32, c.y + ((z - self.center[1]) * self.zoom as f64) as f32)
    }

    fn screen_to_world(&self, rect: Rect, p: Pos2) -> [f64; 2] {
        let c = rect.center();
        [self.center[0] + ((p.x - c.x) / self.zoom) as f64, self.center[1] + ((p.y - c.y) / self.zoom) as f64]
    }

    /// Key of what the preview shows: rebuilt when the selection or the options change
    fn preview_key(&self) -> String {
        let mut o = self.opts.clone();
        // options that do not change the geometry
        o.format = crate::mesh::ExportFormat::Obj;
        o.scale = 1.0;
        o.offset_mode = OffsetMode::None;
        o.prescale = 1;
        format!(
            "{:?}|{}|{}|{:?}|{}|{}|{}",
            self.sel,
            self.floor,
            self.ceil,
            serde_json::to_string(&o).unwrap_or_default(),
            self.world_path,
            self.dim_idx,
            self.reg.as_ref().map(|r| Arc::as_ptr(r) as usize).unwrap_or(0)
        )
    }

    fn start_preview(&mut self) {
        let (Some(reg), Some(src), Some(s)) = (self.reg.clone(), self.source(), self.sel) else {
            self.pv_status = t("Select an area: right-click + drag on the map.");
            return;
        };
        if let Some((_, p)) = &self.pv_job {
            p.cancel.store(true, Ordering::Relaxed);
        }
        self.pv_key = self.preview_key();
        let sel = Selection { x0: s[0], z0: s[1], x1: s[2], z1: s[3], y0: self.floor, y1: self.ceil };
        let mut opts = self.opts.clone();
        opts.optimize = true;
        let prog = Arc::new(Progress::new());
        let p2 = prog.clone();
        let (tx, rx) = unbounded();
        std::thread::spawn(move || {
            let r = build_mesh(&reg, &src, sel, &opts, &p2).map(|m| Arc::new(crate::preview::PreviewMesh::build(&reg, &m)));
            let _ = tx.send(r.map_err(|e| format!("{e:#}")));
        });
        self.pv_job = Some((rx, prog));
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("view-tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.view_3d, false, t("2D map"));
                ui.add_enabled_ui(self.pv_ok, |ui| {
                    ui.selectable_value(&mut self.view_3d, true, t("3D preview"))
                        .on_disabled_hover_text(t("The 3D preview needs a GPU renderer."));
                });
                if self.view_3d {
                    ui.separator();
                    if ui.button(t("Refresh")).clicked() {
                        self.start_preview();
                    }
                    if let Some((_, p)) = &self.pv_job {
                        ui.add(egui::Spinner::new());
                        ui.label(p.stage.lock().unwrap().clone());
                    } else {
                        ui.label(&self.pv_status);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(t("Drag the yellow handles to resize the selection · left drag: rotate · right drag: move · wheel: zoom")).weak());
                    });
                }
            });
        });
        if self.view_3d {
            // rebuilt automatically when the selection or the options change
            if self.pv_job.is_none() && self.sel.is_some() && self.reg.is_some() && self.preview_key() != self.pv_key {
                self.start_preview();
            }
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(rect, 0.0, Color32::from_rgb(135, 168, 208));
            if self.sel.is_none() {
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, t("Select an area: right-click + drag on the map."), egui::FontId::proportional(16.0), Color32::from_gray(40));
            }
            match self.sel {
                Some(s) => {
                    let mut min = [s[0].min(s[2]), self.floor.min(self.ceil), s[1].min(s[3])];
                    let mut max = [s[0].max(s[2]), self.floor.max(self.ceil), s[1].max(s[3])];
                    let before = (min, max);
                    let edit = crate::preview::BoxEdit { min: &mut min, max: &mut max, drag: &mut self.pv_drag };
                    crate::preview::show(ui, rect, &mut self.pv_cam, &self.pv_mesh, self.pv_gen, Some(edit));
                    if (min, max) != before {
                        self.sel = Some([min[0], min[2], max[0], max[2]]);
                        self.floor = min[1];
                        self.ceil = max[1];
                    }
                }
                None => crate::preview::show(ui, rect, &mut self.pv_cam, &self.pv_mesh, self.pv_gen, None),
            }
        } else {
            self.map_view(ui);
        }
    }

    fn map_view(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 30, 34));

        // interactions
        if let Some(hp) = resp.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = (1.0015f32).powf(scroll) * pinch;
            if (factor - 1.0).abs() > 1e-4 {
                let before = self.screen_to_world(rect, hp);
                self.zoom = (self.zoom * factor).clamp(0.05, 64.0);
                let after = self.screen_to_world(rect, hp);
                self.center[0] += before[0] - after[0];
                self.center[1] += before[1] - after[1];
            }
            let w = self.screen_to_world(rect, hp);
            self.hover = Some((w[0].floor() as i32, w[1].floor() as i32));
        } else {
            self.hover = None;
        }
        let shift = ui.input(|i| i.modifiers.shift);
        let selecting = resp.dragged_by(egui::PointerButton::Secondary) || (shift && resp.dragged_by(egui::PointerButton::Primary));
        if selecting {
            if let Some(p) = resp.interact_pointer_pos() {
                let w = self.screen_to_world(rect, p);
                let b = [w[0].floor() as i32, w[1].floor() as i32];
                if resp.drag_started() || self.drag_start.is_none() {
                    self.drag_start = Some(b);
                }
                let s = self.drag_start.unwrap();
                self.sel = Some([s[0].min(b[0]), s[1].min(b[1]), s[0].max(b[0]), s[1].max(b[1])]);
            }
        } else {
            if self.drag_start.is_some() {
                self.save_config();
            }
            self.drag_start = None;
            if resp.dragged_by(egui::PointerButton::Primary) || resp.dragged_by(egui::PointerButton::Middle) {
                let d = resp.drag_delta();
                self.center[0] -= (d.x / self.zoom) as f64;
                self.center[1] -= (d.y / self.zoom) as f64;
            }
        }

        // tiles
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        for ((rx, rz), t) in &self.tiles {
            let a = self.world_to_screen(rect, *rx as f64 * 512.0, *rz as f64 * 512.0);
            let b = self.world_to_screen(rect, (*rx + 1) as f64 * 512.0, (*rz + 1) as f64 * 512.0);
            let r = Rect::from_min_max(a, b);
            if r.intersects(rect) {
                painter.image(t.tex.id(), r, uv, Color32::WHITE);
            }
        }

        // chunk grid
        if self.show_chunks && self.zoom >= 1.5 {
            let tl = self.screen_to_world(rect, rect.min);
            let br = self.screen_to_world(rect, rect.max);
            let stroke = Stroke::new(1.0, Color32::from_black_alpha(70));
            let mut x = (tl[0] / 16.0).floor() * 16.0;
            while x <= br[0] {
                let px = self.world_to_screen(rect, x, 0.0).x;
                painter.line_segment([pos2(px, rect.top()), pos2(px, rect.bottom())], stroke);
                x += 16.0;
            }
            let mut z = (tl[1] / 16.0).floor() * 16.0;
            while z <= br[1] {
                let pz = self.world_to_screen(rect, 0.0, z).y;
                painter.line_segment([pos2(rect.left(), pz), pos2(rect.right(), pz)], stroke);
                z += 16.0;
            }
        }

        // selection
        if let Some(s) = self.sel {
            let a = self.world_to_screen(rect, s[0] as f64, s[1] as f64);
            let b = self.world_to_screen(rect, s[2] as f64 + 1.0, s[3] as f64 + 1.0);
            let r = Rect::from_min_max(a, b);
            painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(220, 60, 40, 60));
            painter.rect_stroke(r, 0.0, Stroke::new(1.5, Color32::from_rgb(255, 90, 60)), egui::StrokeKind::Middle);
        }

        // overlay: zoom and help
        let help = tf(
            "{0}×   left click: pan · right click (or Shift+click): select · wheel: zoom",
            &[&format!("{:.2}", self.zoom)],
        );
        painter.text(rect.left_top() + vec2(8.0, 6.0), egui::Align2::LEFT_TOP, help, egui::FontId::proportional(12.0), Color32::from_white_alpha(160));
        if self.reg.is_none() && self.load_job.is_none() {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, t("Choose a world then “Load”, or drop a schematic here"), egui::FontId::proportional(18.0), Color32::from_white_alpha(140));
        }
        if self.map_pending > 0 {
            painter.text(
                rect.right_top() + vec2(-8.0, 6.0),
                egui::Align2::RIGHT_TOP,
                tf("Rendering map… {0} regions left", &[&self.map_pending]),
                egui::FontId::proportional(12.0),
                Color32::from_white_alpha(180),
            );
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some((x, z)) = self.hover {
                let (rx, rz) = (x.div_euclid(512), z.div_euclid(512));
                let mut txt = format!("X {x}  Z {z}  (chunk {} {})", x.div_euclid(16), z.div_euclid(16));
                if let (Some(t), Some(reg)) = (self.tiles.get(&(rx, rz)), &self.reg) {
                    let i = (z.rem_euclid(512) * 512 + x.rem_euclid(512)) as usize;
                    if t.top[i] != 0 {
                        let k = reg.state(t.top[i]);
                        txt += &format!("   Y {}  {}", t.height[i], k.name.trim_start_matches("minecraft:"));
                    }
                }
                ui.monospace(txt);
                ui.separator();
            }
            ui.label(&self.status);
        });
    }

    fn windows(&mut self, ctx: &egui::Context) {
        if self.show_goto {
            let mut open = true;
            let mut go = false;
            egui::Window::new(t("Go to")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(&mut self.goto[0]));
                    ui.label("Z");
                    ui.add(egui::DragValue::new(&mut self.goto[1]));
                });
                ui.horizontal(|ui| {
                    if ui.button(t("Go")).clicked() {
                        go = true;
                    }
                    if let Some(w) = &self.world {
                        if ui.button(t("Spawn point")).clicked() {
                            self.goto = [w.spawn.0, w.spawn.1];
                            go = true;
                        }
                    }
                    if let Some(s) = self.sel {
                        if ui.button(t("Selection")).clicked() {
                            self.goto = [(s[0] + s[2]) / 2, (s[1] + s[3]) / 2];
                            go = true;
                        }
                    }
                });
            });
            if go {
                self.center = [self.goto[0] as f64 + 0.5, self.goto[1] as f64 + 0.5];
            }
            self.show_goto = open;
        }

        if self.show_settings {
            let mut open = true;
            let mut reload = false;
            egui::Window::new(t("Settings")).open(&mut open).default_width(520.0).show(ctx, |ui| {
                ui.label(egui::RichText::new(t("Game resources (textures and models)")).strong());
                let cur = match &self.jar_override {
                    Some(p) => p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                    None => t("Automatic (world version)"),
                };
                egui::ComboBox::from_id_salt("jar").selected_text(cur).width(400.0).show_ui(ui, |ui| {
                    if ui.selectable_label(self.jar_override.is_none(), t("Automatic (world version)")).clicked() {
                        self.jar_override = None;
                    }
                    for j in &self.jars {
                        let label = format!("{}  (Minecraft {})", j.id, j.game_version);
                        if ui.selectable_label(self.jar_override.as_ref() == Some(&j.path), label).clicked() {
                            self.jar_override = Some(j.path.clone());
                        }
                    }
                });
                if ui.button(t("Choose another jar…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter(t("Minecraft jar"), &["jar"]).pick_file() {
                        self.jar_override = Some(p);
                    }
                }
                if let Some(r) = &self.reg {
                    ui.label(egui::RichText::new(tf("Current: {0}", &[&r.assets.jar_path.display()])).weak());
                }
                ui.separator();
                ui.label(egui::RichText::new(t("Resource packs (the first one has priority)")).strong());
                let mut remove = None;
                let mut up = None;
                for (i, p) in self.packs.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("✖").clicked() {
                            remove = Some(i);
                        }
                        if i > 0 && ui.small_button("⬆").clicked() {
                            up = Some(i);
                        }
                        ui.label(p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                    });
                }
                if let Some(i) = remove {
                    self.packs.remove(i);
                }
                if let Some(i) = up {
                    self.packs.swap(i, i - 1);
                }
                if self.packs.is_empty() {
                    ui.label(egui::RichText::new(t("No pack: vanilla textures.")).weak());
                }
                if ui.button(t("Use the packs enabled in Minecraft")).clicked() {
                    self.packs = assets::enabled_packs(&self.mc_dir);
                }
                let installed: Vec<PathBuf> =
                    assets::installed_packs(&self.mc_dir).into_iter().filter(|p| !self.packs.contains(p)).collect();
                if !installed.is_empty() {
                    ui.collapsing(t("Installed packs"), |ui| {
                        for p in installed {
                            ui.horizontal(|ui| {
                                if ui.small_button("➕").clicked() {
                                    self.packs.push(p.clone());
                                }
                                ui.label(p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                            });
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if ui.button(t("Add a pack (.zip)…")).clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .set_directory(self.mc_dir.join("resourcepacks"))
                            .add_filter(t("Resource pack"), &["zip"])
                            .pick_file()
                        {
                            self.packs.push(p);
                        }
                    }
                    if ui.button(t("Add a folder…")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().set_directory(self.mc_dir.join("resourcepacks")).pick_folder() {
                            self.packs.push(p);
                        }
                    }
                });
                ui.separator();
                ui.label(egui::RichText::new(t("Mods")).strong());
                ui.checkbox(&mut self.use_mods, t("Read blocks from the mods of the world's game folder"))
                    .on_hover_text(t("Textures and block models are read from the .jar files of the mods folder next to the saves folder (Fabric, Forge, NeoForge, Quilt)."));
                if self.use_mods {
                    if self.mods.is_empty() {
                        ui.label(egui::RichText::new(t("No mod found for this world.")).weak());
                    } else {
                        ui.collapsing(tf("{0} mods loaded", &[&self.mods.len()]), |ui| {
                            for m in &self.mods {
                                ui.label(m.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                            }
                        });
                    }
                }
                ui.separator();
                if ui.button(t("Apply and reload")).clicked() {
                    reload = true;
                }
            });
            self.show_settings = open;
            if reload {
                self.loaded_assets_key = None;
                self.reg = None;
                self.save_config();
                self.start_load();
            }
        }

        if self.show_blocks {
            let mut open = true;
            let names = self.reg.as_ref().map(|r| r.block_names()).unwrap_or_default();
            egui::Window::new(t("Blocks to export")).open(&mut open).default_size([360.0, 520.0]).show(ctx, |ui| {
                ui.label(egui::RichText::new(t("Blocks found in the loaded areas. Uncheck to exclude.")).weak());
                ui.horizontal(|ui| {
                    ui.label(t("Filter"));
                    ui.text_edit_singleline(&mut self.block_filter);
                });
                let filt = self.block_filter.to_lowercase();
                let shown: Vec<&String> = names.iter().filter(|n| filt.is_empty() || n.contains(&filt)).collect();
                ui.horizontal(|ui| {
                    if ui.button(t("Check all")).clicked() {
                        self.opts.excluded.retain(|e| !shown.iter().any(|n| *n == e));
                    }
                    if ui.button(t("Uncheck all")).clicked() {
                        for n in &shown {
                            if !self.opts.excluded.contains(n) {
                                self.opts.excluded.push((*n).clone());
                            }
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for n in shown {
                        let mut on = !self.opts.excluded.contains(n);
                        if ui.checkbox(&mut on, n.trim_start_matches("minecraft:")).changed() {
                            if on {
                                self.opts.excluded.retain(|e| e != n);
                            } else {
                                self.opts.excluded.push(n.clone());
                            }
                        }
                    }
                });
            });
            self.show_blocks = open;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_jobs(&ctx);
        // drag and drop of a schematic (or a world folder) onto the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(p) = dropped.into_iter().next() {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if schem::EXTENSIONS.contains(&ext.as_str()) {
                self.start_import(p);
            } else if p.join("level.dat").is_file() {
                self.world_path = p.to_string_lossy().to_string();
                self.start_load();
            } else {
                self.status = t("Unrecognized file: drop a .schem, .schematic, .litematic, .nbt or a world folder.");
            }
        }
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::right("export").default_size(300.0).resizable(true).show(ui, |ui| self.export_panel(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| self.central(ui));
        self.windows(&ctx);
    }
}

#[allow(dead_code)]
fn _unused(_: Vec2) {}

/// Entity categories kept in the export
fn entity_filter(ui: &mut egui::Ui, o: &mut ExportOptions) {
    use crate::entity::EntityKind;
    if !o.render_entities {
        return;
    }
    ui.indent("entity-kinds", |ui| {
        for k in crate::entity::ENTITY_KINDS {
            let label = match k {
                EntityKind::Mob => t("Mobs"),
                EntityKind::ArmorStand => t("Armor stands"),
                EntityKind::Frame => t("Item frames and paintings"),
                EntityKind::Display => t("Display entities"),
                EntityKind::Vehicle => t("Boats and minecarts"),
                EntityKind::Item => t("Dropped items and XP orbs"),
                EntityKind::Projectile => t("Projectiles"),
                EntityKind::Other => t("Other (leash knots, crystals, falling blocks…)"),
            };
            let mut on = !o.excluded_entities.contains(&k);
            if ui.checkbox(&mut on, label).changed() {
                if on {
                    o.excluded_entities.retain(|x| *x != k);
                } else {
                    o.excluded_entities.push(k);
                }
            }
        }
    });
}
