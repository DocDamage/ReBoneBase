use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) enum InspectorTab {
    #[default]
    Selection,
    Appearance,
    Project,
}
#[derive(Default)]
pub(super) struct EditorPanels {
    pub search: String,
    pub collapsed: BTreeSet<u32>,
    pub last_selected: Option<u32>,
    pub reveal: bool,
    pub inspector: InspectorTab,
}
struct HierarchyRow {
    id: u32,
    depth: usize,
    children: bool,
    matches: bool,
}
// Traverse parent relationships instead of assuming bones are stored in tree order.
// A search keeps ancestors as context, and temporarily expands matching branches.
fn hierarchy_rows(rig: &Rig, search: &str, collapsed: &BTreeSet<u32>) -> Vec<HierarchyRow> {
    let query = search.trim().to_lowercase();
    let mut visible = BTreeSet::new();
    let mut matches = BTreeSet::new();
    let mut children: BTreeMap<Option<u32>, Vec<u32>> = BTreeMap::new();
    for bone in &rig.bones {
        children.entry(bone.parent).or_default().push(bone.id);
        if query.is_empty() || bone.name.to_lowercase().contains(&query) {
            matches.insert(bone.id);
            let mut parent = Some(bone.id);
            while let Some(id) = parent {
                if !visible.insert(id) {
                    break;
                }
                parent = rig.bone(id).and_then(|b| b.parent);
            }
        }
    }
    let mut stack: Vec<_> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .map(|id| (*id, 0))
        .collect();
    let mut rows = Vec::new();
    while let Some((id, depth)) = stack.pop() {
        if !visible.contains(&id) {
            continue;
        }
        rows.push(HierarchyRow {
            id,
            depth,
            children: children.contains_key(&Some(id)),
            matches: matches.contains(&id),
        });
        if (!query.is_empty() || !collapsed.contains(&id))
            && let Some(list) = children.get(&Some(id))
        {
            stack.extend(list.iter().rev().map(|child| (*child, depth + 1)));
        }
    }
    rows
}
impl ReboneApp {
    pub(super) fn reveal_selected_bone(&mut self, clear_search: bool) {
        if clear_search {
            self.panels.search.clear();
        }
        let mut parent = self
            .selected
            .and_then(|id| self.project.rig.bone(id))
            .and_then(|b| b.parent);
        while let Some(id) = parent {
            self.panels.collapsed.remove(&id);
            parent = self.project.rig.bone(id).and_then(|b| b.parent);
        }
        self.panels.reveal = true;
    }
    pub(super) fn left_ui(&mut self, ui: &mut egui::Ui) {
        let compact = ui.available_height() < 360.;
        ui.add_space(8.);
        ui.horizontal(|ui| {
            ui.label(RichText::new("SKELETON").small().color(ACCENT).strong());
            ui.label(
                RichText::new(self.project.rig.bones.len().to_string())
                    .small()
                    .color(DIM),
            );
        });
        let clear_width = 24.;
        let search_gap = ui.spacing().item_spacing.x;
        let search_width = (ui.available_width() - clear_width - search_gap).max(32.);
        ui.horizontal(|ui| {
            ui.add_sized(
                [search_width, 26.],
                egui::TextEdit::singleline(&mut self.panels.search)
                    .desired_width(search_width)
                    .hint_text("Find a bone…"),
            );
            let clear = ui
                .scope(|ui| {
                    // A small button keeps the global horizontal padding unless it is overridden.
                    ui.spacing_mut().button_padding.x = 4.;
                    ui.add_sized([clear_width, 26.], egui::Button::new("×").small())
                })
                .inner;
            if clear.on_hover_text("Clear search").clicked() {
                self.panels.search.clear();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.selected.is_some(),
                    egui::Button::new("Reveal selected").small(),
                )
                .clicked()
            {
                self.reveal_selected_bone(true);
            }
            if ui.small_button("Expand all").clicked() {
                self.panels.collapsed.clear();
            }
        });
        if self.panels.last_selected != self.selected {
            self.panels.last_selected = self.selected;
            self.reveal_selected_bone(false);
        }
        let rows = hierarchy_rows(
            &self.project.rig,
            &self.panels.search,
            &self.panels.collapsed,
        );
        let visible_selected = rows.iter().any(|row| Some(row.id) == self.selected);
        if self.selected.is_some() && !visible_selected {
            ui.label(
                RichText::new("Selected bone is hidden. Use Reveal selected.")
                    .small()
                    .color(DIM),
            );
        }
        ui.label(
            RichText::new("Shift-click to select several bones")
                .small()
                .color(DIM),
        );
        let footer_height = if compact { 48. } else { 165. };
        egui::ScrollArea::vertical()
            .id_salt("hierarchy")
            .max_height((ui.available_height() - footer_height).max(60.))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if rows.is_empty() {
                    ui.label(if self.project.rig.bones.is_empty() {
                        "Start with a rig template or draw your first bone."
                    } else {
                        "No bones match your search."
                    });
                }
                for row in rows {
                    let Some(bone) = self.project.rig.bone(row.id) else {
                        continue;
                    };
                    let name = bone.name.clone();
                    let locked = bone.locked;
                    let selected =
                        self.selected == Some(row.id) || self.studio.selection.contains(&row.id);
                    let row_width = ui.available_width();
                    let toggle_width = 18.;
                    let row_spacing = 3.;
                    let indent = (row.depth.min(12) as f32 * 10.)
                        .min((row_width - toggle_width - row_spacing - 48.).max(0.));
                    let label_width = (row_width - indent - toggle_width - row_spacing).max(48.);
                    ui.push_id(row.id, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = row_spacing;
                            ui.add_space(indent);
                            let (toggle_rect, toggle) = ui.allocate_exact_size(
                                EVec2::new(toggle_width, 23.),
                                if row.children {
                                    egui::Sense::click()
                                } else {
                                    egui::Sense::hover()
                                },
                            );
                            if row.children {
                                let closed = self.panels.collapsed.contains(&row.id)
                                    && self.panels.search.trim().is_empty();
                                let center = toggle_rect.center();
                                let points = if closed {
                                    vec![
                                        center + EVec2::new(-2., -4.),
                                        center + EVec2::new(2., 0.),
                                        center + EVec2::new(-2., 4.),
                                    ]
                                } else {
                                    vec![
                                        center + EVec2::new(-4., -2.),
                                        center + EVec2::new(0., 2.),
                                        center + EVec2::new(4., -2.),
                                    ]
                                };
                                let stroke = ui.style().interact(&toggle).fg_stroke;
                                ui.painter().add(egui::Shape::line(points, stroke));
                                let hint = if closed {
                                    "Expand branch"
                                } else {
                                    "Collapse branch"
                                };
                                toggle.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::WidgetType::Button,
                                        ui.is_enabled(),
                                        hint,
                                    )
                                });
                                if toggle.on_hover_text(hint).clicked()
                                    && !self.panels.collapsed.insert(row.id)
                                {
                                    self.panels.collapsed.remove(&row.id);
                                }
                            }
                            let label = format!("{}{}", if locked { "[L] " } else { "" }, name);
                            let response = ui
                                .add_sized(
                                    [label_width, 25.],
                                    egui::Button::selectable(
                                        selected,
                                        RichText::new(label).color(if selected {
                                            ACCENT
                                        } else if row.matches {
                                            Color32::LIGHT_GRAY
                                        } else {
                                            DIM
                                        }),
                                    )
                                    .right_text("")
                                    .truncate(),
                                )
                                .on_hover_text(format!(
                                    "{name} · ID {}{}",
                                    row.id,
                                    if locked { " · Locked" } else { "" }
                                ));
                            if response.clicked() {
                                self.select_bone(row.id, ui.input(|i| i.modifiers.shift));
                            }
                            if self.panels.reveal && Some(row.id) == self.selected {
                                response.scroll_to_me(Some(egui::Align::Center));
                                self.panels.reveal = false;
                            }
                        });
                    });
                }
            });
        ui.separator();
        ui.scope(|ui| {
            if compact {
                ui.spacing_mut().item_spacing.x = 6.;
                ui.spacing_mut().button_padding.x = 6.;
            }
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button("+ Draw bone")
                    .on_hover_text(format!(
                        "Draw bones · {}",
                        self.action_shortcut(rebonebase::workspace::Action::Bone)
                    ))
                    .clicked()
                {
                    self.run_action(rebonebase::workspace::Action::Bone);
                }
                if compact {
                    ui.menu_button("More", |ui| self.skeleton_edit_actions(ui));
                    ui.menu_button("View", |ui| self.skeleton_view_controls(ui));
                } else {
                    self.skeleton_edit_actions(ui);
                }
            });
            if !compact {
                ui.horizontal_wrapped(|ui| self.skeleton_view_controls(ui));
            }
        });
    }
    fn skeleton_edit_actions(&mut self, ui: &mut egui::Ui) {
        if ui
            .add_enabled(self.selected.is_some(), egui::Button::new("Remove"))
            .clicked()
        {
            self.delete();
        }
        ui.menu_button("Starter rigs…", |ui| {
            for i in 0..self.templates.len() {
                if ui.button(&self.templates[i].name).clicked() {
                    self.request(Pending::Template(i));
                    ui.close();
                }
            }
        });
    }
    fn skeleton_view_controls(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.grid, "Grid");
        ui.checkbox(&mut self.bones, "Bones");
        ui.checkbox(&mut self.show_skin, "Skin");
        if ui.checkbox(&mut self.onion, "Onion skin").changed() {
            self.changed();
        }
    }
    pub(super) fn left(&mut self, ctx: &egui::Context) {
        self.dock_panel(ctx, workspaces::Panel::Skeleton, Self::left_ui);
    }
    pub(super) fn inspector(&mut self, ctx: &egui::Context) {
        self.dock_panel(ctx, workspaces::Panel::Inspector, Self::inspector_ui);
    }
    pub(super) fn timeline(&mut self, ctx: &egui::Context) {
        self.dock_panel(ctx, workspaces::Panel::Timeline, Self::timeline_ui);
    }
}
#[cfg(test)]
#[path = "editor_panels_tests.rs"]
mod tests;
