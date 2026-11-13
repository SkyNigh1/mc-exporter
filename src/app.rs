//! Interface graphique (egui / eframe).

use crate::assets::{self, Assets, JarInfo};
use crate::export;
use crate::map::{RegionMap, render_region};
use crate::mesh::{ExportOptions, OffsetMode, Progress, Selection, build_mesh};
use crate::registry::Registry;
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
}

pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("MC Exporter"),
        ..Default::default()
    };
    eframe::run_native("MC Exporter", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

impl App {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let cfg: Config = std::fs::read(config_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
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
        };
        if app.jars.is_empty() {
            app.status = "Aucune version de Minecraft trouvée dans .minecraft/versions : lance le jeu une fois ou choisis un jar dans les paramètres.".into();
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
        let reuse = self.reg.clone();
        let prev_key = self.loaded_assets_key.clone();
        let (tx, rx) = unbounded();
        self.status = match &target {
            LoadTarget::World(_) => "Chargement du monde…".into(),
            LoadTarget::Schem(p) => format!("Import de {}…", p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()),
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
                        // pour un schematic, on garde les ressources déjà chargées
                        (None, Some(r)) => r.assets.jar_path.clone(),
                        _ => {
                            assets::pick_jar(&jars, version.as_deref())
                                .ok_or("Aucun jar Minecraft trouvé (Paramètres → choisir un jar)")?
                                .path
                        }
                    },
                };
                let key = (jar.clone(), packs.clone());
                // on garde le registre si le jar et les packs n'ont pas changé
                let reg = match (reuse, prev_key) {
                    (Some(r), Some(k)) if k == key => r,
                    _ => {
                        let a = Assets::load(&jar, &packs).map_err(|e| format!("{e:#}"))?;
                        Arc::new(Registry::new(Arc::new(a)))
                    }
                };
                match target {
                    LoadTarget::World(_) => Ok(Loaded { info: world.unwrap(), reg, schem: None }),
                    LoadTarget::Schem(p) => {
                        let s = schem::load(&p, &reg).map_err(|e| format!("{e:#}"))?;
                        let info = WorldInfo {
                            path: p.clone(),
                            name: s.name.clone(),
                            version_name: None,
                            spawn: (s.size[0] / 2, s.size[2] / 2),
                            dims: vec![world::DimInfo { id: "schematic".into(), source: Source::Mem(Arc::new(s.world)) }],
                        };
                        Ok(Loaded { info, reg, schem: Some(SchemInfo { format: s.format, size: s.size, blocks: s.blocks }) })
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
                    Ok(Loaded { info, reg, schem: Some(si) }) => {
                        let jar = reg.assets.jar_path.clone();
                        self.loaded_assets_key = Some((jar.clone(), self.packs.clone()));
                        self.status = format!(
                            "Schematic « {} » — {} — {} × {} × {} — {} blocs — ressources : {}",
                            info.name,
                            si.format,
                            si.size[0],
                            si.size[1],
                            si.size[2],
                            si.blocks,
                            jar.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
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
                    Ok(Loaded { info, reg, schem: None }) => {
                        let jar = reg.assets.jar_path.clone();
                        self.loaded_assets_key = Some((jar.clone(), self.packs.clone()));
                        self.status = format!(
                            "« {} » — Minecraft {} — ressources : {}",
                            info.name,
                            info.version_name.clone().unwrap_or("?".into()),
                            jar.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
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
                    Err(e) => self.status = format!("Erreur : {e}"),
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
            self.last_result = Some(Err("Charge un monde et sélectionne une zone sur la carte (clic droit + glisser).".into()));
            return;
        };
        let mut dlg = rfd::FileDialog::new().add_filter("Wavefront OBJ", &["obj"]);
        match &self.last_export {
            Some(p) => {
                if let Some(d) = p.parent() {
                    dlg = dlg.set_directory(d);
                }
                dlg = dlg.set_file_name(p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or("export.obj".into()));
            }
            None => dlg = dlg.set_file_name(format!("{}.obj", w.name.replace(|c: char| !c.is_alphanumeric(), "_"))),
        }
        let Some(mut out) = dlg.save_file() else { return };
        if out.extension().is_none() {
            out.set_extension("obj");
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
                let mesh = build_mesh(&reg, &dir, sel, &opts, &p2)?;
                let s = export::write_obj(&reg, &mesh, &opts, &out, &world_name, &p2)?;
                Ok(format!(
                    "Exporté : {} — {} faces, {} sommets, {} matériaux, {:.1} Mo",
                    out.display(),
                    s.quads,
                    s.vertices,
                    s.materials,
                    s.obj_bytes as f64 / 1e6
                ))
            })();
            let _ = tx.send(r.map_err(|e| format!("{e:#}")));
        });
        self.last_result = None;
        self.export = Some(ExportJob { prog, rx, started: Instant::now() });
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Monde");
            let current = self
                .saves
                .iter()
                .find(|s| s.1.to_string_lossy() == self.world_path)
                .map(|s| s.0.clone())
                .unwrap_or_else(|| "(autre dossier)".into());
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
            if ui.button("…").on_hover_text("Choisir un dossier de monde").clicked() {
                if let Some(p) = rfd::FileDialog::new().set_directory(self.mc_dir.join("saves")).pick_folder() {
                    self.world_path = p.to_string_lossy().to_string();
                    self.start_load();
                }
            }
            if ui
                .add_enabled(self.load_job.is_none(), egui::Button::new("Importer un schematic…"))
                .on_hover_text(".schem (Sponge v1/v2/v3), .schematic (ancien MCEdit), .litematic, .nbt — tu peux aussi glisser le fichier sur la fenêtre")
                .clicked()
            {
                self.pick_schematic();
            }
            if ui.add_enabled(self.load_job.is_none(), egui::Button::new("Charger")).clicked() {
                self.saves = world::list_saves(&self.mc_dir);
                self.start_load();
            }
            if self.load_job.is_some() {
                ui.spinner();
            }
            ui.separator();
            ui.label("Dimension");
            let dims: Vec<(String, bool)> = self
                .world
                .as_ref()
                .map(|w| w.dims.iter().map(|d| (d.id.clone(), !d.source.regions().is_empty())).collect())
                .unwrap_or_default();
            let sel_text = dims.get(self.dim_idx).map(|d| d.0.clone()).unwrap_or("—".into());
            let mut new_dim = None;
            egui::ComboBox::from_id_salt("dims").selected_text(sel_text).width(200.0).show_ui(ui, |ui| {
                for (i, (d, has)) in dims.iter().enumerate() {
                    let label = if *has { d.clone() } else { format!("{d} (vide)") };
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
                if ui.button("Paramètres").clicked() {
                    self.show_settings = !self.show_settings;
                }
                if ui.button("Aller à…").clicked() {
                    self.goto = [self.center[0] as i32, self.center[1] as i32];
                    self.show_goto = !self.show_goto;
                }
            });
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("Plancher");
            let f = ui.add(egui::DragValue::new(&mut self.floor).range(-2048..=4096));
            ui.label("Plafond");
            let c = ui.add(egui::DragValue::new(&mut self.ceil).range(-2048..=4096));
            if (f.lost_focus() || f.drag_stopped() || c.lost_focus() || c.drag_stopped()) && self.reg.is_some() {
                self.start_map();
            }
            if ui.button("Actualiser la carte").clicked() {
                self.start_map();
            }
            ui.checkbox(&mut self.show_chunks, "Afficher les chunks");
            ui.separator();
            let mut s = self.sel.unwrap_or([0, 0, 0, 0]);
            let before = s;
            ui.label("Position 1  X");
            ui.add(egui::DragValue::new(&mut s[0]));
            ui.label("Z");
            ui.add(egui::DragValue::new(&mut s[1]));
            ui.label("  Position 2  X");
            ui.add(egui::DragValue::new(&mut s[2]));
            ui.label("Z");
            ui.add(egui::DragValue::new(&mut s[3]));
            if s != before {
                self.sel = Some(s);
            }
            if let Some(s) = self.sel {
                let (w, d) = ((s[2] - s[0]).abs() + 1, (s[3] - s[1]).abs() + 1);
                ui.label(format!("  {w} × {d} × {} blocs", (self.ceil - self.floor).abs() + 1));
                if ui.small_button("✖").on_hover_text("Effacer la sélection").clicked() {
                    self.sel = None;
                }
            }
        });
        ui.add_space(4.0);
    }

    fn export_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Export OBJ");
            ui.add_space(4.0);
            let o = &mut self.opts;
            egui::Grid::new("scale").num_columns(2).show(ui, |ui| {
                ui.label("Échelle (taille d'un bloc)");
                ui.add(egui::DragValue::new(&mut o.scale).speed(0.01).range(0.001..=1000.0));
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Décalage").strong());
            ui.horizontal(|ui| {
                ui.radio_value(&mut o.offset_mode, OffsetMode::None, "Aucun");
                ui.radio_value(&mut o.offset_mode, OffsetMode::Center, "Centrer");
                ui.radio_value(&mut o.offset_mode, OffsetMode::Custom, "Personnalisé");
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
            ui.label(egui::RichText::new("Contenu").strong());
            let n_ex = o.excluded.len();
            if ui
                .button(if n_ex == 0 { "Choisir les blocs à exporter…".to_string() } else { format!("Choisir les blocs à exporter… ({n_ex} exclus)") })
                .clicked()
            {
                self.show_blocks = true;
            }
            let o = &mut self.opts;
            ui.checkbox(&mut o.render_unknown, "Afficher les blocs inconnus (mods)");
            ui.checkbox(&mut o.render_sides, "Côtés et dessous du monde");
            ui.checkbox(&mut o.render_biomes, "Couleurs des biomes");
            ui.checkbox(&mut o.ore_to_stone, "Convertir les minerais en pierre");
            ui.checkbox(&mut o.render_block_entities, "Coffres, bannières, têtes, panneaux…");
            ui.add_enabled_ui(o.render_block_entities || o.render_entities, |ui| {
                ui.checkbox(&mut o.sign_text, "Texte des panneaux");
            });
            ui.checkbox(&mut o.render_entities, "Entités (mobs, displays, cadres, tableaux…)");
            ui.checkbox(&mut o.download_skins, "Télécharger les skins des têtes de joueur")
                .on_hover_text("Les skins déjà vues en jeu sont lues dans le cache de Minecraft ; les autres sont demandées aux serveurs de Mojang.");
            ui.checkbox(&mut o.ghast_on_ground, "Poser les ghasts sur leurs tentacules")
                .on_hover_text("Dans le jeu, les tentacules des ghasts pendent sous leur position. Cochez pour les remonter au niveau du sol.");
            ui.separator();
            ui.label(egui::RichText::new("Structure").strong());
            ui.checkbox(&mut o.obj_per_material, "Un objet par matériau");
            ui.checkbox(&mut o.obj_per_chunk, "Un objet par chunk");
            ui.checkbox(&mut o.optimize, "Optimiser le maillage (fusion des faces)");
            ui.checkbox(&mut o.double_sided, "Faces recto-verso (plantes, vitres fines)");
            ui.checkbox(&mut o.randomize, "Variantes aléatoires des modèles");
            ui.checkbox(&mut o.use_groups, "Groupes OBJ « g » (compatible Maya)");
            ui.separator();
            ui.label(egui::RichText::new("Textures").strong());
            ui.checkbox(&mut o.export_textures, "Exporter les textures");
            ui.add_enabled_ui(o.export_textures, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Agrandir");
                    egui::ComboBox::from_id_salt("prescale").selected_text(format!("{}×", o.prescale)).show_ui(ui, |ui| {
                        for f in [1u32, 2, 4, 8, 16] {
                            ui.selectable_value(&mut o.prescale, f, format!("{f}×"));
                        }
                    });
                });
                ui.checkbox(&mut o.alpha_separate, "Canal alpha dans des fichiers séparés");
            });
            ui.separator();
            ui.add_space(6.0);
            let busy = self.export.is_some();
            let can = self.sel.is_some() && self.reg.is_some() && !busy;
            let btn = egui::Button::new(egui::RichText::new("Exporter…").size(18.0)).min_size(vec2(ui.available_width(), 36.0));
            if ui.add_enabled(can, btn).clicked() {
                self.save_config();
                self.start_export();
            }
            if self.sel.is_none() {
                ui.label(egui::RichText::new("Sélectionne une zone : clic droit + glisser sur la carte.").weak());
            }
            if let Some(job) = &self.export {
                let stage = job.prog.stage.lock().unwrap().clone();
                ui.add(egui::ProgressBar::new(job.prog.fraction()).text(stage).animate(true));
                if ui.button("Annuler").clicked() {
                    job.prog.cancel.store(true, Ordering::Relaxed);
                }
            }
            match &self.last_result {
                Some(Ok(s)) => {
                    ui.label(egui::RichText::new(s).color(Color32::from_rgb(120, 200, 120)));
                    if let Some(p) = &self.last_export {
                        if ui.button("Ouvrir le dossier").clicked() {
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
        });
    }

    fn world_to_screen(&self, rect: Rect, x: f64, z: f64) -> Pos2 {
        let c = rect.center();
        pos2(c.x + ((x - self.center[0]) * self.zoom as f64) as f32, c.y + ((z - self.center[1]) * self.zoom as f64) as f32)
    }

    fn screen_to_world(&self, rect: Rect, p: Pos2) -> [f64; 2] {
        let c = rect.center();
        [self.center[0] + ((p.x - c.x) / self.zoom) as f64, self.center[1] + ((p.y - c.y) / self.zoom) as f64]
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

        // tuiles
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        for ((rx, rz), t) in &self.tiles {
            let a = self.world_to_screen(rect, *rx as f64 * 512.0, *rz as f64 * 512.0);
            let b = self.world_to_screen(rect, (*rx + 1) as f64 * 512.0, (*rz + 1) as f64 * 512.0);
            let r = Rect::from_min_max(a, b);
            if r.intersects(rect) {
                painter.image(t.tex.id(), r, uv, Color32::WHITE);
            }
        }

        // grille des chunks
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

        // sélection
        if let Some(s) = self.sel {
            let a = self.world_to_screen(rect, s[0] as f64, s[1] as f64);
            let b = self.world_to_screen(rect, s[2] as f64 + 1.0, s[3] as f64 + 1.0);
            let r = Rect::from_min_max(a, b);
            painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(220, 60, 40, 60));
            painter.rect_stroke(r, 0.0, Stroke::new(1.5, Color32::from_rgb(255, 90, 60)), egui::StrokeKind::Middle);
        }

        // incrustation : zoom et aide
        let help = format!(
            "{:.2}×   clic gauche : déplacer · clic droit (ou Maj+clic) : sélectionner · molette : zoom",
            self.zoom
        );
        painter.text(rect.left_top() + vec2(8.0, 6.0), egui::Align2::LEFT_TOP, help, egui::FontId::proportional(12.0), Color32::from_white_alpha(160));
        if self.reg.is_none() && self.load_job.is_none() {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Choisis un monde puis « Charger », ou glisse un schematic ici", egui::FontId::proportional(18.0), Color32::from_white_alpha(140));
        }
        if self.map_pending > 0 {
            painter.text(
                rect.right_top() + vec2(-8.0, 6.0),
                egui::Align2::RIGHT_TOP,
                format!("Rendu de la carte… {} régions restantes", self.map_pending),
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
            egui::Window::new("Aller à").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("X");
                    ui.add(egui::DragValue::new(&mut self.goto[0]));
                    ui.label("Z");
                    ui.add(egui::DragValue::new(&mut self.goto[1]));
                });
                ui.horizontal(|ui| {
                    if ui.button("Aller").clicked() {
                        go = true;
                    }
                    if let Some(w) = &self.world {
                        if ui.button("Point d'apparition").clicked() {
                            self.goto = [w.spawn.0, w.spawn.1];
                            go = true;
                        }
                    }
                    if let Some(s) = self.sel {
                        if ui.button("Sélection").clicked() {
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
            egui::Window::new("Paramètres").open(&mut open).default_width(520.0).show(ctx, |ui| {
                ui.label(egui::RichText::new("Ressources du jeu (textures et modèles)").strong());
                let cur = match &self.jar_override {
                    Some(p) => p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                    None => "Automatique (version du monde)".into(),
                };
                egui::ComboBox::from_id_salt("jar").selected_text(cur).width(400.0).show_ui(ui, |ui| {
                    if ui.selectable_label(self.jar_override.is_none(), "Automatique (version du monde)").clicked() {
                        self.jar_override = None;
                    }
                    for j in &self.jars {
                        let label = format!("{}  (Minecraft {})", j.id, j.game_version);
                        if ui.selectable_label(self.jar_override.as_ref() == Some(&j.path), label).clicked() {
                            self.jar_override = Some(j.path.clone());
                        }
                    }
                });
                if ui.button("Choisir un autre jar…").clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("Jar Minecraft", &["jar"]).pick_file() {
                        self.jar_override = Some(p);
                    }
                }
                if let Some(r) = &self.reg {
                    ui.label(egui::RichText::new(format!("En cours : {}", r.assets.jar_path.display())).weak());
                }
                ui.separator();
                ui.label(egui::RichText::new("Packs de ressources (le premier est prioritaire)").strong());
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
                ui.horizontal(|ui| {
                    if ui.button("Ajouter un pack (.zip)…").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .set_directory(self.mc_dir.join("resourcepacks"))
                            .add_filter("Pack de ressources", &["zip"])
                            .pick_file()
                        {
                            self.packs.push(p);
                        }
                    }
                    if ui.button("Ajouter un dossier…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().set_directory(self.mc_dir.join("resourcepacks")).pick_folder() {
                            self.packs.push(p);
                        }
                    }
                });
                ui.separator();
                if ui.button("Appliquer et recharger").clicked() {
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
            egui::Window::new("Blocs à exporter").open(&mut open).default_size([360.0, 520.0]).show(ctx, |ui| {
                ui.label(egui::RichText::new("Blocs rencontrés dans les zones chargées. Décoche pour exclure.").weak());
                ui.horizontal(|ui| {
                    ui.label("Filtre");
                    ui.text_edit_singleline(&mut self.block_filter);
                });
                let filt = self.block_filter.to_lowercase();
                let shown: Vec<&String> = names.iter().filter(|n| filt.is_empty() || n.contains(&filt)).collect();
                ui.horizontal(|ui| {
                    if ui.button("Tout cocher").clicked() {
                        self.opts.excluded.retain(|e| !shown.iter().any(|n| *n == e));
                    }
                    if ui.button("Tout décocher").clicked() {
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
        // glisser-déposer d'un schematic (ou d'un dossier de monde) sur la fenêtre
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(p) = dropped.into_iter().next() {
            let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if schem::EXTENSIONS.contains(&ext.as_str()) {
                self.start_import(p);
            } else if p.join("level.dat").is_file() {
                self.world_path = p.to_string_lossy().to_string();
                self.start_load();
            } else {
                self.status = "Fichier non reconnu : glisse un .schem, .schematic, .litematic, .nbt ou un dossier de monde.".into();
            }
        }
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::right("export").default_size(300.0).resizable(true).show(ui, |ui| self.export_panel(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| self.map_view(ui));
        self.windows(&ctx);
    }
}

#[allow(dead_code)]
fn _unused(_: Vec2) {}
