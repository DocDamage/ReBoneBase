use super::*;

const STUDIO_DESTINATIONS: [(Action, &str); 7] = [
    (Action::RigTools, "Rig and pins"),
    (Action::AnimationTools, "Animation and curves"),
    (Action::AppearanceTools, "Appearance"),
    (Action::AssetLibrary, "Asset library"),
    (Action::ReferenceTools, "References"),
    (Action::RecoveryTools, "Recovery and files"),
    (Action::BatchExport, "Batch export"),
];
const PRODUCTION_DESTINATIONS: [(Action, &str); 11] = [
    (Action::MotionTools, "Motion and animation templates"),
    (Action::CharacterTools, "3D characters"),
    (Action::PropTools, "Props"),
    (Action::MeshTools, "Meshes"),
    (Action::OutfitTools, "Outfits"),
    (Action::FaceTools, "Face"),
    (Action::SceneTools, "Scene"),
    (Action::AudioTools, "Audio and events"),
    (Action::PixelBoxTools, "Pixels and boxes"),
    (Action::GameExport, "Game export"),
    (
        Action::WorkspaceSettings,
        "Workspace settings and shortcuts",
    ),
];

impl ReboneApp {
    pub(in crate::app) fn action_shortcut(&self, action: Action) -> String {
        self.studio
            .preferences
            .bindings
            .iter()
            .find(|binding| binding.action == action)
            .map_or_else(String::new, workspace::Binding::chord)
    }

    pub(in crate::app) fn open_studio_tab(&mut self, tab: studio::Tab) {
        self.studio.tab = tab;
        self.studio.open = true;
        if tab == studio::Tab::Animation {
            self.mode = Mode::Pose;
            if matches!(self.tool, Tool::Bone | Tool::Radius | Tool::Bulge) {
                self.tool = Tool::Select;
            }
            self.changed();
        }
    }

    pub(in crate::app) fn open_production_tab(&mut self, tab: production_ui::Tab) {
        self.production.tab = tab;
        self.set_panel_visible(Panel::Production, true);
    }

    pub(in crate::app) fn workspace_menu(&mut self, ui: &mut egui::Ui) {
        self.apply_pending_workspace_layout(ui.ctx());
        ui.menu_button("Workspace", |ui| {
            ui.label(RichText::new(&self.studio.preferences.workspace.name).color(DIM));
            ui.menu_button("Layouts", |ui| {
                for (preset, action) in [
                    (WorkspacePreset::Rigging, Action::RiggingWorkspace),
                    (WorkspacePreset::Animation, Action::AnimationWorkspace),
                    (WorkspacePreset::Scene, Action::SceneWorkspace),
                ] {
                    self.workspace_menu_action(ui, action, preset.label());
                }
                if !self.studio.preferences.workspaces.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("Saved layouts").small().color(DIM));
                    let saved: Vec<_> = self
                        .studio
                        .preferences
                        .workspaces
                        .iter()
                        .map(|layout| layout.name.clone())
                        .collect();
                    for (index, name) in saved.iter().enumerate() {
                        if ui.button(name).clicked() {
                            self.load_saved_workspace(index);
                            ui.close();
                        }
                    }
                }
            });
            ui.menu_button("Panels", |ui| {
                for panel in Panel::ALL {
                    let mut visible = self.panel_visible(panel);
                    if ui.checkbox(&mut visible, panel.label()).changed() {
                        self.set_panel_visible(panel, visible);
                    }
                }
                ui.checkbox(&mut self.studio.open, "Studio tools");
                ui.separator();
                self.workspace_menu_action(
                    ui,
                    Action::RecoverPanels,
                    "Recover hidden and floating panels",
                );
                self.workspace_menu_action(ui, Action::ResetLayout, "Reset panel layout");
            });
            ui.separator();
            ui.menu_button("Studio tools", |ui| {
                for (action, label) in STUDIO_DESTINATIONS {
                    self.workspace_menu_action(ui, action, label);
                }
            });
            ui.menu_button("Production tools", |ui| {
                for (action, label) in PRODUCTION_DESTINATIONS {
                    self.workspace_menu_action(ui, action, label);
                }
            });
            ui.separator();
            self.workspace_menu_action(
                ui,
                Action::WorkspaceSettings,
                "Manage layouts and shortcuts…",
            );
            self.workspace_menu_action(ui, Action::CommandPalette, "Search commands…");
        });
        self.apply_pending_workspace_layout(ui.ctx());
    }

    fn workspace_menu_action(&mut self, ui: &mut egui::Ui, action: Action, label: &str) {
        let shortcut = self.action_shortcut(action);
        let label = if shortcut.is_empty() {
            label.to_string()
        } else {
            format!("{label}    {shortcut}")
        };
        if ui.button(label).clicked() {
            self.run_action(action);
            ui.close();
        }
    }

    fn panel_visible(&self, panel: Panel) -> bool {
        self.layout(panel).dock != Dock::Hidden
            && (panel != Panel::Production || self.production.open)
    }

    pub(super) fn set_panel_visible(&mut self, panel: Panel, visible: bool) {
        let was_visible = self.panel_visible(panel);
        let mut layout = self.layout(panel);
        if visible {
            if layout.dock == Dock::Hidden {
                layout.dock = self.workspace_ui.hidden_docks[panel.index()]
                    .filter(|dock| *dock != Dock::Hidden)
                    .unwrap_or(match panel {
                        Panel::Skeleton => Dock::Left,
                        Panel::Inspector => Dock::Right,
                        Panel::Timeline => Dock::Bottom,
                        Panel::Production => Dock::Floating,
                    });
            }
            if !was_visible {
                self.workspace_ui.pending_layout = true;
            }
        } else if panel != Panel::Production && layout.dock != Dock::Hidden {
            self.workspace_ui.hidden_docks[panel.index()] = Some(layout.dock);
            layout.dock = Dock::Hidden;
        }
        if panel == Panel::Production {
            self.production.open = visible;
            self.studio.preferences.workspace.production_open = visible;
        }
        self.set_layout(panel, layout);
    }

    pub(super) fn apply_workspace_layout(&mut self, layout: Workspace) {
        self.production.open = layout.production_open;
        self.studio.preferences.workspace = layout;
        self.workspace_ui.hidden_docks = [None; 4];
        self.workspace_ui.pending_layout = true;
    }

    pub(super) fn apply_workspace_preset(&mut self, preset: WorkspacePreset) {
        self.apply_workspace_layout(preset.layout());
        if self.production.open {
            self.production.tab = production_ui::Tab::Scene;
        }
    }

    pub(super) fn load_saved_workspace(&mut self, index: usize) {
        if let Some(layout) = self.studio.preferences.workspaces.get(index).cloned() {
            self.production.layout_index = index;
            self.apply_workspace_layout(layout);
        }
    }

    pub(super) fn recover_workspace_panels(&mut self) {
        for panel in Panel::ALL {
            self.set_panel_visible(panel, true);
        }
        self.workspace_ui.pending_layout = true;
    }

    pub(super) fn apply_pending_workspace_layout(&mut self, ctx: &egui::Context) {
        if !self.workspace_ui.pending_layout {
            return;
        }
        let available = ctx.content_rect().shrink(8.);
        for panel in Panel::ALL {
            let mut layout = self.layout(panel);
            if layout.dock == Dock::Floating {
                layout.width = layout.width.clamp(160., available.width().max(160.));
                layout.height = layout.height.clamp(120., available.height().max(120.));
                layout.x = layout.x.clamp(
                    available.left(),
                    (available.right() - layout.width).max(available.left()),
                );
                layout.y = layout.y.clamp(
                    available.top(),
                    (available.bottom() - layout.height).max(available.top()),
                );
                self.set_layout(panel, layout);
            }
        }
        self.production.apply_layout = true;
        self.workspace_ui.pending_layout = false;
    }

    pub(in crate::app) fn run_action(&mut self, action: Action) {
        match action {
            Action::New => self.request(Pending::New),
            Action::Open => self.choose_open(),
            Action::Save => {
                self.save(false);
            }
            Action::SaveAs => {
                self.save(true);
            }
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Play => self.toggle_play(),
            Action::Delete => {
                if self.studio.keys.is_empty() {
                    self.delete();
                } else {
                    self.record();
                    rebonebase::editing::delete_keys(
                        self.project.rig.active_clip_mut(),
                        &self.studio.keys,
                    );
                    self.studio.keys.clear();
                }
            }
            Action::Fit => self.fit = true,
            Action::Bone => {
                self.mode = Mode::Rest;
                self.tool = Tool::Bone;
                self.changed();
            }
            Action::Select => self.tool = Tool::Select,
            Action::MoveRig => self.tool = Tool::MoveRig,
            Action::IK => {
                self.mode = Mode::Pose;
                self.tool = Tool::IK;
                self.changed();
            }
            Action::Radius => {
                self.mode = Mode::Rest;
                self.tool = Tool::Radius;
                self.changed();
            }
            Action::NextFrame => self.seek_frame(self.frame + 1.),
            Action::PreviousFrame => self.seek_frame(self.frame - 1.),
            Action::FirstFrame => self.seek_frame(1.),
            Action::LastFrame => self.seek_frame(self.project.rig.active_clip().length as f32),
            Action::NextKey => self.seek_key(true),
            Action::PreviousKey => self.seek_key(false),
            Action::KeyPose => self.key_selected(),
            Action::CopyKeys => self.copy_timeline_keys(),
            Action::PasteKeys => self.paste_timeline_keys(true),
            Action::CyclePick => {
                if !self.studio.pick_ids.is_empty() {
                    self.studio.pick_cycle =
                        (self.studio.pick_cycle + 1) % self.studio.pick_ids.len();
                    self.select_bone(self.studio.pick_ids[self.studio.pick_cycle], false);
                }
            }
            Action::Studio => self.studio.open = !self.studio.open,
            Action::Production => {
                self.set_panel_visible(Panel::Production, !self.panel_visible(Panel::Production));
            }
            Action::CommandPalette => {
                self.production.palette_open = true;
                self.production.palette_focus = true;
                self.production.palette_query.clear();
                self.workspace_ui.palette_index = 0;
                self.workspace_ui.palette_query.clear();
                self.workspace_ui.palette_scroll = true;
            }
            Action::Paint => {
                self.mode = Mode::Pose;
                self.tool = Tool::Pixel;
                self.changed();
            }
            Action::Boxes => {
                self.mode = Mode::Pose;
                self.tool = Tool::Box;
                self.changed();
            }
            Action::ShowSkeleton => self.set_panel_visible(Panel::Skeleton, true),
            Action::ShowInspector => self.set_panel_visible(Panel::Inspector, true),
            Action::ShowTimeline => self.set_panel_visible(Panel::Timeline, true),
            Action::RecoverPanels => self.recover_workspace_panels(),
            Action::ResetLayout => self.apply_workspace_layout(Workspace::default()),
            Action::RiggingWorkspace => self.apply_workspace_preset(WorkspacePreset::Rigging),
            Action::AnimationWorkspace => self.apply_workspace_preset(WorkspacePreset::Animation),
            Action::SceneWorkspace => self.apply_workspace_preset(WorkspacePreset::Scene),
            Action::RigTools => self.open_studio_tab(studio::Tab::Rig),
            Action::AnimationTools => self.open_studio_tab(studio::Tab::Animation),
            Action::AppearanceTools => self.open_studio_tab(studio::Tab::Appearance),
            Action::AssetLibrary => self.open_studio_tab(studio::Tab::Library),
            Action::ReferenceTools => self.open_studio_tab(studio::Tab::References),
            Action::RecoveryTools => self.open_studio_tab(studio::Tab::Files),
            Action::BatchExport => self.open_studio_tab(studio::Tab::Export),
            Action::MotionTools => self.open_production_tab(production_ui::Tab::Motion),
            Action::CharacterTools => self.open_production_tab(production_ui::Tab::Characters),
            Action::PropTools => self.open_production_tab(production_ui::Tab::Props),
            Action::MeshTools => self.open_production_tab(production_ui::Tab::Meshes),
            Action::OutfitTools => self.open_production_tab(production_ui::Tab::Looks),
            Action::FaceTools => self.open_production_tab(production_ui::Tab::Face),
            Action::SceneTools => self.open_production_tab(production_ui::Tab::Scene),
            Action::AudioTools => self.open_production_tab(production_ui::Tab::AudioEvents),
            Action::PixelBoxTools => self.open_production_tab(production_ui::Tab::PixelsBoxes),
            Action::GameExport => self.open_production_tab(production_ui::Tab::Export),
            Action::WorkspaceSettings => self.open_production_tab(production_ui::Tab::Workspace),
            Action::Cancel => {
                if let Some(g) = self.gesture.take() {
                    self.project = g.base;
                    self.changed();
                }
                self.playing = false;
                self.production.player.stop();
                self.production.palette_open = false;
                self.studio.key_drag = None;
                self.studio.curve_drag = None;
                self.studio.marquee = None;
            }
        }
    }
}
