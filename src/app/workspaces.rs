use super::*;
use rebonebase::workspace::{self, Action, Dock, PanelLayout};
#[derive(Clone, Copy)]
pub(super) enum Panel {
    Skeleton,
    Inspector,
    Timeline,
    Production,
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
                                self.production.apply_layout = true;
                            }
                        }
                    });
            });
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Save current layout").clicked() {
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
                self.studio.preferences.workspace = Default::default();
                self.production.apply_layout = true;
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
            self.studio.preferences.workspace = layout;
            self.production.apply_layout = true;
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
        if previous != self.production.binding_action
            && let Some(b) = self
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
    pub(super) fn run_action(&mut self, action: Action) {
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
            Action::NextFrame => {
                self.frame = (self.frame + 1.).min(self.project.rig.active_clip().length as f32);
                self.mode = Mode::Pose;
                self.changed();
            }
            Action::PreviousFrame => {
                self.frame = (self.frame - 1.).max(1.);
                self.mode = Mode::Pose;
                self.changed();
            }
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
                if self.studio.preferences.workspace.production.dock == Dock::Hidden {
                    self.studio.preferences.workspace.production.dock = Dock::Floating;
                    self.production.open = true;
                } else {
                    self.production.open = !self.production.open;
                }
            }
            Action::CommandPalette => {
                self.production.palette_open = true;
                self.production.palette_focus = true;
                self.production.palette_query.clear();
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
    pub(super) fn command_palette(&mut self, ctx: &egui::Context) {
        if !self.production.palette_open {
            return;
        }
        let mut open = true;
        let mut dismiss = false;
        let mut action = None;
        egui::Window::new("Commands")
            .open(&mut open)
            .default_width(470.)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                let input = ui.add(
                    egui::TextEdit::singleline(&mut self.production.palette_query)
                        .hint_text("Search commands…")
                        .desired_width(f32::INFINITY),
                );
                if self.production.palette_focus {
                    input.request_focus();
                    self.production.palette_focus = false;
                }
                let query = self.production.palette_query.to_lowercase();
                let commands: Vec<_> = Action::ALL
                    .into_iter()
                    .filter(|a| a.label().to_lowercase().contains(&query))
                    .collect();
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    dismiss = true;
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    action = commands.first().copied();
                }
                egui::ScrollArea::vertical()
                    .max_height(360.)
                    .show(ui, |ui| {
                        for a in commands {
                            let key = self
                                .studio
                                .preferences
                                .bindings
                                .iter()
                                .find(|b| b.action == a)
                                .map_or(String::new(), |b| b.chord());
                            if ui.button(format!("{}    {}", a.label(), key)).clicked() {
                                action = Some(a);
                            }
                        }
                    });
            });
        self.production.palette_open = open && !dismiss;
        if let Some(a) = action {
            self.production.palette_open = false;
            self.run_action(a);
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
