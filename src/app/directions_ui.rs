//! Directional set controls in the Export window.
use super::*;
use rebonebase::directions::{self, Count, DirectionExport, DirectionSet, Slot, Source};

/// Directional export choices; kept for the session, never in the project.
#[derive(Default)]
pub(super) struct DirectionsUi {
    pub set: DirectionSet,
    /// 3D sets: clips rendered from every direction.
    pub clips: Vec<usize>,
    pub previews: Vec<(String, egui::TextureHandle)>,
    pub preview_error: Option<String>,
    /// What the previews show, so stale previews can be cleared.
    pub preview_key: String,
}

impl ReboneApp {
    pub(super) fn direction_export(&self) -> DirectionExport {
        DirectionExport {
            set: self.exports.directions.set.clone(),
            clips: self.exports.directions.clips.clone(),
            engine: self.production.engine,
            atlas: self.studio.preferences.engine_atlas,
        }
    }
    /// Pick a sensible source for this project the first time the panel opens.
    fn ensure_direction_source(&mut self) {
        let has_3d = self.project.extras.characters3d.iter().any(|c| c.enabled);
        let d = &mut self.exports.directions;
        let n = d.set.count.size();
        match &mut d.set.source {
            Source::Camera { .. } if !has_3d => {
                let mut slots = vec![Slot::Empty; n];
                slots[0] = Slot::Clip(self.project.rig.active_clip);
                d.set.source = Source::Clips {
                    name: self.project.rig.active_clip().name.clone(),
                    slots,
                };
            }
            Source::Clips { slots, .. } => slots.resize(n, Slot::Empty),
            Source::Camera { .. } => {}
        }
        if d.clips.is_empty() {
            d.clips.push(self.project.rig.active_clip);
        }
        d.clips.retain(|c| *c < self.project.rig.clips.len());
    }
    pub(super) fn direction_options(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        self.ensure_direction_source();
        let has_3d = self.project.extras.characters3d.iter().any(|c| c.enabled);
        let clip_names: Vec<String> = self
            .project
            .rig
            .clips
            .iter()
            .map(|c| c.name.clone())
            .collect();
        let d = &mut self.exports.directions;
        ui.horizontal(|ui| {
            ui.label("Directions");
            ui.selectable_value(&mut d.set.count, Count::Four, "4");
            ui.selectable_value(&mut d.set.count, Count::Eight, "8");
            ui.add(
                egui::DragValue::new(&mut d.set.start_angle)
                    .range(-360. ..=360.)
                    .speed(1.)
                    .suffix("°")
                    .prefix("Start "),
            )
            .on_hover_text("Angle of the first direction, clockwise from screen right. 45° gives diagonal four-direction sets.");
        });
        self.direction_previews(ui, ctx);
        let d = &mut self.exports.directions;
        let dirs = directions::directions(d.set.count, d.set.start_angle);
        let ids: Vec<String> = dirs
            .iter()
            .map(|d| format!("{} · {:.0}°", d.id, d.angle))
            .collect();
        ui.horizontal(|ui| {
            let camera = matches!(d.set.source, Source::Camera { .. });
            if ui
                .add_enabled(has_3d, egui::Button::selectable(camera, "3D camera views"))
                .on_disabled_hover_text("Import a 3D character to render views automatically")
                .clicked()
                && !camera
            {
                d.set.source = Source::Camera {
                    facing: directions::FACING_VIEWER,
                };
            }
            if ui
                .selectable_label(!camera, "2D clips per direction")
                .clicked()
                && camera
            {
                d.set.source = Source::Clips {
                    name: clip_names.first().cloned().unwrap_or_default(),
                    slots: vec![Slot::Empty; dirs.len()],
                };
            }
        });
        match &mut d.set.source {
            Source::Camera { facing } => {
                egui::ComboBox::from_label("Characters currently face")
                    .selected_text(compass_label(*facing))
                    .show_ui(ui, |ui| {
                        for (i, id) in directions::COMPASS.iter().enumerate() {
                            ui.selectable_value(
                                facing,
                                i as f32 * 45.,
                                format!("{id} · {}°", i * 45),
                            );
                        }
                    });
                ui.label(RichText::new("Rotates every enabled 3D character's camera yaw. 2D artwork is not rotated.").small().color(DIM));
                ui.label("Clips");
                for (i, name) in clip_names.iter().enumerate() {
                    let mut on = d.clips.contains(&i);
                    if ui.checkbox(&mut on, name).changed() {
                        if on {
                            d.clips.push(i);
                            d.clips.sort_unstable();
                        } else {
                            d.clips.retain(|c| *c != i);
                        }
                    }
                }
            }
            Source::Clips { name, slots } => {
                ui.horizontal(|ui| {
                    ui.label("Animation name");
                    ui.text_edit_singleline(name);
                });
                ui.label(RichText::new("A flat drawing has no unseen views: choose an authored clip for each direction, or mirror another one.").small().color(DIM));
                let snapshot = slots.clone();
                egui::Grid::new("direction-slots")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for (i, slot) in slots.iter_mut().enumerate() {
                            ui.label(&ids[i]);
                            let text = match *slot {
                                Slot::Clip(c) => clip_names
                                    .get(c)
                                    .cloned()
                                    .unwrap_or_else(|| "Missing clip".into()),
                                Slot::Mirror(m) => format!(
                                    "Mirror of {}",
                                    dirs.get(m).map_or("?", |d| d.id.as_str())
                                ),
                                Slot::Empty => "Choose…".into(),
                            };
                            egui::ComboBox::from_id_salt(("direction-slot", i))
                                .selected_text(text)
                                .show_ui(ui, |ui| {
                                    for (c, clip) in clip_names.iter().enumerate() {
                                        ui.selectable_value(slot, Slot::Clip(c), clip);
                                    }
                                    for (m, other) in dirs.iter().enumerate() {
                                        if m != i && matches!(snapshot[m], Slot::Clip(_)) {
                                            ui.selectable_value(
                                                slot,
                                                Slot::Mirror(m),
                                                format!("Mirror of {}", other.id),
                                            );
                                        }
                                    }
                                });
                            ui.end_row();
                        }
                    });
            }
        }
        ui.separator();
        let before = self.project.clone();
        self.engine_options(ui);
        self.commit_studio_edit(before);
    }
    fn direction_previews(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let set = self.exports.directions.set.clone();
        let key = format!("{set:?}{}{}", self.frame, self.project.rig.active_clip);
        if self.exports.directions.preview_key != key {
            self.exports.directions.previews.clear();
            self.exports.directions.preview_error = None;
        }
        if ui
            .button("Preview orientations")
            .on_hover_text("Render the current frame from every direction")
            .clicked()
        {
            let result = directions::preview(
                &self.project,
                &set,
                self.exports
                    .directions
                    .clips
                    .first()
                    .copied()
                    .unwrap_or(self.project.rig.active_clip),
                self.frame,
            );
            let d = &mut self.exports.directions;
            d.preview_key = key;
            match result {
                Ok(views) => {
                    d.previews = views
                        .into_iter()
                        .map(|(dir, image)| {
                            let texture = ctx.load_texture(
                                format!("direction-{}", dir.id),
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [image.width() as usize, image.height() as usize],
                                    image.as_raw(),
                                ),
                                egui::TextureOptions::NEAREST,
                            );
                            (dir.id, texture)
                        })
                        .collect();
                }
                Err(e) => d.preview_error = Some(e.to_string()),
            }
        }
        let d = &self.exports.directions;
        if let Some(e) = &d.preview_error {
            ui.label(
                RichText::new(e)
                    .small()
                    .color(Color32::from_rgb(255, 150, 140)),
            );
        }
        if !d.previews.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for (id, texture) in &d.previews {
                    ui.vertical(|ui| {
                        let size = texture.size_vec2();
                        let scale = (72. / size.x.max(size.y)).min(4.);
                        ui.add(egui::Image::new((texture.id(), size * scale)))
                            .on_hover_text("All directions share this canvas and pivot");
                        ui.label(RichText::new(id).small());
                    });
                }
            });
        }
    }
}
fn compass_label(angle: f32) -> String {
    let i = (angle.rem_euclid(360.) / 45.).round() as usize % 8;
    format!("{} · {}°", directions::COMPASS[i], i * 45)
}
