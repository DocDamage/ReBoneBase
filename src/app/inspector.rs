use super::editor_panels::InspectorTab;
use super::production_ui::PaintMode;
use super::*;
use rebonebase::model::Bone;

impl ReboneApp {
    pub(super) fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.);
        ui.label(RichText::new("INSPECTOR").small().color(ACCENT).strong());
        ui.horizontal_wrapped(|ui| {
            for (tab, label) in [
                (InspectorTab::Selection, "Selection"),
                (InspectorTab::Appearance, "Appearance"),
                (InspectorTab::Project, "Project"),
            ] {
                ui.selectable_value(&mut self.panels.inspector, tab, label);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt(("inspector", self.panels.inspector as u8))
            .auto_shrink([false, false])
            .show(ui, |ui| match self.panels.inspector {
                InspectorTab::Selection => self.inspector_selection(ui),
                InspectorTab::Appearance => self.inspector_appearance(ui),
                InspectorTab::Project => self.inspector_project(ui),
            });
    }
    // Panel controls operate on a candidate and only publish valid edits to history.
    pub(super) fn commit_panel_edit(&mut self, before: Project) {
        if self.project == before {
            return;
        }
        if let Err(error) = self.project.validate() {
            self.project = before;
            self.status = format!("Change rejected: {error}");
            return;
        }
        self.history.record(&before);
        self.changed();
    }
    fn inspector_selection(&mut self, ui: &mut egui::Ui) {
        if matches!(self.tool, Tool::Pixel | Tool::Box) {
            self.inspector_canvas_tool(ui);
            return;
        }
        let Some(bone) = self
            .selected
            .and_then(|id| self.project.rig.bone(id))
            .cloned()
        else {
            ui.heading("Select a bone");
            ui.label("Choose a bone on the canvas or in the skeleton tree to edit it here.");
            ui.add_space(8.);
            ui.label(
                RichText::new(if self.mode == Mode::Rest {
                    "Rig mode changes the character's rest pose and shape."
                } else {
                    "Animate mode writes pose changes as keys at the current frame."
                })
                .color(DIM),
            );
            if ui.button("Rig & pins…").clicked() {
                self.open_studio_tab(studio::Tab::Rig);
            }
            return;
        };
        let count = self.selected_bones().len();
        if count > 1 {
            ui.label(
                RichText::new(format!("{count} bones selected"))
                    .color(ACCENT)
                    .strong(),
            );
            ui.label(format!(
                "These properties edit {}. Use Rig & pins for group transforms.",
                bone.name
            ));
        }
        ui.heading(if self.mode == Mode::Rest {
            "Rest pose"
        } else {
            "Animated pose"
        });
        if self.mode == Mode::Pose {
            ui.label(
                RichText::new(format!("Frame {} · auto-key is on", self.frame as u32))
                    .color(ACCENT),
            );
        }
        let before = self.project.clone();
        let mut edited = bone.clone();
        ui.checkbox(&mut edited.locked, "Lock bone")
            .on_hover_text("Lock direct canvas and inspector edits.");
        ui.add_enabled_ui(!edited.locked, |ui| {
            ui.add(egui::TextEdit::singleline(&mut edited.name).desired_width(f32::INFINITY));
        });
        let parent = bone.parent.and_then(|id| self.project.rig.bone(id));
        ui.label(
            RichText::new(match parent {
                Some(parent) => format!("ID {} · child of {}", bone.id, parent.name),
                None => format!("ID {} · root anchor", bone.id),
            })
            .small()
            .color(DIM),
        );
        ui.add_space(5.);
        ui.add_enabled_ui(!edited.locked, |ui| {
            if self.mode == Mode::Rest {
                self.inspector_rest_bone(ui, &mut edited);
            } else {
                self.inspector_pose_bone(ui, &edited);
            }
        });
        if let Some(target) = self.project.rig.bone_mut(bone.id) {
            *target = edited.clone();
        }
        self.commit_panel_edit(before);
        if edited.locked {
            ui.label(
                RichText::new("Unlock this bone to change it.")
                    .small()
                    .color(DIM),
            );
        } else if self.mode == Mode::Pose {
            ui.add_space(6.);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("Key current pose")
                    .on_hover_text(self.action_shortcut(rebonebase::workspace::Action::KeyPose))
                    .clicked()
                {
                    self.key_selected();
                }
                if ui.button("Reset pose here").clicked() {
                    self.reset_inspector_pose(bone.id);
                }
            });
            ui.label(
                RichText::new("Changing rotation or scale writes a key at this frame.")
                    .small()
                    .color(DIM),
            );
        }
        ui.separator();
        if ui.button("Rig & pins…").clicked() {
            self.open_studio_tab(studio::Tab::Rig);
        }
        if self.mode == Mode::Pose && ui.button("Clip & key tools…").clicked() {
            self.open_studio_tab(studio::Tab::Animation);
        }
    }
    fn inspector_rest_bone(&mut self, ui: &mut egui::Ui, bone: &mut Bone) {
        ui.add_enabled_ui(!bone.length_locked, |ui| {
            row(ui, "Length", &mut bone.length, 1. ..=512.)
        });
        row(ui, "Angle", &mut bone.angle, -360. ..=360.);
        row(ui, "Head radius", &mut bone.radius_head, 0. ..=64.);
        row(ui, "Tail radius", &mut bone.radius_tail, 0. ..=64.);
        if !bone.connected {
            ui.add_enabled_ui(false, |ui| {
                row(ui, "Head X", &mut bone.x, -4096. ..=4096.);
                row(ui, "Head Y", &mut bone.y, -4096. ..=4096.);
            });
        }
        if bone.parent.is_none() {
            ui.label(
                RichText::new("Use Move Rig to reposition the root anchor.")
                    .small()
                    .color(DIM),
            );
        }
        egui::ComboBox::from_label("Group")
            .width(110.)
            .selected_text(
                self.project
                    .rig
                    .parts
                    .iter()
                    .find(|p| p.id == bone.part)
                    .map_or("None", |p| p.name.as_str()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut bone.part, 0, "None");
                for part in &self.project.rig.parts {
                    ui.selectable_value(&mut bone.part, part.id, &part.name);
                }
            });
        if !bone.bulges_left.is_empty() || !bone.bulges_right.is_empty() {
            egui::CollapsingHeader::new("Skin bulges").show(ui, |ui| {
                for (label, list) in [("L", &mut bone.bulges_left), ("R", &mut bone.bulges_right)] {
                    for (i, bulge) in list.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{label}{}", i + 1));
                            ui.add(
                                egui::DragValue::new(&mut bulge.t)
                                    .range(0.01..=0.99)
                                    .speed(0.01),
                            );
                            ui.add(
                                egui::DragValue::new(&mut bulge.d)
                                    .range(-64. ..=64.)
                                    .speed(0.1),
                            );
                        });
                    }
                    list.sort_by(|a, b| a.t.total_cmp(&b.t));
                }
                if ui.button("Clear bulges").clicked() {
                    bone.bulges_left.clear();
                    bone.bulges_right.clear();
                }
            });
        }
    }
    fn inspector_pose_bone(&mut self, ui: &mut egui::Ui, bone: &Bone) {
        let original = self
            .project
            .rig
            .active_clip()
            .sample(self.frame)
            .get(&bone.id)
            .copied()
            .unwrap_or_default();
        let mut pose = original;
        row(
            ui,
            "Rotation",
            &mut pose.rotation,
            bone.rotation_min.unwrap_or(-720.)..=bone.rotation_max.unwrap_or(720.),
        );
        ui.add_enabled_ui(false, |ui| {
            row(ui, "Offset X", &mut pose.x, -4096. ..=4096.);
            row(ui, "Offset Y", &mut pose.y, -4096. ..=4096.);
        });
        ui.add_enabled_ui(!bone.length_locked, |ui| {
            row(ui, "Scale", &mut pose.scale, 0.1..=4.)
        });
        if pose != original {
            pose.rotation = bone.clamp_rotation(pose.rotation);
            pose.scale = bone.clamp_scale(pose.scale);
            let clip = self.project.rig.active_clip_mut();
            let previous = clip.clone();
            for (channel, value, old) in [
                (Channel::Rotation, pose.rotation, original.rotation),
                (Channel::X, pose.x, original.x),
                (Channel::Y, pose.y, original.y),
                (Channel::Scale, pose.scale, original.scale),
            ] {
                if value != old
                    && let Err(error) = clip.set_key(bone.id, channel, self.frame, value)
                {
                    *clip = previous;
                    self.status = format!("Could not key pose: {error}");
                    break;
                }
            }
        }
    }
    fn reset_inspector_pose(&mut self, id: u32) {
        let before = self.project.clone();
        let Some(bone) = self.project.rig.bone(id) else {
            return;
        };
        let values = [
            (Channel::Rotation, bone.clamp_rotation(0.)),
            (Channel::X, 0.),
            (Channel::Y, 0.),
            (Channel::Scale, bone.clamp_scale(1.)),
        ];
        for (channel, value) in values {
            if let Err(error) = self
                .project
                .rig
                .active_clip_mut()
                .set_key(id, channel, self.frame, value)
            {
                self.project = before;
                self.status = format!("Could not reset pose: {error}");
                return;
            }
        }
        self.commit_panel_edit(before);
        self.status = format!("Pose reset at frame {}", self.frame as u32);
    }
    fn inspector_canvas_tool(&mut self, ui: &mut egui::Ui) {
        ui.heading(if self.tool == Tool::Pixel {
            "Pixel corrections"
        } else {
            "Collision regions"
        });
        ui.label(
            RichText::new(format!(
                "{} · frame {}",
                self.project.rig.active_clip().name,
                self.frame as u32
            ))
            .color(ACCENT),
        );
        if self.tool == Tool::Pixel {
            ui.horizontal_wrapped(|ui| {
                ui.color_edit_button_srgba_unmultiplied(&mut self.production.pixel_color);
                ui.add(
                    egui::DragValue::new(&mut self.production.brush)
                        .range(1..=8)
                        .prefix("Brush "),
                );
            });
            for (mode, label) in [
                (PaintMode::Paint, "Paint"),
                (PaintMode::Erase, "Erase ink"),
                (PaintMode::Restore, "Restore generated"),
            ] {
                ui.selectable_value(&mut self.production.paint_mode, mode, label);
            }
            ui.label("Corrections belong to this clip and frame. Restore removes a correction and reveals the generated artwork.");
        } else {
            ui.checkbox(&mut self.production.show_boxes, "Show collision overlays");
            ui.checkbox(
                &mut self.production.box_attached,
                "Attach new region to selected bone",
            );
            ui.horizontal_wrapped(|ui| {
                for kind in [
                    rebonebase::production::BoxKind::Hurt,
                    rebonebase::production::BoxKind::Hit,
                    rebonebase::production::BoxKind::Interaction,
                ] {
                    ui.selectable_value(&mut self.production.box_kind, kind, format!("{kind:?}"));
                }
            });
            ui.label("Drag on the canvas to create a region. Edit its geometry and timing in Pixels & boxes.");
        }
        if ui.button("Pixels & boxes…").clicked() {
            self.open_production_tab(production_ui::Tab::PixelsBoxes);
        }
        if ui.button("Return to bone selection").clicked() {
            self.tool = Tool::Select;
        }
    }
}
