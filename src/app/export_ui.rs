//! The Export window and every export entry point. Menus, Studio and
//! Production all start the same background job, so one progress bar and
//! Cancel button cover every export.
use super::*;
use rebonebase::{
    atlas::{self, AtlasOptions},
    export::{self, ExportRequest, ExportSummary, Target},
    jobs::{Job, Outcome},
    production::engine_export::EngineTarget,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Kind {
    Frame,
    IndexedFrame,
    #[default]
    Sheet,
    IndexedSheet,
    Gif,
    Aseprite,
    AsepriteAll,
    Batch,
    Engine,
    Directions,
}
impl Kind {
    pub const SINGLE: [Self; 7] = [
        Self::Frame,
        Self::IndexedFrame,
        Self::Sheet,
        Self::IndexedSheet,
        Self::Gif,
        Self::Aseprite,
        Self::AsepriteAll,
    ];
    pub const FOLDER: [Self; 3] = [Self::Batch, Self::Engine, Self::Directions];
    pub fn label(self) -> &'static str {
        match self {
            Self::Frame => "Current frame · PNG",
            Self::IndexedFrame => "Current frame · indexed PNG",
            Self::Sheet => "Sprite sheet · PNG",
            Self::IndexedSheet => "Sprite sheet · indexed PNG",
            Self::Gif => "Animation · GIF",
            Self::Aseprite => "Layered animation · Aseprite",
            Self::AsepriteAll => "All clips · Aseprite tags",
            Self::Batch => "Batch preset folder",
            Self::Engine => "Game engine package",
            Self::Directions => "Directional set · 4 or 8 views",
        }
    }
}

pub(super) struct ExportJob {
    pub job: Job<ExportSummary>,
    pub destination: PathBuf,
}

#[derive(Default)]
pub(super) struct Exports {
    pub open: bool,
    pub kind: Kind,
    /// Folder targets: replace files that already exist.
    pub overwrite: bool,
    pub job: Option<ExportJob>,
    /// Outcome of the last finished export, and whether it succeeded.
    pub last: Option<(bool, String)>,
    pub directions: super::directions_ui::DirectionsUi,
}

impl ReboneApp {
    pub(super) fn export_target(&self, kind: Kind) -> Target {
        match kind {
            Kind::Frame => Target::Frame { indexed: false },
            Kind::IndexedFrame => Target::Frame { indexed: true },
            Kind::Sheet => Target::Sheet {
                columns: self.sheet_cols,
                indexed: false,
            },
            Kind::IndexedSheet => Target::Sheet {
                columns: self.sheet_cols,
                indexed: true,
            },
            Kind::Gif => Target::Gif,
            Kind::Aseprite => Target::Aseprite { all_clips: false },
            Kind::AsepriteAll => Target::Aseprite { all_clips: true },
            Kind::Batch => Target::Batch(self.studio.batch.clone()),
            Kind::Engine => {
                Target::Engine(self.production.engine, self.studio.preferences.engine_atlas)
            }
            Kind::Directions => Target::Directions(Box::new(self.direction_export())),
        }
    }
    pub(super) fn open_export(&mut self, kind: Kind) {
        self.exports.kind = kind;
        self.exports.open = true;
    }
    /// Ask for a destination, then export in the background.
    pub(super) fn export_kind(&mut self, kind: Kind) {
        if self.exports.job.is_some() {
            self.status = "Wait for the current export to finish or cancel it".into();
            return;
        }
        let target = self.export_target(kind);
        let destination = match target.extension() {
            None => rfd::FileDialog::new().pick_folder(),
            Some(ext) => rfd::FileDialog::new()
                .add_filter(ext, &[ext])
                .set_file_name(format!("{}.{ext}", self.project.rig.name))
                .save_file(),
        };
        if let Some(destination) = destination {
            self.start_export(target, destination);
        }
    }
    pub(super) fn start_export(&mut self, target: Target, destination: PathBuf) {
        if self.exports.job.is_some() {
            self.status = "Wait for the current export to finish or cancel it".into();
            return;
        }
        let label = target.label();
        let request = ExportRequest {
            target,
            destination: destination.clone(),
            overwrite: self.exports.overwrite,
            frame: self.frame,
            pose: self.pose(),
        };
        // The job owns a snapshot; editing continues while it renders.
        let project = self.project.clone();
        self.status = format!("Exporting {label}…");
        self.exports.last = None;
        self.exports.job = Some(ExportJob {
            job: Job::spawn(label, move |progress| {
                export::run(&project, &request, progress)
            }),
            destination,
        });
    }
    pub(super) fn poll_export(&mut self, ctx: &egui::Context) {
        let Some(running) = &self.exports.job else {
            return;
        };
        let Some(outcome) = running.job.poll() else {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        };
        let destination = running.destination.clone();
        self.exports.job = None;
        let result = match outcome {
            Outcome::Finished(summary) => (true, summary.describe(&destination)),
            Outcome::Failed(e) => (false, format!("Export failed: {e}")),
            Outcome::Cancelled => (false, "Export cancelled · no files were written".into()),
        };
        self.status = result.1.clone();
        self.exports.last = Some(result);
    }
    /// Progress bar and Cancel button; draws nothing when no export runs.
    pub(super) fn export_progress(&mut self, ui: &mut egui::Ui, width: f32) {
        let Some(running) = &self.exports.job else {
            return;
        };
        let p = running.job.progress();
        let stage = if p.cancelling {
            "Cancelling…".to_string()
        } else if p.stage.is_empty() {
            running.job.label().to_string()
        } else {
            p.stage.clone()
        };
        let text = match p.fraction() {
            Some(_) if p.total > 0 => format!("{stage} · {}/{}", p.done.min(p.total), p.total),
            _ => stage,
        };
        let mut cancel = false;
        ui.horizontal(|ui| {
            let bar = egui::ProgressBar::new(p.fraction().unwrap_or(0.))
                .desired_width(width)
                .text(RichText::new(text).small())
                .animate(p.fraction().is_none());
            ui.add(bar).on_hover_text(format!(
                "{:.1} s elapsed",
                running.job.elapsed().as_secs_f32()
            ));
            cancel = ui
                .add_enabled(
                    p.cancellable && !p.cancelling,
                    egui::Button::new("Cancel").small(),
                )
                .on_hover_text("Stops before any file is written")
                .on_disabled_hover_text("Files are being written; the export will finish")
                .clicked();
        });
        if cancel {
            running.job.cancel();
        }
    }
    pub(super) fn export_window(&mut self, ctx: &egui::Context) {
        if !self.exports.open {
            return;
        }
        let mut open = true;
        egui::Window::new("Export")
            .open(&mut open)
            .default_width(380.)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} · {} × {} px · {} clips",
                        self.project.rig.name,
                        self.project.rig.width,
                        self.project.rig.height,
                        self.project.rig.clips.len()
                    ))
                    .small()
                    .color(DIM),
                );
                ui.add_space(4.);
                ui.label(RichText::new("Single file").strong());
                for kind in Kind::SINGLE {
                    ui.selectable_value(&mut self.exports.kind, kind, kind.label());
                }
                ui.add_space(4.);
                ui.label(RichText::new("Folder").strong());
                for kind in Kind::FOLDER {
                    ui.selectable_value(&mut self.exports.kind, kind, kind.label());
                }
                ui.separator();
                self.export_options(ui);
                let target = self.export_target(self.exports.kind);
                let frames = target.frame_count(&self.project);
                ui.label(
                    RichText::new(format!(
                        "{frames} frame{} · repeated frames are rendered once",
                        if frames == 1 { "" } else { "s" }
                    ))
                    .small()
                    .color(DIM),
                );
                ui.add_space(4.);
                let destination = if target.writes_folder() {
                    "Choose folder and export…"
                } else {
                    "Choose file and export…"
                };
                if ui
                    .add_enabled(
                        self.exports.job.is_none(),
                        egui::Button::new(RichText::new(destination).color(ACCENT)),
                    )
                    .clicked()
                {
                    self.export_kind(self.exports.kind);
                }
                self.export_progress(ui, 260.);
                if let Some((ok, message)) = &self.exports.last {
                    let color = if *ok {
                        ACCENT
                    } else {
                        Color32::from_rgb(255, 150, 140)
                    };
                    ui.label(RichText::new(message).small().color(color));
                }
            });
        self.exports.open = open;
    }
    fn export_options(&mut self, ui: &mut egui::Ui) {
        match self.exports.kind {
            Kind::Sheet | Kind::IndexedSheet => {
                ui.add(
                    egui::DragValue::new(&mut self.sheet_cols)
                        .range(1..=32)
                        .prefix("Sheet columns "),
                );
            }
            Kind::IndexedFrame => {
                ui.label(
                    RichText::new("Colors map to the project palette (Studio → Appearance).")
                        .small()
                        .color(DIM),
                );
            }
            Kind::Batch => {
                self.batch_preset_editor(ui);
                ui.checkbox(&mut self.exports.overwrite, "Replace existing files");
            }
            Kind::Engine => {
                let before = self.project.clone();
                self.engine_options(ui);
                self.commit_studio_edit(before);
                ui.checkbox(
                    &mut self.exports.overwrite,
                    "Replace existing package files",
                );
            }
            Kind::Directions => {
                let ctx = ui.ctx().clone();
                egui::ScrollArea::vertical()
                    .max_height(420.)
                    .show(ui, |ui| self.direction_options(ui, &ctx));
                ui.checkbox(
                    &mut self.exports.overwrite,
                    "Replace existing package files",
                );
            }
            Kind::Frame | Kind::Gif | Kind::Aseprite | Kind::AsepriteAll => {}
        }
    }
    /// Engine choice, sprite pivot and page layout. Callers commit the pivot
    /// edit to history; the layout is saved with the preferences.
    pub(super) fn engine_options(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for engine in [
                EngineTarget::Godot,
                EngineTarget::Unity,
                EngineTarget::Generic,
            ] {
                ui.selectable_value(&mut self.production.engine, engine, format!("{engine:?}"));
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.project.extras.production.pivot.x)
                    .range(0. ..=1.)
                    .speed(0.01)
                    .prefix("Pivot X "),
            );
            ui.add(
                egui::DragValue::new(&mut self.project.extras.production.pivot.y)
                    .range(0. ..=1.)
                    .speed(0.01)
                    .prefix("Y "),
            );
        });
        if atlas_options(ui, &mut self.studio.preferences.engine_atlas) {
            self.save_preferences();
        }
    }
}

/// Packing controls shared by batch presets and engine packages. Returns
/// whether anything changed.
pub(super) fn atlas_options(ui: &mut egui::Ui, options: &mut AtlasOptions) -> bool {
    let before = *options;
    ui.checkbox(&mut options.packed, "Pack frames into atlas pages")
        .on_hover_text("Trim, reuse and pack frames instead of one full-frame grid per clip");
    if options.packed {
        ui.indent("atlas", |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut options.trim, "Trim transparent borders");
                ui.checkbox(&mut options.reuse, "Reuse identical frames")
                    .on_hover_text("Identical images share pixels; each frame keeps its own timing, events and regions");
            });
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut options.padding)
                        .range(0..=atlas::MAX_PADDING)
                        .prefix("Padding "),
                );
                ui.add(
                    egui::DragValue::new(&mut options.extrude)
                        .range(0..=atlas::MAX_EXTRUDE)
                        .prefix("Extrude "),
                )
                .on_hover_text("Repeat edge pixels to prevent filtering seams");
            });
            egui::ComboBox::from_label("Maximum texture size")
                .selected_text(format!("{0} × {0}", options.max_size))
                .show_ui(ui, |ui| {
                    for size in [256, 512, 1024, 2048, 4096] {
                        ui.selectable_value(&mut options.max_size, size, format!("{size} × {size}"));
                    }
                });
            ui.label(
                RichText::new("Frames that do not fit continue on additional pages.")
                    .small()
                    .color(DIM),
            );
        });
    }
    *options != before
}
