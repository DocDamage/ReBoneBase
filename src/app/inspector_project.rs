use super::*;
impl ReboneApp {
    pub(super) fn inspector_appearance(&mut self, ui: &mut egui::Ui) {
        ui.heading("Groups & colors");
        ui.label(RichText::new("Color and draw order for the whole character.").small().color(DIM));
        let before = self.project.clone();
        for part in &mut self.project.rig.parts {
            ui.push_id(part.id, |ui| {
                ui.horizontal(|ui| {
                    ui.color_edit_button_srgba_unmultiplied(&mut part.color);
                    ui.add(egui::TextEdit::singleline(&mut part.name).desired_width(85.));
                    ui.add(egui::DragValue::new(&mut part.z).prefix("z ").range(-100..=100)).on_hover_text("Higher values draw in front");
                });
            });
        }
        if ui.add_enabled(self.project.rig.parts.len() < 256, egui::Button::new("+ New group")).clicked() {
            let id = self.project.rig.add_part("New group", [119, 225, 187, 255]);
            if let Some(bone) = self.selected.and_then(|id| self.project.rig.bone_mut(id)) { bone.part = id; }
        }
        self.commit_panel_edit(before);
        ui.separator();
        if ui.button("Skin, outlines & shading…").clicked() { self.open_studio_tab(studio::Tab::Appearance); }
        if ui.button("Outfits & palettes…").clicked() { self.open_production_tab(production_ui::Tab::Looks); }
        if ui.button("Props & artwork…").clicked() { self.open_production_tab(production_ui::Tab::Props); }
    }
    pub(super) fn inspector_project(&mut self, ui: &mut egui::Ui) {
        ui.heading("Project settings");
        let before = self.project.clone();
        ui.label("Project name");
        ui.add(egui::TextEdit::singleline(&mut self.project.rig.name).desired_width(f32::INFINITY));
        self.commit_panel_edit(before);
        ui.add_space(8.);
        egui::CollapsingHeader::new("Canvas & symmetry").default_open(true).show(ui, |ui| {
            let before = self.project.clone();
            ui.checkbox(&mut self.project.extras.auto_canvas, "Grow canvas with character");
            let (mut width, mut height) = (self.project.rig.width, self.project.rig.height);
            ui.add_enabled_ui(!self.project.extras.auto_canvas, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Pixels");
                    ui.add(egui::DragValue::new(&mut width).range(1..=4096));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut height).range(1..=4096));
                });
            });
            if (width, height) != (self.project.rig.width, self.project.rig.height) {
                let origin = self.project.extras.canvas_origin;
                if let Err(error) = rebonebase::canvas::set_bounds(&mut self.project, origin, width, height) { self.status = error.to_string(); }
            }
            ui.checkbox(&mut self.project.rig.mirror.on, "Mirror rest-pose gestures");
            row(ui, "Mirror X", &mut self.project.rig.mirror.x, -4096. ..=4096.);
            self.commit_panel_edit(before);
            if ui.button("Fit canvas").on_hover_text(self.action_shortcut(rebonebase::workspace::Action::Fit)).clicked() { self.fit = true; }
        });
        egui::CollapsingHeader::new("Pose library").show(ui, |ui| self.inspector_poses(ui));
        egui::CollapsingHeader::new("Reference image").show(ui, |ui| {
            if ui.button("Import image…").clicked() { self.import_reference(); }
            let before = self.project.clone();
            if let Some(reference) = &mut self.project.reference {
                row(ui, "X", &mut reference.x, -4096. ..=4096.);
                row(ui, "Y", &mut reference.y, -4096. ..=4096.);
                row(ui, "Scale", &mut reference.scale, 0.001..=100.);
                row(ui, "Opacity", &mut reference.opacity, 0. ..=1.);
                if ui.button("Remove reference").clicked() {
                    self.project.reference = None;
                    self.ref_tex = None;
                }
            } else { ui.label(RichText::new("No reference image loaded.").small().color(DIM)); }
            self.commit_panel_edit(before);
            if ui.button("Reference sequences…").clicked() { self.open_studio_tab(studio::Tab::References); }
        });
        ui.separator();
        if ui.button("Recovery & files…").clicked() { self.open_studio_tab(studio::Tab::Files); }
    }
    fn inspector_poses(&mut self, ui: &mut egui::Ui) {
        if ui.button("+ Capture current frame").clicked() {
            let before = self.project.clone();
            let name = format!("Pose {}", self.project.poses.len() + 1);
            match self.project.capture_pose(name, self.frame) {
                Ok(()) => { self.commit_panel_edit(before); self.status = "Pose captured".into(); }
                Err(error) => { self.project = before; self.status = error.to_string(); }
            }
        }
        for index in 0..self.project.poses.len() {
            ui.push_id(index, |ui| {
                let before = self.project.clone();
                ui.text_edit_singleline(&mut self.project.poses[index].name);
                self.commit_panel_edit(before);
                ui.horizontal_wrapped(|ui| {
                    for (label, mirror) in [("Apply", false), ("Mirror", true)] {
                        if ui.small_button(label).clicked() {
                            let before = self.project.clone();
                            match self.project.apply_pose(index, self.frame, mirror) {
                                Ok(()) => {
                                    self.seek_frame(self.frame);
                                    self.commit_panel_edit(before);
                                    self.status = "Saved pose applied".into();
                                }
                                Err(error) => { self.project = before; self.status = error.to_string(); }
                            }
                        }
                    }
                });
            });
        }
        if !self.project.poses.is_empty() && ui.button("Remove last pose").clicked() {
            let before = self.project.clone();
            self.project.poses.pop();
            self.commit_panel_edit(before);
        }
    }
}
