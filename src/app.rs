use eframe::egui::{self, Color32, Pos2, Rect, RichText, Stroke, Vec2 as EVec2};
use rebonebase::{
    animation::{Channel, Clip, Ease, Pose},
    ik,
    model::{Bulge, Rig, Vec2},
    motion::{Motion, Projection},
    project::{self, History, Project},
    skin,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
mod animation_import_ui;
mod canvas_ui;
mod characters_ui;
mod import_ui;
mod picking;
mod production_assets;
mod production_motion;
mod production_scene;
mod production_ui;
mod studio;
mod studio_animation;
mod studio_assets;
mod studio_rig;
mod workspaces;
use studio::Studio;
#[cfg(test)]
mod tests_more;
#[cfg(test)]
mod tests_movement;
#[cfg(test)]
mod tests_production;
#[cfg(test)]
mod tests_studio;

const ACCENT: Color32 = Color32::from_rgb(119, 225, 187);
const DIM: Color32 = Color32::from_rgb(136, 151, 162);
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Rest,
    Pose,
}
#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Select,
    Bone,
    Radius,
    Bulge,
    IK,
    MoveRig,
    Pan,
    Pixel,
    Box,
}
#[derive(Clone)]
enum Pending {
    New,
    Template(usize),
    Open(PathBuf),
    Restore(Box<Project>),
    Close,
}
struct Gesture {
    base: Project,
    start: Vec2,
    id: Option<u32>,
    tail: bool,
    tool: Tool,
    recorded: bool,
}
struct MotionSetup {
    motion: Motion,
    projection: Projection,
    scale: f32,
    mapping: Vec<Option<usize>>,
    preview_frame: usize,
    source: Option<PathBuf>,
    animation_names: Vec<String>,
    animation_index: usize,
    selected_animations: Vec<bool>,
    target_ids: Vec<u32>,
    profile_name: String,
    profile_index: Option<usize>,
    preview_base: Option<Rig>,
    preview_options: Option<(Projection, f32, Vec<Option<usize>>, usize)>,
    preview_target: Option<Rig>,
    preview_texture: Option<egui::TextureHandle>,
    preview_texture_frame: Option<usize>,
    preview_extras: Option<rebonebase::features::Extras>,
    preview_error: Option<String>,
    pending: Option<import_ui::ImportJob>,
}

pub struct ReboneApp {
    project: Project,
    saved: Project,
    history: History,
    path: Option<PathBuf>,
    templates: Vec<Rig>,
    selected: Option<u32>,
    mode: Mode,
    tool: Tool,
    frame: f32,
    playing: bool,
    last_tick: Instant,
    zoom: f32,
    pan: EVec2,
    fit: bool,
    grid: bool,
    bones: bool,
    show_skin: bool,
    onion: bool,
    skin_tex: Option<egui::TextureHandle>,
    ref_tex: Option<egui::TextureHandle>,
    onion_tex: Option<egui::TextureHandle>,
    gesture: Option<Gesture>,
    pending: Option<Pending>,
    status: String,
    render_error: Option<String>,
    help: bool,
    easing: Ease,
    sheet_cols: u32,
    allow_close: bool,
    last_canvas: Rect,
    motion_setup: Option<MotionSetup>,
    import_notes: Option<Vec<String>>,
    studio: Studio,
    production: production_ui::ProductionUi,
    canvas_snapshot: Option<Project>,
}
impl ReboneApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::with_context(&cc.egui_ctx);
        app.init_services();
        if let Some(path) = std::env::args_os().nth(1) {
            app.open(PathBuf::from(path));
        }
        app
    }
    fn with_context(ctx: &egui::Context) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::from_rgb(25, 31, 38);
        visuals.window_fill = Color32::from_rgb(30, 37, 44);
        visuals.extreme_bg_color = Color32::from_rgb(17, 22, 28);
        visuals.selection.bg_fill = Color32::from_rgb(41, 87, 76);
        visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
        visuals.widgets.active.bg_fill = Color32::from_rgb(43, 91, 77);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(52, 65, 73);
        ctx.set_visuals(visuals);
        ctx.style_mut(|s| {
            s.spacing.item_spacing = EVec2::new(9., 8.);
            s.spacing.button_padding = EVec2::new(12., 7.);
        });
        let templates = project::templates();
        let rig = templates[0].clone();
        let mut project = Project::new(rig);
        let _ = rebonebase::canvas::grow_all(&mut project);
        Self {
            saved: project.clone(),
            project,
            templates,
            history: History::default(),
            path: None,
            selected: None,
            mode: Mode::Rest,
            tool: Tool::Select,
            frame: 1.,
            playing: false,
            last_tick: Instant::now(),
            zoom: 8.,
            pan: EVec2::ZERO,
            fit: true,
            grid: false,
            bones: true,
            show_skin: true,
            onion: false,
            skin_tex: None,
            ref_tex: None,
            onion_tex: None,
            gesture: None,
            pending: None,
            status: "Ready · Select a bone to begin".into(),
            render_error: None,
            help: false,
            easing: Ease::Ease,
            sheet_cols: 8,
            allow_close: false,
            last_canvas: Rect::NOTHING,
            motion_setup: None,
            import_notes: None,
            studio: Default::default(),
            production: Default::default(),
            canvas_snapshot: None,
        }
    }
    fn dirty(&self) -> bool {
        self.project != self.saved || self.studio.recovered
    }
    fn changed(&mut self) {
        self.skin_tex = None;
        self.onion_tex = None;
    }
    fn record(&mut self) {
        self.history.record(&self.project);
        self.changed();
    }
    fn pose(&self) -> Option<Pose> {
        (self.mode == Mode::Pose).then(|| {
            let from = self.project.pose_at(self.frame);
            if self.production.transition_preview
                && self.production.transition_to < self.project.rig.clips.len()
            {
                let mut target = self.project.clone();
                target.rig.active_clip = self.production.transition_to;
                rebonebase::advanced_motion::transition_pose(
                    &self.project.rig,
                    &from,
                    &target.pose_at(1.),
                    self.production.transition_weight,
                )
            } else {
                from
            }
        })
    }
    fn request(&mut self, action: Pending) {
        if self.dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action);
        }
    }
    fn perform(&mut self, action: Pending) {
        match action {
            Pending::Close => self.allow_close = true,
            Pending::Open(p) => self.open(p),
            Pending::Restore(project) => {
                self.replace(*project, None);
                self.studio.recovered = true;
                self.status = "Recovered project opened · save it to keep your changes".into();
            }
            Pending::New => self.replace(Project::new(Rig::new("Untitled", 64, 64)), None),
            Pending::Template(i) => self.replace(Project::new(self.templates[i].clone()), None),
        }
    }
    fn replace(&mut self, p: Project, path: Option<PathBuf>) {
        self.canvas_snapshot = None;
        self.render_error = None;
        self.reset_production_document();
        let _ = self.studio.store.clear_recovery(&self.studio.session);
        self.reset_studio_document();
        self.saved = p.clone();
        self.project = p;
        self.production.character = self.project.extras.characters3d.first().map(|c| c.id);
        self.production.move_character =
            !self.project.extras.show_2d && self.production.character.is_some();
        if self.production.move_character {
            self.bones = false;
        }
        self.path = path;
        self.history = History::default();
        self.selected = None;
        self.playing = false;
        self.frame = 1.;
        self.mode = Mode::Rest;
        self.tool = Tool::Select;
        self.fit = true;
        self.gesture = None;
        self.motion_setup = None;
        self.ref_tex = None;
        self.changed();
    }
    fn open(&mut self, path: PathBuf) {
        if path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("bbrig"))
        {
            let result = (|| -> anyhow::Result<_> {
                anyhow::ensure!(
                    std::fs::metadata(&path)?.len() <= 16 * 1024 * 1024,
                    "Rig file exceeds 16 MiB"
                );
                rebonebase::legacy::import(&std::fs::read_to_string(&path)?)
                    .map_err(anyhow::Error::msg)
            })();
            match result {
                Ok((mut rig, notes)) => {
                    rig.name = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    self.replace(Project::new(rig), None);
                    self.status = "Imported Bonebase rig · save as a .rebone project".into();
                    if !notes.is_empty() {
                        self.import_notes = Some(notes);
                    }
                }
                Err(e) => self.status = format!("Rig import failed: {e}"),
            }
            return;
        }
        match project::load_with_report(&path) {
            Ok(report) => {
                let migrated = report.migrated_projects;
                self.replace(report.project, Some(path));
                self.status = if migrated > 0 {
                    "Project upgraded in memory to schema 2 · Save writes the new format; the original is unchanged until then".into()
                } else {
                    "Project opened".into()
                };
            }
            Err(e) => self.status = format!("Could not open project: {e}"),
        }
    }
    fn choose_open(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("ReBoneBase / Bonebase rig", &["rebone", "bbrig"])
            .pick_file()
        {
            self.request(Pending::Open(p));
        }
    }
    fn save(&mut self, save_as: bool) -> bool {
        let p = if !save_as { self.path.clone() } else { None }.or_else(|| {
            rfd::FileDialog::new()
                .add_filter("ReBoneBase project", &["rebone"])
                .set_file_name(format!("{}.rebone", self.project.rig.name))
                .save_file()
        });
        let Some(p) = p else { return false };
        match self.studio.store.save_with_backup_report(&p, &self.project) {
            Ok(report) => {
                self.path = Some(p);
                self.saved = self.project.clone();
                self.studio.recovered = false;
                let _ = self.studio.store.clear_recovery(&self.studio.session);
                self.status = if report.warnings.is_empty() {
                    "Project saved".into()
                } else {
                    format!("Project saved · {}", report.warnings.join("; "))
                };
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }
    fn export(&mut self, kind: &str) {
        self.sync_canvas();
        let ext = match kind {
            "gif" => "gif",
            "aseprite" | "aseall" => "aseprite",
            _ => "png",
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter(ext, &[ext])
            .set_file_name(format!("{}.{}", self.project.rig.name, ext))
            .save_file()
        else {
            return;
        };
        let result = (|| -> anyhow::Result<()> {
            let mut prepared = self.project.clone();
            rebonebase::canvas::grow_all(&mut prepared)?;
            let bytes = match kind {
                "aseprite" => rebonebase::aseprite::encode_project(&self.project, false)?,
                "aseall" => rebonebase::aseprite::encode_project(&self.project, true)?,
                "gif" => project::gif_project(&self.project)?,
                _ => {
                    let img = if kind == "sheet" {
                        project::sheet_project(&self.project, self.sheet_cols)?
                    } else {
                        prepared.try_render_pose(self.frame, self.pose().as_ref())?
                    };
                    if kind == "indexed" {
                        rebonebase::batch::indexed_png(&img, &self.project.extras.palette)?
                    } else {
                        let mut out = std::io::Cursor::new(Vec::new());
                        img.write_to(&mut out, image::ImageFormat::Png)?;
                        out.into_inner()
                    }
                }
            };
            project::atomic_write(&path, &bytes)?;
            Ok(())
        })();
        self.status = match result {
            Ok(()) => format!("Exported {}", path.display()),
            Err(e) => format!("Export failed: {e:#}"),
        };
    }
    fn import_reference(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Reference image", &["png", "jpg", "jpeg", "gif"])
            .pick_file()
        else {
            return;
        };
        match project::import_reference(&path) {
            Ok(mut r) => {
                self.record();
                r.scale = (self.project.rig.width as f32 / r.width as f32)
                    .min(self.project.rig.height as f32 / r.height as f32);
                self.project.reference = Some(r);
                self.ref_tex = None;
                self.status = "Reference image added · adjust it in the inspector".into();
            }
            Err(e) => self.status = format!("Image import failed: {e}"),
        }
    }
    fn import_motion(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Animation for 2D rig",
                &["bvh", "gltf", "glb", "fbx", "anim", "uasset"],
            )
            .pick_file()
        else {
            return;
        };
        match MotionSetup::from_path(path, &self.project.rig) {
            Ok(setup) => self.motion_setup = Some(setup),
            Err(e) => self.status = format!("Motion import failed: {e}"),
        }
    }
    fn undo(&mut self) {
        let previous_center = self.canvas_center();
        self.gesture = None;
        self.studio.key_drag = None;
        self.studio.keys.clear();
        self.studio.curve_drag = None;
        if self.history.undo(&mut self.project) {
            self.rebase_canvas_camera(previous_center);
            self.selected = self
                .selected
                .filter(|id| self.project.rig.bone(*id).is_some());
            self.frame = self.frame.min(self.project.rig.active_clip().length as f32);
            self.ref_tex = None;
            self.changed();
        }
    }
    fn redo(&mut self) {
        let previous_center = self.canvas_center();
        self.gesture = None;
        self.studio.key_drag = None;
        self.studio.keys.clear();
        self.studio.curve_drag = None;
        if self.history.redo(&mut self.project) {
            self.rebase_canvas_camera(previous_center);
            self.frame = self.frame.min(self.project.rig.active_clip().length as f32);
            self.ref_tex = None;
            self.changed();
        }
    }
    fn delete(&mut self) {
        if let Some(id) = self.selected {
            if self.project.rig.bones.iter().any(|b| {
                let mut current = Some(b.id);
                while let Some(node) = current {
                    if node == id {
                        return b.locked;
                    }
                    current = self.project.rig.bone(node).and_then(|b| b.parent);
                }
                false
            }) {
                self.status = "Unlock the bone and locked descendants before deleting".into();
                return;
            }
            self.record();
            self.project.rig.delete_bone(id);
            self.project
                .extras
                .production
                .prune_bones(&self.project.rig);
            for p in &mut self.project.poses {
                p.pose.retain(|id, _| {
                    self.project.rig.bone(*id).is_some()
                        || self.project.rig.parts.iter().any(|p| p.id == *id)
                });
            }
            self.project
                .extras
                .pins
                .retain(|p| self.project.rig.bone(p.lower).is_some());
            self.studio
                .selection
                .retain(|id| self.project.rig.bone(*id).is_some());
            self.selected = None;
            self.status = "Bone and descendants removed".into();
        }
    }
    fn keyboard(&mut self, ctx: &egui::Context) {
        let editing_text = ctx
            .memory(|m| m.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        if self.pending.is_some() || self.motion_setup.is_some() || editing_text {
            return;
        }
        let keys = ctx.input(|i| i.events.clone());
        for event in keys {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
                && let Some(action) = self
                    .studio
                    .preferences
                    .bindings
                    .iter()
                    .find(|b| b.matches(key, modifiers))
                    .map(|b| b.action)
            {
                self.run_action(action);
                break;
            }
        }
    }

    fn toggle_play(&mut self) {
        self.production.player.stop();
        self.production.audio_until = None;
        self.production.audio_error = false;
        self.playing = !self.playing;
        self.mode = Mode::Pose;
        self.last_tick = Instant::now();
        let clip = self.project.rig.active_clip();
        if clip.playback == rebonebase::animation::Playback::Reverse && self.frame <= 1. {
            self.frame = clip.length as f32;
        }
        self.studio.playback_ms = clip
            .frame_sequence()
            .into_iter()
            .take_while(|f| *f != self.frame as u32)
            .map(|f| clip.duration_ms(f) as f64)
            .sum();
        self.changed();
    }
    fn key_selected(&mut self) {
        if let Some(id) = self.selected {
            if self.project.rig.bone(id).is_some_and(|b| b.locked) {
                self.status = "Unlock this bone before keying it".into();
                return;
            }
            self.record();
            let p = self
                .project
                .rig
                .active_clip()
                .sample(self.frame)
                .get(&id)
                .copied()
                .unwrap_or_default();
            for (c, v) in [
                (Channel::Rotation, p.rotation),
                (Channel::X, p.x),
                (Channel::Y, p.y),
                (Channel::Scale, p.scale),
            ] {
                let _ = self
                    .project
                    .rig
                    .active_clip_mut()
                    .set_key(id, c, self.frame, v);
            }
            self.status = format!("Pose keyed at frame {}", self.frame as u32);
        }
    }
    fn top(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(7.);
            ui.horizontal(|ui| {
                ui.label(RichText::new("RE").size(22.).color(ACCENT).strong());
                ui.label(RichText::new("BONEBASE").size(22.).strong());
                ui.label(RichText::new("NATIVE STUDIO").size(10.).color(DIM));
                ui.separator();
                ui.menu_button("File", |ui| {
                    if ui.button("New project       Ctrl+N").clicked() {
                        self.request(Pending::New);
                        ui.close();
                    }
                    if ui.button("Open…                 Ctrl+O").clicked() {
                        self.choose_open();
                        ui.close();
                    }
                    if ui.button("Save                    Ctrl+S").clicked() {
                        self.save(false);
                        ui.close();
                    }
                    if ui.button("Save as…").clicked() {
                        self.save(true);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Import reference image…").clicked() {
                        self.import_reference();
                        ui.close();
                    }
                    if ui.button("Import animation for 2D rig…").clicked() {
                        self.import_motion();
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            self.production.character.is_some(),
                            egui::Button::new("Import animation for 3D character…"),
                        )
                        .clicked()
                    {
                        self.import_character_animation();
                        ui.close();
                    }
                    if ui.button("Import 3D character…").clicked() {
                        self.import_character3d();
                        ui.close();
                    }
                });
                if ui.button("Undo").clicked() {
                    self.undo();
                }
                if ui.button("Redo").clicked() {
                    self.redo();
                }
                if ui.button(RichText::new("Studio").color(ACCENT)).clicked() {
                    self.studio.open = !self.studio.open;
                }
                if ui
                    .button(RichText::new("Production").color(ACCENT))
                    .clicked()
                {
                    self.run_action(rebonebase::workspace::Action::Production);
                }
                if ui.button("Commands").clicked() {
                    self.run_action(rebonebase::workspace::Action::CommandPalette);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button(RichText::new("Export  ↗").color(ACCENT), |ui| {
                        if ui.button("Current frame · PNG").clicked() {
                            self.export("frame");
                            ui.close();
                        }
                        if ui.button("Current frame · indexed PNG").clicked() {
                            self.export("indexed");
                            ui.close();
                        }
                        if ui.button("Sprite sheet · PNG").clicked() {
                            self.export("sheet");
                            ui.close();
                        }
                        if ui.button("Animation · GIF").clicked() {
                            self.export("gif");
                            ui.close();
                        }
                        if ui.button("Layered animation · Aseprite").clicked() {
                            self.export("aseprite");
                            ui.close();
                        }
                        if ui.button("All clips · Aseprite tags").clicked() {
                            self.export("aseall");
                            ui.close();
                        }
                        if ui.button("Batch export presets…").clicked() {
                            self.studio.open = true;
                            self.studio.tab = studio::Tab::Export;
                            ui.close();
                        }
                        ui.horizontal(|ui| {
                            ui.label("Sheet columns");
                            ui.add(egui::DragValue::new(&mut self.sheet_cols).range(1..=32));
                        });
                    });
                    if ui.button("Save project").clicked() {
                        self.save(false);
                    }
                    if ui.button("Help").clicked() {
                        self.help = true;
                    }
                    ui.label(
                        RichText::new(format!(
                            "{}{}",
                            self.project.rig.name,
                            if self.dirty() { " •" } else { "" }
                        ))
                        .color(DIM),
                    );
                });
            });
            ui.add_space(7.);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status).small().color(DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{} × {} px   ·   {} bones   ·   {:.0}%",
                            self.project.rig.width,
                            self.project.rig.height,
                            self.project.rig.bones.len(),
                            self.zoom * 100.
                        ))
                        .small()
                        .color(DIM),
                    );
                });
            });
        });
    }
    fn left(&mut self, ctx: &egui::Context) {
        self.dock_panel(ctx, workspaces::Panel::Skeleton, Self::left_ui);
    }
    fn left_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.);
        ui.label(RichText::new("PROJECT").small().color(ACCENT).strong());
        ui.add_space(6.);
        let old_name = self.project.rig.name.clone();
        if ui
            .text_edit_singleline(&mut self.project.rig.name)
            .changed()
        {
            let mut old = self.project.clone();
            old.rig.name = old_name;
            self.history.record(&old);
        }
        ui.add_space(12.);
        egui::CollapsingHeader::new("Starter rigs · 20 templates")
            .default_open(false)
            .show(ui, |ui| {
                for i in 0..self.templates.len() {
                    if ui
                        .selectable_label(false, &self.templates[i].name)
                        .clicked()
                    {
                        self.request(Pending::Template(i));
                    }
                }
            });
        ui.add_space(8.);
        ui.separator();
        ui.add_space(8.);
        ui.horizontal(|ui| {
            ui.label(RichText::new("SKELETON").small().color(ACCENT).strong());
            ui.label(
                RichText::new(format!("{}", self.project.rig.bones.len()))
                    .small()
                    .color(DIM),
            );
        });
        ui.label(
            RichText::new("Select a bone to edit its properties")
                .small()
                .color(DIM),
        );
        ui.add_space(8.);
        egui::ScrollArea::vertical()
            .id_salt("hierarchy")
            .max_height((ui.available_height() - 180.).max(100.))
            .show(ui, |ui| {
                for b in self.project.rig.bones.clone() {
                    let mut depth = 0;
                    let mut p = b.parent;
                    while let Some(id) = p {
                        depth += 1;
                        p = self.project.rig.bone(id).and_then(|b| b.parent);
                        if depth > 24 {
                            break;
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(depth as f32 * 9.);
                        ui.label(RichText::new("◇").color(if self.selected == Some(b.id) {
                            ACCENT
                        } else {
                            DIM
                        }));
                        if ui
                            .selectable_label(
                                self.selected == Some(b.id)
                                    || self.studio.selection.contains(&b.id),
                                format!("{}{}", if b.locked { "[L] " } else { "" }, b.name),
                            )
                            .clicked()
                        {
                            self.select_bone(b.id, ui.input(|i| i.modifiers.shift));
                        }
                    });
                }
            });
        ui.separator();
        if ui.button("+ Draw bones   B").clicked() {
            self.mode = Mode::Rest;
            self.tool = Tool::Bone;
            self.changed();
        }
        if ui
            .add_enabled(
                self.selected.is_some(),
                egui::Button::new("Remove selected"),
            )
            .clicked()
        {
            self.delete();
        }
        ui.add_space(10.);
        ui.label(RichText::new("VIEW").small().color(ACCENT).strong());
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.grid, "Grid");
            ui.checkbox(&mut self.bones, "Bones");
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.show_skin, "Skin");
            if ui.checkbox(&mut self.onion, "Onion skin").changed() {
                self.changed();
            }
        });
    }
    fn inspector(&mut self, ctx: &egui::Context) {
        self.dock_panel(ctx, workspaces::Panel::Inspector, Self::inspector_ui);
    }
    fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.);
        ui.label(RichText::new("INSPECTOR").small().color(ACCENT).strong());
        ui.add_space(8.);
        egui::ScrollArea::vertical().show(ui,|ui| {
                if let Some(id)=self.selected {
                    if let Some(b)=self.project.rig.bone(id).cloned() {
                        let before=self.project.clone();let mut new=b.clone();
                        ui.label(RichText::new("Bone properties").size(18.).strong());ui.add_space(6.);
                        ui.text_edit_singleline(&mut new.name);
                        ui.label(RichText::new(format!("ID {}   ·   {}",id,if b.connected {"Connected"}else if b.parent.is_some(){"Anchored joint"}else{"Root"})).small().color(DIM));
                        if b.parent.is_none(){ui.label(RichText::new("Root anchor · use Move Rig (M) to change position").small().color(ACCENT));}
                        ui.add_space(10.);
                        ui.add_enabled_ui(!b.locked,|ui|{
                        if self.mode==Mode::Rest {
                            ui.add_enabled_ui(!b.length_locked,|ui|row(ui,"Length",&mut new.length,1. ..=512.));
                            row(ui,"Angle",&mut new.angle,-360. ..=360.);
                            row(ui,"Head radius",&mut new.radius_head,0. ..=64.);
                            row(ui,"Tail radius",&mut new.radius_tail,0. ..=64.);
                            if !new.connected {ui.add_enabled_ui(false,|ui|{row(ui,"Head X",&mut new.x,-4096. ..=4096.);row(ui,"Head Y",&mut new.y,-4096. ..=4096.);});}
                            egui::ComboBox::from_label("Group").selected_text(self.project.rig.parts.iter().find(|p|p.id==new.part).map_or("None",|p|p.name.as_str())).show_ui(ui,|ui| {
                                ui.selectable_value(&mut new.part,0,"None");for p in &self.project.rig.parts {ui.selectable_value(&mut new.part,p.id,&p.name);}
                            });
                            if new!=b {self.history.record(&before);*self.project.rig.bone_mut(id).unwrap()=new;self.changed();}
                        } else {
                            if new.name!=b.name {self.history.record(&before);self.project.rig.bone_mut(id).unwrap().name=new.name.clone();}
                            let pose=self.project.rig.active_clip().sample(self.frame);let original=pose.get(&id).copied().unwrap_or_default();let mut p=original;
                            row(ui,"Rotation",&mut p.rotation,b.rotation_min.unwrap_or(-720.)..=b.rotation_max.unwrap_or(720.));ui.add_enabled_ui(false,|ui|{row(ui,"Offset X",&mut p.x,-4096. ..=4096.);row(ui,"Offset Y",&mut p.y,-4096. ..=4096.);});ui.add_enabled_ui(!b.length_locked,|ui|row(ui,"Scale",&mut p.scale,0.1..=4.));p.rotation=b.clamp_rotation(p.rotation);p.scale=b.clamp_scale(p.scale);
                            if p!=original {self.history.record(&before);for (ch,v,old) in [(Channel::Rotation,p.rotation,original.rotation),(Channel::X,p.x,original.x),(Channel::Y,p.y,original.y),(Channel::Scale,p.scale,original.scale)] {if v!=old {let _=self.project.rig.active_clip_mut().set_key(id,ch,self.frame,v);}}self.changed();}
                            if ui.button("Key current pose   K").clicked() {self.key_selected();}
                            if ui.button("Reset pose at this frame").clicked() {self.record();for (ch,v) in [(Channel::Rotation,0.),(Channel::X,0.),(Channel::Y,0.),(Channel::Scale,1.)] {let _=self.project.rig.active_clip_mut().set_key(id,ch,self.frame,v);}}
                            ui.label(RichText::new("Pose edits automatically create keyframes.").small().color(DIM));
                        }
                        if !b.bulges_left.is_empty() || !b.bulges_right.is_empty() {
                            ui.separator();ui.label("Skin bulges");
                            let mut left=b.bulges_left.clone();let mut right=b.bulges_right.clone();
                            for (label,list) in [("L",&mut left),("R",&mut right)] {for (i,bulge) in list.iter_mut().enumerate() {ui.horizontal(|ui| {ui.label(format!("{label}{}",i+1));ui.add(egui::DragValue::new(&mut bulge.t).range(0.01..=0.99).speed(0.01));ui.add(egui::DragValue::new(&mut bulge.d).range(-64. ..=64.).speed(0.1));});}list.sort_by(|a,b|a.t.total_cmp(&b.t));}
                            if left!=b.bulges_left || right!=b.bulges_right {self.record();let bone=self.project.rig.bone_mut(id).unwrap();bone.bulges_left=left;bone.bulges_right=right;}
                            if ui.button("Clear bulges").clicked(){self.record();let bone=self.project.rig.bone_mut(id).unwrap();bone.bulges_left.clear();bone.bulges_right.clear();}
                        }
                        });
                    }
                } else {ui.label(RichText::new("Shape your next character.").size(18.).strong());ui.label(RichText::new("Pick a bone in the canvas or skeleton tree. Drag its end to change the rig.").color(DIM));}
                ui.add_space(14.);ui.separator();
                egui::CollapsingHeader::new("Groups & colors").default_open(true).show(ui,|ui| {
                    let before=self.project.clone();
                    for p in &mut self.project.rig.parts {ui.horizontal(|ui|{ui.color_edit_button_srgba_unmultiplied(&mut p.color);ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(90.));ui.add(egui::DragValue::new(&mut p.z).prefix("z ").range(-100..=100));});}
                    if before!=self.project {self.history.record(&before);self.changed();}
                    if ui.button("+ New group").clicked() && self.project.rig.parts.len()<256 {self.record();let id=self.project.rig.add_part("New group",[119,225,187,255]);if let Some(b)=self.selected.and_then(|id|self.project.rig.bone_mut(id)){b.part=id;}}
                });
                ui.separator();egui::CollapsingHeader::new("Canvas & symmetry").show(ui,|ui| {
                    let before=self.project.clone();ui.checkbox(&mut self.project.extras.auto_canvas,"Grow canvas with character");let(mut width,mut height)=(self.project.rig.width,self.project.rig.height);ui.add_enabled_ui(!self.project.extras.auto_canvas,|ui|{ui.horizontal(|ui|{ui.label("Pixels");ui.add(egui::DragValue::new(&mut width).range(1..=4096));ui.label("×");ui.add(egui::DragValue::new(&mut height).range(1..=4096));});});if (width,height)!=(self.project.rig.width,self.project.rig.height){let origin=self.project.extras.canvas_origin;if let Err(e)=rebonebase::canvas::set_bounds(&mut self.project,origin,width,height){self.status=e.to_string();}}
                    ui.checkbox(&mut self.project.rig.mirror.on,"Mirror rest-pose gestures");row(ui,"Mirror X",&mut self.project.rig.mirror.x,-4096. ..=4096.);
                    if before!=self.project {self.history.record(&before);self.changed();}
                    if ui.button("Fit canvas    F").clicked(){self.fit=true;}
                });
                ui.separator();egui::CollapsingHeader::new("Pose library").show(ui,|ui| {
                    if ui.button("+ Capture current frame").clicked() {self.record();let name=format!("Pose {}",self.project.poses.len()+1);match self.project.capture_pose(name,self.frame) {Ok(())=>self.status="Pose captured".into(),Err(e)=>self.status=e.to_string()}}
                    for i in 0..self.project.poses.len() {
                        let old=self.project.clone();ui.text_edit_singleline(&mut self.project.poses[i].name);if old!=self.project{self.history.record(&old);}
                        ui.horizontal(|ui| {
                            for (label,mirror) in [("Apply",false),("Mirror",true)] {if ui.small_button(label).clicked() {self.record();self.mode=Mode::Pose;match self.project.apply_pose(i,self.frame,mirror){Ok(())=>self.status="Saved pose applied".into(),Err(e)=>self.status=e.to_string()}}}
                        });
                    }
                    if !self.project.poses.is_empty() && ui.button("Remove last pose").clicked(){self.record();self.project.poses.pop();}
                });
                ui.separator();egui::CollapsingHeader::new("Reference image").show(ui,|ui| {
                    if ui.button("Import image…").clicked() {self.import_reference();}
                    let before=self.project.clone();
                    if let Some(r)=&mut self.project.reference {row(ui,"X",&mut r.x,-4096. ..=4096.);row(ui,"Y",&mut r.y,-4096. ..=4096.);row(ui,"Scale",&mut r.scale,0.001..=100.);row(ui,"Opacity",&mut r.opacity,0. ..=1.);if ui.button("Remove reference").clicked(){self.project.reference=None;self.ref_tex=None;}}
                    if before!=self.project {self.history.record(&before);}
                });
            });
    }
    fn timeline(&mut self, ctx: &egui::Context) {
        self.dock_panel(ctx, workspaces::Panel::Timeline, Self::timeline_ui);
    }
    fn timeline_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui
                .button(if self.playing { "Pause" } else { "Play  ▷" })
                .clicked()
            {
                self.toggle_play();
            }
            if ui.small_button("|◁").clicked() {
                self.frame = 1.;
                self.mode = Mode::Pose;
                self.changed();
            }
            ui.label(
                RichText::new(format!(
                    "{:03} / {:03}",
                    self.frame as u32,
                    self.project.rig.active_clip().length
                ))
                .monospace()
                .color(ACCENT),
            );
            let old = self.project.rig.active_clip;
            egui::ComboBox::from_id_salt("clip")
                .selected_text(&self.project.rig.active_clip().name)
                .show_ui(ui, |ui| {
                    for (i, c) in self.project.rig.clips.iter().enumerate() {
                        ui.selectable_value(&mut self.project.rig.active_clip, i, &c.name);
                    }
                });
            if self.project.rig.active_clip != old {
                self.studio.keys.clear();
                self.studio.key_drag = None;
                self.playing = false;
                let mut before = self.project.clone();
                before.rig.active_clip = old;
                self.history.record(&before);
                self.frame = 1.;
                self.changed();
            }
            if ui.small_button("+ Clip").clicked() && self.project.rig.clips.len() < 32 {
                self.record();
                let n = self.project.rig.clips.len() + 1;
                self.project.rig.clips.push(Clip {
                    name: format!("Clip {n}"),
                    ..Default::default()
                });
                self.project.rig.active_clip = n - 1;
                self.frame = 1.;
            }
            let before = self.project.clone();
            let c = self.project.rig.active_clip_mut();
            ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(100.));
            ui.add(
                egui::DragValue::new(&mut c.fps)
                    .range(1. / 60. ..=240.)
                    .prefix("FPS "),
            );
            let oldlen = c.length;
            let mut newlen = oldlen;
            ui.add(
                egui::DragValue::new(&mut newlen)
                    .range(1..=240)
                    .prefix("Frames "),
            );
            ui.checkbox(&mut c.looped, "Loop");
            if newlen != oldlen {
                let index = self.project.rig.active_clip;
                match rebonebase::production::edit_clip(
                    &mut self.project,
                    rebonebase::production::ClipEdit::Resize {
                        index,
                        length: newlen,
                    },
                ) {
                    Ok(()) => {
                        self.frame = self.frame.min(newlen as f32);
                        self.studio.keys.clear();
                    }
                    Err(e) => self.status = e,
                }
            }
            if before != self.project {
                self.history.record(&before);
                self.changed();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("DOPE SHEET").small().color(DIM));
            if ui.button("Animation templates…").clicked() {
                self.production.open = true;
                self.production.tab = production_ui::Tab::Motion;
                if self.studio.preferences.workspace.production.dock
                    == rebonebase::workspace::Dock::Hidden
                {
                    self.studio.preferences.workspace.production.dock =
                        rebonebase::workspace::Dock::Floating;
                }
            }
            if ui
                .add_enabled(self.selected.is_some(), egui::Button::new("+ Key pose"))
                .clicked()
            {
                self.key_selected();
            }
            if ui
                .add_enabled(
                    self.selected.is_some(),
                    egui::Button::new("Remove keys here"),
                )
                .clicked()
            {
                self.record();
                let id = self.selected.unwrap();
                let frame = self.frame;
                for t in &mut self.project.rig.active_clip_mut().tracks {
                    if t.bone == id {
                        t.keys.retain(|k| (k.frame - frame).abs() > 0.01);
                    }
                }
            }
            egui::ComboBox::from_id_salt("easing")
                .selected_text(format!("{:?}", self.easing))
                .show_ui(ui, |ui| {
                    for e in [
                        Ease::Linear,
                        Ease::Ease,
                        Ease::EaseIn,
                        Ease::EaseOut,
                        Ease::Back,
                        Ease::Bounce,
                        Ease::Step,
                    ] {
                        ui.selectable_value(&mut self.easing, e, format!("{e:?}"));
                    }
                });
            if ui.button("Apply easing to keys here").clicked() {
                self.record();
                for t in &mut self.project.rig.active_clip_mut().tracks {
                    if self.selected.is_none_or(|id| id == t.bone) {
                        for k in &mut t.keys {
                            if (k.frame - self.frame).abs() < 0.01 {
                                k.easing = self.easing;
                            }
                        }
                    }
                }
            }
        });
        self.editable_dope(ui);
    }
    fn canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(18, 23, 29)))
            .show(ctx, |ui| {
                ui.add_space(8.);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(10.);
                    for (m, label) in [(Mode::Rest, "Rig"), (Mode::Pose, "Animate")] {
                        if ui
                            .selectable_label(self.mode == m, RichText::new(label).strong())
                            .clicked()
                        {
                            self.mode = m;
                            self.playing = false;
                            self.tool = Tool::Select;
                            self.changed();
                        }
                    }
                    ui.separator();
                    for (t, label) in [
                        (Tool::Select, "Select"),
                        (Tool::Bone, "Bone"),
                        (Tool::Radius, "Radius"),
                        (Tool::Bulge, "Bulge"),
                        (Tool::IK, "IK"),
                        (Tool::MoveRig, "Move Rig"),
                        (Tool::Pan, "Pan"),
                        (Tool::Pixel,"Pixels"),
                        (Tool::Box,"Boxes"),
                    ] {
                        let allowed = if self.mode == Mode::Rest {
                            t != Tool::IK
                        } else {
                            t != Tool::Bone && t != Tool::Radius && t != Tool::Bulge
                        };
                        if ui
                            .add_enabled(allowed, egui::Button::new(label).selected(self.tool == t))
                            .clicked()
                        {
                            self.tool = t;
                            if matches!(t,Tool::Pixel|Tool::Box){self.mode=Mode::Pose;self.production.transition_preview=false;self.changed();}
                        }
                    }
                    if ui.button("Fit").clicked() {
                        self.fit = true;
                    }
                });
                ui.add_space(6.);
                if let Some(error) = &self.render_error {
                    ui.colored_label(Color32::from_rgb(255, 140, 140), format!("Preview unavailable: {error}"));
                }
                self.sync_canvas();
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                self.last_canvas = rect;
                if self.fit {
                    self.zoom = ((rect.width() - 70.) / self.project.rig.width as f32)
                        .min((rect.height() - 70.) / self.project.rig.height as f32)
                        .clamp(0.1, 32.);
                    self.pan = EVec2::ZERO;
                    self.fit = false;
                }
                let center = rect.center() + self.pan;
                let raster_origin = center
                    - EVec2::new(
                        self.project.rig.width as f32 * self.zoom,
                        self.project.rig.height as f32 * self.zoom,
                    ) * 0.5;
                let origin=raster_origin-EVec2::new(self.project.extras.canvas_origin.x,self.project.extras.canvas_origin.y)*self.zoom;
                let zoom = self.zoom;
                let to_screen = |v: Vec2| origin + EVec2::new(v.x, v.y) * zoom;
                let to_world =
                    |p: Pos2| Vec2::new((p.x - origin.x) / zoom, (p.y - origin.y) / zoom);
                let p = ui.painter_at(rect);
                let canvas = Rect::from_min_size(
                    raster_origin,
                    EVec2::new(
                        self.project.rig.width as f32 * self.zoom,
                        self.project.rig.height as f32 * self.zoom,
                    ),
                );
                // Checkerboard is clipped to the actual canvas and viewport.
                let check = 8. * self.zoom;
                let visible = canvas.intersect(rect);
                let ix0 = ((visible.left() - raster_origin.x) / check).floor().max(0.) as i32;
                let iy0 = ((visible.top() - raster_origin.y) / check).floor().max(0.) as i32;
                let ix1 = ((visible.right() - raster_origin.x) / check).ceil().max(0.) as i32;
                let iy1 = ((visible.bottom() - raster_origin.y) / check).ceil().max(0.) as i32;
                let cp = p.with_clip_rect(visible);
                for y in iy0..iy1 {
                    for x in ix0..ix1 {
                        cp.rect_filled(
                            Rect::from_min_size(
                                raster_origin + EVec2::new(x as f32 * check, y as f32 * check),
                                EVec2::splat(check),
                            ),
                            0.,
                            if (x + y) % 2 == 0 {
                                Color32::from_rgb(39, 46, 53)
                            } else {
                                Color32::from_rgb(34, 41, 48)
                            },
                        );
                    }
                }
                let reference_index=self.project.extras.references.as_ref().map(|seq|seq.index_at(self.timeline_ms()));
                if reference_index!=self.studio.ref_index{self.ref_tex=None;self.studio.ref_index=reference_index;}
                let reference=self.project.extras.references.as_ref().and_then(|seq|reference_index.and_then(|i|seq.frames.get(i))).or(self.project.reference.as_ref());
                if let Some(r) = reference {
                    if self.ref_tex.is_none() {
                        self.ref_tex = Some(ctx.load_texture(
                            "reference",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [r.width as usize, r.height as usize],
                                &r.pixels,
                            ),
                            egui::TextureOptions::NEAREST,
                        ));
                    }
                    let rrect = Rect::from_min_size(
                        to_screen(Vec2::new(r.x, r.y)),
                        EVec2::new(r.width as f32, r.height as f32) * r.scale * self.zoom,
                    );
                    p.image(
                        self.ref_tex.as_ref().unwrap().id(),
                        rrect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                        Color32::from_white_alpha((r.opacity * 255.) as u8),
                    );
                }
                if self.onion && self.mode == Mode::Pose {
                    if self.onion_tex.is_none() {
                        let img = self.onion_image();
                        self.onion_tex = Some(ctx.load_texture(
                            "onion",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [img.width() as usize, img.height() as usize],
                                img.as_raw(),
                            ),
                            egui::TextureOptions::NEAREST,
                        ));
                    }
                    p.image(
                        self.onion_tex.as_ref().unwrap().id(),
                        canvas,
                        unit_rect(),
                        Color32::WHITE,
                    );
                }
                if self.show_skin {
                    if self.skin_tex.is_none() {
                        let img = match self.project.try_render_pose(self.frame,self.pose().as_ref()) {
                            Ok(image) => {
                                self.render_error = None;
                                image
                            }
                            Err(error) => {
                                let message = format!("{error:#}");
                                self.status = format!("Render failed: {message}");
                                self.render_error = Some(message);
                                image::RgbaImage::from_pixel(
                                    self.project.rig.width.clamp(1, 4096),
                                    self.project.rig.height.clamp(1, 4096),
                                    image::Rgba([190, 0, 90, 255]),
                                )
                            }
                        };
                        self.skin_tex = Some(ctx.load_texture(
                            "skin",
                            egui::ColorImage::from_rgba_unmultiplied(
                                [img.width() as usize, img.height() as usize],
                                img.as_raw(),
                            ),
                            egui::TextureOptions::NEAREST,
                        ));
                    }
                    p.image(
                        self.skin_tex.as_ref().unwrap().id(),
                        canvas,
                        unit_rect(),
                        Color32::WHITE,
                    );
                }
                if self.grid && self.zoom >= 5. {
                    let x0 = ((visible.left() - raster_origin.x) / self.zoom).floor().max(0.) as u32;
                    let x1 = ((visible.right() - raster_origin.x) / self.zoom)
                        .ceil()
                        .min(self.project.rig.width as f32) as u32;
                    let y0 = ((visible.top() - raster_origin.y) / self.zoom).floor().max(0.) as u32;
                    let y1 = ((visible.bottom() - raster_origin.y) / self.zoom)
                        .ceil()
                        .min(self.project.rig.height as f32) as u32;
                    for x in x0..=x1 {
                        let x = raster_origin.x + x as f32 * self.zoom;
                        p.line_segment(
                            [Pos2::new(x, visible.top()), Pos2::new(x, visible.bottom())],
                            Stroke::new(0.5_f32, Color32::from_white_alpha(16)),
                        );
                    }
                    for y in y0..=y1 {
                        let y = raster_origin.y + y as f32 * self.zoom;
                        p.line_segment(
                            [Pos2::new(visible.left(), y), Pos2::new(visible.right(), y)],
                            Stroke::new(0.5_f32, Color32::from_white_alpha(16)),
                        );
                    }
                }
                p.rect_stroke(
                    canvas,
                    0.,
                    Stroke::new(1.0_f32, Color32::from_rgb(77, 91, 99)),
                    egui::StrokeKind::Outside,
                );
                if self.project.rig.mirror.on && self.mode == Mode::Rest {
                    let x = to_screen(Vec2::new(self.project.rig.mirror.x, 0.)).x;
                    p.line_segment(
                        [Pos2::new(x, canvas.top()), Pos2::new(x, canvas.bottom())],
                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(119, 225, 187, 50)),
                    );
                }
                let world = self.project.rig.world(self.pose().as_ref());
                if self.production.show_boxes{for region in rebonebase::production::collision_shapes(&self.project,self.frame){let color=match region.kind{rebonebase::production::BoxKind::Hit=>Color32::from_rgb(255,105,95),rebonebase::production::BoxKind::Hurt=>Color32::from_rgb(100,175,255),rebonebase::production::BoxKind::Interaction=>Color32::from_rgb(255,215,100)};let mut points:Vec<_>=region.corners.iter().map(|v|to_screen(*v)).collect();points.push(points[0]);p.add(egui::Shape::line(points,Stroke::new(1.5_f32,color)));p.text(to_screen(region.center),egui::Align2::CENTER_CENTER,&region.name,egui::FontId::proportional(10.),color);}}
                if self.tool==Tool::Box&&let Some(g)=&self.gesture&&let Some(at)=response.interact_pointer_pos(){p.rect_stroke(Rect::from_two_pos(to_screen(g.start),at),0.,Stroke::new(1.5_f32,Color32::YELLOW),egui::StrokeKind::Inside);}
                if self.tool==Tool::Pixel&&let Some(at)=response.hover_pos(){let point=to_world(at);let start=to_screen(Vec2::new(point.x.floor(),point.y.floor()));p.rect_stroke(Rect::from_min_size(start,EVec2::splat(self.production.brush as f32*self.zoom)),0.,Stroke::new(1.0_f32,Color32::WHITE),egui::StrokeKind::Inside);}
                self.studio.hover=response.hover_pos().and_then(|pos|self.pick(&world,to_world(pos),false).map(|h|h.0));
                if let Some(y)=self.project.extras.ground{p.line_segment([to_screen(Vec2::new(0.,y)),to_screen(Vec2::new(self.project.rig.width as f32,y))],Stroke::new(1.0_f32,Color32::from_rgb(230,175,90)));}
                if self.studio.show_path{
                    let mut root=self.selected.or_else(||self.project.rig.bones.iter().find(|b|b.parent.is_none()).map(|b|b.id));
                    while let Some(parent)=root.and_then(|id|self.project.rig.bone(id)).and_then(|b|b.parent){root=Some(parent);}
                    if let Some(root)=root{let path=rebonebase::editing::root_path(&self.project.rig,self.project.rig.active_clip,root).unwrap_or_default();for points in path.windows(2){p.line_segment([to_screen(points[0]),to_screen(points[1])],Stroke::new(1.5_f32,Color32::from_rgb(255,197,91)));}}
                }
                if self.mode==Mode::Pose{for pin in self.project.extras.pins.iter().filter(|p|p.enabled){let target=to_screen(pin.target);p.circle_stroke(target,7.,Stroke::new(2.0_f32,Color32::from_rgb(255,197,91)));p.line_segment([target-EVec2::new(10.,0.),target+EVec2::new(10.,0.)],Stroke::new(1.0_f32,Color32::from_rgb(255,197,91)));}}
                if self.bones {
                    for b in &world {
                        let h = to_screen(b.head);
                        let t = to_screen(b.tail);
                        let selected = self.selected == Some(b.id)||self.studio.selection.contains(&b.id);
                        let col = if selected {
                            ACCENT
                        } else if self.studio.hover==Some(b.id) {Color32::from_rgb(255,211,124)} else {
                            Color32::from_rgba_unmultiplied(224, 235, 240, 195)
                        };
                        let d = t - h;
                        let len = d.length().max(0.01);
                        let n = EVec2::new(-d.y / len, d.x / len) * if selected { 4. } else { 2.5 };
                        let mid = h + d * 0.22;
                        p.add(egui::Shape::convex_polygon(
                            vec![h, mid + n, t, mid - n],
                            Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 40),
                            Stroke::new(if selected { 1.8_f32 } else { 1.0_f32 }, col),
                        ));
                        p.circle_filled(h, 3., Color32::from_rgb(22, 30, 35));
                        p.circle_stroke(h, 3., Stroke::new(1.0_f32, col));
                        p.circle_filled(t, if selected { 4. } else { 2.2 }, col);
                        if selected {
                            p.text(
                                t + EVec2::new(10., -7.),
                                egui::Align2::LEFT_BOTTOM,
                                self.project.rig.bone(b.id).unwrap().name.clone(),
                                egui::FontId::proportional(11.),
                                ACCENT,
                            );
                        }
                    }
                }
                if let Some((a,b))=self.studio.marquee{p.rect_stroke(Rect::from_two_pos(to_screen(a),to_screen(b)),0.,Stroke::new(1.0_f32,ACCENT),egui::StrokeKind::Inside);}
                if self.tool == Tool::Bone
                    && let Some(g) = &self.gesture
                    && let Some(pos) = response.interact_pointer_pos()
                {
                    p.line_segment([to_screen(g.start), pos], Stroke::new(2.0_f32, ACCENT));
                }
                p.text(
                    rect.left_bottom() + EVec2::new(16., -13.),
                    egui::Align2::LEFT_BOTTOM,
                    match self.tool {
                        Tool::Bone => "Drag to draw a bone · start near a joint to connect",
                        Tool::Radius => "Drag away from a joint to adjust its skin radius",
                        Tool::Bulge => "Drag a bone edge to shape its skin",
                        Tool::IK => {
                            "Drag a lower limb to pose its two-bone chain · Shift flips the bend"
                        }
                        Tool::Pan => "Drag to pan · scroll to zoom",
                        Tool::Pixel=>"Paint frame corrections · use Production → Pixels & boxes for brush, erase and restore",
                        Tool::Box=>"Drag a collision region · configure its timing and attachment in Production",
                        Tool::MoveRig => "Drag to move the whole mannequin · M selects Move Rig · Pose mode keys its movement",
                        Tool::Select => {
                            if self.mode == Mode::Pose {
                                "Drag to pose · changes are keyed automatically"
                            } else {
                                "Drag a bone end to reshape · middle or right drag to pan"
                            }
                        }
                    },
                    egui::FontId::proportional(11.),
                    DIM,
                );
                let pointer = response
                    .interact_pointer_pos()
                    .or_else(|| ctx.input(|i| i.pointer.hover_pos()));
                if response.hovered() {
                    let delta = ctx.input(|i| i.smooth_scroll_delta.y);
                    if delta.abs() > 0.01 {
                        let old = self.zoom;
                        let new = (old * (delta * 0.002).exp()).clamp(0.1, 64.);
                        if let Some(pos) = pointer {
                            self.pan += (pos - center) * (1. - new / old);
                        }
                        self.zoom = new;
                    }
                }
                if response.dragged_by(egui::PointerButton::Middle)
                    || response.dragged_by(egui::PointerButton::Secondary)
                    || (self.tool == Tool::Pan && response.dragged())
                {
                    self.pan += ctx.input(|i| i.pointer.delta());
                }
                if response.clicked()
                    && self.tool != Tool::Pan
                    && let Some(pos) = pointer
                {
                    if self.tool==Tool::Pixel{self.paint_stroke(to_world(pos),to_world(pos),true);}else if self.tool!=Tool::Box{if let Some((id,_))=self.pick(&world,to_world(pos),ctx.input(|i|i.modifiers.alt)){self.select_bone(id,ctx.input(|i|i.modifiers.shift));}else if !ctx.input(|i|i.modifiers.shift){self.selected=None;self.studio.selection.clear();}}
                }
                if response.drag_started_by(egui::PointerButton::Primary)
                    && self.tool != Tool::Pan
                    && let Some(pos) = pointer
                {
                    let at = to_world(ctx.input(|i| i.pointer.press_origin()).unwrap_or(pos));
                    let hit=if matches!(self.tool,Tool::Pixel|Tool::Box){None}else{self.pick(&world,at,false)};let id=hit.map(|h|h.0);let tail=hit.is_some_and(|h|h.1);
                    self.production.last_pixel=Some(at);
                    if let Some(id)=id {if !self.studio.selection.contains(&id){self.select_bone(id,ctx.input(|i|i.modifiers.shift));}else{self.selected=Some(id);}}
                    self.playing = false;
                    self.gesture = Some(Gesture {
                        base: self.project.clone(),
                        start: at,
                        id,
                        tail,
                        tool: self.tool,
                        recorded: false,
                    });
                }
                if response.dragged_by(egui::PointerButton::Primary)
                    && self.tool != Tool::Pan
                    && let Some(pos) = pointer
                {
                    self.drag(to_world(pos), ctx.input(|i| i.modifiers.shift));
                }
                if response.drag_stopped_by(egui::PointerButton::Primary)
                    && let Some(g) = self.gesture.take()
                    && let Some(pos) = pointer
                {
                    if g.tool==Tool::Pixel{self.production.last_pixel=None;return;}
                    if g.tool==Tool::Box{self.draw_collision(g.start,to_world(pos));return;}
                    if g.tool==Tool::Select && g.id.is_none(){self.finish_marquee(&world,ctx.input(|i|i.modifiers.shift));return;}
                    if g.tool!=Tool::Bone{return;}
                    self.history.record(&g.base);
                    let at = to_world(pos);
                    let parent =
                        g.id.filter(|id| {
                            g.base.rig.world(None).iter().any(|w| {
                                w.id == *id && (w.tail - g.start).length() < 10. / self.zoom
                            })
                        });
                    let start = parent
                        .and_then(|id| world.iter().find(|w| w.id == id).map(|w| w.tail))
                        .unwrap_or(g.start);
                    match self.project.rig.add_bone(start, at, parent) {
                        Ok(id) => {
                            self.selected = Some(id);
                            self.status = "Bone added".into();
                        }
                        Err(e) => self.status = e,
                    }
                    self.changed();
                }
            });
    }
    fn drag(&mut self, target: Vec2, shift: bool) {
        let Some(mut g) = self.gesture.take() else {
            return;
        };
        if g.tool == Tool::Bone || g.tool == Tool::Box {
            self.gesture = Some(g);
            return;
        }
        if g.tool == Tool::Pixel {
            let start = self.production.last_pixel.unwrap_or(g.start);
            self.paint_stroke(start, target, false);
            self.production.last_pixel = Some(target);
            if !g.recorded && self.project != g.base {
                self.history.record(&g.base);
                g.recorded = true;
            }
            self.gesture = Some(g);
            return;
        }
        if g.tool == Tool::MoveRig {
            let delta = target - g.start;
            let mut candidate = g.base.clone();
            if self.production.move_character
                && let Some(c) = self.production.character.and_then(|id| {
                    candidate
                        .extras
                        .characters3d
                        .iter_mut()
                        .find(|c| c.id == id)
                })
            {
                c.position = c.position + delta;
                if let Err(e) = candidate.validate() {
                    self.status = format!("3D movement rejected: {e}");
                    self.gesture = Some(g);
                    return;
                }
                if !g.recorded && candidate != g.base {
                    self.history.record(&g.base);
                    g.recorded = true;
                }
                self.project = candidate;
                self.status = "Move Rig · 3D character repositioned".into();
                self.changed();
                self.gesture = Some(g);
                return;
            }
            let motion_frame = candidate.rig.active_clip().evaluation_frame(self.frame);
            if let Some(actor) = self.production.actor.and_then(|id| {
                candidate
                    .extras
                    .production
                    .actors
                    .iter_mut()
                    .find(|a| a.id == id)
            }) {
                if self.mode == Mode::Rest {
                    actor.offset = actor.offset + delta;
                    for k in &mut actor.keys {
                        k.offset = k.offset + delta;
                    }
                } else {
                    if let Err(error) = candidate.rig.active_clip_mut().release_hold(self.frame) {
                        self.status = error;
                        self.gesture = Some(g);
                        return;
                    }
                    let mut placement = rebonebase::production::actor_placement(
                        actor,
                        candidate.rig.active_clip,
                        motion_frame,
                    );
                    if !actor
                        .keys
                        .iter()
                        .any(|k| k.clip == candidate.rig.active_clip)
                        && self.frame > 1.
                    {
                        actor.keys.push(rebonebase::production::ActorKey {
                            clip: candidate.rig.active_clip,
                            frame: 1.,
                            offset: actor.offset,
                            scale: actor.scale,
                            visible: actor.visible,
                        });
                    }
                    placement.frame = self.frame;
                    placement.offset = placement.offset + delta;
                    actor
                        .keys
                        .retain(|k| k.clip != candidate.rig.active_clip || k.frame != self.frame);
                    actor.keys.push(placement);
                    actor
                        .keys
                        .sort_by(|a, b| a.clip.cmp(&b.clip).then(a.frame.total_cmp(&b.frame)));
                }
                if let Err(e) = candidate.validate() {
                    self.status = format!("Actor movement exceeds document limits: {e}");
                    self.gesture = Some(g);
                    return;
                }
                if !g.recorded && candidate != g.base {
                    self.history.record(&g.base);
                    g.recorded = true;
                }
                self.project = candidate;
                self.changed();
                self.gesture = Some(g);
                return;
            }
            let roots: Vec<_> = candidate
                .rig
                .bones
                .iter()
                .filter(|b| b.parent.is_none())
                .map(|b| b.id)
                .collect();
            if roots.is_empty() {
                self.status = "Add a skeleton before using Move Rig".into();
                self.gesture = Some(g);
                return;
            }
            if self.mode == Mode::Rest {
                for id in roots {
                    let b = candidate.rig.bone_mut(id).unwrap();
                    b.x += delta.x;
                    b.y += delta.y;
                }
                candidate.rig.mirror.x += delta.x;
                for pin in &mut candidate.extras.pins {
                    pin.target = pin.target + delta;
                }
                for c in &mut candidate.extras.production.motion.chains {
                    c.target = c.target + delta;
                }
                for c in &mut candidate.extras.production.motion.contacts {
                    for k in &mut c.keys {
                        k.target = k.target + delta;
                    }
                }
                if let Some(y) = &mut candidate.extras.ground {
                    *y += delta.y;
                }
            } else {
                let pose = g.base.rig.active_clip().sample(self.frame);
                for id in roots {
                    let old = pose.get(&id).copied().unwrap_or_default();
                    let clip = candidate.rig.active_clip_mut();
                    if clip
                        .set_key(id, Channel::X, self.frame, old.x + delta.x)
                        .is_err()
                        || clip
                            .set_key(id, Channel::Y, self.frame, old.y + delta.y)
                            .is_err()
                    {
                        self.status = "Move Rig exceeds animation limits".into();
                        self.gesture = Some(g);
                        return;
                    }
                }
            }
            match candidate.validate() {
                Ok(()) => {
                    if !g.recorded && candidate != g.base {
                        self.history.record(&g.base);
                        g.recorded = true;
                    }
                    self.project = candidate;
                    self.status = "Move Rig · whole mannequin repositioned".into();
                    self.changed();
                }
                Err(e) => self.status = format!("Move Rig exceeds document limits: {e}"),
            }
            self.gesture = Some(g);
            return;
        }
        let Some(id) = g.id else {
            if g.tool == Tool::Select {
                self.studio.marquee = Some((g.start, target));
            }
            self.gesture = Some(g);
            return;
        };
        if g.base.rig.bone(id).is_some_and(|b| b.locked) {
            self.status = "Bone is locked · unlock it in Studio".into();
            self.gesture = Some(g);
            return;
        }
        if g.base.rig.bone(id).is_some_and(|b| b.parent.is_none())
            && matches!(g.tool, Tool::Select | Tool::IK)
        {
            self.status =
                "Root anchor is locked · use Move Rig (M) to reposition the mannequin".into();
            self.gesture = Some(g);
            return;
        }
        let pose = (self.mode == Mode::Pose).then(|| g.base.rig.active_clip().sample(self.frame));
        let world = g.base.rig.world(pose.as_ref());
        let b = g.base.rig.bone(id).unwrap();
        let w = world.iter().find(|w| w.id == id).unwrap();
        if matches!(g.tool, Tool::Select | Tool::IK)
            && !g.tail
            && (g.start - w.head).length() < 2. / self.zoom
        {
            self.status = "Joint pivot selected · drag the bone body or its end to pose it".into();
            self.gesture = Some(g);
            return;
        }
        if !g.recorded {
            self.history.record(&g.base);
            g.recorded = true;
        }
        self.project = g.base.clone();
        if self.mode == Mode::Pose
            && g.tool == Tool::IK
            && g.tail
            && !g
                .base
                .extras
                .production
                .baked_motion
                .contains(&g.base.rig.active_clip)
        {
            if let Some(chain) = self
                .project
                .extras
                .production
                .motion
                .chains
                .iter_mut()
                .find(|c| c.enabled && c.bones.last() == Some(&id))
            {
                chain.target = target;
                self.changed();
                self.gesture = Some(g);
                return;
            }
            let clip = self.project.rig.active_clip;
            let contacts = &self.project.extras.production.motion.contacts;
            let contact_index = contacts
                .iter()
                .position(|c| c.enabled && c.lower == id && c.clip == Some(clip))
                .or_else(|| {
                    contacts
                        .iter()
                        .position(|c| c.enabled && c.lower == id && c.clip.is_none())
                });
            if let Some(index) = contact_index {
                let mut candidate = self.project.clone();
                let global = candidate.extras.production.motion.contacts[index]
                    .clip
                    .is_none();
                let sample_frame = if global {
                    candidate.rig.active_clip().source_frame(self.frame)
                } else {
                    candidate.rig.active_clip().evaluation_frame(self.frame)
                };
                if let Err(error) = candidate.rig.active_clip_mut().release_hold(self.frame) {
                    self.status = error;
                    self.gesture = Some(g);
                    return;
                }
                let authored_frame = if global {
                    candidate.rig.active_clip().source_frame(self.frame)
                } else {
                    self.frame
                };
                let contact = &mut candidate.extras.production.motion.contacts[index];
                let base = rebonebase::advanced_motion::contact_at(contact, sample_frame)
                    .unwrap_or_default();
                contact.keys.retain(|k| k.frame != authored_frame);
                contact.keys.push(rebonebase::advanced_motion::ContactKey {
                    frame: authored_frame,
                    target,
                    bend: if shift { -base.bend } else { base.bend },
                    ..base
                });
                contact.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                match candidate.validate() {
                    Ok(()) => {
                        self.project = candidate;
                        self.changed();
                    }
                    Err(error) => self.status = format!("Contact key rejected: {error}"),
                }
                self.gesture = Some(g);
                return;
            }
        }
        let ids: Vec<_> = self
            .selected_bones()
            .into_iter()
            .filter(|id| g.base.rig.bone(*id).is_some_and(|b| b.parent.is_some()))
            .collect();
        if g.tool == Tool::Select && ids.len() > 1 {
            let angle = rebonebase::animation::wrap_degrees(
                (target - w.head).angle() - (g.start - w.head).angle(),
            );
            let result = studio_rig::rotate_selection(
                &mut self.project.rig,
                &ids,
                angle,
                self.frame,
                self.mode,
            );
            if let Err(e) = result {
                self.status = e;
            }
            self.changed();
            self.gesture = Some(g);
            return;
        }
        if self.mode == Mode::Pose
            && g.tool == Tool::IK
            && let Some(pin) = self
                .project
                .extras
                .pins
                .iter_mut()
                .find(|p| p.lower == id && p.enabled)
        {
            pin.target = target;
            if shift {
                pin.bend = -pin.bend;
            }
            self.changed();
            self.gesture = Some(g);
            return;
        }
        if self.mode == Mode::Rest {
            match g.tool {
                Tool::Radius => {
                    let r = (target - if g.tail { w.tail } else { w.head })
                        .length()
                        .clamp(0., 64.);
                    let out = self.project.rig.bone_mut(id).unwrap();
                    if g.tail {
                        out.radius_tail = r
                    } else {
                        out.radius_head = r
                    };
                }
                Tool::Bulge => {
                    let delta = target - w.head;
                    let local = delta.rotate(-w.angle);
                    let t = (local.x / b.length).clamp(0.01, 0.99);
                    let out = self.project.rig.bone_mut(id).unwrap();
                    let list = if local.y < 0. {
                        &mut out.bulges_left
                    } else {
                        &mut out.bulges_right
                    };
                    let base = b.radius_head + (b.radius_tail - b.radius_head) * t;
                    let bulge = Bulge {
                        t,
                        d: (local.y.abs() - base).clamp(-64., 64.),
                    };
                    if let Some(q) = list.iter_mut().find(|q| (q.t - t).abs() < 0.15) {
                        *q = bulge
                    } else if list.len() < 128 {
                        list.push(bulge)
                    }
                    list.sort_by(|a, b| a.t.total_cmp(&b.t));
                }
                _ => {
                    if g.tail {
                        let _ = self.project.rig.move_tail(id, target);
                    } else {
                        let angle = rebonebase::animation::wrap_degrees(
                            (target - w.head).angle() - (g.start - w.head).angle(),
                        );
                        let out = self.project.rig.bone_mut(id).unwrap();
                        out.angle = rebonebase::animation::wrap_degrees(b.angle + angle);
                    }
                }
            }
            if self.project.rig.mirror.on
                && let Some(twin) = b.twin.filter(|twin| {
                    g.base
                        .rig
                        .bone(*twin)
                        .is_some_and(|b| !b.locked && b.parent.is_some())
                })
            {
                let mx = self.project.rig.mirror.x;
                match g.tool {
                    Tool::Radius => {
                        let original = self.project.rig.bone(id).unwrap().clone();
                        if let Some(t) = self.project.rig.bone_mut(twin) {
                            t.radius_head = original.radius_head;
                            t.radius_tail = original.radius_tail;
                        }
                    }
                    Tool::Bulge => {
                        let original = self.project.rig.bone(id).unwrap().clone();
                        if let Some(t) = self.project.rig.bone_mut(twin) {
                            t.bulges_left = original.bulges_right;
                            t.bulges_right = original.bulges_left;
                        }
                    }
                    _ => {
                        let updated = self.project.rig.world(None);
                        if let Some(current) = updated.iter().find(|w| w.id == id) {
                            let tail = Vec2::new(2. * mx - current.tail.x, current.tail.y);
                            let tw = self.project.rig.bone(twin).unwrap().clone();
                            if g.tail {
                                let _ = self.project.rig.move_tail(twin, tail);
                            } else {
                                let parent_angle = tw
                                    .parent
                                    .and_then(|pid| updated.iter().find(|w| w.id == pid))
                                    .map_or(0., |w| w.angle);
                                self.project.rig.bone_mut(twin).unwrap().angle =
                                    rebonebase::animation::wrap_degrees(
                                        180. - current.angle - parent_angle,
                                    );
                            }
                        }
                    }
                }
            }
        } else {
            let current = pose
                .as_ref()
                .and_then(|p| p.get(&id))
                .copied()
                .unwrap_or_default();
            if g.tool == Tool::IK
                && g.tail
                && b.connected
                && let Some(upper_id) = b.parent
                && let Some(upper) = g.base.rig.bone(upper_id)
                && upper.parent.is_some()
                && let Some(uw) = world.iter().find(|w| w.id == upper_id)
            {
                let _ = uw;
                let mut constrained = g.base.rig.clone();
                if shift {
                    constrained.bone_mut(id).unwrap().bend = -b.bend;
                }
                let mut result = pose.clone().unwrap_or_default();
                match ik::apply_two_bone(&constrained, &mut result, upper_id, id, target) {
                    Ok(()) => {
                        for bid in [upper_id, id] {
                            let rotation = result.get(&bid).copied().unwrap_or_default().rotation;
                            let _ = self.project.rig.active_clip_mut().set_key(
                                bid,
                                Channel::Rotation,
                                self.frame,
                                rotation,
                            );
                        }
                    }
                    Err(e) => self.status = e,
                }
            } else {
                let delta = rebonebase::animation::wrap_degrees(
                    (target - w.head).angle() - (g.start - w.head).angle(),
                );
                let _ = self.project.rig.active_clip_mut().set_key(
                    id,
                    Channel::Rotation,
                    self.frame,
                    b.clamp_rotation(current.rotation + delta),
                );
            }
        }
        self.changed();
        self.gesture = Some(g);
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        self.import_dialogs(ctx);
        self.animation_import_dialog(ctx);
        self.studio_window(ctx);
        self.production_window(ctx);
        self.command_palette(ctx);
        if let Some(action) = self.pending.clone() {
            egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
                ui.set_width(380.);
                ui.heading("Save your changes?");
                ui.label("This project has unsaved changes.");
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    if ui.button("Save & continue").clicked() && self.save(false) {
                        self.pending = None;
                        self.perform(action.clone());
                    }
                    if ui.button("Discard changes").clicked() {
                        self.pending = None;
                        self.perform(action);
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending = None;
                    }
                });
            });
        }
        if self.help {
            egui::Window::new("Welcome to ReBoneBase").open(&mut self.help).default_width(480.).show(ctx,|ui| {
                ui.heading("From rig to pixels.");ui.label("A standalone Rust port of the Bonebase skeletal animation workflow.");ui.add_space(10.);
                for line in ["Rig: drag non-root bone ends, adjust radii and bulges; mirror edits linked twins.","Move Rig (M): drag anywhere in the canvas to move the whole mannequin. Animate mode keys the movement.","Root anchors stay fixed during ordinary bone drags. Drag limb bodies to rotate at their joints.","Bone tool (B): drag from a joint to grow a chain, or empty space for a root.","Animate: drag bones for FK poses, or use IK (I) on connected limb ends.","Every pose edit creates a key at the current frame. K keys a whole bone pose.","Scrub the dope sheet; Space plays. Arrow keys step frames.","Middle / right drag pans the camera. Scroll zooms. F or 0 fits the canvas.","Ctrl+Z / Ctrl+Shift+Z: undo / redo. Ctrl+S: save. Ctrl+O: open.","Export current PNG, a sprite sheet, or animated GIF from the top right."] {ui.label(line);}
                ui.separator();ui.label(RichText::new("Projects use .rebone JSON. Import legacy .bbrig rigs or BVH / glTF / GLB motion from File. Source features that cannot be transferred are listed during import.").color(DIM));
            });
        }
    }
}
impl eframe::App for ReboneApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.pending.is_none() {
                self.request(Pending::Close);
            }
        }
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.keyboard(ctx);
        if self.playing {
            self.studio.playback_ms += self.last_tick.elapsed().as_secs_f64() * 1000.;
            self.last_tick = Instant::now();
            let (frame, finished) = self
                .project
                .rig
                .active_clip()
                .frame_at_ms(self.studio.playback_ms);
            if self.frame != frame as f32 {
                self.frame = frame as f32;
                self.changed();
            }
            if finished {
                self.playing = false;
            }
            ctx.request_repaint_after(Duration::from_millis(10));
        }
        self.show(ctx);
        self.tick_services(ctx);
        self.production_tick(ctx);
    }
}
impl ReboneApp {
    fn show(&mut self, ctx: &egui::Context) {
        if self.studio.last_primary != self.selected {
            self.studio.selection.clear();
            self.studio.selection.extend(self.selected);
            self.studio.last_primary = self.selected;
            self.studio.parent = self
                .selected
                .and_then(|id| self.project.rig.bone(id))
                .and_then(|b| b.parent);
        }
        self.top(ctx);
        self.left(ctx);
        self.inspector(ctx);
        self.timeline(ctx);
        self.production_dock(ctx);
        if let Err(e) = self.project.validate() {
            if !self.history.undo(&mut self.project) {
                self.project = self.saved.clone();
            }
            self.gesture = None;
            self.changed();
            self.status = format!("Edit exceeds document limits: {e}");
        }
        self.canvas(ctx);
        self.dialogs(ctx);
        self.production.apply_layout = false;
    }
}
fn unit_rect() -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.))
}
fn row(ui: &mut egui::Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>) {
    ui.horizontal(|ui| {
        ui.add_sized([95., 22.], egui::Label::new(label));
        ui.add(egui::DragValue::new(value).range(range).speed(0.1));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pass(app: &mut ReboneApp, ctx: &egui::Context, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1280., 720.))),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.show(ctx));
    }
    fn pointer(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    #[test]
    fn ik_pointer_gesture_uses_original_press_and_undo_restores_keys() {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        app.mode = Mode::Pose;
        app.tool = Tool::IK;
        pass(&mut app, &ctx, vec![]);
        pass(&mut app, &ctx, vec![]);
        let bone = app
            .project
            .rig
            .bones
            .iter()
            .find(|b| b.name == "forearm_L")
            .unwrap()
            .id;
        let w = app
            .project
            .rig
            .world(None)
            .into_iter()
            .find(|w| w.id == bone)
            .unwrap();
        let origin =
            app.last_canvas.center() + app.pan - EVec2::new(64. * app.zoom, 64. * app.zoom) * 0.5;
        let from = origin + EVec2::new(w.tail.x, w.tail.y) * app.zoom;
        let to = from + EVec2::new(55., -85.);
        pass(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(from), pointer(from, true)],
        );
        pass(&mut app, &ctx, vec![egui::Event::PointerMoved(to)]);
        pass(&mut app, &ctx, vec![pointer(to, false)]);
        assert_eq!(app.selected, Some(bone));
        assert!(
            app.project
                .rig
                .active_clip()
                .tracks
                .iter()
                .any(|t| t.bone == bone && t.keys[0].value.abs() > 1.)
        );
        app.project.validate().unwrap();
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1280., 720.))),
            modifiers: egui::Modifiers::NONE,
            events: vec![egui::Event::Key {
                key: egui::Key::Z,
                physical_key: Some(egui::Key::Z),
                pressed: true,
                repeat: false,
                modifiers,
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            app.keyboard(ctx);
            app.show(ctx)
        });
        assert!(app.project.rig.active_clip().tracks.is_empty());
        app.redo();
        assert!(!app.project.rig.active_clip().tracks.is_empty());
    }
    #[test]
    fn rest_edit_gesture_and_pending_discard_are_transactional() {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        let id = app
            .project
            .rig
            .bones
            .iter()
            .find(|b| b.parent.is_some())
            .unwrap()
            .id;
        let old = app.project.clone();
        let tail = app
            .project
            .rig
            .world(None)
            .into_iter()
            .find(|b| b.id == id)
            .unwrap()
            .tail;
        app.gesture = Some(Gesture {
            base: old.clone(),
            start: tail,
            id: Some(id),
            tail: true,
            tool: Tool::Select,
            recorded: false,
        });
        app.drag(tail + Vec2::new(3., -4.), false);
        assert!(app.dirty());
        app.project.validate().unwrap();
        app.request(Pending::New);
        assert!(app.pending.is_some());
        assert_eq!(app.project.rig.name, old.rig.name);
        app.undo();
        assert_eq!(app.project, old);
        app.perform(Pending::New);
        assert!(app.project.rig.bones.is_empty());
        assert!(!app.dirty());
    }
}
