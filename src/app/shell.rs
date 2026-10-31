use super::*;
use rebonebase::workspace::Action;

impl ReboneApp {
    pub(super) fn top(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(4.);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.;
                ui.label(RichText::new("RE").size(21.).color(ACCENT).strong());
                ui.label(RichText::new("BONEBASE").size(21.).strong());
                ui.separator();
                self.file_menu(ui);
                self.workspace_menu(ui);
                if ui.add_enabled(self.history.can_undo(), egui::Button::new("Undo"))
                    .on_hover_text(self.action_shortcut(Action::Undo)).clicked()
                {
                    self.undo();
                }
                if ui.add_enabled(self.history.can_redo(), egui::Button::new("Redo"))
                    .on_hover_text(self.action_shortcut(Action::Redo)).clicked()
                {
                    self.redo();
                }
                if ui.button("Studio").on_hover_text(self.action_shortcut(Action::Studio)).clicked() {
                    self.run_action(Action::Studio);
                }
                if ui.button("Production").on_hover_text(self.action_shortcut(Action::Production)).clicked() {
                    self.run_action(Action::Production);
                }
                if ui.button("Commands").on_hover_text(self.action_shortcut(Action::CommandPalette)).clicked() {
                    self.run_action(Action::CommandPalette);
                }
                if ui.button(RichText::new("Save project").color(ACCENT))
                    .on_hover_text(self.action_shortcut(Action::Save)).clicked()
                {
                    self.save(false);
                }
                self.export_menu(ui);
                if ui.button("Help").clicked() {
                    self.help = true;
                }
            });
            ui.horizontal(|ui| {
                let state = if self.studio.recovered {
                    "Recovered · save to keep changes"
                } else if self.dirty() {
                    "Unsaved changes"
                } else if self.path.is_some() {
                    "Saved"
                } else {
                    "Not saved yet"
                };
                let color = if self.dirty() { Color32::from_rgb(255, 209, 105) } else { DIM };
                let name_width = (ui.available_width() - 420.).max(140.);
                let path = self.path.as_ref().map_or_else(|| "Choose Save project to create a .rebone file.".into(), |p| p.display().to_string());
                ui.add_sized([name_width, 20.], egui::Label::new(RichText::new(&self.project.rig.name).strong()).truncate())
                    .on_hover_text(path);
                ui.label(RichText::new(state).small().color(color));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{} workspace", self.studio.preferences.workspace.name)).small().color(DIM));
                });
            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 285.).max(100.);
                ui.add_sized([width, 20.], egui::Label::new(RichText::new(&self.status).small().color(DIM)).truncate())
                    .on_hover_text(&self.status);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{} × {} px · {} bones · {:.0}%", self.project.rig.width, self.project.rig.height, self.project.rig.bones.len(), self.zoom * 100.)).small().color(DIM));
                });
            });
        });
    }
    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            for (action, label) in [
                (Action::New, "New project"),
                (Action::Open, "Open…"),
                (Action::Save, "Save"),
                (Action::SaveAs, "Save as…"),
            ] {
                if ui.add(egui::Button::new(label).shortcut_text(self.action_shortcut(action))).clicked() {
                    self.run_action(action);
                    ui.close();
                }
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
            if ui.add_enabled(self.production.character.is_some(), egui::Button::new("Import animation for 3D character…")).clicked() {
                self.import_character_animation();
                ui.close();
            }
            if ui.button("Import 3D character…").clicked() {
                self.import_character3d();
                ui.close();
            }
        });
    }
    fn export_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(RichText::new("Export ↗").color(ACCENT), |ui| {
            for (kind, label) in [
                ("frame", "Current frame · PNG"),
                ("indexed", "Current frame · indexed PNG"),
                ("sheet", "Sprite sheet · PNG"),
                ("gif", "Animation · GIF"),
                ("aseprite", "Layered animation · Aseprite"),
                ("aseall", "All clips · Aseprite tags"),
            ] {
                if ui.button(label).clicked() {
                    self.export(kind);
                    ui.close();
                }
            }
            if ui.button("Batch export presets…").clicked() {
                self.open_studio_tab(studio::Tab::Export);
                ui.close();
            }
            ui.horizontal(|ui| {
                ui.label("Sheet columns");
                ui.add(egui::DragValue::new(&mut self.sheet_cols).range(1..=32));
            });
        });
    }
}
