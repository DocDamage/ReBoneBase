use super::*;

mod curves;
#[cfg(test)]
use curves::replace_key;
use rebonebase::{
    animation::{Keyframe, Playback},
    editing::{self, KeySelection},
};

impl ReboneApp {
    pub(super) fn studio_animation(&mut self, ui: &mut egui::Ui) {
        ui.heading("Clip, keyframe and curve editing");
        self.animation_templates_ui(ui);
        let index = self.project.rig.active_clip;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Duplicate clip").clicked() {
                match rebonebase::production::edit_clip(
                    &mut self.project,
                    rebonebase::production::ClipEdit::Duplicate(index),
                ) {
                    Ok(_) => {
                        self.activate_clip(self.project.rig.active_clip);
                    }
                    Err(e) => self.status = e,
                }
            }
            if ui.button("Delete clip").clicked() {
                match rebonebase::production::edit_clip(
                    &mut self.project,
                    rebonebase::production::ClipEdit::Delete(index),
                ) {
                    Ok(()) => {
                        self.activate_clip(self.project.rig.active_clip);
                    }
                    Err(e) => self.status = e,
                }
            }
            for (label, target) in [
                ("Move left", index.checked_sub(1)),
                (
                    "Move right",
                    (index + 1 < self.project.rig.clips.len()).then_some(index + 1),
                ),
            ] {
                if ui
                    .add_enabled(target.is_some(), egui::Button::new(label))
                    .clicked()
                {
                    match rebonebase::production::edit_clip(
                        &mut self.project,
                        rebonebase::production::ClipEdit::Move {
                            from: index,
                            to: target.unwrap(),
                        },
                    ) {
                        Ok(()) => self.activate_clip(self.project.rig.active_clip),
                        Err(error) => self.status = error,
                    }
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.studio.trim_start)
                    .range(1..=240)
                    .prefix("Start "),
            );
            ui.add(
                egui::DragValue::new(&mut self.studio.trim_end)
                    .range(1..=240)
                    .prefix("End "),
            );
            if ui.button("Trim to range").clicked() {
                let i = self.project.rig.active_clip;
                if let Err(e) = rebonebase::production::edit_clip(
                    &mut self.project,
                    rebonebase::production::ClipEdit::Trim {
                        index: i,
                        start: self.studio.trim_start,
                        end: self.studio.trim_end,
                    },
                ) {
                    self.status = e;
                } else {
                    self.activate_clip(self.project.rig.active_clip);
                }
            }
            if ui.button("Split at current frame").clicked() {
                let i = self.project.rig.active_clip;
                match rebonebase::production::edit_clip(
                    &mut self.project,
                    rebonebase::production::ClipEdit::Split {
                        index: i,
                        at: self.frame as u32,
                    },
                ) {
                    Ok(_) => {
                        self.activate_clip(self.project.rig.active_clip);
                    }
                    Err(e) => self.status = e,
                }
            }
        });
        if let Some(source) = &self.project.rig.active_clip().source {
            ui.label("Trimmed animation keeps its original keys and live controllers. Earlier and later keys are retained.");
            if source.end < self.project.rig.active_clip().length {
                ui.label(format!(
                    "Motion holds after frame {}. Adding a key later resumes the animation through that frame.",
                    source.end
                ));
            }
        }
        ui.horizontal_wrapped(|ui| {
            let clip = self.project.rig.active_clip_mut();
            egui::ComboBox::from_label("Playback")
                .selected_text(format!("{:?}", clip.playback))
                .show_ui(ui, |ui| {
                    for mode in [Playback::Forward, Playback::Reverse, Playback::PingPong] {
                        ui.selectable_value(&mut clip.playback, mode, format!("{mode:?}"));
                    }
                });
            let mut duration = clip.duration_ms(self.frame as u32);
            if ui
                .add(
                    egui::DragValue::new(&mut duration)
                        .range(1..=60000)
                        .prefix("Current hold ms "),
                )
                .changed()
            {
                clip.durations.insert(self.frame as u32, duration);
            }
            if ui.button("Reset frame hold").clicked() {
                clip.durations.remove(&(self.frame as u32));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut self.studio.show_path, "Show root-motion path");
            if ui.button("Convert clip to in-place").clicked() {
                let i = self.project.rig.active_clip;
                if let Err(e) = editing::make_in_place(&mut self.project.rig, i) {
                    self.status = e;
                }
            }
        });
        ui.separator();
        ui.label(format!(
            "{} keys selected · Shift-click timeline diamonds to add/remove",
            self.studio.keys.len()
        ));
        ui.horizontal_wrapped(|ui| {
            if ui.button("Select all keys").clicked() {
                self.select_all_timeline_keys();
            }
            if ui.button("Clear key selection").clicked() {
                self.studio.keys.clear();
            }
            if ui.button("Copy").clicked() {
                self.copy_timeline_keys();
            }
            if ui.button("Paste at playhead").clicked() {
                self.paste_timeline_keys(false);
            }
            if ui.button("Delete keys").clicked() {
                self.delete_timeline_keys(false);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.studio.key_offset)
                    .prefix("Offset ")
                    .speed(0.25),
            );
            ui.add(
                egui::DragValue::new(&mut self.studio.key_scale)
                    .range(0.05..=20.)
                    .prefix("Time scale ")
                    .speed(0.05),
            );
            if ui.button("Retime selection").clicked() {
                let result = editing::retime_keys(
                    self.project.rig.active_clip_mut(),
                    &self.studio.keys,
                    self.studio.key_offset,
                    self.studio.key_scale,
                    self.frame,
                );
                match result {
                    Ok(()) => {
                        for k in &mut self.studio.keys {
                            k.frame = self.frame
                                + (k.frame - self.frame) * self.studio.key_scale
                                + self.studio.key_offset;
                        }
                    }
                    Err(e) => self.status = e,
                }
            }
            ui.add(
                egui::DragValue::new(&mut self.studio.key_snap)
                    .range(0.25..=24.)
                    .prefix("Snap ")
                    .speed(0.25),
            );
            if ui.button("Snap keys").clicked() {
                if let Err(e) = editing::snap_keys(
                    self.project.rig.active_clip_mut(),
                    &self.studio.keys,
                    self.studio.key_snap,
                ) {
                    self.status = e;
                } else {
                    self.studio.keys.clear();
                }
            }
        });
        ui.separator();
        ui.push_id("studio_curves", |ui| self.curve_editor(ui));
    }
    pub(super) fn copy_timeline_keys(&mut self) {
        self.studio.clipboard = Some(editing::copy_keys(
            self.project.rig.active_clip(),
            &self.studio.keys,
        ));
        self.status = format!(
            "Copied {} keys",
            self.studio.clipboard.as_ref().unwrap().keys.len()
        );
    }
    pub(super) fn paste_timeline_keys(&mut self, record: bool) {
        if let Some(clipboard) = &self.studio.clipboard {
            let before = self.project.clone();
            match editing::paste_keys(self.project.rig.active_clip_mut(), clipboard, self.frame) {
                Ok(()) => {
                    if record && self.project != before {
                        self.history.record(&before);
                    }
                    self.seek_frame(self.frame);
                    self.status = "Timeline keys pasted".into();
                }
                Err(e) => self.status = e,
            }
        }
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    #[test]
    fn incoming_and_outgoing_curve_handles_change_the_displayed_segment() {
        let (mut app, ctx, id) = app();
        app.project
            .rig
            .active_clip_mut()
            .set_key(id, Channel::Rotation, 9., 80.)
            .unwrap();
        app.studio.keys = vec![KeySelection {
            bone: id,
            channel: Channel::Rotation,
            frame: 9.,
        }];
        for incoming in [true, false] {
            app.studio.keys = vec![KeySelection {
                bone: id,
                channel: Channel::Rotation,
                frame: 9.,
            }];
            let before = app.project.clone();
            let out = pass(&mut app, &ctx, vec![], true);
            let mut handles: Vec<_> = out
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Circle(c) if c.fill == Color32::YELLOW && c.radius == 3. => {
                        Some(c.center)
                    }
                    _ => None,
                })
                .collect();
            handles.sort_by(|a, b| a.x.total_cmp(&b.x));
            let center = handles[usize::from(!incoming)];
            pass(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(center), press(center, true)],
                true,
            );
            let end = center + EVec2::new(0., -24.);
            pass(&mut app, &ctx, vec![egui::Event::PointerMoved(end)], true);
            pass(&mut app, &ctx, vec![press(end, false)], true);
            let track = &app.project.rig.active_clip().tracks[0];
            assert!(if incoming {
                track.keys[1].tangent_in != 0. && track.keys[0].easing == Ease::Cubic
            } else {
                track.keys[1].tangent_out != 0.
            });
            app.undo();
            assert_eq!(app.project, before);
        }
    }
    #[test]
    fn changing_incoming_tangent_enables_its_preceding_segment() {
        let (mut app, _, id) = app();
        app.project
            .rig
            .active_clip_mut()
            .set_key(id, Channel::Rotation, 9., 80.)
            .unwrap();
        let clip = app.project.rig.active_clip();
        let track = &clip.tracks[0];
        let before = clip.value(track, 5.);
        let mut key = track.keys[1].clone();
        key.tangent_in = 20.;
        replace_key(
            &mut app.project.rig,
            KeySelection {
                bone: id,
                channel: Channel::Rotation,
                frame: 9.,
            },
            key,
        )
        .unwrap();
        let clip = app.project.rig.active_clip();
        assert_eq!(clip.tracks[0].keys[0].easing, Ease::Cubic);
        assert!((clip.value(&clip.tracks[0], 5.) - before).abs() > 1.);
    }
    #[test]
    fn curve_edits_release_held_time_and_use_the_retained_curve_loop() {
        use rebonebase::production::{ClipEdit, edit_clip};
        let (mut app, _, id) = app();
        app.project.rig.active_clip_mut().looped = true;
        app.project
            .rig
            .active_clip_mut()
            .set_key(id, Channel::Rotation, 6., 80.)
            .unwrap();
        for length in [3, 7] {
            edit_clip(&mut app.project, ClipEdit::Resize { index: 0, length }).unwrap();
        }
        app.project.rig.active_clip_mut().looped = false;
        let mut first = app.project.rig.active_clip().tracks[0].keys[0].clone();
        first.tangent_in = 20.;
        replace_key(
            &mut app.project.rig,
            KeySelection {
                bone: id,
                channel: Channel::Rotation,
                frame: 1.,
            },
            first,
        )
        .unwrap();
        assert_eq!(
            app.project.rig.active_clip().tracks[0].keys[1].easing,
            Ease::Cubic
        );
        assert_eq!(app.project.rig.active_clip().evaluation_frame(6.), 3.);
        let mut later = app.project.rig.active_clip().tracks[0].keys[1].clone();
        later.value = 45.;
        replace_key(
            &mut app.project.rig,
            KeySelection {
                bone: id,
                channel: Channel::Rotation,
                frame: 6.,
            },
            later,
        )
        .unwrap();
        assert_eq!(app.project.rig.active_clip().evaluation_frame(6.), 6.);
        assert_eq!(app.project.rig.active_clip().sample(6.)[&id].rotation, 45.);
        assert_eq!(app.project.rig.active_clip().tracks[0].keys.len(), 2);
        app.project.validate().unwrap();
    }
    fn pass(
        app: &mut ReboneApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        curve: bool,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(900., 700.))),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if curve {
                        app.curve_editor(ui);
                    } else {
                        app.editable_dope(ui);
                    }
                });
            },
        )
    }
    fn press(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn app() -> (ReboneApp, egui::Context, u32) {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        let id = app
            .project
            .rig
            .bones
            .iter()
            .find(|b| b.name == "forearm_L")
            .unwrap()
            .id;
        app.select_bone(id, false);
        app.mode = Mode::Pose;
        app.project
            .rig
            .active_clip_mut()
            .set_key(id, Channel::Rotation, 1., 0.)
            .unwrap();
        (app, ctx, id)
    }
    #[test]
    fn key_drag_uses_press_origin_after_a_large_first_pointer_move() {
        let (mut app, ctx, id) = app();
        let before = app.project.clone();
        let out = pass(&mut app, &ctx, vec![], false);
        let center = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Path(path)
                    if path.closed && path.points.len() == 4 && path.fill == ACCENT =>
                {
                    Some(
                        path.points
                            .iter()
                            .fold(EVec2::ZERO, |sum, p| sum + p.to_vec2())
                            / 4.,
                    )
                }
                _ => None,
            })
            .unwrap()
            .to_pos2();
        pass(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(center), press(center, true)],
            false,
        );
        let end = center + EVec2::new(96., 0.);
        pass(&mut app, &ctx, vec![egui::Event::PointerMoved(end)], false);
        pass(&mut app, &ctx, vec![press(end, false)], false);
        assert!(
            app.project
                .rig
                .active_clip()
                .tracks
                .iter()
                .find(|t| t.bone == id)
                .unwrap()
                .keys
                .iter()
                .any(|k| k.frame == 5.),
            "drag should move the pressed key by four frames"
        );
        app.undo();
        assert_eq!(app.project, before);
    }
    #[test]
    fn empty_timeline_drag_continuously_scrubs() {
        let (mut app, ctx, _) = app();
        pass(&mut app, &ctx, vec![], false);
        let start = Pos2::new(140., 18.);
        pass(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(start), press(start, true)],
            false,
        );
        pass(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(start + EVec2::new(50., 0.))],
            false,
        );
        let intermediate = app.frame;
        pass(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(start + EVec2::new(170., 0.))],
            false,
        );
        assert!(
            app.frame > intermediate,
            "dragging empty header should keep moving the playhead"
        );
    }
    #[test]
    fn curve_drag_uses_the_pressed_point_and_is_one_undo() {
        let (mut app, ctx, id) = app();
        app.project
            .rig
            .active_clip_mut()
            .set_key(id, Channel::Rotation, 10., 30.)
            .unwrap();
        let before = app.project.clone();
        let out = pass(&mut app, &ctx, vec![], true);
        let center = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Circle(c) if c.fill == ACCENT && c.radius == 4. => Some(c.center),
                _ => None,
            })
            .unwrap();
        pass(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(center), press(center, true)],
            true,
        );
        let end = center + EVec2::new(90., -20.);
        pass(&mut app, &ctx, vec![egui::Event::PointerMoved(end)], true);
        pass(&mut app, &ctx, vec![press(end, false)], true);
        assert_ne!(app.project, before, "pressed curve point should move");
        app.undo();
        assert_eq!(app.project, before);
    }
}
