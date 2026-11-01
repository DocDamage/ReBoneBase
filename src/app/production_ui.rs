use super::*;
use rebonebase::{
    advanced_motion::WalkSettings,
    production::{self, engine_export::EngineTarget},
    workspace::{Action, Dock},
};
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(super) enum Tab {
    #[default]
    Motion,
    Props,
    Meshes,
    Looks,
    Face,
    Scene,
    AudioEvents,
    PixelsBoxes,
    Export,
    Workspace,
    Characters,
}
impl Tab {
    pub const ALL: [Self; 11] = [
        Self::Motion,
        Self::Characters,
        Self::Props,
        Self::Meshes,
        Self::Looks,
        Self::Face,
        Self::Scene,
        Self::AudioEvents,
        Self::PixelsBoxes,
        Self::Export,
        Self::Workspace,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Motion => "Motion",
            Self::Characters => "3D characters",
            Self::Props => "Props",
            Self::Meshes => "Meshes",
            Self::Looks => "Outfits",
            Self::Face => "Face",
            Self::Scene => "Scene",
            Self::AudioEvents => "Audio & events",
            Self::PixelsBoxes => "Pixels & boxes",
            Self::Export => "Game export",
            Self::Workspace => "Workspace",
        }
    }
}
#[derive(Clone, Copy, PartialEq, Default)]
pub(super) enum PaintMode {
    #[default]
    Paint,
    Erase,
    Restore,
}
pub(super) struct ProductionUi {
    pub animation_source: Option<super::animation_import_ui::SourceJob>,
    pub animation_dialog: Option<super::animation_import_ui::AnimationDialog>,
    pub character: Option<u64>,
    pub character_joint: usize,
    pub move_character: bool,
    pub auto_key_3d: bool,
    pub character_job:
        Option<std::sync::mpsc::Receiver<Result<rebonebase::character3d::Character, String>>>,
    pub animation_preset: rebonebase::animation_templates::Preset,
    pub animation_strength: f32,
    pub open: bool,
    pub tab: Tab,
    pub gait: WalkSettings,
    pub chain_length: u32,
    pub loop_frames: u32,
    pub velocity: bool,
    pub transition_to: usize,
    pub transition_frames: u32,
    pub transition_preview: bool,
    pub transition_weight: f32,
    pub layer_clip: usize,
    pub asset: Option<u64>,
    pub prop: Option<u64>,
    pub prop_parent: Option<u32>,
    pub last_prop: Option<u64>,
    pub mesh: Option<u64>,
    pub face: Option<u64>,
    pub actor: Option<u64>,
    pub region: Option<u64>,
    pub look: usize,
    pub mesh_columns: u32,
    pub mesh_rows: u32,
    pub mesh_origin: Vec2,
    pub mesh_size: Vec2,
    pub mesh_vertex: usize,
    pub mesh_animate: bool,
    pub pixel_color: [u8; 4],
    pub brush: u32,
    pub paint_mode: PaintMode,
    pub last_pixel: Option<Vec2>,
    pub show_boxes: bool,
    pub box_kind: production::BoxKind,
    pub box_attached: bool,
    pub event_name: String,
    pub event_payload: String,
    pub player: production::audio::Player,
    pub audition: bool,
    pub audio_until: Option<Instant>,
    pub audio_error: bool,
    pub last_frame: f32,
    pub last_event: Option<(usize, u32)>,
    pub engine: EngineTarget,
    pub palette_open: bool,
    pub palette_query: String,
    pub palette_focus: bool,
    pub binding_action: Action,
    pub binding_key: String,
    pub binding_ctrl: bool,
    pub binding_shift: bool,
    pub binding_alt: bool,
    pub layout_index: usize,
    pub apply_layout: bool,
    pub preview_texture: Option<egui::TextureHandle>,
    pub preview_project: Option<Project>,
    pub preview_frame: f32,
    pub preview_error: Option<String>,
}
impl Default for ProductionUi {
    fn default() -> Self {
        Self {
            character: None,
            character_joint: 0,
            move_character: false,
            auto_key_3d: true,
            character_job: None,
            animation_source: None,
            animation_dialog: None,
            animation_preset: Default::default(),
            animation_strength: 1.,
            open: false,
            tab: Tab::Motion,
            gait: Default::default(),
            chain_length: 3,
            loop_frames: 4,
            velocity: true,
            transition_to: 0,
            transition_frames: 8,
            transition_preview: false,
            transition_weight: 0.5,
            layer_clip: 0,
            asset: None,
            prop: None,
            prop_parent: None,
            last_prop: None,
            mesh: None,
            face: None,
            actor: None,
            region: None,
            look: 0,
            mesh_columns: 3,
            mesh_rows: 3,
            mesh_origin: Vec2::new(16., 16.),
            mesh_size: Vec2::new(32., 32.),
            mesh_vertex: 0,
            mesh_animate: false,
            pixel_color: [255, 255, 255, 255],
            brush: 1,
            paint_mode: Default::default(),
            last_pixel: None,
            show_boxes: true,
            box_kind: Default::default(),
            box_attached: false,
            event_name: "footstep".into(),
            event_payload: String::new(),
            player: Default::default(),
            audition: false,
            audio_until: None,
            audio_error: false,
            last_frame: 1.,
            last_event: None,
            engine: Default::default(),
            palette_open: false,
            palette_query: String::new(),
            palette_focus: false,
            binding_action: Action::MoveRig,
            binding_key: "M".into(),
            binding_ctrl: false,
            binding_shift: false,
            binding_alt: false,
            layout_index: 0,
            apply_layout: false,
            preview_texture: None,
            preview_project: None,
            preview_frame: 0.,
            preview_error: None,
        }
    }
}
impl ReboneApp {
    pub(super) fn production_ui(&mut self, ui: &mut egui::Ui) {
        let before = self.project.clone();
        ui.horizontal_wrapped(|ui| {
            for tab in Tab::ALL {
                ui.selectable_value(&mut self.production.tab, tab, tab.label());
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("production_scroll")
            .show(ui, |ui| match self.production.tab {
                Tab::Motion => self.production_motion(ui),
                Tab::Characters => self.characters_ui(ui),
                Tab::Props => self.production_props(ui),
                Tab::Meshes => self.production_meshes(ui),
                Tab::Looks => self.production_looks(ui),
                Tab::Face => self.production_face(ui),
                Tab::Scene => self.production_scene(ui),
                Tab::AudioEvents => self.production_audio_events(ui),
                Tab::PixelsBoxes => self.production_pixels_boxes(ui),
                Tab::Export => self.production_export(ui),
                Tab::Workspace => self.production_workspace(ui),
            });
        if self.project != before {
            match self.project.validate() {
                Ok(()) => {
                    self.history.record(&before);
                    self.changed();
                }
                Err(e) => {
                    self.project = before;
                    self.status = format!("Change rejected: {e}");
                }
            }
        }
    }
    pub(super) fn production_window(&mut self, ctx: &egui::Context) {
        if !self.production.open {
            return;
        }
        let layout = self.studio.preferences.workspace.production.clone();
        if layout.dock != Dock::Floating {
            return;
        }
        let mut open = true;
        let mut window = egui::Window::new("Production tools")
            .open(&mut open)
            .default_pos([layout.x, layout.y])
            .default_size([layout.width, layout.height])
            .max_height((ctx.content_rect().height() - 40.).max(200.));
        if self.production.apply_layout {
            window = window
                .current_pos([layout.x, layout.y])
                .fixed_size([layout.width, layout.height]);
        }
        if let Some(response) = window.show(ctx, |ui| self.production_ui(ui)) {
            let p = &mut self.studio.preferences.workspace.production;
            p.x = response.response.rect.left();
            p.y = response.response.rect.top();
            p.width = response.response.rect.width().max(160.);
            p.height = response.response.rect.height().max(120.);
        }
        self.production.open = open;
    }
    pub(super) fn art_picker(&mut self, ui: &mut egui::Ui) {
        let name = self
            .production
            .asset
            .and_then(|id| {
                self.project
                    .extras
                    .production
                    .assets
                    .iter()
                    .find(|a| a.id == id)
            })
            .map_or("Choose image", |a| a.name.as_str())
            .to_string();
        egui::ComboBox::from_label("Artwork")
            .selected_text(name)
            .show_ui(ui, |ui| {
                for a in &self.project.extras.production.assets {
                    ui.selectable_value(&mut self.production.asset, Some(a.id), &a.name);
                }
            });
        if ui.button("Import artwork PNG / JPEG…").clicked() {
            self.import_artwork();
        }
    }
    fn import_artwork(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Artwork", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        let result = (|| -> anyhow::Result<u64> {
            let image = project::import_reference(&path)?;
            let p = &mut self.project.extras.production;
            let id = p.next_id();
            p.assets.push(production::SpriteAsset {
                id,
                name: path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                image,
            });
            Ok(id)
        })();
        match result {
            Ok(id) => {
                self.production.asset = Some(id);
                self.status = "Artwork imported".into();
            }
            Err(e) => self.status = e.to_string(),
        }
    }
    pub(super) fn art_preview(&mut self, ui: &mut egui::Ui) -> Rect {
        if self.production.preview_project.as_ref() != Some(&self.project)
            || self.production.preview_frame != self.frame
        {
            match self.project.try_render_at(self.frame) {
                Ok(image) => {
                    self.production.preview_error = None;
                    self.production.preview_texture = Some(ui.ctx().load_texture(
                        "production_preview",
                        egui::ColorImage::from_rgba_unmultiplied(
                            [image.width() as usize, image.height() as usize],
                            image.as_raw(),
                        ),
                        egui::TextureOptions::NEAREST,
                    ));
                }
                Err(error) => {
                    self.production.preview_texture = None;
                    self.production.preview_error = Some(format!("Preview failed: {error:#}"));
                }
            }
            self.production.preview_project = Some(self.project.clone());
            self.production.preview_frame = self.frame;
        }
        if let Some(error) = &self.production.preview_error {
            ui.colored_label(Color32::YELLOW, error);
        }
        let scale = (ui.available_width().min(500.) / self.project.rig.width as f32)
            .min(300. / self.project.rig.height as f32);
        let (rect, _) = ui.allocate_exact_size(
            EVec2::new(
                self.project.rig.width as f32,
                self.project.rig.height as f32,
            ) * scale,
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 3., Color32::from_rgb(32, 40, 48));
        if let Some(tex) = &self.production.preview_texture {
            painter.image(tex.id(), rect, unit_rect(), Color32::WHITE);
        }
        rect
    }
    pub(super) fn production_tick(&mut self, ctx: &egui::Context) {
        self.poll_character_import(ctx);
        let (time, reverse) = if self.playing {
            production::audio::playback_cursor(
                self.project.rig.active_clip(),
                self.studio.playback_ms,
            )
        } else {
            (
                self.project.rig.active_clip().source_time_ms(self.frame),
                false,
            )
        };
        let audition_active = self
            .production
            .audio_until
            .is_some_and(|until| Instant::now() < until);
        if !audition_active {
            self.production.audio_until = None;
            if let Err(e) = self.production.player.sync_direction(
                self.project.extras.production.audio.as_ref(),
                time,
                self.playing && !self.production.audio_error,
                reverse,
            ) && !self.production.audio_error
            {
                self.status = e.to_string();
                self.production.audio_error = true;
            }
        }
        if !self.playing
            && self.production.audition
            && self.production.last_frame != self.frame
            && let Some(audio) = &self.project.extras.production.audio
        {
            let _ = self.production.player.play(audio, time, 160);
            self.production.audio_until = Some(Instant::now() + Duration::from_millis(160));
            ctx.request_repaint_after(Duration::from_millis(170));
        }
        let at = (self.project.rig.active_clip, self.frame as u32);
        if self.playing && self.production.last_event != Some(at) {
            let names: Vec<_> = self
                .project
                .extras
                .production
                .events
                .iter()
                .filter(|e| e.clip == at.0 && (e.frame - self.frame).abs() < 0.5)
                .map(|e| e.name.as_str())
                .collect();
            if !names.is_empty() {
                self.status = format!("Event · {}", names.join(", "));
            }
        }
        self.production.last_event = Some(at);
        self.production.last_frame = self.frame;
    }
    pub(super) fn reset_production_document(&mut self) {
        self.production.character = None;
        self.production.move_character = false;
        self.production.character_joint = 0;
        self.production.character_job = None;
        self.production.animation_source = None;
        self.production.animation_dialog = None;
        self.production.player.stop();
        self.production.prop = None;
        self.production.last_prop = None;
        self.production.mesh = None;
        self.production.face = None;
        self.production.actor = None;
        self.production.region = None;
        self.production.asset = None;
        self.production.preview_project = None;
        self.production.transition_preview = false;
        self.production.last_pixel = None;
        self.production.last_event = None;
    }
}
pub(super) fn bone_combo(ui: &mut egui::Ui, rig: &Rig, label: &str, value: &mut Option<u32>) {
    egui::ComboBox::from_label(label)
        .selected_text(
            value
                .and_then(|id| rig.bone(id))
                .map_or("World", |b| b.name.as_str()),
        )
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "World");
            for b in &rig.bones {
                ui.selectable_value(value, Some(b.id), &b.name);
            }
        });
}
pub(super) fn clip_combo(ui: &mut egui::Ui, rig: &Rig, label: &str, value: &mut usize) {
    *value = (*value).min(rig.clips.len() - 1);
    egui::ComboBox::from_label(label)
        .selected_text(&rig.clips[*value].name)
        .show_ui(ui, |ui| {
            for (i, c) in rig.clips.iter().enumerate() {
                ui.selectable_value(value, i, &c.name);
            }
        });
}
