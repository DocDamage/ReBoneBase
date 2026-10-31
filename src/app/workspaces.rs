use super::*;
use rebonebase::workspace::{self, Action, Dock, PanelLayout, Workspace, WorkspacePreset};

pub(super) struct WorkspaceUi {
    palette_index: usize,
    palette_query: String,
    palette_scroll: bool,
    pending_layout: bool,
    hidden_docks: [Option<Dock>; 4],
}
impl Default for WorkspaceUi {
    fn default() -> Self {
        Self {
            palette_index: 0,
            palette_query: String::new(),
            palette_scroll: true,
            pending_layout: true,
            hidden_docks: [None; 4],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Panel {
    Skeleton,
    Inspector,
    Timeline,
    Production,
}
impl Panel {
    const ALL: [Self; 4] = [
        Self::Skeleton,
        Self::Inspector,
        Self::Timeline,
        Self::Production,
    ];

    fn index(self) -> usize {
        match self {
            Self::Skeleton => 0,
            Self::Inspector => 1,
            Self::Timeline => 2,
            Self::Production => 3,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Skeleton => "Skeleton",
            Self::Inspector => "Inspector",
            Self::Timeline => "Timeline",
            Self::Production => "Production tools",
        }
    }
}

mod navigation;
mod palette;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy)]
enum PaletteCommand {
    Action(Action),
    SavedLayout(usize),
}

struct PaletteEntry {
    command: PaletteCommand,
    label: String,
    shortcut: String,
}

impl ReboneApp {
    pub(super) fn production_workspace(&mut self, ui: &mut egui::Ui) {
        ui.heading("Docking, saved layouts and keyboard commands");
        ui.label("Choose a dock edge or a floating window for each panel. Resize dock dividers, then save the layout.");
        let workspace = &mut self.studio.preferences.workspace;
        ui.text_edit_singleline(&mut workspace.name);
        for (name, panel) in [
            ("Skeleton", &mut workspace.skeleton),
            ("Inspector", &mut workspace.inspector),
            ("Timeline", &mut workspace.timeline),
            ("Production", &mut workspace.production),
        ] {
            ui.horizontal(|ui| {
                ui.label(name);
                egui::ComboBox::from_id_salt(("dock", name))
                    .selected_text(format!("{:?}", panel.dock))
                    .show_ui(ui, |ui| {
                        for dock in [
                            Dock::Left,
                            Dock::Right,
                            Dock::Bottom,
                            Dock::Floating,
                            Dock::Hidden,
                        ] {
                            if ui
                                .selectable_value(&mut panel.dock, dock, format!("{dock:?}"))
                                .clicked()
                            {
                                self.workspace_ui.pending_layout = true;
                            }
                        }
                    });
            });
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Save current layout").clicked() {
                self.studio.preferences.workspace.production_open = self.production.open;
                let layout = self.studio.preferences.workspace.clone();
                if let Some(i) = self
                    .studio
                    .preferences
                    .workspaces
                    .iter()
                    .position(|w| w.name == layout.name)
                {
                    self.studio.preferences.workspaces[i] = layout;
                } else if self.studio.preferences.workspaces.len() < 32 {
                    self.studio.preferences.workspaces.push(layout);
                }
                self.save_preferences();
            }
            if ui.button("Reset panel layout").clicked() {
                self.apply_workspace_layout(Workspace::default());
            }
            if ui.button("Recover hidden and floating panels").clicked() {
                self.recover_workspace_panels();
            }
        });
        egui::ComboBox::from_label("Saved layout")
            .selected_text(
                self.studio
                    .preferences
                    .workspaces
                    .get(self.production.layout_index)
                    .map_or("Choose layout", |w| w.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for (i, w) in self.studio.preferences.workspaces.iter().enumerate() {
                    ui.selectable_value(&mut self.production.layout_index, i, &w.name);
                }
            });
        if ui.button("Load saved layout").clicked()
            && let Some(layout) = self
                .studio
                .preferences
                .workspaces
                .get(self.production.layout_index)
                .cloned()
        {
            self.apply_workspace_layout(layout);
            self.save_preferences();
        }
        ui.separator();
        ui.label(RichText::new("Remappable shortcuts").strong());
        let previous = self.production.binding_action;
        egui::ComboBox::from_label("Command")
            .selected_text(self.production.binding_action.label())
            .show_ui(ui, |ui| {
                for action in Action::ALL {
                    ui.selectable_value(
                        &mut self.production.binding_action,
                        action,
                        action.label(),
                    );
                }
            });
        if previous != self.production.binding_action {
            if let Some(b) = self
                .studio
                .preferences
                .bindings
                .iter()
                .find(|b| b.action == self.production.binding_action)
            {
                self.production.binding_key = b.key.clone();
                self.production.binding_ctrl = b.ctrl;
                self.production.binding_shift = b.shift;
                self.production.binding_alt = b.alt;
            } else {
                self.production.binding_key.clear();
                self.production.binding_ctrl = false;
                self.production.binding_shift = false;
                self.production.binding_alt = false;
            }
        }
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.production.binding_key).desired_width(100.),
            );
            ui.checkbox(&mut self.production.binding_ctrl, "Ctrl");
            ui.checkbox(&mut self.production.binding_shift, "Shift");
            ui.checkbox(&mut self.production.binding_alt, "Alt");
        });
        ui.label("Examples: M, Space, ArrowRight, F3. Letter keys are entered without modifiers in the key field.");
        ui.horizontal(|ui| {
            if ui.button("Apply shortcut").clicked() {
                let mut bindings = self.studio.preferences.bindings.clone();
                bindings.retain(|b| b.action != self.production.binding_action);
                let mut key = self.production.binding_key.trim().to_string();
                if key.len() == 1 {
                    key = key.to_ascii_uppercase();
                }
                bindings.push(workspace::Binding {
                    action: self.production.binding_action,
                    key,
                    ctrl: self.production.binding_ctrl,
                    shift: self.production.binding_shift,
                    alt: self.production.binding_alt,
                });
                match workspace::validate_bindings(&bindings) {
                    Ok(()) => {
                        self.studio.preferences.bindings = bindings;
                        self.save_preferences();
                        self.status = "Shortcut saved".into();
                    }
                    Err(e) => self.status = e.to_string(),
                }
            }
            if ui.button("Restore default shortcuts").clicked() {
                self.studio.preferences.bindings = workspace::default_bindings();
                self.save_preferences();
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("binding_list")
            .max_height(200.)
            .show(ui, |ui| {
                for b in &self.studio.preferences.bindings {
                    ui.label(format!("{} — {}", b.chord(), b.action.label()));
                }
            });
        if ui.button("Open command palette").clicked() {
            self.run_action(Action::CommandPalette);
        }
    }
    fn layout(&self, panel: Panel) -> PanelLayout {
        let w = &self.studio.preferences.workspace;
        match panel {
            Panel::Skeleton => w.skeleton.clone(),
            Panel::Inspector => w.inspector.clone(),
            Panel::Timeline => w.timeline.clone(),
            Panel::Production => w.production.clone(),
        }
    }
    fn set_layout(&mut self, panel: Panel, layout: PanelLayout) {
        let w = &mut self.studio.preferences.workspace;
        *match panel {
            Panel::Skeleton => &mut w.skeleton,
            Panel::Inspector => &mut w.inspector,
            Panel::Timeline => &mut w.timeline,
            Panel::Production => &mut w.production,
        } = layout;
    }
    pub(super) fn dock_panel(
        &mut self,
        ctx: &egui::Context,
        panel: Panel,
        content: fn(&mut Self, &mut egui::Ui),
    ) {
        let mut layout = self.layout(panel);
        let (id, title) = match panel {
            Panel::Skeleton => ("assets", "Skeleton"),
            Panel::Inspector => ("inspector", "Inspector"),
            Panel::Timeline => ("timeline", "Timeline"),
            Panel::Production => ("production_dock", "Production"),
        };
        let available = ctx.content_rect();
        match layout.dock {
            Dock::Hidden => {}
            Dock::Floating => {
                if matches!(panel, Panel::Production) {
                    return;
                }
                let mut open = true;
                let mut window = egui::Window::new(title)
                    .id(egui::Id::new((id, "floating")))
                    .open(&mut open)
                    .default_pos([layout.x, layout.y])
                    .default_size([layout.width, layout.height])
                    .max_height((available.height() - 30.).max(120.));
                if self.production.apply_layout {
                    window = window
                        .current_pos([layout.x, layout.y])
                        .fixed_size([layout.width, layout.height]);
                }
                if let Some(response) = window.show(ctx, |ui| content(self, ui)) {
                    let rect = response.response.rect;
                    layout.x = rect.left();
                    layout.y = rect.top();
                    layout.width = rect.width().max(160.);
                    layout.height = rect.height().max(120.);
                }
                if !open {
                    self.workspace_ui.hidden_docks[panel.index()] = Some(Dock::Floating);
                    layout.dock = Dock::Hidden;
                }
            }
            Dock::Left | Dock::Right => {
                let mut side = if layout.dock == Dock::Left {
                    egui::SidePanel::left(id)
                } else {
                    egui::SidePanel::right(id)
                }
                .default_width(layout.width)
                .min_width(160.)
                .max_width((available.width() * 0.6).max(160.))
                .resizable(true);
                if self.production.apply_layout {
                    side = side.exact_width(layout.width.min((available.width() * 0.6).max(160.)));
                }
                let response = side.show(ctx, |ui| content(self, ui));
                layout.width = response.response.rect.width().max(160.);
            }
            Dock::Bottom => {
                let mut bottom = egui::TopBottomPanel::bottom(id)
                    .default_height(layout.height)
                    .min_height(120.)
                    .max_height((available.height() * 0.65).max(120.))
                    .resizable(true);
                if self.production.apply_layout {
                    bottom = bottom
                        .exact_height(layout.height.min((available.height() * 0.65).max(120.)));
                }
                let response = bottom.show(ctx, |ui| content(self, ui));
                layout.height = response.response.rect.height().max(120.);
            }
        }
        self.set_layout(panel, layout);
    }
    pub(super) fn production_dock(&mut self, ctx: &egui::Context) {
        if self.production.open
            && self.studio.preferences.workspace.production.dock != Dock::Floating
        {
            self.dock_panel(ctx, Panel::Production, Self::production_ui);
        }
    }
}
