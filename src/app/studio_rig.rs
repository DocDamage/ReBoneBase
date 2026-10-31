use super::*;
use rebonebase::{editing, features::Pin};
use std::collections::BTreeSet;

#[cfg(test)]
#[path = "studio_rig_tests.rs"]
mod tests;

impl ReboneApp {
    pub(super) fn studio_rig(&mut self, ui: &mut egui::Ui) {
        ui.heading("Rig editing and persistent pins");
        ui.label("Shift-click bones to add/remove selection; drag empty canvas to box-select. Alt-click or Tab cycles overlapping bones.");
        let ids = self.selected_bones();
        ui.label(format!("{} selected", ids.len()));
        ui.horizontal(|ui| {
            if ui.button("Select all unlocked").clicked() {
                self.studio.selection = self
                    .project
                    .rig
                    .bones
                    .iter()
                    .filter(|b| !b.locked)
                    .map(|b| b.id)
                    .collect();
                self.selected = self.studio.selection.iter().next().copied();
            }
            if ui.button("Clear selection").clicked() {
                self.studio.selection.clear();
                self.selected = None;
            }
            if ui.button("Lock selected").clicked() {
                for id in &ids {
                    if let Some(b) = self.project.rig.bone_mut(*id) {
                        b.locked = true;
                    }
                }
            }
            if ui.button("Unlock all").clicked() {
                for b in &mut self.project.rig.bones {
                    b.locked = false;
                }
            }
        });
        if let Some(id) = self.selected
            && let Some(mut b) = self.project.rig.bone(id).cloned()
        {
            ui.separator();
            ui.label(RichText::new(&b.name).strong());
            ui.horizontal(|ui| {
                ui.checkbox(&mut b.locked, "Locked for editing");
                ui.checkbox(&mut b.length_locked, "Lock length");
            });
            let mut limited = b.rotation_min.is_some() || b.rotation_max.is_some();
            if ui.checkbox(&mut limited, "Limit pose rotation").changed() {
                b.rotation_min = limited.then_some(-90.);
                b.rotation_max = limited.then_some(90.);
            }
            if limited {
                let (mut min, mut max) = (
                    b.rotation_min.unwrap_or(-180.),
                    b.rotation_max.unwrap_or(180.),
                );
                ui.horizontal(|ui| {
                    ui.label("Degrees");
                    ui.add(egui::DragValue::new(&mut min).range(-720. ..=720.));
                    ui.label("to");
                    ui.add(egui::DragValue::new(&mut max).range(-720. ..=720.));
                });
                b.rotation_min = Some(min.min(max));
                b.rotation_max = Some(max.max(min));
            }
            ui.horizontal(|ui| {
                ui.label("Preferred bend");
                ui.selectable_value(&mut b.bend, 1., "Positive");
                ui.selectable_value(&mut b.bend, -1., "Negative");
            });
            *self.project.rig.bone_mut(id).unwrap() = b.clone();
            ui.separator();
            ui.label("Joint attachment");
            ui.add_enabled_ui(b.parent.is_none(), |ui| {
                egui::ComboBox::from_label("New parent")
                    .selected_text(
                        self.studio
                            .parent
                            .and_then(|id| self.project.rig.bone(id))
                            .map_or("None", |b| b.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.studio.parent, None, "None");
                        for candidate in &self.project.rig.bones {
                            if candidate.id != id {
                                ui.selectable_value(
                                    &mut self.studio.parent,
                                    Some(candidate.id),
                                    &candidate.name,
                                );
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    for (label, parent, connected) in [
                        ("Reparent", self.studio.parent, false),
                        ("Connect", self.studio.parent, true),
                        ("Detach", None, false),
                    ] {
                        if ui.button(label).clicked() {
                            let before = self.project.clone();
                            match editing::reparent(&mut self.project.rig, id, parent, connected) {
                                Ok(()) => match self.finish_rig_hierarchy_edit(before) {
                                    Ok(()) => {
                                        self.status =
                                            "Bone hierarchy updated without a rest-pose jump".into()
                                    }
                                    Err(e) => self.status = e,
                                },
                                Err(e) => self.status = e,
                            }
                        }
                    }
                });
            });
            ui.label(RichText::new("Attached joints stay fixed. New unattached bones can be connected without moving their heads.").small().color(DIM));
        }
        ui.separator();
        ui.label(RichText::new("Transform selection").strong());
        ui.horizontal(|ui| {
            ui.add_enabled_ui(false, |ui| {
                ui.add(
                    egui::DragValue::new(&mut self.studio.group_delta.x)
                        .prefix("X ")
                        .speed(0.25),
                );
                ui.add(
                    egui::DragValue::new(&mut self.studio.group_delta.y)
                        .prefix("Y ")
                        .speed(0.25),
                );
            });
            ui.add(
                egui::DragValue::new(&mut self.studio.group_angle)
                    .prefix("Angle ")
                    .speed(0.5),
            );
        });
        ui.horizontal(|ui| {
            if ui.button("Rotate at selected joints").clicked() {
                match rotate_selection(
                    &mut self.project.rig,
                    &ids,
                    self.studio.group_angle,
                    self.frame,
                    self.mode,
                ) {
                    Ok(()) => self.status = "Selection rotated".into(),
                    Err(e) => self.status = e,
                }
            }
            ui.add_enabled(false, egui::Button::new("Move rest joints"))
                .on_hover_text(
                    "Joint heads stay anchored; use Move Rig (M) to translate the mannequin.",
                );
        });
        ui.label(RichText::new("Root anchors are excluded; use Move Rig (M) for whole-body movement. Locks protect editing, not existing animation.").small().color(DIM));
        ui.separator();
        ui.label(RichText::new("Hand and foot pins").strong());
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.selected.is_some(),
                    egui::Button::new("Pin selected limb end"),
                )
                .clicked()
                && let Some(id) = self.selected
            {
                self.add_pin(id, false);
            }
            if ui.button("Plant both feet").clicked() {
                let feet: Vec<_> = self
                    .project
                    .rig
                    .bones
                    .iter()
                    .filter(|b| {
                        b.name.to_lowercase().contains("shin")
                            || b.name.to_lowercase().contains("lowerleg")
                    })
                    .map(|b| b.id)
                    .collect();
                if self.project.extras.ground.is_none() {
                    self.project.extras.ground = Some(self.project.rig.height as f32 - 2.);
                }
                for id in feet {
                    self.add_pin(id, true);
                }
            }
        });
        let mut ground = self.project.extras.ground.is_some();
        if ui
            .checkbox(&mut ground, "Show ground and constrain grounded pins")
            .changed()
        {
            self.project.extras.ground = ground.then_some(self.project.rig.height as f32 - 2.);
        }
        if let Some(y) = &mut self.project.extras.ground {
            row(ui, "Ground Y", y, -4096. ..=4096.);
        }
        let mut remove = None;
        for (i, pin) in self.project.extras.pins.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut pin.enabled,
                    self.project
                        .rig
                        .bone(pin.lower)
                        .map_or("Missing limb", |b| b.name.as_str()),
                );
                ui.add(
                    egui::DragValue::new(&mut pin.target.x)
                        .prefix("X ")
                        .speed(0.25),
                );
                ui.add(
                    egui::DragValue::new(&mut pin.target.y)
                        .prefix("Y ")
                        .speed(0.25),
                );
                ui.checkbox(&mut pin.grounded, "Grounded");
                if ui.small_button("Flip bend").clicked() {
                    pin.bend = -pin.bend;
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.project.extras.pins.remove(i);
        }
        ui.label(RichText::new("Pins persist across frames and are baked into animation exports. In IK mode, drag a pinned end to move its target.").small().color(DIM));
    }
    pub(super) fn add_pin(&mut self, id: u32, grounded: bool) {
        let Some(bone) = self.project.rig.bone(id) else {
            return;
        };
        let lower = if bone.name.to_lowercase().contains("hand")
            || bone.name.to_lowercase().contains("foot")
        {
            bone.parent.unwrap_or(id)
        } else if bone.connected {
            id
        } else {
            self.project
                .rig
                .bones
                .iter()
                .find(|b| b.parent == Some(id) && b.connected)
                .map_or(id, |b| b.id)
        };
        let Some(b) = self.project.rig.bone(lower) else {
            return;
        };
        let Some(upper) = b.parent.and_then(|id| self.project.rig.bone(id)) else {
            self.status = "Select the end of a two-bone limb".into();
            return;
        };
        if !b.connected || upper.parent.is_none() {
            self.status = "Pins cannot drive a root or torso anchor".into();
            return;
        }
        let world = self
            .project
            .rig
            .world(Some(&self.project.pose_at(self.frame)));
        let target = world.iter().find(|w| w.id == lower).unwrap().tail;
        let pin = Pin {
            lower,
            target,
            enabled: true,
            grounded,
            bend: b.bend,
        };
        if let Some(existing) = self
            .project
            .extras
            .pins
            .iter_mut()
            .find(|p| p.lower == lower)
        {
            *existing = pin;
        } else if self.project.extras.pins.len() < 128 {
            self.project.extras.pins.push(pin);
        }
        self.mode = Mode::Pose;
        self.status = "Persistent limb pin added".into();
    }
    fn prune_invalid_limb_pins(&mut self) {
        let rig = &self.project.rig;
        self.project.extras.pins.retain(|pin| {
            rig.bone(pin.lower)
                .filter(|lower| lower.connected)
                .and_then(|lower| lower.parent)
                .and_then(|id| rig.bone(id))
                .is_some_and(|upper| upper.parent.is_some())
        });
        self.project.extras.production.prune_bones(rig);
    }
    fn finish_rig_hierarchy_edit(&mut self, before: Project) -> Result<(), String> {
        self.prune_invalid_limb_pins();
        if let Err(e) = self.project.validate() {
            self.project = before;
            return Err(format!("Hierarchy change rejected: {e}"));
        }
        Ok(())
    }
}
pub(super) fn rotate_selection(
    rig: &mut Rig,
    ids: &[u32],
    angle: f32,
    frame: f32,
    mode: Mode,
) -> Result<(), String> {
    let selected: BTreeSet<_> = ids
        .iter()
        .copied()
        .filter(|id| {
            rig.bone(*id)
                .is_some_and(|b| !b.locked && b.parent.is_some())
        })
        .collect();
    let roots: Vec<_> = selected
        .iter()
        .copied()
        .filter(|id| {
            let mut parent = rig.bone(*id).and_then(|b| b.parent);
            while let Some(p) = parent {
                if selected.contains(&p) {
                    return false;
                }
                parent = rig.bone(p).and_then(|b| b.parent);
            }
            true
        })
        .collect();
    if roots.is_empty() {
        return Err("Select an unlocked non-root bone".into());
    }
    let mut candidate = rig.clone();
    for id in roots {
        let pose = (mode == Mode::Pose).then(|| candidate.active_clip().sample(frame));
        let world = candidate.world(pose.as_ref());
        let pivot = world.iter().find(|w| w.id == id).unwrap().head;
        match mode {
            Mode::Rest => {
                editing::transform_rest(&mut candidate, &[id], Vec2::default(), angle, pivot)?
            }
            Mode::Pose => editing::transform_pose(
                &mut candidate,
                &[id],
                Vec2::default(),
                angle,
                pivot,
                frame,
            )?,
        }
    }
    *rig = candidate;
    Ok(())
}

#[cfg(test)]
mod hierarchy_controller_regressions {
    use super::*;
    use rebonebase::advanced_motion::{Chain, ContactKey, ContactTrack, Spring};
    fn with_controllers() -> (ReboneApp, egui::Context, u32) {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        let lower = app
            .project
            .rig
            .bones
            .iter()
            .find(|b| b.name == "shin_L")
            .unwrap()
            .id;
        let upper = app.project.rig.bone(lower).unwrap().parent.unwrap();
        app.selected = Some(lower);
        app.studio.selection = [lower].into_iter().collect();
        app.mode = Mode::Rest;
        app.project
            .extras
            .production
            .motion
            .contacts
            .push(ContactTrack {
                lower,
                keys: vec![ContactKey::default()],
                ..Default::default()
            });
        app.project.extras.production.motion.chains.push(Chain {
            bones: vec![upper, lower],
            ..Default::default()
        });
        app.project.extras.production.motion.springs.push(Spring {
            bone: lower,
            ..Default::default()
        });
        (app, ctx, lower)
    }
    fn click(app: &mut ReboneApp, ctx: &egui::Context, label: &str) {
        super::super::tests_studio::click(app, ctx, studio::Tab::Rig, label);
    }
    #[test]
    fn ordinary_detach_is_blocked_and_preserves_the_controller_graph() {
        let (mut app, ctx, lower) = with_controllers();
        let before = app.project.clone();
        click(&mut app, &ctx, "Detach");
        assert!(app.project.rig.bone(lower).unwrap().parent.is_some());
        assert_eq!(app.project, before);
        app.project.validate().unwrap();
    }
    #[test]
    fn rest_translation_is_blocked_and_preserves_the_controller_graph() {
        let (mut app, ctx, _) = with_controllers();
        app.studio.group_delta = Vec2::new(2., 0.);
        let before = app.project.clone();
        click(&mut app, &ctx, "Move rest joints");
        assert_eq!(app.project, before);
        app.project.validate().unwrap();
    }
    #[test]
    fn rejected_studio_changes_preserve_document_and_undo_history() {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        let before = app.project.clone();
        app.project.rig.name = "x".repeat(1025);
        app.commit_studio_edit(before.clone());
        assert_eq!(app.project, before);
        assert!(!app.history.undo(&mut app.project));
        app.project.rig.name = "Changed".into();
        app.commit_studio_edit(before.clone());
        assert!(app.history.undo(&mut app.project));
        assert_eq!(app.project, before);
    }
}
