use super::*;

impl ReboneApp {
    pub(super) fn palette_commands(&self) -> Vec<PaletteEntry> {
        let query = self.production.palette_query.to_lowercase();
        let words: Vec<_> = query.split_whitespace().collect();
        let mut entries: Vec<_> = Action::ALL
            .into_iter()
            .map(|action| PaletteEntry {
                command: PaletteCommand::Action(action),
                label: action.label().into(),
                shortcut: self.action_shortcut(action),
            })
            .collect();
        entries.extend(self.studio.preferences.workspaces.iter().enumerate().map(
            |(index, layout)| PaletteEntry {
                command: PaletteCommand::SavedLayout(index),
                label: format!("Load saved workspace: {}", layout.name),
                shortcut: String::new(),
            },
        ));
        entries.retain(|entry| {
            let searchable = format!("{} {}", entry.label, entry.shortcut).to_lowercase();
            words.iter().all(|word| searchable.contains(*word))
        });
        entries
    }

    pub(in crate::app) fn command_palette(&mut self, ctx: &egui::Context) {
        if !self.production.palette_open {
            return;
        }
        let mut open = true;
        let (up, down, enter, dismiss) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
            )
        });
        let mut command = None;
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
                if self.workspace_ui.palette_query != self.production.palette_query {
                    self.workspace_ui.palette_query = self.production.palette_query.clone();
                    self.workspace_ui.palette_index = 0;
                    self.workspace_ui.palette_scroll = true;
                }
                let commands = self.palette_commands();
                if !commands.is_empty() {
                    self.workspace_ui.palette_index =
                        self.workspace_ui.palette_index.min(commands.len() - 1);
                    if up {
                        self.workspace_ui.palette_index =
                            (self.workspace_ui.palette_index + commands.len() - 1) % commands.len();
                    }
                    if down {
                        self.workspace_ui.palette_index =
                            (self.workspace_ui.palette_index + 1) % commands.len();
                    }
                    self.workspace_ui.palette_scroll |= up || down;
                    if enter && !dismiss {
                        command = Some(commands[self.workspace_ui.palette_index].command);
                    }
                } else {
                    self.workspace_ui.palette_index = 0;
                    ui.label("No commands match your search.");
                }
                egui::ScrollArea::vertical()
                    .id_salt("command_results")
                    .max_height(360.)
                    .show(ui, |ui| {
                        for (index, entry) in commands.iter().enumerate() {
                            let selected = index == self.workspace_ui.palette_index;
                            let label = if entry.shortcut.is_empty() {
                                entry.label.clone()
                            } else {
                                format!("{}    {}", entry.label, entry.shortcut)
                            };
                            let response = ui.add_sized(
                                [ui.available_width(), 0.],
                                egui::Button::new(label).selected(selected),
                            );
                            if selected && self.workspace_ui.palette_scroll {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }
                            if response.clicked() {
                                command = Some(entry.command);
                            }
                        }
                    });
                self.workspace_ui.palette_scroll = false;
                ui.label(
                    RichText::new("Up / Down Choose  ·  Enter Run  ·  Esc Close")
                        .small()
                        .color(DIM),
                );
            });
        self.production.palette_open = open && !dismiss;
        if let Some(command) = command {
            self.production.palette_open = false;
            match command {
                PaletteCommand::Action(action) => self.run_action(action),
                PaletteCommand::SavedLayout(index) => self.load_saved_workspace(index),
            }
            ctx.request_repaint();
        }
    }
}
