use super::*;

impl ReboneApp {
    pub(in crate::app) fn curve_editor(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Animation curves").strong());
        let visible_length = self.project.rig.active_clip().length as f32;
        let Some(id) = self.selected else {
            ui.label("Select a bone to edit its curves.");
            return;
        };
        let anchored = self
            .project
            .rig
            .bone(id)
            .is_some_and(|b| b.parent.is_some());
        if anchored && matches!(self.studio.channel, Channel::X | Channel::Y) {
            self.studio.channel = Channel::Rotation;
        }
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_label("Channel")
                .selected_text(format!("{:?}", self.studio.channel))
                .show_ui(ui, |ui| {
                    for ch in [
                        Channel::Rotation,
                        Channel::X,
                        Channel::Y,
                        Channel::Scale,
                        Channel::Z,
                        Channel::Visibility,
                    ] {
                        if anchored && matches!(ch, Channel::X | Channel::Y) {
                            continue;
                        }
                        ui.selectable_value(&mut self.studio.channel, ch, format!("{ch:?}"));
                    }
                });
            if ui.button("Fit values").clicked() {
                let values: Vec<_> = self
                    .project
                    .rig
                    .active_clip()
                    .tracks
                    .iter()
                    .filter(|t| t.bone == id && t.channel == self.studio.channel)
                    .flat_map(|t| {
                        t.keys
                            .iter()
                            .filter(move |k| (1. ..=visible_length).contains(&k.frame))
                            .map(|k| k.value)
                    })
                    .collect();
                if !values.is_empty() {
                    let min = values.iter().copied().fold(f32::INFINITY, f32::min);
                    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    let pad = ((max - min) * 0.2).max(1.);
                    self.studio.curve_bounds = (min - pad, max + pad);
                }
            }
            ui.add(egui::DragValue::new(&mut self.studio.curve_bounds.0).prefix("Min "));
            ui.add(egui::DragValue::new(&mut self.studio.curve_bounds.1).prefix("Max "));
        });
        let channel = self.studio.channel;
        let id = if matches!(channel, Channel::Z | Channel::Visibility) {
            self.project.rig.bone(id).map_or(id, |b| b.part)
        } else {
            id
        };
        if matches!(channel, Channel::Z | Channel::Visibility)
            && !self.project.rig.parts.iter().any(|p| p.id == id)
        {
            ui.label("Assign this bone to a group to animate group visibility or depth.");
            return;
        }
        let clip = self.project.rig.active_clip().clone();
        let track = clip
            .tracks
            .iter()
            .find(|t| t.bone == id && t.channel == channel)
            .cloned();
        if ui.button("+ Key this channel at playhead").clicked() {
            let value = clip
                .sample(self.frame)
                .get(&id)
                .copied()
                .unwrap_or_default();
            let value = match channel {
                Channel::Rotation => value.rotation,
                Channel::X => value.x,
                Channel::Y => value.y,
                Channel::Scale => value.scale,
                Channel::Z => value.z.unwrap_or(0.),
                Channel::Visibility => value.visibility.unwrap_or(1.),
            };
            match self.project.rig.active_clip_mut().set_key(id, channel, self.frame, value) {
                Ok(()) => {
                    self.studio.keys = vec![KeySelection { bone: id, channel, frame: self.frame }];
                    self.seek_frame(self.frame);
                }
                Err(error) => self.status = error,
            }
        }
        let (rect, response) = ui.allocate_exact_size(
            EVec2::new(ui.available_width(), 200.),
            egui::Sense::click_and_drag(),
        );
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4., Color32::from_rgb(16, 22, 28));
        let (lo, hi) = self.studio.curve_bounds;
        let span = (hi - lo).max(0.001);
        let length = clip.length.max(2) as f32;
        let screen = |frame: f32, value: f32| {
            Pos2::new(
                rect.left() + 12. + (frame - 1.) / (length - 1.) * (rect.width() - 24.),
                rect.bottom() - 12. - (value - lo) / span * (rect.height() - 24.),
            )
        };
        let value_at = |pos: Pos2| {
            (
                ((pos.x - rect.left() - 12.) / (rect.width() - 24.) * (length - 1.) + 1.)
                    .clamp(1., clip.length as f32),
                lo + (rect.bottom() - 12. - pos.y) / (rect.height() - 24.) * span,
            )
        };
        for step in 0..=4 {
            let value = lo + span * step as f32 / 4.;
            let y = screen(1., value).y;
            p.line_segment(
                [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
                Stroke::new(1.0_f32, Color32::from_rgb(36, 45, 53)),
            );
            p.text(
                Pos2::new(rect.left() + 3., y),
                egui::Align2::LEFT_TOP,
                format!("{value:.1}"),
                egui::FontId::monospace(9.),
                DIM,
            );
        }
        if let Some(track) = &track {
            let points: Vec<_> = (0..=240)
                .map(|i| {
                    let f = 1. + i as f32 / 240. * (length - 1.);
                    screen(f, clip.value(track, f))
                })
                .collect();
            p.add(egui::Shape::line(points, Stroke::new(1.5_f32, ACCENT)));
            let mut handles = vec![];
            for key in track
                .keys
                .iter()
                .filter(|key| (1. ..=visible_length).contains(&key.frame))
            {
                let point = screen(key.frame, key.value);
                p.circle_filled(point, 4., ACCENT);
                handles.push((
                    point,
                    KeySelection {
                        bone: id,
                        channel,
                        frame: key.frame,
                    },
                    0,
                ));
                if self.studio.keys.contains(&KeySelection {
                    bone: id,
                    channel,
                    frame: key.frame,
                }) {
                    for (handle, sign, tangent) in
                        [(-1, -1., key.tangent_in), (1, 1., key.tangent_out)]
                    {
                        let pos = screen(key.frame + sign * 2., key.value + sign * 2. * tangent);
                        p.line_segment([point, pos], Stroke::new(1.0_f32, Color32::YELLOW));
                        p.circle_filled(pos, 3., Color32::YELLOW);
                        handles.push((
                            pos,
                            KeySelection {
                                bone: id,
                                channel,
                                frame: key.frame,
                            },
                            handle,
                        ));
                    }
                }
            }
            if (response.clicked() || response.drag_started())
                && let Some(pos) = if response.drag_started() {
                    ui.input(|i| i.pointer.press_origin())
                } else {
                    response.interact_pointer_pos()
                }
                && let Some((_, key, handle)) = handles
                    .iter()
                    .filter(|(p, _, _)| p.distance(pos) < 9.)
                    .min_by(|a, b| a.0.distance(pos).total_cmp(&b.0.distance(pos)))
            {
                self.studio.keys = vec![*key];
                self.seek_frame(key.frame);
                if response.drag_started() {
                    self.studio.curve_drag = Some(studio::CurveDrag {
                        base: self.project.clone(),
                        key: *key,
                        handle: *handle,
                        recorded: false,
                    });
                }
            }
            if response.dragged()
                && let Some(pos) = response.interact_pointer_pos()
                && let Some(mut drag) = self.studio.curve_drag.take()
            {
                let mut candidate = drag.base.clone();
                let source = candidate
                    .rig
                    .active_clip()
                    .tracks
                    .iter()
                    .find(|t| t.bone == drag.key.bone && t.channel == drag.key.channel)
                    .and_then(|t| t.keys.iter().find(|k| k.frame == drag.key.frame))
                    .cloned();
                if let Some(mut key) = source {
                    let (f, v) = value_at(pos);
                    if drag.handle == 0 {
                        key.frame = (f * 4.).round() / 4.;
                        key.value = if channel == Channel::Scale {
                            v.clamp(0.1, 4.)
                        } else {
                            v.clamp(-100_000., 100_000.)
                        };
                    } else {
                        let slope = ((v - key.value) / (f - key.frame).abs().max(0.05)
                            * (f - key.frame).signum())
                        .clamp(-100_000., 100_000.);
                        if drag.handle < 0 {
                            key.tangent_in = slope;
                        } else {
                            key.tangent_out = slope;
                        }
                        key.easing = Ease::Cubic;
                    }
                    if replace_key(&mut candidate.rig, drag.key, key.clone()).is_ok() {
                        if candidate != self.project && !drag.recorded {
                            self.history.record(&drag.base);
                            drag.recorded = true;
                        }
                        self.project = candidate;
                        self.studio.keys = vec![KeySelection {
                            frame: key.frame,
                            ..drag.key
                        }];
                        self.studio.curve_history_frame = true;
                        self.seek_frame(key.frame);
                    }
                }
                self.studio.curve_drag = Some(drag);
            }
            if response.drag_stopped() {
                self.studio.curve_drag = None;
            }
            if let Some(selected) = self
                .studio
                .keys
                .iter()
                .find(|k| k.bone == id && k.channel == channel)
                .copied()
                && let Some(mut key) = track
                    .keys
                    .iter()
                    .find(|k| k.frame == selected.frame)
                    .cloned()
            {
                let old = key.clone();
                let field_history_id = ui.id().with((
                    "curve_field_edit",
                    self.project.rig.active_clip,
                    id,
                    channel as u8,
                ));
                let mut recorded_control =
                    ui.data_mut(|data| data.get_temp::<egui::Id>(field_history_id));
                let mut active_control = None;
                let mut edited_control = None;
                ui.horizontal_wrapped(|ui| {
                    for response in [
                        ui.add(
                            egui::DragValue::new(&mut key.frame)
                                .range(1. ..=clip.length as f32)
                                .prefix("Frame ")
                                .speed(0.25),
                        ),
                        ui.add(
                            egui::DragValue::new(&mut key.value)
                                .prefix("Value ")
                                .speed(0.25),
                        ),
                        ui.add(
                            egui::DragValue::new(&mut key.tangent_in)
                                .prefix("In ")
                                .speed(0.1),
                        ),
                        ui.add(
                            egui::DragValue::new(&mut key.tangent_out)
                                .prefix("Out ")
                                .speed(0.1),
                        ),
                    ] {
                        if response.has_focus() || response.dragged() {
                            active_control = Some(response.id);
                        }
                        if response.changed() {
                            edited_control = Some(response.id);
                        }
                    }
                    egui::ComboBox::from_id_salt("curve_ease")
                        .selected_text(format!("{:?}", key.easing))
                        .show_ui(ui, |ui| {
                            for e in [
                                Ease::Linear,
                                Ease::Ease,
                                Ease::EaseIn,
                                Ease::EaseOut,
                                Ease::Back,
                                Ease::Bounce,
                                Ease::Step,
                                Ease::Cubic,
                            ] {
                                ui.selectable_value(&mut key.easing, e, format!("{e:?}"));
                            }
                        });
                });
                if key != old {
                    let before = self.project.clone();
                    match replace_key(&mut self.project.rig, selected, key.clone()) {
                        Ok(()) => {
                            if edited_control.is_some() && self.project != before {
                                if recorded_control != edited_control {
                                    self.history.record(&before);
                                }
                                recorded_control = edited_control;
                                self.studio.curve_history_frame = true;
                            }
                            self.studio.keys = vec![KeySelection {
                                frame: key.frame,
                                ..selected
                            }];
                            self.seek_frame(key.frame);
                        }
                        Err(e) => self.status = e,
                    }
                }
                ui.data_mut(|data| {
                    if let Some(control) =
                        recorded_control.filter(|control| Some(*control) == active_control)
                    {
                        data.insert_temp(field_history_id, control);
                    } else {
                        data.remove::<egui::Id>(field_history_id);
                    }
                });
            }
        } else {
            p.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Add a key to start a curve",
                egui::FontId::proportional(14.),
                DIM,
            );
        }
        ui.label(RichText::new("Drag curve points or yellow tangent handles. Cubic uses value-per-frame tangents; incoming handles enable Cubic on the preceding segment.").small().color(DIM));
    }
}
pub(super) fn replace_key(rig: &mut Rig, old: KeySelection, key: Keyframe) -> Result<(), String> {
    let mut candidate = rig.clone();
    candidate.active_clip_mut().release_hold(key.frame)?;
    let clip = candidate.active_clip();
    let looped = clip
        .source
        .as_ref()
        .map_or(clip.looped, |source| source.looped);
    let track = candidate
        .active_clip_mut()
        .tracks
        .iter_mut()
        .find(|t| t.bone == old.bone && t.channel == old.channel)
        .ok_or("Track is missing")?;
    let incoming_changed = track
        .keys
        .iter()
        .find(|k| k.frame == old.frame)
        .is_some_and(|k| k.tangent_in != key.tangent_in);
    let frame = key.frame;
    track
        .keys
        .retain(|k| k.frame != old.frame && k.frame != key.frame);
    track.keys.push(key);
    track.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    if incoming_changed && let Some(index) = track.keys.iter().position(|k| k.frame == frame) {
        let preceding = if index > 0 {
            Some(index - 1)
        } else if looped && track.keys.len() > 1 {
            Some(track.keys.len() - 1)
        } else {
            None
        };
        if let Some(previous) = preceding {
            track.keys[previous].easing = Ease::Cubic;
        }
    }
    candidate.validate()?;
    *rig = candidate;
    Ok(())
}

