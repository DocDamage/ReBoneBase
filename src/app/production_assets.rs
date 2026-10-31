use super::production_ui::{bone_combo, clip_combo};
use super::*;
use rebonebase::production::{
    self, Attachment, AttachmentKey, FaceKey, FaceTrack, Look, MeshKey, Weight,
};
fn asset_combo(
    ui: &mut egui::Ui,
    assets: &[production::SpriteAsset],
    label: &str,
    value: &mut u64,
) {
    egui::ComboBox::from_label(label)
        .selected_text(
            assets
                .iter()
                .find(|a| a.id == *value)
                .map_or("Missing image", |a| a.name.as_str()),
        )
        .show_ui(ui, |ui| {
            for a in assets {
                ui.selectable_value(value, a.id, &a.name);
            }
        });
}
impl ReboneApp {
    pub(super) fn production_props(&mut self, ui: &mut egui::Ui) {
        ui.heading("Bone-attached props");
        self.art_picker(ui);
        if ui.button("Attach artwork to selected bone").clicked() {
            if let Some(asset) = self.production.asset {
                let id = self.project.extras.production.next_id();
                let bone = self.selected;
                let offset = bone
                    .and_then(|id| self.project.rig.bone(id))
                    .map_or(Vec2::new(8., 8.), |b| Vec2::new(b.length, 0.));
                self.project.extras.production.attachments.push(Attachment {
                    id,
                    asset,
                    bone,
                    offset,
                    pivot: Vec2::new(0.5, 0.5),
                    ..Default::default()
                });
                self.production.prop = Some(id);
            } else {
                self.status = "Import or choose artwork first".into();
            }
        }
        let items: Vec<_> = self
            .project
            .extras
            .production
            .attachments
            .iter()
            .map(|a| (a.id, a.name.clone()))
            .collect();
        egui::ComboBox::from_label("Prop")
            .selected_text(
                self.production
                    .prop
                    .and_then(|id| items.iter().find(|a| a.0 == id))
                    .map_or("Choose prop", |a| a.1.as_str()),
            )
            .show_ui(ui, |ui| {
                for (id, name) in &items {
                    ui.selectable_value(&mut self.production.prop, Some(*id), name);
                }
            });
        if let Some(id) = self.production.prop
            && let Some(index) = self
                .project
                .extras
                .production
                .attachments
                .iter()
                .position(|a| a.id == id)
        {
            let mut a = self.project.extras.production.attachments[index].clone();
            if self.production.last_prop != Some(id) {
                let frame = self.project.rig.active_clip().evaluation_frame(self.frame);
                self.production.prop_parent =
                    production::prop_keys(&a, self.project.rig.active_clip)
                        .iter()
                        .rev()
                        .find(|k| k.frame <= frame)
                        .map_or(a.bone, |k| k.bone);
                self.production.last_prop = Some(id);
            }
            ui.text_edit_singleline(&mut a.name);
            ui.checkbox(&mut a.enabled, "Visible prop");
            asset_combo(
                ui,
                &self.project.extras.production.assets,
                "Prop image",
                &mut a.asset,
            );
            bone_combo(
                ui,
                &self.project.rig,
                "New parent",
                &mut self.production.prop_parent,
            );
            row(ui, "Offset X", &mut a.offset.x, -4096. ..=4096.);
            row(ui, "Offset Y", &mut a.offset.y, -4096. ..=4096.);
            row(ui, "Angle", &mut a.angle, -720. ..=720.);
            row(ui, "Scale", &mut a.scale, 0.01..=100.);
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut a.pivot.x)
                        .range(0. ..=1.)
                        .prefix("Pivot X "),
                );
                ui.add(
                    egui::DragValue::new(&mut a.pivot.y)
                        .range(0. ..=1.)
                        .prefix("Y "),
                );
                ui.add(
                    egui::DragValue::new(&mut a.z)
                        .range(-100..=100)
                        .prefix("Layer "),
                );
            });
            let mut key = false;
            let mut parent = false;
            let mut remove = false;
            ui.horizontal_wrapped(|ui| {
                key = ui.button("Key prop state here").clicked();
                parent = ui.button("Switch to parent without a jump").clicked();
                remove = ui.button("Remove prop").clicked();
            });
            if key {
                match self.project.rig.active_clip_mut().release_hold(self.frame) {
                    Ok(()) => {
                        let state = AttachmentKey {
                            frame: self.frame,
                            bone: a.bone,
                            offset: a.offset,
                            angle: a.angle,
                            asset: a.asset,
                            visible: a.enabled,
                        };
                        let keys = production::prop_keys_mut(
                            &mut a,
                            self.project.rig.active_clip,
                            self.project.rig.active_clip().length,
                        );
                        keys.retain(|k| k.frame != self.frame);
                        keys.push(state);
                        keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                    }
                    Err(error) => self.status = error,
                }
            }
            let new_parent = self.production.prop_parent;
            self.project.extras.production.attachments[index] = a;
            if parent
                && let Err(e) = production::switch_attachment_parent(
                    &mut self.project,
                    id,
                    new_parent,
                    self.frame,
                )
            {
                self.status = e.to_string();
            }
            let old_keys = production::prop_keys(
                &self.project.extras.production.attachments[index],
                self.project.rig.active_clip,
            )
            .to_vec();
            let mut keys = old_keys.clone();
            let mut drop_key = None;
            for (i, k) in keys.iter_mut().enumerate() {
                if k.frame < 1. || k.frame > self.project.rig.active_clip().length as f32 {
                    continue;
                }
                ui.push_id(("propkey", i), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Frame {:.2}", k.frame));
                        ui.checkbox(&mut k.visible, "Visible");
                        ui.add(egui::DragValue::new(&mut k.offset.x).prefix("X "));
                        ui.add(egui::DragValue::new(&mut k.offset.y).prefix("Y "));
                        ui.add(egui::DragValue::new(&mut k.angle).prefix("Angle "));
                        if ui.small_button("Delete prop key").clicked() {
                            drop_key = Some(i);
                        }
                    });
                    asset_combo(
                        ui,
                        &self.project.extras.production.assets,
                        "Key image",
                        &mut k.asset,
                    );
                });
            }
            if let Some(i) = drop_key {
                keys.remove(i);
            }
            if keys != old_keys {
                let changed_frame = keys
                    .iter()
                    .filter(|key| !old_keys.contains(key))
                    .map(|key| key.frame)
                    .max_by(f32::total_cmp);
                let result = changed_frame.map_or(Ok(()), |frame| {
                    self.project.rig.active_clip_mut().release_hold(frame)
                });
                match result {
                    Ok(()) => {
                        *production::prop_keys_mut(
                            &mut self.project.extras.production.attachments[index],
                            self.project.rig.active_clip,
                            self.project.rig.active_clip().length,
                        ) = keys
                    }
                    Err(error) => self.status = error,
                }
            }
            if remove {
                self.project.extras.production.attachments.remove(index);
                for l in &mut self.project.extras.production.looks {
                    l.attachments.retain(|v| *v != id);
                }
                self.production.prop = None;
            }
        }
        ui.label("Base fields set the static prop; keyed states override them from their frame onward. Choose Parent, then Switch, to preserve the displayed world position.");
        self.art_preview(ui);
    }
    pub(super) fn production_meshes(&mut self, ui: &mut egui::Ui) {
        ui.heading("Weighted artwork and deformable meshes");
        self.art_picker(ui);
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::DragValue::new(&mut self.production.mesh_origin.x).prefix("Left "));
            ui.add(egui::DragValue::new(&mut self.production.mesh_origin.y).prefix("Top "));
            ui.add(
                egui::DragValue::new(&mut self.production.mesh_size.x)
                    .range(1. ..=4096.)
                    .prefix("Width "),
            );
            ui.add(
                egui::DragValue::new(&mut self.production.mesh_size.y)
                    .range(1. ..=4096.)
                    .prefix("Height "),
            );
            ui.add(
                egui::DragValue::new(&mut self.production.mesh_columns)
                    .range(1..=12)
                    .prefix("Columns "),
            );
            ui.add(
                egui::DragValue::new(&mut self.production.mesh_rows)
                    .range(1..=12)
                    .prefix("Rows "),
            );
        });
        if ui.button("Create weighted mesh from artwork").clicked() {
            if let Some(asset) = self.production.asset {
                let id = self.project.extras.production.next_id();
                match production::grid_mesh(
                    &self.project.rig,
                    id,
                    asset,
                    self.production.mesh_origin,
                    self.production.mesh_size,
                    self.production.mesh_columns,
                    self.production.mesh_rows,
                    &self.selected_bones(),
                ) {
                    Ok(m) => {
                        self.project.extras.production.meshes.push(m);
                        self.production.mesh = Some(id);
                        self.production.mesh_vertex = 0;
                    }
                    Err(e) => self.status = e.to_string(),
                }
            } else {
                self.status = "Choose artwork and select binding bones".into();
            }
        }
        let p = &mut self.project.extras.production;
        egui::ComboBox::from_label("Mesh")
            .selected_text(
                self.production
                    .mesh
                    .and_then(|id| p.meshes.iter().find(|m| m.id == id))
                    .map_or("Choose mesh", |m| m.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for m in &p.meshes {
                    ui.selectable_value(&mut self.production.mesh, Some(m.id), &m.name);
                }
            });
        if let Some(id) = self.production.mesh
            && let Some(index) = self
                .project
                .extras
                .production
                .meshes
                .iter()
                .position(|m| m.id == id)
        {
            let selected = self.selected_bones();
            let m = &mut self.project.extras.production.meshes[index];
            ui.text_edit_singleline(&mut m.name);
            ui.horizontal(|ui| {
                ui.checkbox(&mut m.enabled, "Visible mesh");
                ui.add(
                    egui::DragValue::new(&mut m.z)
                        .range(-100..=100)
                        .prefix("Layer "),
                );
            });
            egui::ComboBox::from_label("Replace capsule group")
                .selected_text(
                    m.replace_part
                        .and_then(|id| self.project.rig.parts.iter().find(|p| p.id == id))
                        .map_or("Keep all capsule groups", |p| p.name.as_str()),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut m.replace_part, None, "Keep all capsule groups");
                    for p in &self.project.rig.parts {
                        ui.selectable_value(&mut m.replace_part, Some(p.id), &p.name);
                    }
                });
            self.production.mesh_vertex = self
                .production
                .mesh_vertex
                .min(m.vertices.len().saturating_sub(1));
            ui.add(
                egui::DragValue::new(&mut self.production.mesh_vertex)
                    .range(0..=m.vertices.len().saturating_sub(1))
                    .prefix("Vertex "),
            );
            if let Some(v) = m.vertices.get_mut(self.production.mesh_vertex) {
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::DragValue::new(&mut v.rest.x).prefix("Rest X "));
                    ui.add(egui::DragValue::new(&mut v.rest.y).prefix("Y "));
                    ui.add(
                        egui::DragValue::new(&mut v.uv.x)
                            .range(0. ..=1.)
                            .speed(0.01)
                            .prefix("U "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut v.uv.y)
                            .range(0. ..=1.)
                            .speed(0.01)
                            .prefix("V "),
                    );
                });
                for w in &mut v.weights {
                    ui.horizontal(|ui| {
                        ui.label(
                            self.project
                                .rig
                                .bone(w.bone)
                                .map_or("Bone", |b| b.name.as_str()),
                        );
                        ui.add(egui::Slider::new(&mut w.weight, 0. ..=1.).text("Weight"));
                    });
                }
                let sum: f32 = v.weights.iter().map(|w| w.weight).sum();
                if sum > 0. {
                    for w in &mut v.weights {
                        w.weight /= sum;
                    }
                }
                if ui.button("Bind vertex to selected bones").clicked() && !selected.is_empty() {
                    let n = selected.len().min(4);
                    v.weights = selected
                        .iter()
                        .take(4)
                        .map(|bone| Weight {
                            bone: *bone,
                            weight: 1. / n as f32,
                        })
                        .collect();
                    for w in self
                        .project
                        .rig
                        .world(None)
                        .iter()
                        .filter(|w| selected.contains(&w.id))
                    {
                        m.bind.entry(w.id).or_insert(production::BindTransform {
                            head: w.head,
                            angle: w.angle,
                            length: (w.tail - w.head).length(),
                        });
                    }
                }
            }
            ui.checkbox(
                &mut self.production.mesh_animate,
                "Edit deformation key at playhead",
            );
            if self.production.mesh_animate {
                let vi = self.production.mesh_vertex;
                let clip = self.project.rig.active_clip;
                let length = self.project.rig.active_clip().length;
                let frame = self.project.rig.active_clip().evaluation_frame(self.frame);
                let offsets = production::mesh_offsets(m, clip, frame);
                let old = offsets.get(vi).copied().unwrap_or_default();
                let mut offset = old;
                row(ui, "Deform X", &mut offset.x, -4096. ..=4096.);
                row(ui, "Deform Y", &mut offset.y, -4096. ..=4096.);
                if offset != old {
                    match self.project.rig.active_clip_mut().release_hold(self.frame) {
                        Ok(()) => {
                            let mut offsets = offsets;
                            offsets[vi] = offset;
                            let keys = production::mesh_keys_mut(m, clip, length);
                            keys.retain(|k| k.frame != self.frame);
                            keys.push(MeshKey {
                                frame: self.frame,
                                offsets,
                            });
                            keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                        }
                        Err(error) => self.status = error,
                    }
                }
                if ui.button("Remove current deformation key").clicked() {
                    production::mesh_keys_mut(m, clip, length).retain(|k| k.frame != self.frame);
                }
            }
            if ui.button("Remove mesh").clicked() {
                self.project.extras.production.meshes.remove(index);
                for l in &mut self.project.extras.production.looks {
                    l.meshes.retain(|v| *v != id);
                }
                self.production.mesh = None;
            }
        }
        let rect = self.art_preview(ui);
        if let Some(m) = self.production.mesh.and_then(|id| {
            self.project
                .extras
                .production
                .meshes
                .iter()
                .find(|m| m.id == id)
        }) {
            let points = production::mesh_vertices(
                &self.project.rig,
                Some(&self.project.pose_at(self.frame)),
                m,
                self.project.rig.active_clip().evaluation_frame(self.frame),
            );
            let painter = ui.painter_at(rect);
            let scale = rect.width() / self.project.rig.width as f32;
            for (i, point) in points.iter().enumerate() {
                let point = *point - self.project.extras.canvas_origin;
                let point = rect.min + EVec2::new(point.x, point.y) * scale;
                painter.circle_filled(
                    point,
                    if i == self.production.mesh_vertex {
                        4.
                    } else {
                        2.
                    },
                    if i == self.production.mesh_vertex {
                        Color32::YELLOW
                    } else {
                        ACCENT
                    },
                );
            }
            let response = ui.interact(
                rect,
                egui::Id::new("mesh_vertex_pick"),
                egui::Sense::click(),
            );
            if response.clicked()
                && let Some(at) = response.interact_pointer_pos()
                && let Some((i, _)) = points
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let p = *p - self.project.extras.canvas_origin;
                        (i, (rect.min + EVec2::new(p.x, p.y) * scale).distance(at))
                    })
                    .filter(|(_, d)| *d < 12.)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
            {
                self.production.mesh_vertex = i;
            }
        }
    }
    pub(super) fn production_looks(&mut self, ui: &mut egui::Ui) {
        ui.heading("Reusable outfits and equipment variants");
        if ui.button("Capture current outfit").clicked() {
            let p = &mut self.project.extras.production;
            let look = Look {
                name: format!("Outfit {}", p.looks.len() + 1),
                colors: self
                    .project
                    .rig
                    .parts
                    .iter()
                    .map(|p| (p.id, p.color))
                    .collect(),
                attachments: p
                    .attachments
                    .iter()
                    .filter(|a| a.enabled)
                    .map(|a| a.id)
                    .collect(),
                meshes: p
                    .meshes
                    .iter()
                    .filter(|m| m.enabled)
                    .map(|m| m.id)
                    .collect(),
            };
            p.looks.push(look);
            self.production.look = p.looks.len() - 1;
            p.active_look = Some(self.production.look);
        }
        let p = &mut self.project.extras.production;
        egui::ComboBox::from_label("Active outfit")
            .selected_text(
                p.active_look
                    .and_then(|i| p.looks.get(i))
                    .map_or("Base appearance", |l| l.name.as_str()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut p.active_look, None, "Base appearance");
                for (i, l) in p.looks.iter().enumerate() {
                    if ui
                        .selectable_value(&mut p.active_look, Some(i), &l.name)
                        .clicked()
                    {
                        self.production.look = i;
                    }
                }
            });
        if !p.looks.is_empty() {
            self.production.look = self.production.look.min(p.looks.len() - 1);
            let look = &mut p.looks[self.production.look];
            ui.text_edit_singleline(&mut look.name);
            for part in &self.project.rig.parts {
                ui.horizontal(|ui| {
                    ui.label(&part.name);
                    let c = look.colors.entry(part.id).or_insert(part.color);
                    ui.color_edit_button_srgba_unmultiplied(c);
                });
            }
            ui.label("Included props");
            for a in &p.attachments {
                let mut enabled = look.attachments.contains(&a.id);
                if ui.checkbox(&mut enabled, &a.name).changed() {
                    if enabled {
                        look.attachments.push(a.id);
                    } else {
                        look.attachments.retain(|id| *id != a.id);
                    }
                }
            }
            ui.label("Included meshes");
            for m in &p.meshes {
                let mut enabled = look.meshes.contains(&m.id);
                if ui.checkbox(&mut enabled, &m.name).changed() {
                    if enabled {
                        look.meshes.push(m.id);
                    } else {
                        look.meshes.retain(|id| *id != m.id);
                    }
                }
            }
            if ui.button("Delete outfit").clicked() {
                p.looks.remove(self.production.look);
                p.active_look = None;
                self.production.look = 0;
            }
        }
        self.art_preview(ui);
    }
    pub(super) fn production_face(&mut self, ui: &mut egui::Ui) {
        ui.heading("Expressions, blinks and mouth shapes");
        self.art_picker(ui);
        if ui.button("Add facial sprite track").clicked() {
            if let Some(asset) = self.production.asset {
                let id = self.project.extras.production.next_id();
                let bone = self.selected.or_else(|| {
                    self.project
                        .rig
                        .bones
                        .iter()
                        .find(|b| b.name.to_lowercase() == "head")
                        .map(|b| b.id)
                });
                self.project.extras.production.faces.push(FaceTrack {
                    id,
                    bone,
                    clip: self.project.rig.active_clip,
                    angle: bone
                        .and_then(|id| {
                            self.project
                                .rig
                                .world(None)
                                .iter()
                                .find(|w| w.id == id)
                                .map(|w| -w.angle)
                        })
                        .unwrap_or(0.),
                    keys: vec![FaceKey {
                        frame: 1.,
                        asset,
                        offset: Vec2::default(),
                    }],
                    ..Default::default()
                });
                self.production.face = Some(id);
            } else {
                self.status = "Choose a transparent face sprite first".into();
            }
        }
        let p = &mut self.project.extras.production;
        egui::ComboBox::from_label("Face track")
            .selected_text(
                self.production
                    .face
                    .and_then(|id| p.faces.iter().find(|f| f.id == id))
                    .map_or("Choose track", |f| f.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for f in &p.faces {
                    ui.selectable_value(&mut self.production.face, Some(f.id), &f.name);
                }
            });
        if let Some(id) = self.production.face
            && let Some(index) = p.faces.iter().position(|f| f.id == id)
        {
            let f = &mut p.faces[index];
            ui.text_edit_singleline(&mut f.name);
            ui.checkbox(&mut f.enabled, "Visible facial track");
            bone_combo(ui, &self.project.rig, "Face parent", &mut f.bone);
            clip_combo(ui, &self.project.rig, "Face clip", &mut f.clip);
            row(ui, "Offset X", &mut f.offset.x, -4096. ..=4096.);
            row(ui, "Offset Y", &mut f.offset.y, -4096. ..=4096.);
            row(ui, "Face angle", &mut f.angle, -720. ..=720.);
            row(ui, "Scale", &mut f.scale, 0.01..=100.);
            ui.add(
                egui::DragValue::new(&mut f.z)
                    .range(-100..=100)
                    .prefix("Layer "),
            );
            if ui.button("Key chosen expression / mouth here").clicked()
                && let Some(asset) = self.production.asset
            {
                match self.project.rig.clips[f.clip].release_hold(self.frame) {
                    Ok(()) => {
                        f.keys.retain(|k| k.frame != self.frame);
                        f.keys.push(FaceKey {
                            frame: self.frame,
                            asset,
                            offset: Vec2::default(),
                        });
                        f.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                    }
                    Err(error) => self.status = error,
                }
            }
            ui.label("Choose the closed-eye sprite, then add a blink; the current sprite is restored after two frames.");
            if ui.button("Add blink with chosen sprite").clicked()
                && let Some(closed) = self.production.asset
            {
                let frame = self.project.rig.clips[f.clip].evaluation_frame(self.frame);
                let open = f
                    .keys
                    .iter()
                    .rev()
                    .find(|k| k.frame <= frame)
                    .or_else(|| f.keys.first())
                    .map(|k| k.asset);
                if let Some(open) = open
                    && self.frame + 3. <= self.project.rig.clips[f.clip].length as f32
                {
                    match self.project.rig.clips[f.clip].release_hold(self.frame + 3.) {
                        Ok(()) => {
                            for (frame, asset) in [
                                (self.frame, open),
                                (self.frame + 1., closed),
                                (self.frame + 3., open),
                            ] {
                                f.keys.retain(|k| k.frame != frame);
                                f.keys.push(FaceKey {
                                    frame,
                                    asset,
                                    offset: Vec2::default(),
                                });
                            }
                            f.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                        }
                        Err(error) => self.status = error,
                    }
                } else {
                    self.status = "Leave at least three frames after the blink start".into();
                }
            }
            let mut remove = None;
            let old_keys = f.keys.clone();
            for (i, k) in f.keys.iter_mut().enumerate() {
                if k.frame < 1. || k.frame > self.project.rig.clips[f.clip].length as f32 {
                    continue;
                }
                ui.push_id(("facekey", i), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!("Frame {:.2}", k.frame));
                        ui.add(egui::DragValue::new(&mut k.offset.x).prefix("Look X "));
                        ui.add(egui::DragValue::new(&mut k.offset.y).prefix("Y "));
                        if ui.small_button("Delete expression key").clicked() {
                            remove = Some(i);
                        }
                    });
                    asset_combo(ui, &p.assets, "Expression image", &mut k.asset);
                });
            }
            if let Some(i) = remove {
                f.keys.remove(i);
            }
            if let Some(frame) = f
                .keys
                .iter()
                .filter(|key| !old_keys.contains(key))
                .map(|key| key.frame)
                .max_by(f32::total_cmp)
                && let Err(error) = self.project.rig.clips[f.clip].release_hold(frame)
            {
                f.keys = old_keys;
                self.status = error;
            }
            if ui.button("Remove facial track").clicked() {
                p.faces.remove(index);
                self.production.face = None;
            }
        }
        self.art_preview(ui);
    }
}
