use super::*;

impl ReboneApp {
    pub(super) fn timeline_transport(&mut self, ui: &mut egui::Ui) {
        let length = self.project.rig.active_clip().length;
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(if self.playing { "Pause" } else { "Play ▷" })
                .on_hover_text(self.timeline_action_hint(rebonebase::workspace::Action::Play))
                .clicked()
            {
                self.toggle_play();
            }
            if ui
                .small_button("|◀")
                .on_hover_text(self.timeline_action_hint(rebonebase::workspace::Action::FirstFrame))
                .clicked()
            {
                self.seek_frame(1.);
                self.timeline.reveal_playhead = true;
            }
            if ui
                .small_button("‹ Key")
                .on_hover_text(
                    self.timeline_action_hint(rebonebase::workspace::Action::PreviousKey),
                )
                .clicked()
            {
                self.seek_key(false);
            }
            let mut frame = self.frame;
            if ui
                .add(
                    egui::DragValue::new(&mut frame)
                        .range(1. ..=length as f32)
                        .speed(1.)
                        .prefix("Frame "),
                )
                .changed()
            {
                self.seek_frame(frame.round());
            }
            ui.label(
                RichText::new(format!("/ {length}"))
                    .monospace()
                    .color(ACCENT),
            );
            if ui
                .small_button("Key ›")
                .on_hover_text(self.timeline_action_hint(rebonebase::workspace::Action::NextKey))
                .clicked()
            {
                self.seek_key(true);
            }
            if ui
                .small_button("▶|")
                .on_hover_text(self.timeline_action_hint(rebonebase::workspace::Action::LastFrame))
                .clicked()
            {
                self.seek_frame(length as f32);
                self.timeline.reveal_playhead = true;
            }
            let mut index = self.project.rig.active_clip;
            egui::ComboBox::from_id_salt("clip")
                .width(150.)
                .selected_text(&self.project.rig.active_clip().name)
                .show_ui(ui, |ui| {
                    for (i, c) in self.project.rig.clips.iter().enumerate() {
                        ui.selectable_value(&mut index, i, &c.name);
                    }
                });
            if index != self.project.rig.active_clip {
                self.record();
                self.activate_clip(index);
            }
            if ui
                .add_enabled(
                    self.project.rig.clips.len() < 32,
                    egui::Button::new("+ Clip"),
                )
                .clicked()
            {
                self.record();
                let index = self.project.rig.clips.len();
                self.project.rig.clips.push(Clip {
                    name: format!("Clip {}", index + 1),
                    ..Default::default()
                });
                self.activate_clip(index);
            }
            if ui
                .menu_button("Clip settings", |ui| self.timeline_clip_metadata(ui))
                .inner
                .is_none()
            {
                self.timeline.metadata_control = None;
            }
        });
    }
    fn timeline_clip_metadata(&mut self, ui: &mut egui::Ui) {
        let before = self.project.clone();
        let mut active_control = None;
        let mut edited_control = None;
        let mut new_length = self.project.rig.active_clip().length;
        ui.horizontal_wrapped(|ui| {
            let clip = self.project.rig.active_clip_mut();
            for response in [
                ui.add(
                    egui::TextEdit::singleline(&mut clip.name)
                        .desired_width(125.)
                        .hint_text("Clip name"),
                ),
                ui.add(
                    egui::DragValue::new(&mut clip.fps)
                        .range(1. / 60. ..=240.)
                        .prefix("FPS "),
                ),
                ui.add(
                    egui::DragValue::new(&mut new_length)
                        .range(1..=240)
                        .prefix("Frames "),
                ),
            ] {
                if response.has_focus() || response.dragged() {
                    active_control = Some(response.id);
                }
                if response.changed() {
                    edited_control = Some(response.id);
                }
            }
            ui.checkbox(&mut clip.looped, "Loop");
        });
        let resized = new_length != self.project.rig.active_clip().length;
        if resized {
            let index = self.project.rig.active_clip;
            if let Err(e) = rebonebase::production::edit_clip(
                &mut self.project,
                rebonebase::production::ClipEdit::Resize {
                    index,
                    length: new_length,
                },
            ) {
                self.project = before.clone();
                self.status = e;
            }
        }
        if before != self.project {
            if let Err(e) = self.project.validate() {
                self.project = before;
                self.status = format!("Change rejected: {e}");
            } else {
                if edited_control.is_none() || self.timeline.metadata_control != edited_control {
                    self.history.record(&before);
                }
                if resized {
                    self.studio.keys.clear();
                    self.studio.key_drag = None;
                    self.studio.curve_drag = None;
                }
                self.seek_frame(self.frame);
                self.timeline.metadata_control = edited_control;
            }
        }
        if self.timeline.metadata_control != active_control {
            self.timeline.metadata_control = None;
        }
    }
    fn timeline_action_hint(&self, action: rebonebase::workspace::Action) -> String {
        let shortcut = self.action_shortcut(action);
        if shortcut.is_empty() {
            action.label().into()
        } else {
            format!("{} ({shortcut})", action.label())
        }
    }
}
