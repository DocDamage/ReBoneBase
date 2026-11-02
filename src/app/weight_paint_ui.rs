//! Meshes tab: weight brush, heat map, locked influences, bulk assignment and
//! rebinding. A brush stroke is recorded as one undo step.
use super::*;
use rebonebase::production::{
    self,
    weights::{self, Brush, BrushMode, Falloff, RebindMode},
};
use std::collections::BTreeSet;

pub(super) struct WeightPaintUi {
    pub paint: bool,
    pub bone: Option<u32>,
    pub brush: Brush,
    pub limit: usize,
    pub value: f32,
    pub heat: bool,
    pub selection: BTreeSet<usize>,
    pub rebind_mode: RebindMode,
    pub rebind_preview: bool,
    stroke: bool,
    stroke_recorded: bool,
    last_dab: Option<Vec2>,
}
impl Default for WeightPaintUi {
    fn default() -> Self {
        Self {
            paint: false,
            bone: None,
            brush: Brush::default(),
            limit: weights::MAX_INFLUENCES,
            value: 1.,
            heat: true,
            selection: BTreeSet::new(),
            rebind_mode: RebindMode::KeepShape,
            rebind_preview: false,
            stroke: false,
            stroke_recorded: false,
            last_dab: None,
        }
    }
}
impl WeightPaintUi {
    /// True while a stroke owns its own undo step.
    pub fn stroking(&self) -> bool {
        self.stroke
    }
}

/// Blue (0) through green (0.5) to red (1).
fn heat(weight: f32) -> Color32 {
    let w = weight.clamp(0., 1.);
    let (r, g, b) = if w < 0.5 {
        let t = w * 2.;
        (0., t, 1. - t)
    } else {
        let t = (w - 0.5) * 2.;
        (t, 1. - t, 0.)
    };
    Color32::from_rgba_unmultiplied((r * 255.) as u8, (g * 255.) as u8, (b * 255.) as u8, 150)
}

impl ReboneApp {
    fn weight_mesh_index(&self) -> Option<usize> {
        let id = self.production.mesh?;
        self.project
            .extras
            .production
            .meshes
            .iter()
            .position(|m| m.id == id)
    }
    /// Vertices targeted by bulk tools: the selection, else the edited vertex.
    fn weight_targets(&self) -> Vec<usize> {
        if self.production.weights.selection.is_empty() {
            vec![self.production.mesh_vertex]
        } else {
            self.production.weights.selection.iter().copied().collect()
        }
    }
    pub(super) fn begin_weight_stroke(&mut self) {
        let w = &mut self.production.weights;
        w.stroke = true;
        w.stroke_recorded = false;
        w.last_dab = None;
    }
    pub(super) fn end_weight_stroke(&mut self) {
        let w = &mut self.production.weights;
        w.stroke = false;
        w.stroke_recorded = false;
        w.last_dab = None;
    }
    /// One brush dab at canvas position `at` on the chosen mesh. The first
    /// change of a stroke records the undo step for the whole stroke.
    pub(super) fn weight_dab(&mut self, at: Vec2) {
        let (Some(index), Some(bone)) = (self.weight_mesh_index(), self.production.weights.bone)
        else {
            self.status = "Choose a mesh and a bone to paint".into();
            return;
        };
        if !at.is_finite() {
            return;
        }
        let before = self.project.clone();
        let frame = self.project.rig.active_clip().evaluation_frame(self.frame);
        let pose = self.project.pose_at(self.frame);
        let rest = self.project.rig.world(None);
        let mesh = &self.project.extras.production.meshes[index];
        let points = production::mesh_vertices(&self.project.rig, Some(&pose), mesh, frame);
        let brush = self.production.weights.brush;
        let limit = self.production.weights.limit;
        let result = weights::paint(
            &mut self.project.extras.production.meshes[index],
            &rest,
            &points,
            at,
            &brush,
            bone,
            limit,
        )
        .and_then(|_| self.project.validate());
        if let Err(e) = result {
            self.project = before;
            self.status = format!("Weight paint rejected: {e}");
            return;
        }
        if self.project != before {
            let w = &mut self.production.weights;
            if !w.stroke_recorded {
                self.history.record(&before);
                w.stroke_recorded = w.stroke;
            }
            self.changed();
        }
        self.production.weights.last_dab = Some(at);
    }
    fn weight_bulk(
        &mut self,
        label: &str,
        edit: impl FnOnce(&mut production::Mesh) -> Result<usize, String>,
    ) {
        let Some(index) = self.weight_mesh_index() else {
            return;
        };
        let before = self.project.clone();
        let result = edit(&mut self.project.extras.production.meshes[index]).and_then(|n| {
            self.project
                .validate()
                .map(|_| n)
                .map_err(|e| e.to_string())
        });
        match result {
            Ok(n) => self.status = format!("{label}: {n} vertices changed"),
            Err(e) => {
                self.project = before;
                self.status = format!("{label} rejected: {e}");
            }
        }
    }
    pub(super) fn weight_paint_panel(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.weight_mesh_index() else {
            return;
        };
        ui.separator();
        ui.strong("Weight painting");
        let w = &mut self.production.weights;
        if w.bone.is_none() {
            w.bone = self.selected;
        }
        let mesh = &self.project.extras.production.meshes[index];
        egui::ComboBox::from_label("Paint bone")
            .selected_text(
                w.bone
                    .and_then(|id| self.project.rig.bone(id))
                    .map_or("Choose bone", |b| b.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for b in &self.project.rig.bones {
                    ui.selectable_value(&mut w.bone, Some(b.id), &b.name);
                }
            });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut w.paint, "Paint on preview");
            ui.checkbox(&mut w.heat, "Heat map");
            for mode in BrushMode::ALL {
                ui.selectable_value(&mut w.brush.mode, mode, mode.label());
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::Slider::new(&mut w.brush.radius, 1. ..=128.)
                    .logarithmic(true)
                    .text("Radius"),
            );
            ui.add(egui::Slider::new(&mut w.brush.strength, 0.01..=1.).text("Strength"));
            egui::ComboBox::from_label("Falloff")
                .selected_text(w.brush.falloff.label())
                .show_ui(ui, |ui| {
                    for f in Falloff::ALL {
                        ui.selectable_value(&mut w.brush.falloff, f, f.label());
                    }
                });
            ui.add(
                egui::DragValue::new(&mut w.limit)
                    .range(1..=weights::MAX_INFLUENCES)
                    .prefix("Influence limit "),
            );
        });
        ui.label("Influences (click a name to paint it; locked bones keep their weight):");
        let mut toggle_lock = None;
        for bone in weights::influences(mesh) {
            let locked = mesh.locked.contains(&bone);
            ui.horizontal(|ui| {
                let mut lock = locked;
                if ui.checkbox(&mut lock, "Lock").changed() {
                    toggle_lock = Some(bone);
                }
                let name = self
                    .project
                    .rig
                    .bone(bone)
                    .map_or("Bone", |b| b.name.as_str());
                ui.selectable_value(&mut w.bone, Some(bone), name);
            });
        }
        if let Some(bone) = toggle_lock {
            let locked = &mut self.project.extras.production.meshes[index].locked;
            if let Some(i) = locked.iter().position(|b| *b == bone) {
                locked.remove(i);
            } else {
                locked.push(bone);
            }
        }
        let count = self.project.extras.production.meshes[index].vertices.len();
        let w = &mut self.production.weights;
        w.selection.retain(|i| *i < count);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "{} vertices selected (Shift+click to add)",
                w.selection.len()
            ));
            if ui.button("Select all").clicked() {
                w.selection = (0..count).collect();
            }
            if ui.button("Clear selection").clicked() {
                w.selection.clear();
            }
        });
        let mut value = w.value;
        let (bone, limit) = (w.bone, w.limit);
        let mut assign = false;
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::Slider::new(&mut value, 0. ..=1.).text("Weight"));
            assign = ui
                .add_enabled(bone.is_some(), egui::Button::new("Set weight on selected"))
                .clicked();
        });
        self.production.weights.value = value;
        if assign && let Some(bone) = bone {
            let targets = self.weight_targets();
            let rest = self.project.rig.world(None);
            self.weight_bulk("Set weight", |m| {
                weights::assign(m, &rest, &targets, bone, value, limit).map_err(|e| e.to_string())
            });
        }
        if ui.button("Normalize and apply influence limit").clicked() {
            self.weight_bulk("Normalize", |m| Ok(weights::normalize(m, limit)));
        }
        ui.separator();
        ui.strong("Rebinding");
        let w = &mut self.production.weights;
        ui.horizontal_wrapped(|ui| {
            ui.radio_value(
                &mut w.rebind_mode,
                RebindMode::KeepShape,
                "Keep the artwork's current shape",
            );
            ui.radio_value(
                &mut w.rebind_mode,
                RebindMode::OriginalPlacement,
                "Return artwork to where it was drawn",
            );
        });
        ui.checkbox(&mut w.rebind_preview, "Preview rebind (white outline)");
        let mode = w.rebind_mode;
        match weights::rebind(
            &self.project.rig,
            &self.project.extras.production.meshes[index],
            mode,
        ) {
            Ok(rebound) => {
                let mesh = &self.project.extras.production.meshes[index];
                let moved = production::mesh_vertices(&self.project.rig, None, mesh, 0.)
                    .iter()
                    .zip(production::mesh_vertices(
                        &self.project.rig,
                        None,
                        &rebound,
                        0.,
                    ))
                    .map(|(a, b)| (*a - b).length())
                    .fold(0., f32::max);
                ui.label(format!(
                    "Rebinding moves rest-pose vertices by up to {moved:.1} px."
                ));
                if ui.button("Rebind to current rest pose").clicked() {
                    self.project.extras.production.meshes[index] = rebound;
                    self.production.weights.rebind_preview = false;
                    self.status = "Mesh rebound to the current rest pose".into();
                }
            }
            Err(e) => {
                ui.colored_label(Color32::YELLOW, format!("Cannot rebind: {e}"));
            }
        }
    }
    /// Draws the heat map, rebind preview and brush over the art preview and
    /// handles painting or vertex selection. Returns true when the pointer was
    /// used for painting.
    pub(super) fn weight_preview_overlay(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        points: &[Vec2],
        response: &egui::Response,
    ) -> bool {
        let Some(index) = self.weight_mesh_index() else {
            return false;
        };
        let scale = rect.width() / self.project.rig.width as f32;
        let origin = self.project.extras.canvas_origin;
        let to_screen = |p: Vec2| {
            let p = p - origin;
            rect.min + EVec2::new(p.x, p.y) * scale
        };
        let painter = ui.painter_at(rect);
        let mesh = &self.project.extras.production.meshes[index];
        let w = &self.production.weights;
        if w.heat
            && let Some(bone) = w.bone
        {
            let mut shape = egui::epaint::Mesh::default();
            for (v, p) in mesh.vertices.iter().zip(points) {
                shape.colored_vertex(to_screen(*p), heat(weights::weight_of(v, bone)));
            }
            for t in &mesh.triangles {
                if t.iter().all(|i| *i < points.len()) {
                    shape.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
                }
            }
            painter.add(shape);
        }
        if w.rebind_preview
            && let Ok(rebound) = weights::rebind(&self.project.rig, mesh, w.rebind_mode)
        {
            let after = production::mesh_vertices(
                &self.project.rig,
                Some(&self.project.pose_at(self.frame)),
                &rebound,
                self.project.rig.active_clip().evaluation_frame(self.frame),
            );
            let stroke = Stroke::new(1., Color32::WHITE);
            for t in &rebound.triangles {
                for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    if let (Some(a), Some(b)) = (after.get(a), after.get(b)) {
                        painter.line_segment([to_screen(*a), to_screen(*b)], stroke);
                    }
                }
            }
        }
        for i in &w.selection {
            if let Some(p) = points.get(*i) {
                painter.circle_stroke(to_screen(*p), 5., Stroke::new(1.5, Color32::WHITE));
            }
        }
        let to_canvas = |at: Pos2| {
            let local = (at - rect.min) / scale;
            Vec2::new(local.x, local.y) + origin
        };
        let hover = response.hover_pos().map(to_canvas);
        if !w.paint {
            if self.production.weights.stroke {
                self.end_weight_stroke();
            }
            return false;
        }
        if let Some(at) = hover {
            painter.circle_stroke(
                to_screen(at),
                w.brush.radius * scale,
                Stroke::new(1., Color32::YELLOW),
            );
        }
        let down = response.is_pointer_button_down_on();
        if down && !self.production.weights.stroke {
            self.begin_weight_stroke();
        }
        if self.production.weights.stroke {
            if let Some(at) = response.interact_pointer_pos().map(to_canvas) {
                // Dabs are spaced along the stroke so speed and frame rate do
                // not change how much weight is laid down.
                let spacing = (self.production.weights.brush.radius * 0.25).max(0.5);
                let due = self
                    .production
                    .weights
                    .last_dab
                    .is_none_or(|last| (at - last).length() >= spacing);
                if down && due {
                    self.weight_dab(at);
                }
            }
            if !down {
                self.end_weight_stroke();
            }
        }
        true
    }
}
