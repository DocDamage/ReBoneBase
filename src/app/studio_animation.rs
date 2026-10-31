use super::*;
use rebonebase::{
    animation::{Keyframe, Playback},
    editing::{self, KeySelection},
};

impl ReboneApp {
    pub(super) fn studio_animation(&mut self, ui: &mut egui::Ui) {
        if self.mode != Mode::Pose {
            self.mode = Mode::Pose;
            self.changed();
        }
        ui.heading("Clip, keyframe and curve editing");
        self.animation_templates_ui(ui);
        let index = self.project.rig.active_clip;
        ui.horizontal(|ui| {
            if ui.button("Duplicate clip").clicked() {
                match rebonebase::production::edit_clip(
                    &mut self.project,
                    rebonebase::production::ClipEdit::Duplicate(index),
                ) {
                    Ok(_) => {
                        self.frame = 1.;
                        self.studio.keys.clear();
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
                        self.frame = 1.;
                        self.studio.keys.clear();
                    }
                    Err(e) => self.status = e,
                }
            }
            if ui.button("Move left").clicked()
                && index > 0
                && let Err(e) = rebonebase::production::edit_clip(
                    &mut self.project,
                    rebonebase::production::ClipEdit::Move {
                        from: index,
                        to: index - 1,
                    },
                )
            {
                self.status = e;
            }
            if ui.button("Move right").clicked()
                && index + 1 < self.project.rig.clips.len()
                && let Err(e) = rebonebase::production::edit_clip(
                    &mut self.project,
                    rebonebase::production::ClipEdit::Move {
                        from: index,
                        to: index + 1,
                    },
                )
            {
                self.status = e;
            }
        });
        ui.horizontal(|ui| {
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
                    self.frame = 1.;
                    self.studio.keys.clear();
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
                        self.frame = 1.;
                        self.studio.keys.clear();
                    }
                    Err(e) => self.status = e,
                }
            }
        });
        ui.horizontal(|ui| {
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
        ui.horizontal(|ui| {
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
        ui.horizontal(|ui| {
            if ui.button("Select all keys").clicked() {
                self.studio.keys = self
                    .project
                    .rig
                    .active_clip()
                    .tracks
                    .iter()
                    .flat_map(|t| {
                        t.keys.iter().map(|k| KeySelection {
                            bone: t.bone,
                            channel: t.channel,
                            frame: k.frame,
                        })
                    })
                    .collect();
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
                editing::delete_keys(self.project.rig.active_clip_mut(), &self.studio.keys);
                self.studio.keys.clear();
            }
        });
        ui.horizontal(|ui| {
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
        self.curve_editor(ui);
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
                    if record {
                        self.history.record(&before);
                    }
                    self.changed();
                    self.status = "Timeline keys pasted".into();
                }
                Err(e) => self.status = e,
            }
        }
    }
    pub(super) fn editable_dope(&mut self, ui: &mut egui::Ui) {
        struct Row {
            id: u32,
            name: String,
        }
        let length = self.project.rig.active_clip().length;
        let mut rows: Vec<_> = self
            .project
            .rig
            .bones
            .iter()
            .filter(|b| self.studio.selection.is_empty() || self.studio.selection.contains(&b.id))
            .map(|b| Row {
                id: b.id,
                name: b.name.clone(),
            })
            .collect();
        rows.extend(
            self.project
                .rig
                .parts
                .iter()
                .filter(|p| {
                    self.project
                        .rig
                        .active_clip()
                        .tracks
                        .iter()
                        .any(|t| t.bone == p.id)
                })
                .map(|p| Row {
                    id: p.id,
                    name: format!("Group · {}", p.name),
                }),
        );
        let height = ui.available_height().max(40.);
        egui::ScrollArea::both()
            .id_salt("dope")
            .max_height(height)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let cell = 24.;
                let labelw = 125.;
                let (rect, response) = ui.allocate_exact_size(
                    EVec2::new(labelw + cell * length as f32, 24. + rows.len() as f32 * 23.),
                    egui::Sense::click_and_drag(),
                );
                let p = ui.painter_at(rect);
                p.rect_filled(rect, 3., Color32::from_rgb(20, 26, 32));
                for f in 1..=length {
                    let x = rect.left() + labelw + (f - 1) as f32 * cell;
                    if f == self.frame as u32 {
                        p.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(x, rect.top()),
                                EVec2::new(cell, rect.height()),
                            ),
                            0.,
                            Color32::from_rgb(39, 74, 65),
                        );
                    }
                    p.line_segment(
                        [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                        Stroke::new(1., Color32::from_rgb(40, 48, 55)),
                    );
                    p.text(
                        Pos2::new(x + cell * 0.5, rect.top() + 11.),
                        egui::Align2::CENTER_CENTER,
                        f.to_string(),
                        egui::FontId::monospace(10.),
                        DIM,
                    );
                }
                for (i, b) in rows.iter().enumerate() {
                    let y = rect.top() + 35. + i as f32 * 23.;
                    if y < ui.clip_rect().top() - 16. || y > ui.clip_rect().bottom() + 16. {
                        continue;
                    }
                    p.text(
                        Pos2::new(rect.left() + 8., y),
                        egui::Align2::LEFT_CENTER,
                        &b.name,
                        egui::FontId::proportional(11.),
                        Color32::LIGHT_GRAY,
                    );
                    for t in &self.project.rig.active_clip().tracks {
                        if t.bone == b.id {
                            for key in &t.keys {
                                let x = rect.left() + labelw + (key.frame - 0.5) * cell;
                                let selected = self.studio.keys.contains(&KeySelection {
                                    bone: b.id,
                                    channel: t.channel,
                                    frame: key.frame,
                                });
                                let c = if selected {
                                    Color32::from_rgb(255, 209, 105)
                                } else {
                                    ACCENT
                                };
                                p.add(egui::Shape::convex_polygon(
                                    vec![
                                        Pos2::new(x, y - 4.),
                                        Pos2::new(x + 4., y),
                                        Pos2::new(x, y + 4.),
                                        Pos2::new(x - 4., y),
                                    ],
                                    c,
                                    Stroke::NONE,
                                ));
                            }
                        }
                    }
                }
                let at = response.interact_pointer_pos();
                if (response.clicked() || response.drag_started())
                    && let Some(pos) = at
                    && pos.x >= rect.left() + labelw
                {
                    let pos = if response.drag_started() {
                        ui.input(|i| i.pointer.press_origin()).unwrap_or(pos)
                    } else {
                        pos
                    };
                    let frame =
                        ((pos.x - rect.left() - labelw) / cell + 0.5).clamp(1., length as f32);
                    let row = ((pos.y - rect.top() - 24.) / 23.).floor() as i32;
                    let hits: Vec<_> = if row >= 0 {
                        rows.get(row as usize).map_or(vec![], |bone| {
                            self.project
                                .rig
                                .active_clip()
                                .tracks
                                .iter()
                                .filter(|t| t.bone == bone.id)
                                .flat_map(|t| {
                                    t.keys
                                        .iter()
                                        .filter(|k| (k.frame - frame).abs() * cell < 7.)
                                        .map(|k| KeySelection {
                                            bone: t.bone,
                                            channel: t.channel,
                                            frame: k.frame,
                                        })
                                })
                                .collect()
                        })
                    } else {
                        vec![]
                    };
                    if response.clicked() || !hits.iter().any(|k| self.studio.keys.contains(k)) {
                        let shift = ui.input(|i| i.modifiers.shift);
                        if !shift {
                            self.studio.keys.clear();
                        }
                        for hit in &hits {
                            if shift && self.studio.keys.contains(hit) {
                                self.studio.keys.retain(|k| k != hit);
                            } else if !self.studio.keys.contains(hit) {
                                self.studio.keys.push(*hit);
                            }
                        }
                    }
                    self.frame = frame.round();
                    self.mode = Mode::Pose;
                    self.playing = false;
                    self.changed();
                    if response.drag_started() && !hits.is_empty() {
                        self.studio.key_drag = Some(studio::KeyDrag {
                            base: self.project.clone(),
                            selected: self.studio.keys.clone(),
                            start: frame,
                            offset: 0.,
                            recorded: false,
                        });
                    }
                }
                if response.dragged()
                    && let Some(pos) = at
                    && let Some(mut drag) = self.studio.key_drag.take()
                {
                    let frame = (pos.x - rect.left() - labelw) / cell + 0.5;
                    let offset = (frame - drag.start).round();
                    let mut candidate = drag.base.clone();
                    match editing::retime_keys(
                        candidate.rig.active_clip_mut(),
                        &drag.selected,
                        offset,
                        1.,
                        drag.start,
                    ) {
                        Ok(()) => {
                            if offset != 0. && !drag.recorded {
                                self.history.record(&drag.base);
                                drag.recorded = true;
                            }
                            self.project = candidate;
                            self.studio.keys = drag
                                .selected
                                .iter()
                                .map(|k| KeySelection {
                                    frame: k.frame + offset,
                                    ..*k
                                })
                                .collect();
                            drag.offset = offset;
                            self.changed();
                        }
                        Err(e) => self.status = e,
                    }
                    self.studio.key_drag = Some(drag);
                }
                if response.dragged()
                    && self.studio.key_drag.is_none()
                    && let Some(pos) = at
                {
                    self.frame = ((pos.x - rect.left() - labelw) / cell + 0.5)
                        .round()
                        .clamp(1., length as f32);
                    self.mode = Mode::Pose;
                    self.playing = false;
                    self.changed();
                }
                if response.drag_stopped() {
                    self.studio.key_drag = None;
                }
            });
    }
    fn curve_editor(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Animation curves").strong());
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
        ui.horizontal(|ui| {
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
                    .flat_map(|t| t.keys.iter().map(|k| k.value))
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
            let _ = self
                .project
                .rig
                .active_clip_mut()
                .set_key(id, channel, self.frame, value);
            self.studio.keys = vec![KeySelection {
                bone: id,
                channel,
                frame: self.frame,
            }];
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
                Stroke::new(1., Color32::from_rgb(36, 45, 53)),
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
            p.add(egui::Shape::line(points, Stroke::new(1.5, ACCENT)));
            let mut handles = vec![];
            for key in &track.keys {
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
                        p.line_segment([point, pos], Stroke::new(1., Color32::YELLOW));
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
                self.frame = key.frame;
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
                        if !drag.recorded {
                            self.history.record(&drag.base);
                            drag.recorded = true;
                        }
                        self.project = candidate;
                        self.studio.keys = vec![KeySelection {
                            frame: key.frame,
                            ..drag.key
                        }];
                        self.studio.curve_history_frame = true;
                        self.changed();
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
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut key.frame)
                            .range(1. ..=clip.length as f32)
                            .prefix("Frame ")
                            .speed(0.25),
                    );
                    ui.add(
                        egui::DragValue::new(&mut key.value)
                            .prefix("Value ")
                            .speed(0.25),
                    );
                    ui.add(
                        egui::DragValue::new(&mut key.tangent_in)
                            .prefix("In ")
                            .speed(0.1),
                    );
                    ui.add(
                        egui::DragValue::new(&mut key.tangent_out)
                            .prefix("Out ")
                            .speed(0.1),
                    );
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
                    match replace_key(&mut self.project.rig, selected, key.clone()) {
                        Ok(()) => {
                            self.studio.keys = vec![KeySelection {
                                frame: key.frame,
                                ..selected
                            }]
                        }
                        Err(e) => self.status = e,
                    }
                }
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
fn replace_key(rig: &mut Rig, old: KeySelection, key: Keyframe) -> Result<(), String> {
    let mut candidate = rig.clone();
    let looped = candidate.active_clip().looped;
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
