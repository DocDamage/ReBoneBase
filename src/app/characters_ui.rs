use super::*;
use rebonebase::character3d::{self, Character};
impl ReboneApp {
    pub(super) fn import_character3d(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("3D character", &["glb", "gltf", "fbx", "obj"])
            .pick_file()
        else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(character3d::load(&path).map_err(|e| e.to_string()));
        });
        self.production.character_job = Some(rx);
        self.production.open = true;
        self.production.tab = production_ui::Tab::Characters;
        self.status = "Importing 3D character…".into();
    }
    pub(super) fn add_character3d(&mut self, mut c: Character) -> anyhow::Result<()> {
        let before = self.project.clone();
        let mut candidate = before.clone();
        c.id = candidate
            .extras
            .characters3d
            .iter()
            .map(|c| c.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("No more 3D character IDs are available"))?;
        let origin = candidate.extras.canvas_origin;
        if let Some((min, max)) = c.bounds(0.) {
            let h = (max.y - min.y).max(0.001);
            let w = (max.x - min.x).max(0.001);
            let factor = (candidate.rig.height as f32 * 0.8 / h)
                .min(candidate.rig.width as f32 * 0.8 / w)
                .clamp(0.001, 10000.);
            c.scale *= factor;
            let center = ((min + max) * 0.5 - c.position) * factor;
            c.position = origin
                + Vec2::new(
                    candidate.rig.width as f32 * 0.5,
                    candidate.rig.height as f32 * 0.5,
                )
                - center;
        }
        if candidate.extras.characters3d.is_empty() {
            candidate.extras.show_2d = false;
        }
        let id = c.id;
        candidate.extras.characters3d.push(c);
        candidate.validate()?;
        rebonebase::canvas::grow_all(&mut candidate)?;
        self.history.record(&before);
        self.project = candidate;
        self.production.character = Some(id);
        self.production.move_character = true;
        self.production.actor = None;
        self.bones = false;
        self.show_skin = true;
        self.mode = Mode::Pose;
        self.frame = 1.;
        self.fit = true;
        self.changed();
        self.status =
            "3D character imported · choose its animation or rotate a joint in Production".into();
        Ok(())
    }
    pub(super) fn poll_character_import(&mut self, ctx: &egui::Context) {
        if let Some(rx) = self.production.character_job.take() {
            match rx.try_recv() {
                Ok(Ok(c)) => {
                    if let Err(e) = self.add_character3d(c) {
                        self.status = format!("3D import rejected: {e}");
                    }
                }
                Ok(Err(e)) => self.status = format!("3D import failed: {e}"),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.production.character_job = Some(rx);
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
                Err(_) => self.status = "3D importer stopped".into(),
            }
        }
    }
    pub(super) fn characters_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("3D characters");
        if ui
            .add_enabled(
                self.production.character_job.is_none(),
                egui::Button::new("Import 3D character…"),
            )
            .clicked()
        {
            self.import_character3d();
        }
        if self.production.character_job.is_some() {
            ui.spinner();
        }
        ui.label("Import GLB/glTF, FBX or OBJ. Render a 3D model into pixel animation, choose an included motion, or pose its joints.");
        ui.checkbox(
            &mut self.project.extras.show_2d,
            "Show 2D mannequin in the scene",
        );
        egui::ComboBox::from_id_salt("character3d")
            .selected_text(
                self.production
                    .character
                    .and_then(|id| self.project.extras.characters3d.iter().find(|c| c.id == id))
                    .map_or("Choose character", |c| c.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for c in &self.project.extras.characters3d {
                    ui.selectable_value(&mut self.production.character, Some(c.id), &c.name);
                }
            });
        if self.production.character.is_some()
            && ui
                .button("Import animation for selected 3D character…")
                .clicked()
        {
            self.import_character_animation();
        }
        ui.label("Unity/Unreal: import FBX, GLB/glTF or BVH. Text Unity .anim transform curves are also supported. Unreal .uasset requires FBX export in Unreal.");
        let clip_index = self.project.rig.active_clip;
        let mut remove = None;
        let mut make_timeline = None;
        if let Some(c) = self.production.character.and_then(|id| {
            self.project
                .extras
                .characters3d
                .iter_mut()
                .find(|c| c.id == id)
        }) {
            ui.text_edit_singleline(&mut c.name);
            ui.checkbox(&mut c.enabled, "Visible 3D character");
            if ui
                .checkbox(
                    &mut self.production.move_character,
                    "Move Rig targets this 3D character",
                )
                .changed()
                && self.production.move_character
            {
                self.production.actor = None;
            }
            ui.horizontal_wrapped(|ui| {
                ui.add_enabled(false, egui::DragValue::new(&mut c.position.x).prefix("X "));
                ui.add_enabled(false, egui::DragValue::new(&mut c.position.y).prefix("Y "));
                ui.add(
                    egui::DragValue::new(&mut c.scale)
                        .range(0.001..=10000.)
                        .speed(0.05)
                        .prefix("Pixels/unit "),
                );
                ui.add(
                    egui::DragValue::new(&mut c.z)
                        .range(-100..=100)
                        .prefix("Layer "),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::Slider::new(&mut c.yaw, -180. ..=180.).text("Camera yaw"));
                ui.add(egui::Slider::new(&mut c.pitch, -89. ..=89.).text("Camera pitch"));
            });
            let mut playback = c.playback(clip_index);
            let old_playback = playback.clone();
            ui.label("Animation for the active timeline clip");
            egui::ComboBox::from_id_salt("character_animation")
                .selected_text(
                    playback
                        .animation
                        .and_then(|i| c.model.animations.get(i))
                        .map_or("Rest pose", |a| a.name.as_str()),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut playback.animation, None, "Rest pose");
                    for (i, a) in c.model.animations.iter().enumerate() {
                        ui.selectable_value(&mut playback.animation, Some(i), &a.name);
                    }
                });
            ui.horizontal(|ui| {
                ui.checkbox(&mut playback.looped, "Loop 3D motion");
                ui.add(egui::Slider::new(&mut playback.speed, 0.05..=4.).text("Speed"));
            });
            if let Some(a) = playback.animation.and_then(|i| c.model.animations.get(i))
                && ui.button("Create timeline for 3D animation").clicked()
            {
                make_timeline = Some((
                    a.name.clone(),
                    a.duration / playback.speed.max(0.01),
                    c.id,
                    playback.clone(),
                ));
            }
            if playback != old_playback {
                c.clips.insert(clip_index, playback);
            }
            ui.separator();
            ui.strong("Pose a joint");
            self.production.character_joint = self
                .production
                .character_joint
                .min(c.model.nodes.len().saturating_sub(1));
            egui::ComboBox::from_id_salt("character_joint")
                .selected_text(
                    c.model
                        .nodes
                        .get(self.production.character_joint)
                        .map_or("No joints", |n| n.name.as_str()),
                )
                .show_ui(ui, |ui| {
                    for (i, n) in c.model.nodes.iter().enumerate() {
                        ui.selectable_value(&mut self.production.character_joint, i, &n.name);
                    }
                });
            if !c.model.nodes.is_empty() {
                let index = self.production.character_joint;
                let mut rot = c.joints.get(&index).copied().unwrap_or([0.; 3]);
                let old = rot;
                ui.horizontal_wrapped(|ui| {
                    for (i, label) in ["Joint X° ", "Joint Y° ", "Joint Z° "]
                        .into_iter()
                        .enumerate()
                    {
                        ui.add(
                            egui::DragValue::new(&mut rot[i])
                                .range(-180. ..=180.)
                                .speed(1.)
                                .prefix(label),
                        );
                    }
                });
                if rot != old {
                    c.joints.insert(index, rot);
                }
                if ui.button("Reset selected joint").clicked() {
                    c.joints.remove(&index);
                }
                if ui.button("Reset all 3D joint poses").clicked() {
                    c.joints.clear();
                }
            }
            ui.label("Joint rotations are additive pose overrides. Joint positions stay attached to their parents; imported animation supplies the motion.");
            for warning in &c.model.warnings {
                ui.label(RichText::new(warning).color(Color32::YELLOW));
            }
            if ui.button("Remove 3D character").clicked() {
                remove = Some(c.id);
            }
        }
        if let Some((name, duration, id, mut playback)) = make_timeline {
            if self.project.rig.clips.len() < 32 {
                let length = (duration * 24.).ceil().clamp(2., 240.) as u32;
                self.project.rig.clips.push(Clip {
                    name,
                    length,
                    fps: (length as f32 / duration.max(0.01)).clamp(1. / 60., 240.),
                    ..Default::default()
                });
                self.project.rig.active_clip = self.project.rig.clips.len() - 1;
                playback.offset_ms = 0.;
                if let Some(c) = self
                    .project
                    .extras
                    .characters3d
                    .iter_mut()
                    .find(|c| c.id == id)
                {
                    c.clips.insert(self.project.rig.active_clip, playback);
                }
                self.frame = 1.;
            } else {
                self.status = "A project supports at most 32 clips".into();
            }
        }
        if let Some(id) = remove {
            self.project.extras.characters3d.retain(|c| c.id != id);
            self.production.character = None;
            self.production.move_character = false;
        }
        self.art_preview(ui);
    }
}
