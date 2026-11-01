use super::*;
use rebonebase::{batch::ExportPreset, features, project::SavedPose, storage};

impl ReboneApp {
    pub(super) fn studio_appearance(&mut self, ui: &mut egui::Ui) {
        ui.heading("Onion skins, outlines and palettes");
        ui.checkbox(&mut self.onion, "Enable onion skin in Animate mode");
        let onion = &mut self.project.extras.onion;
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut onion.before)
                    .range(0..=8)
                    .prefix("Before "),
            );
            ui.add(
                egui::DragValue::new(&mut onion.after)
                    .range(0..=8)
                    .prefix("After "),
            );
            ui.add(
                egui::DragValue::new(&mut onion.spacing)
                    .range(1..=24)
                    .prefix("Spacing "),
            );
            ui.add(egui::Slider::new(&mut onion.opacity, 0. ..=1.).text("Opacity"));
        });
        ui.horizontal(|ui| {
            ui.label("Previous tint");
            ui.color_edit_button_srgb(&mut onion.before_color);
            ui.label("Next tint");
            ui.color_edit_button_srgb(&mut onion.after_color);
        });
        ui.separator();
        ui.label(RichText::new("Group outlines").strong());
        for part in &mut self.project.rig.parts {
            ui.horizontal(|ui| {
                ui.add_sized([120., 20.], egui::Label::new(&part.name));
                ui.color_edit_button_srgba_unmultiplied(&mut part.color);
                ui.add(
                    egui::DragValue::new(&mut part.outline_width)
                        .range(0..=16)
                        .prefix("Width "),
                );
                ui.color_edit_button_srgba_unmultiplied(&mut part.outline_color);
            });
        }
        ui.label(RichText::new("Outlines are rendered as pixel edges and included in PNG, GIF and layered exports.").small().color(DIM));
        ui.separator();
        ui.label(RichText::new("Shared palette").strong());
        ui.horizontal(|ui| {
            if ui.button("Capture group colors").clicked() {
                for part in &self.project.rig.parts {
                    for color in [part.color, part.outline_color] {
                        if self.project.extras.palette.len() < 256
                            && !self.project.extras.palette.contains(&color)
                        {
                            self.project.extras.palette.push(color);
                        }
                    }
                }
            }
            if ui.button("Add swatch").clicked() && self.project.extras.palette.len() < 256 {
                self.project.extras.palette.push([255, 255, 255, 255]);
            }
            if ui.button("Remove last").clicked() {
                self.project.extras.palette.pop();
            }
        });
        let mut apply = None;
        ui.horizontal_wrapped(|ui| {
            for (i, color) in self.project.extras.palette.iter_mut().enumerate() {
                ui.vertical(|ui| {
                    ui.color_edit_button_srgba_unmultiplied(color);
                    if ui.small_button(format!("Apply {}", i + 1)).clicked() {
                        apply = Some(*color);
                    }
                });
            }
        });
        if let Some(color) = apply {
            let selected = self.selected_bones();
            let parts: std::collections::BTreeSet<_> = selected
                .iter()
                .filter_map(|id| self.project.rig.bone(*id).map(|b| b.part))
                .collect();
            for part in &mut self.project.rig.parts {
                if parts.contains(&part.id) {
                    part.color = color;
                }
            }
        }
        ui.label("Apply a swatch to the groups of selected bones. Indexed PNG maps pixels to this palette and preserves transparency.");
    }
    pub(super) fn studio_library(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.heading("Cross-project rig and pose library");
        ui.horizontal(|ui| {
            ui.label("Search");
            ui.text_edit_singleline(&mut self.studio.search);
            ui.checkbox(&mut self.studio.favorites, "Favorites only");
            if ui.button("Refresh").clicked() {
                self.refresh_library();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.studio.library_name);
            ui.label("Tags");
            ui.text_edit_singleline(&mut self.studio.library_tags);
        });
        ui.horizontal(|ui| {
            if ui.button("Save rig to library").clicked() {
                let tags = self
                    .studio
                    .library_tags
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                match self.studio.store.save_library(
                    &self.studio.library_name,
                    &self.project,
                    tags,
                    false,
                ) {
                    Ok(_) => {
                        self.refresh_library();
                        self.status = "Rig saved to the shared library".into();
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
            if ui.button("Save current pose to library").clicked() {
                let mut p = self.project.clone();
                p.poses = vec![SavedPose {
                    name: self.studio.library_name.clone(),
                    pose: self.project.pose_at(self.frame),
                }];
                let tags = self
                    .studio
                    .library_tags
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                match self.studio.store.save_library_kind(
                    &self.studio.library_name,
                    &p,
                    tags,
                    false,
                    "Pose",
                ) {
                    Ok(_) => self.refresh_library(),
                    Err(e) => self.status = e.to_string(),
                }
            }
        });
        let query = self.studio.search.to_lowercase();
        let entries: Vec<_> = self
            .studio
            .library
            .iter()
            .filter(|e| {
                (!self.studio.favorites || e.favorite)
                    && (e.name.to_lowercase().contains(&query)
                        || e.tags.iter().any(|t| t.to_lowercase().contains(&query)))
            })
            .cloned()
            .collect();
        ui.label(format!("{} entries", entries.len()));
        egui::ScrollArea::vertical()
            .id_salt("library_items")
            .max_height(350.)
            .show_rows(ui, 86., entries.len(), |ui, range| {
                for i in range {
                    let mut entry = entries[i].clone();
                    ui.horizontal(|ui| {
                        if !self.studio.thumbnails.contains_key(&entry.id)
                            && storage::Store::safe_id(&entry.id).is_ok()
                        {
                            let path = self
                                .studio
                                .store
                                .root
                                .join("library")
                                .join(format!("{}.png", entry.id));
                            if let Ok(image) = project::import_reference(&path) {
                                self.studio.thumbnails.insert(
                                    entry.id.clone(),
                                    ctx.load_texture(
                                        format!("library-{}", entry.id),
                                        egui::ColorImage::from_rgba_unmultiplied(
                                            [image.width as usize, image.height as usize],
                                            &image.pixels,
                                        ),
                                        egui::TextureOptions::NEAREST,
                                    ),
                                );
                            }
                        }
                        if let Some(texture) = self.studio.thumbnails.get(&entry.id) {
                            ui.add(egui::Image::new(texture).fit_to_exact_size(EVec2::splat(70.)));
                        }
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(format!("{} · {}", entry.name, entry.kind)).strong(),
                            );
                            ui.label(entry.tags.join(", "));
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut entry.favorite, "Favorite").changed() {
                                    match self.studio.store.update_entry(&entry) {
                                        Ok(()) => {
                                            if let Some(e) = self
                                                .studio
                                                .library
                                                .iter_mut()
                                                .find(|e| e.id == entry.id)
                                            {
                                                e.favorite = entry.favorite;
                                            }
                                        }
                                        Err(e) => self.status = e.to_string(),
                                    }
                                }
                                if ui.button("Load rig").clicked() {
                                    match self.studio.store.load_library(&entry.id) {
                                        Ok(p) => self.request(Pending::Restore(Box::new(p))),
                                        Err(e) => self.status = e.to_string(),
                                    }
                                }
                                if ui.button("Apply pose by bone name").clicked() {
                                    match self.studio.store.load_library(&entry.id) {
                                        Ok(mut source) => {
                                            if let Some(pose) = source.poses.first().cloned() {
                                                source.rig.active_clip_mut().tracks.clear();
                                                for (id, p) in pose.pose {
                                                    for (ch, v) in [
                                                        (Channel::Rotation, p.rotation),
                                                        (Channel::X, p.x),
                                                        (Channel::Y, p.y),
                                                        (Channel::Scale, p.scale),
                                                    ] {
                                                        let _ = source
                                                            .rig
                                                            .active_clip_mut()
                                                            .set_key(id, ch, 1., v);
                                                    }
                                                }
                                            }
                                            match features::apply_foreign_pose(
                                                &mut self.project,
                                                &source,
                                                1.,
                                                self.frame,
                                            ) {
                                                Ok(n) => {
                                                    self.mode = Mode::Pose;
                                                    self.status = format!(
                                                        "Applied pose to {n} matching bones"
                                                    );
                                                }
                                                Err(e) => self.status = e.to_string(),
                                            }
                                        }
                                        Err(e) => self.status = e.to_string(),
                                    }
                                }
                            });
                        });
                    });
                    ui.separator();
                }
            });
    }
    pub(super) fn studio_references(&mut self, ui: &mut egui::Ui) {
        ui.heading("Synchronized image sequences and video");
        ui.horizontal(|ui| {
            if ui.button("Import image sequence…").clicked() {
                self.start_reference_job(false);
            }
            if ui.button("Import video / animated GIF…").clicked() {
                self.start_reference_job(true);
            }
        });
        let options = &mut self.studio.preferences.video;
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut options.fps)
                    .range(1. ..=60.)
                    .prefix("FPS "),
            );
            ui.add(
                egui::DragValue::new(&mut options.start)
                    .range(0. ..=36000.)
                    .prefix("Start sec "),
            );
            ui.add(
                egui::DragValue::new(&mut options.seconds)
                    .range(0.1..=60.)
                    .prefix("Duration sec "),
            );
            ui.add(
                egui::DragValue::new(&mut options.size)
                    .range(32..=256)
                    .prefix("Max size "),
            );
        });
        ui.label("Video decoding uses FFmpeg. Set its executable below if it is not on PATH. Imports run in the background and keep at most 240 frames.");
        ui.horizontal(|ui| {
            ui.label(options.ffmpeg.to_string_lossy());
            if ui.button("Choose FFmpeg…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Executable", &["exe"])
                    .pick_file()
            {
                options.ffmpeg = path;
            }
        });
        if ui.button("Save reference import defaults").clicked() {
            self.save_preferences();
        }
        if let Some(seq) = &mut self.project.extras.references {
            ui.separator();
            ui.label(format!(
                "{} frames · {} × {} pixels",
                seq.frames.len(),
                seq.frames[0].width,
                seq.frames[0].height
            ));
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut seq.fps)
                        .range(1. / 60. ..=240.)
                        .prefix("Timeline FPS "),
                );
                ui.add(
                    egui::DragValue::new(&mut seq.start_ms)
                        .range(0..=86_400_000)
                        .prefix("Start ms "),
                );
                ui.checkbox(&mut seq.looped, "Loop reference");
            });
            let mut template = seq.frames[0].clone();
            row(ui, "Offset X", &mut template.x, -4096. ..=4096.);
            row(ui, "Offset Y", &mut template.y, -4096. ..=4096.);
            row(ui, "Scale", &mut template.scale, 0.001..=1000.);
            row(ui, "Opacity", &mut template.opacity, 0. ..=1.);
            for frame in &mut seq.frames {
                frame.x = template.x;
                frame.y = template.y;
                frame.scale = template.scale;
                frame.opacity = template.opacity;
            }
            if ui.button("Remove synchronized reference").clicked() {
                self.project.extras.references = None;
                self.ref_tex = None;
                self.studio.ref_index = None;
            }
        }
        if let Some(job) = &self.studio.job {
            let progress = job.job.progress();
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(if progress.cancelling {
                    "Cancelling…".to_string()
                } else {
                    format!("{}…", job.job.label())
                });
                if ui
                    .add_enabled(!progress.cancelling, egui::Button::new("Cancel").small())
                    .clicked()
                {
                    job.job.cancel();
                }
            });
        }
    }
    pub(super) fn studio_files(&mut self, ui: &mut egui::Ui) {
        ui.heading("Autosave, recovery and backups");
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.studio.preferences.autosave_seconds)
                    .range(10..=600)
                    .prefix("Snapshot interval seconds "),
            );
            if ui.button("Save settings").clicked() {
                self.save_preferences();
            }
            if ui.button("Snapshot now").clicked() {
                match self.studio.store.write_recovery(
                    &self.studio.session,
                    &self.project,
                    self.path.clone(),
                ) {
                    Ok(()) => {
                        self.studio.autosaved = Some(self.project.clone());
                        self.status = "Recovery snapshot saved".into();
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
        });
        ui.label(format!(
            "Application data: {}",
            self.studio.store.root.display()
        ));
        if ui.button("Refresh recovery snapshots").clicked() {
            match self.studio.store.recoveries_report() {
                Ok(report) => {
                    self.studio.recoveries = report.recoveries;
                    self.status = if report.warnings.is_empty() {
                        "Recovery snapshots refreshed".into()
                    } else {
                        format!("Recovery: {}", report.warnings.join("; "))
                    };
                }
                Err(error) => self.status = format!("Recovery discovery failed: {error:#}"),
            }
        }
        let recoveries = self.studio.recoveries.clone();
        for recovery in recoveries {
            ui.horizontal(|ui| {
                ui.label(&recovery.name);
                if ui.button("Recover as new project").clicked() {
                    match self.studio.store.load_recovery(&recovery.id) {
                        Ok(p) => self.request(Pending::Restore(Box::new(p))),
                        Err(e) => self.status = e.to_string(),
                    }
                }
            });
        }
        ui.separator();
        ui.label("Saving an existing project keeps the last ten previous versions.");
        if let Some(path) = &self.path.clone() {
            match self.studio.store.backups(path) {
                Ok(backups) => {
                    for backup in backups {
                        ui.horizontal(|ui| {
                            ui.label(backup.file_name().unwrap_or_default().to_string_lossy());
                            if ui.button("Open backup as new project").clicked() {
                                match project::load(&backup) {
                                    Ok(p) => self.request(Pending::Restore(Box::new(p))),
                                    Err(e) => self.status = e.to_string(),
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    ui.label(e.to_string());
                }
            }
        }
    }
    pub(super) fn studio_export(&mut self, ui: &mut egui::Ui) {
        ui.heading("Batch exports and reusable presets");
        self.batch_preset_editor(ui);
        ui.checkbox(&mut self.exports.overwrite, "Replace existing files");
        if ui
            .add_enabled(
                self.exports.job.is_none(),
                egui::Button::new("Choose folder and export"),
            )
            .clicked()
        {
            self.export_kind(super::export_ui::Kind::Batch);
        }
        self.export_progress(ui, 220.);
        ui.label("Exports include atlas.json with clip names, source frames, rectangles and hold durations. Aseprite files include named animation tags. Pins and outlines are baked consistently.");
        if ui.button("Reset to game-asset preset").clicked() {
            self.studio.batch = ExportPreset::default();
        }
    }
    /// Preset selection and fields, shared with the Export window.
    pub(super) fn batch_preset_editor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("Saved preset")
                .selected_text(
                    self.studio
                        .preferences
                        .presets
                        .get(self.studio.preset_index)
                        .map_or("None", |p| p.name.as_str()),
                )
                .show_ui(ui, |ui| {
                    for (i, p) in self.studio.preferences.presets.iter().enumerate() {
                        if ui
                            .selectable_label(i == self.studio.preset_index, &p.name)
                            .clicked()
                        {
                            self.studio.preset_index = i;
                            self.studio.batch = p.clone();
                        }
                    }
                });
            if ui.button("Save / replace preset").clicked() {
                if let Some(index) = self
                    .studio
                    .preferences
                    .presets
                    .iter()
                    .position(|p| p.name == self.studio.batch.name)
                {
                    self.studio.preferences.presets[index] = self.studio.batch.clone();
                    self.studio.preset_index = index;
                } else if self.studio.preferences.presets.len() < 32 {
                    self.studio
                        .preferences
                        .presets
                        .push(self.studio.batch.clone());
                    self.studio.preset_index = self.studio.preferences.presets.len() - 1;
                }
                self.save_preferences();
            }
        });
        let preset = &mut self.studio.batch;
        ui.horizontal(|ui| {
            ui.label("Preset name");
            ui.text_edit_singleline(&mut preset.name);
        });
        egui::ComboBox::from_label("Format")
            .selected_text(format!("{:?}", preset.format))
            .show_ui(ui, |ui| {
                for format in rebonebase::export::BATCH_FORMATS {
                    ui.selectable_value(&mut preset.format, format, format!("{format:?}"));
                }
            });
        ui.horizontal(|ui| {
            ui.label("Prefix");
            ui.text_edit_singleline(&mut preset.prefix);
            ui.label("Suffix");
            ui.text_edit_singleline(&mut preset.suffix);
        });
        ui.add(
            egui::DragValue::new(&mut preset.columns)
                .range(1..=32)
                .prefix("Columns "),
        );
        ui.checkbox(&mut preset.all_clips, "Export every clip");
    }
}
