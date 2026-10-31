use super::*;
use rebonebase::{
    batch::ExportPreset,
    editing::{KeyClipboard, KeySelection},
    motion::RetargetProfile,
    references::VideoOptions,
    storage::{LibraryEntry, Recovery, Store},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    sync::mpsc,
};

#[derive(Clone, Copy, PartialEq, Default)]
pub(super) enum Tab {
    #[default]
    Rig,
    Animation,
    Appearance,
    Library,
    References,
    Files,
    Export,
}
pub(super) enum JobResult {
    References(rebonebase::features::ReferenceSequence),
    Export(usize),
}
pub(super) struct Job {
    pub receiver: mpsc::Receiver<Result<JobResult, String>>,
    pub generation: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct Preferences {
    pub workspace: rebonebase::workspace::Workspace,
    pub workspaces: Vec<rebonebase::workspace::Workspace>,
    pub bindings: Vec<rebonebase::workspace::Binding>,
    pub autosave_seconds: u64,
    pub video: VideoOptions,
    pub presets: Vec<ExportPreset>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            workspace: Default::default(),
            workspaces: vec![Default::default()],
            bindings: rebonebase::workspace::default_bindings(),
            autosave_seconds: 30,
            video: Default::default(),
            presets: vec![Default::default()],
        }
    }
}
pub(super) struct KeyDrag {
    pub base: Project,
    pub selected: Vec<KeySelection>,
    pub start: f32,
    pub offset: f32,
    pub recorded: bool,
}
pub(super) struct CurveDrag {
    pub base: Project,
    pub key: KeySelection,
    pub handle: i8,
    pub recorded: bool,
}
pub(super) struct Studio {
    pub open: bool,
    pub tab: Tab,
    pub selection: BTreeSet<u32>,
    pub last_primary: Option<u32>,
    pub hover: Option<u32>,
    pub pick_cycle: usize,
    pub pick_ids: Vec<u32>,
    pub marquee: Option<(Vec2, Vec2)>,
    pub group_delta: Vec2,
    pub group_angle: f32,
    pub parent: Option<u32>,
    pub keys: Vec<KeySelection>,
    pub clipboard: Option<KeyClipboard>,
    pub key_drag: Option<KeyDrag>,
    pub key_offset: f32,
    pub key_scale: f32,
    pub key_snap: f32,
    pub trim_start: u32,
    pub trim_end: u32,
    pub channel: Channel,
    pub curve_bounds: (f32, f32),
    pub curve_drag: Option<CurveDrag>,
    pub curve_history_frame: bool,
    pub store: Store,
    pub preferences: Preferences,
    pub session: String,
    pub last_autosave: Instant,
    pub autosaved: Option<Project>,
    pub recoveries: Vec<Recovery>,
    pub recovered: bool,
    pub library: Vec<LibraryEntry>,
    pub search: String,
    pub favorites: bool,
    pub library_name: String,
    pub library_tags: String,
    pub thumbnails: HashMap<String, egui::TextureHandle>,
    pub profiles: Vec<RetargetProfile>,
    pub batch: ExportPreset,
    pub preset_index: usize,
    pub overwrite: bool,
    pub job: Option<Job>,
    pub generation: u64,
    pub ref_index: Option<usize>,
    pub show_path: bool,
    pub playback_ms: f64,
}
impl Default for Studio {
    fn default() -> Self {
        Self {
            open: false,
            tab: Tab::Rig,
            selection: BTreeSet::new(),
            last_primary: None,
            hover: None,
            pick_cycle: 0,
            pick_ids: vec![],
            marquee: None,
            group_delta: Vec2::default(),
            group_angle: 0.,
            parent: None,
            keys: vec![],
            clipboard: None,
            key_drag: None,
            key_offset: 1.,
            key_scale: 1.,
            key_snap: 1.,
            trim_start: 1,
            trim_end: 12,
            channel: Channel::Rotation,
            curve_bounds: (-180., 180.),
            curve_drag: None,
            curve_history_frame: false,
            store: if cfg!(test) {
                Store::at(std::env::temp_dir().join(format!(
                    "rebonebase-ui-test-{}",
                    rebonebase::storage::unique()
                )))
            } else {
                Store::user()
            },
            preferences: Default::default(),
            session: format!(
                "session-{}-{}",
                std::process::id(),
                rebonebase::storage::unique()
            ),
            last_autosave: Instant::now(),
            autosaved: None,
            recoveries: vec![],
            recovered: false,
            library: vec![],
            search: String::new(),
            favorites: false,
            library_name: "My rig".into(),
            library_tags: String::new(),
            thumbnails: HashMap::new(),
            profiles: vec![],
            batch: Default::default(),
            preset_index: 0,
            overwrite: false,
            job: None,
            generation: 0,
            ref_index: None,
            show_path: false,
            playback_ms: 0.,
        }
    }
}

impl ReboneApp {
    pub(super) fn init_services(&mut self) {
        let store = &self.studio.store;
        if let Ok(bytes) = std::fs::read(store.root.join("preferences.json"))
            && bytes.len() <= 1024 * 1024
            && let Ok(p) = serde_json::from_slice::<Preferences>(&bytes)
        {
            self.studio.preferences = p;
        }
        self.studio.preferences.autosave_seconds =
            self.studio.preferences.autosave_seconds.clamp(10, 600);
        if self.studio.preferences.workspace.validate().is_err() {
            self.studio.preferences.workspace = Default::default();
        }
        self.studio
            .preferences
            .workspaces
            .retain(|w| w.validate().is_ok());
        self.studio.preferences.workspaces.truncate(32);
        if rebonebase::workspace::validate_bindings(&self.studio.preferences.bindings).is_err() {
            self.studio.preferences.bindings = rebonebase::workspace::default_bindings();
        }
        match store.recoveries_report() {
            Ok(report) => {
                self.studio.recoveries = report.recoveries;
                if !report.warnings.is_empty() {
                    self.status = format!("Recovery: {}", report.warnings.join("; "));
                }
            }
            Err(error) => self.status = format!("Recovery discovery failed: {error:#}"),
        }
        self.studio.profiles = store.read_records("profiles").unwrap_or_default();
        if !self.studio.recoveries.is_empty() {
            self.studio.open = true;
            self.studio.tab = Tab::Files;
        }
        self.studio.preferences.presets.truncate(32);
        if self.studio.preferences.video.ffmpeg == std::path::Path::new("ffmpeg") {
            let bundled = PathBuf::from(r"C:\Program Files\Krita (x64)\bin\ffmpeg.exe");
            if bundled.is_file() {
                self.studio.preferences.video.ffmpeg = bundled;
            }
        }
        self.refresh_library();
    }
    pub(super) fn refresh_library(&mut self) {
        match self.studio.store.search_library("", false) {
            Ok(entries) => self.studio.library = entries,
            Err(e) => self.status = format!("Library could not be read: {e}"),
        }
    }
    pub(super) fn select_bone(&mut self, id: u32, additive: bool) {
        if additive {
            if !self.studio.selection.insert(id) {
                self.studio.selection.remove(&id);
            }
        } else {
            self.studio.selection.clear();
            self.studio.selection.insert(id);
        }
        self.selected = if self.studio.selection.contains(&id) {
            Some(id)
        } else {
            self.studio.selection.iter().next().copied()
        };
        self.studio.last_primary = self.selected;
        self.studio.parent = self
            .selected
            .and_then(|id| self.project.rig.bone(id))
            .and_then(|b| b.parent);
    }
    pub(super) fn selected_bones(&self) -> Vec<u32> {
        if self.studio.selection.is_empty() {
            self.selected.into_iter().collect()
        } else {
            self.studio.selection.iter().copied().collect()
        }
    }
    pub(super) fn reset_studio_document(&mut self) {
        self.studio.selection.clear();
        self.studio.last_primary = None;
        self.studio.keys.clear();
        self.studio.key_drag = None;
        self.studio.curve_drag = None;
        self.studio.marquee = None;
        self.studio.recovered = false;
        self.studio.autosaved = None;
        self.studio.generation += 1;
        self.studio.ref_index = None;
        self.studio.playback_ms = 0.;
    }
    pub(super) fn tick_services(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.studio.job.take() {
            match job.receiver.try_recv() {
                Ok(Ok(JobResult::References(mut seq))) => {
                    if job.generation == Some(self.studio.generation) {
                        let scale = (self.project.rig.width as f32 / seq.frames[0].width as f32)
                            .min(self.project.rig.height as f32 / seq.frames[0].height as f32);
                        for frame in &mut seq.frames {
                            frame.scale = scale;
                        }
                        self.record();
                        self.project.extras.references = Some(seq);
                        self.ref_tex = None;
                        self.studio.ref_index = None;
                        self.status = "Synchronized reference sequence imported".into();
                    } else {
                        self.status = "Reference import finished for the previous project".into();
                    }
                }
                Ok(Ok(JobResult::Export(n))) => {
                    self.status = format!("Batch export complete · {n} files")
                }
                Ok(Err(e)) => self.status = e,
                Err(mpsc::TryRecvError::Empty) => {
                    self.studio.job = Some(job);
                    ctx.request_repaint_after(Duration::from_millis(40));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status = "Background task ended unexpectedly".into()
                }
            }
        }
        if self.dirty()
            && self.studio.last_autosave.elapsed().as_secs()
                >= self.studio.preferences.autosave_seconds
            && self.studio.autosaved.as_ref() != Some(&self.project)
        {
            self.studio.last_autosave = Instant::now();
            match self.studio.store.write_recovery(
                &self.studio.session,
                &self.project,
                self.path.clone(),
            ) {
                Ok(()) => self.studio.autosaved = Some(self.project.clone()),
                Err(e) => self.status = format!("Recovery snapshot could not be saved: {e}"),
            }
        }
        ctx.request_repaint_after(Duration::from_secs(
            self.studio.preferences.autosave_seconds,
        ));
    }
    pub(super) fn start_reference_job(&mut self, video: bool) {
        if self.studio.job.is_some() {
            self.status = "Wait for the current background task".into();
            return;
        }
        let (sender, receiver) = mpsc::channel();
        if video {
            let Some(path) = rfd::FileDialog::new()
                .add_filter(
                    "Video reference",
                    &["mp4", "mov", "mkv", "webm", "avi", "gif"],
                )
                .pick_file()
            else {
                return;
            };
            let options = self.studio.preferences.video.clone();
            std::thread::spawn(move || {
                let result = rebonebase::references::video_frames(&path, &options)
                    .map(JobResult::References)
                    .map_err(|e| format!("{e:#}"));
                let _ = sender.send(result);
            });
        } else {
            let Some(paths) = rfd::FileDialog::new()
                .add_filter("Reference sequence", &["png", "jpg", "jpeg"])
                .pick_files()
            else {
                return;
            };
            let fps = self.studio.preferences.video.fps;
            std::thread::spawn(move || {
                let result = rebonebase::references::load_sequence(&paths, fps)
                    .map(JobResult::References)
                    .map_err(|e| format!("{e:#}"));
                let _ = sender.send(result);
            });
        }
        self.studio.job = Some(Job {
            receiver,
            generation: Some(self.studio.generation),
        });
        self.status = "Importing reference frames…".into();
    }
    pub(super) fn start_batch(&mut self) {
        if self.studio.job.is_some() {
            self.status = "Wait for the current background task".into();
            return;
        }
        let Some(folder) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let project = self.project.clone();
        let preset = self.studio.batch.clone();
        let overwrite = self.studio.overwrite;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = rebonebase::batch::export(&project, &folder, &preset, overwrite)
                .map(|r| JobResult::Export(r.files.len()))
                .map_err(|e| format!("{e:#}"));
            let _ = sender.send(result);
        });
        self.studio.job = Some(Job {
            receiver,
            generation: None,
        });
        self.status = "Rendering batch export…".into();
    }
    pub(super) fn studio_window(&mut self, ctx: &egui::Context) {
        if !self.studio.open {
            return;
        }
        let mut open = true;
        let before = self.project.clone();
        self.studio.curve_history_frame = false;
        egui::Window::new("Studio tools")
            .open(&mut open)
            .default_size([860., 600.])
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (tab, name) in [
                        (Tab::Rig, "Rig & pins"),
                        (Tab::Animation, "Animation"),
                        (Tab::Appearance, "Appearance"),
                        (Tab::Library, "Library"),
                        (Tab::References, "References"),
                        (Tab::Files, "Recovery & files"),
                        (Tab::Export, "Batch export"),
                    ] {
                        ui.selectable_value(&mut self.studio.tab, tab, name);
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("studio_scroll")
                    .show(ui, |ui| match self.studio.tab {
                        Tab::Rig => self.studio_rig(ui),
                        Tab::Animation => self.studio_animation(ui),
                        Tab::Appearance => self.studio_appearance(ui),
                        Tab::Library => self.studio_library(ui, ctx),
                        Tab::References => self.studio_references(ui),
                        Tab::Files => self.studio_files(ui),
                        Tab::Export => self.studio_export(ui),
                    });
            });
        self.studio.open = open;
        self.commit_studio_edit(before);
    }
    pub(super) fn commit_studio_edit(&mut self, before: Project) {
        if self.project != before {
            if let Err(e) = self.project.validate() {
                self.project = before;
                self.status = format!("Change rejected: {e}");
                return;
            }
            if !self.studio.curve_history_frame {
                self.history.record(&before);
            }
            self.changed();
        }
    }
    pub(super) fn save_preferences(&mut self) {
        let result = (|| -> anyhow::Result<()> {
            std::fs::create_dir_all(&self.studio.store.root)?;
            self.studio.store.write_json(
                &self.studio.store.root.join("preferences.json"),
                &self.studio.preferences,
            )
        })();
        if let Err(e) = result {
            self.status = e.to_string();
        }
    }
    pub(super) fn timeline_ms(&self) -> u64 {
        self.project.rig.active_clip().source_time_ms(self.frame)
    }
}
