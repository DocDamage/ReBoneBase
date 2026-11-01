//! Production → 3D characters → Clean up motion. Each tool previews its
//! result next to the current pose before it writes correction keys.
use super::*;
use rebonebase::character3d::cleanup::{self, Cleanup, CleanupReport};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum Tool {
    #[default]
    Smooth,
    Plant,
    Reduce,
}
pub(super) struct Preview {
    /// The project the candidate was computed from; any edit invalidates it.
    base: Project,
    candidate: Project,
    report: CleanupReport,
    frame: f32,
    textures: Option<(egui::TextureHandle, egui::TextureHandle)>,
}
pub(super) struct CleanupUi {
    pub tool: Tool,
    pub start: u32,
    pub end: u32,
    pub all_joints: bool,
    pub radius: u32,
    pub strength: f32,
    pub blend_in: u32,
    pub blend_out: u32,
    pub floor: Option<f32>,
    pub tolerance: f32,
    pub preview: Option<Preview>,
}
impl Default for CleanupUi {
    fn default() -> Self {
        Self {
            tool: Tool::Smooth,
            start: 1,
            end: 1,
            all_joints: false,
            radius: 2,
            strength: 1.,
            blend_in: 2,
            blend_out: 2,
            floor: None,
            tolerance: 1.,
            preview: None,
        }
    }
}
impl ReboneApp {
    fn cleanup_request(&self, character: &rebonebase::character3d::Character) -> Cleanup {
        let s = &self.production.cleanup;
        let joint = self.production.character_joint;
        let clip = self.project.rig.active_clip;
        let joints = if s.all_joints {
            match s.tool {
                Tool::Reduce => character
                    .tracks
                    .get(&clip)
                    .map_or_else(Vec::new, |t| t.keys().copied().collect()),
                _ => (0..character.model.nodes.len()).collect(),
            }
        } else {
            vec![joint]
        };
        match s.tool {
            Tool::Smooth => Cleanup::Smooth {
                joints,
                start: s.start,
                end: s.end,
                radius: s.radius,
                strength: s.strength,
            },
            Tool::Plant => Cleanup::Plant {
                foot: joint,
                start: s.start,
                end: s.end,
                blend_in: s.blend_in,
                blend_out: s.blend_out,
                floor: s.floor,
            },
            Tool::Reduce => Cleanup::Reduce {
                joints,
                start: s.start,
                end: s.end,
                tolerance: s.tolerance,
            },
        }
    }
    pub(super) fn motion_cleanup_ui(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.production.character else {
            return;
        };
        let Some(character) = self
            .project
            .extras
            .characters3d
            .iter()
            .find(|c| c.id == id)
            .cloned()
        else {
            return;
        };
        let length = self.project.rig.active_clip().length.max(1);
        let joint_name = character
            .model
            .nodes
            .get(self.production.character_joint)
            .map_or("–", |n| n.name.as_str())
            .to_string();
        egui::CollapsingHeader::new("Clean up motion").show(ui, |ui| {
            ui.label("Writes correction keys for the active clip; the imported motion is unchanged. Delete the keys or undo to revert.");
            let before = (
                self.production.cleanup.tool,
                self.production.cleanup.start,
                self.production.cleanup.end,
            );
            let s = &mut self.production.cleanup;
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut s.tool, Tool::Smooth, "Reduce jitter");
                ui.selectable_value(&mut s.tool, Tool::Plant, "Plant foot");
                ui.selectable_value(&mut s.tool, Tool::Reduce, "Reduce keys");
            });
            s.start = s.start.clamp(1, length);
            s.end = s.end.clamp(s.start, length);
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::DragValue::new(&mut s.start).range(1..=length).prefix("Frames "));
                ui.add(egui::DragValue::new(&mut s.end).range(1..=length).prefix("to "));
                if ui.small_button("Whole clip").clicked() {
                    s.start = 1;
                    s.end = length;
                }
            });
            match s.tool {
                Tool::Smooth => {
                    ui.checkbox(&mut s.all_joints, format!("All joints (otherwise {joint_name})"));
                    ui.horizontal_wrapped(|ui| {
                        ui.add(egui::Slider::new(&mut s.radius, 1..=12).text("Radius (frames)"));
                        ui.add(egui::Slider::new(&mut s.strength, 0. ..=1.).text("Strength"));
                    });
                }
                Tool::Plant => {
                    ui.label(format!("Foot: {joint_name} · its parent and grandparent bend to hold it."));
                    ui.horizontal_wrapped(|ui| {
                        ui.add(egui::DragValue::new(&mut s.blend_in).range(0..=24).prefix("Blend in "));
                        ui.add(egui::DragValue::new(&mut s.blend_out).range(0..=24).prefix("Blend out "));
                    });
                    ui.horizontal_wrapped(|ui| {
                        let mut floor = s.floor.is_some();
                        ui.checkbox(&mut floor, "Align to floor");
                        if floor {
                            let mut v = s.floor.unwrap_or(0.);
                            ui.add(egui::DragValue::new(&mut v).speed(0.005).prefix("Height "));
                            s.floor = Some(v);
                        } else {
                            s.floor = None;
                        }
                    });
                }
                Tool::Reduce => {
                    ui.checkbox(&mut s.all_joints, format!("All keyed joints (otherwise {joint_name})"));
                    ui.add(egui::Slider::new(&mut s.tolerance, 0. ..=10.).text("Pose tolerance °"));
                }
            }
            if s.tool == Tool::Plant {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Detect floor").clicked() {
                        match cleanup::floor_height(&self.project, id, self.production.character_joint) {
                            Ok(v) => self.production.cleanup.floor = Some(v),
                            Err(e) => self.status = e.to_string(),
                        }
                    }
                    if ui
                        .button("Detect contact")
                        .on_hover_text("Find the contact at or after the playhead")
                        .clicked()
                    {
                        let at = self.frame.round().max(1.) as u32;
                        match cleanup::detect_contacts(&self.project, id, self.production.character_joint, 0.05, 0.05) {
                            Ok(found) => match found.iter().find(|r| r.1 >= at) {
                                Some(&(a, b)) => {
                                    self.production.cleanup.start = a;
                                    self.production.cleanup.end = b;
                                }
                                None => self.status = "No foot contact found after the playhead".into(),
                            },
                            Err(e) => self.status = e.to_string(),
                        }
                    }
                });
            }
            let s = &self.production.cleanup;
            if before != (s.tool, s.start, s.end) {
                self.production.cleanup.preview = None;
            }
            if ui.button("Preview cleanup").clicked() {
                match cleanup::apply(&self.project, id, &self.cleanup_request(&character)) {
                    Ok((candidate, report)) => {
                        self.production.cleanup.preview = Some(Preview {
                            base: self.project.clone(),
                            candidate,
                            report,
                            frame: f32::NAN,
                            textures: None,
                        })
                    }
                    Err(e) => {
                        self.production.cleanup.preview = None;
                        self.status = format!("Cleanup: {e}");
                    }
                }
            }
            if self
                .production
                .cleanup
                .preview
                .as_ref()
                .is_some_and(|p| p.base != self.project)
            {
                self.production.cleanup.preview = None;
            }
            let frame = self.frame;
            let mut apply = None;
            if let Some(p) = &mut self.production.cleanup.preview {
                let r = &p.report;
                ui.label(format!(
                    "{}: {:.3} → {:.3}{}",
                    r.measure,
                    r.before,
                    r.after,
                    if r.before > 0. {
                        format!(" ({:+.0}%)", (r.after / r.before - 1.) * 100.)
                    } else {
                        String::new()
                    }
                ));
                if r.max_error > 0. {
                    ui.label(format!("Largest remaining difference: {:.3}", r.max_error));
                }
                if p.frame != frame || p.textures.is_none() {
                    p.frame = frame;
                    let image = |project: &Project, name: &str| {
                        project.try_render_at(frame).map(|img| {
                            ui.ctx().load_texture(
                                name,
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [img.width() as usize, img.height() as usize],
                                    img.as_raw(),
                                ),
                                egui::TextureOptions::NEAREST,
                            )
                        })
                    };
                    p.textures = image(&p.base, "cleanup-before")
                        .and_then(|a| Ok((a, image(&p.candidate, "cleanup-after")?)))
                        .ok();
                }
                if let Some((a, b)) = &p.textures {
                    ui.horizontal(|ui| {
                        for (label, t) in [("Before", a), ("After", b)] {
                            ui.vertical(|ui| {
                                ui.label(format!("{label} · frame {frame:.0}"));
                                ui.add(egui::Image::new(t).fit_to_exact_size(EVec2::new(150., 150.)));
                            });
                        }
                    });
                }
                if ui.button("Apply cleanup").clicked() {
                    apply = Some(p.candidate.clone());
                }
            }
            if let Some(next) = apply {
                self.project = next;
                self.production.cleanup.preview = None;
                self.status = "Cleanup applied as correction keys".into();
            }
        });
    }
}
