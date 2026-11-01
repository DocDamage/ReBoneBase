use super::production_ui::{PaintMode, bone_combo, clip_combo};
use super::*;
use rebonebase::production::{self, BoxKey, BoxTrack, EventMarker, SceneActor, audio};
impl ReboneApp {
    pub(super) fn production_scene(&mut self, ui: &mut egui::Ui) {
        if self.production.actor.is_some() {
            self.production.move_character = false;
        }
        ui.heading("Multiple characters and props in one scene");
        if ui.button("Import project as scene actor…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("ReBoneBase project", &["rebone"])
                .pick_file()
        {
            match project::load(&path) {
                Ok(mut actor) => {
                    actor.extras.production.actors.clear();
                    let id = self.project.extras.production.next_id();
                    self.project.extras.production.actors.push(SceneActor {
                        id,
                        name: actor.rig.name.clone(),
                        project: Box::new(actor),
                        ..Default::default()
                    });
                    self.production.actor = Some(id);
                    self.status = "Scene actor imported; nested actors are excluded".into();
                }
                Err(e) => self.status = e.to_string(),
            }
        }
        if ui.button("Duplicate active character as actor").clicked() {
            let mut actor = self.project.clone();
            actor.extras.production.actors.clear();
            let id = self.project.extras.production.next_id();
            self.project.extras.production.actors.push(SceneActor {
                id,
                name: format!("{} copy", actor.rig.name),
                project: Box::new(actor),
                offset: Vec2::new(12., 0.),
                ..Default::default()
            });
            self.production.actor = Some(id);
        }
        let p = &mut self.project.extras.production;
        let target_name = if self.production.move_character {
            self.production
                .character
                .and_then(|id| self.project.extras.characters3d.iter().find(|c| c.id == id))
                .map_or("Choose 3D character", |c| c.name.as_str())
        } else {
            self.production
                .actor
                .and_then(|id| p.actors.iter().find(|a| a.id == id))
                .map_or("Main character", |a| a.name.as_str())
        }
        .to_string();
        egui::ComboBox::from_label("Move Rig target")
            .selected_text(target_name)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(
                        !self.production.move_character && self.production.actor.is_none(),
                        "Main character",
                    )
                    .clicked()
                {
                    self.production.actor = None;
                    self.production.move_character = false;
                }
                for a in &p.actors {
                    if ui
                        .selectable_label(
                            !self.production.move_character && self.production.actor == Some(a.id),
                            &a.name,
                        )
                        .clicked()
                    {
                        self.production.actor = Some(a.id);
                        self.production.move_character = false;
                    }
                }
                for c in &self.project.extras.characters3d {
                    if ui
                        .selectable_label(
                            self.production.move_character
                                && self.production.character == Some(c.id),
                            format!("3D · {}", c.name),
                        )
                        .clicked()
                    {
                        self.production.actor = None;
                        self.production.character = Some(c.id);
                        self.production.move_character = true;
                    }
                }
            });
        let mut remove = None;
        for (i, a) in p.actors.iter_mut().enumerate() {
            ui.push_id(("actor", a.id), |ui| {
                egui::CollapsingHeader::new(&a.name)
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.text_edit_singleline(&mut a.name);
                        ui.horizontal_wrapped(|ui| {
                            ui.checkbox(&mut a.visible, "Visible actor");
                            ui.add(egui::DragValue::new(&mut a.offset.x).prefix("X "));
                            ui.add(egui::DragValue::new(&mut a.offset.y).prefix("Y "));
                            ui.add(
                                egui::DragValue::new(&mut a.scale)
                                    .range(0.05..=16.)
                                    .speed(0.05)
                                    .prefix("Scale "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut a.z)
                                    .range(-100..=100)
                                    .prefix("Layer "),
                            );
                        });
                        let mut clip = a.project.rig.active_clip;
                        clip_combo(ui, &a.project.rig, "Actor animation", &mut clip);
                        a.project.rig.active_clip = clip;
                        ui.horizontal_wrapped(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut a.frame_offset)
                                    .range(-240. ..=240.)
                                    .prefix("Start offset frames "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut a.speed)
                                    .range(0. ..=8.)
                                    .speed(0.05)
                                    .prefix("Speed "),
                            );
                            if ui.button("Key actor placement here").clicked() {
                                if let Err(error) =
                                    self.project.rig.active_clip_mut().release_hold(self.frame)
                                {
                                    self.status = error;
                                    return;
                                }
                                a.keys.retain(|k| {
                                    k.clip != self.project.rig.active_clip || k.frame != self.frame
                                });
                                a.keys.push(production::ActorKey {
                                    clip: self.project.rig.active_clip,
                                    frame: self.frame,
                                    offset: a.offset,
                                    scale: a.scale,
                                    visible: a.visible,
                                });
                                a.keys.sort_by(|a, b| {
                                    a.clip.cmp(&b.clip).then(a.frame.total_cmp(&b.frame))
                                });
                            }
                            if ui.button("Move this actor with Move Rig").clicked() {
                                self.production.actor = Some(a.id);
                                self.tool = Tool::MoveRig;
                            }
                            if ui.button("Remove actor").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
            });
        }
        if let Some(i) = remove {
            let id = p.actors.remove(i).id;
            if self.production.actor == Some(id) {
                self.production.actor = None;
            }
        }
        ui.label("Each actor keeps its rig, artwork, controllers and clips. Its clip, offset, speed and visibility are independent; scene playback and export evaluate them together.");
        self.art_preview(ui);
    }
    pub(super) fn production_audio_events(&mut self, ui: &mut egui::Ui) {
        ui.heading("Audio synchronization and event markers");
        if ui.button("Import PCM16 WAV…").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("PCM16 WAV", &["wav"])
                .pick_file()
        {
            match audio::load_wav(&path) {
                Ok(a) => {
                    self.project.extras.production.audio = Some(a);
                    self.production.audio_error = false;
                    self.production.player.stop();
                }
                Err(e) => self.status = e.to_string(),
            }
        }
        let mut remove = false;
        if let Some(a) = &mut self.project.extras.production.audio {
            ui.label(format!(
                "{} · {:.2} seconds · {} Hz · {} channels",
                a.name,
                a.duration_ms() as f32 / 1000.,
                a.sample_rate,
                a.channels
            ));
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut a.enabled, "Audio enabled");
                if ui
                    .add(egui::Slider::new(&mut a.volume, 0. ..=2.).text("Volume"))
                    .changed()
                {
                    self.production.player.stop();
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut a.start_ms)
                            .range(0..=86_400_000)
                            .prefix("Start ms "),
                    )
                    .changed()
                {
                    self.production.player.stop();
                }
                ui.checkbox(&mut self.production.audition, "Audition while scrubbing");
            });
            let clip = self.project.rig.active_clip();
            let duration = (clip.source_time_ms(clip.length as f32)
                + u64::from(clip.duration_ms(clip.length)))
            .max(a.duration_ms() + a.start_ms)
            .max(1);
            let values = audio::waveform(a, 512);
            let (rect, response) = ui.allocate_exact_size(
                EVec2::new(ui.available_width(), 110.),
                egui::Sense::click_and_drag(),
            );
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 3., Color32::from_rgb(16, 22, 29));
            let x0 = rect.left() + a.start_ms as f32 / duration as f32 * rect.width();
            let width = a.duration_ms() as f32 / duration as f32 * rect.width();
            for (i, (min, max)) in values.iter().enumerate() {
                let x = x0 + i as f32 / values.len() as f32 * width;
                p.line_segment(
                    [
                        Pos2::new(x, rect.center().y + min * 45.),
                        Pos2::new(x, rect.center().y + max * 45.),
                    ],
                    Stroke::new(1.0_f32, ACCENT),
                );
            }
            let play_time = if self.playing {
                audio::playback_cursor(self.project.rig.active_clip(), self.studio.playback_ms).0
            } else {
                self.project.rig.active_clip().source_time_ms(self.frame)
            };
            let x = rect.left() + play_time as f32 / duration as f32 * rect.width();
            p.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(1.5_f32, Color32::YELLOW),
            );
            if (response.clicked() || response.dragged())
                && let Some(at) = response.interact_pointer_pos()
            {
                let time = ((at.x - rect.left()) / rect.width() * duration as f32).max(0.) as u64;
                let f = audio::frame_at_source_time(self.project.rig.active_clip(), time);
                self.frame = f as f32;
                self.mode = Mode::Pose;
                self.playing = false;
                self.changed();
            }
            ui.horizontal(|ui| {
                if ui.button("Audition from playhead").clicked() {
                    let time = self.project.rig.active_clip().source_time_ms(self.frame);
                    match self.production.player.play(
                        self.project.extras.production.audio.as_ref().unwrap(),
                        time,
                        5000,
                    ) {
                        Ok(()) => {
                            self.production.audio_until =
                                Some(Instant::now() + Duration::from_secs(5));
                            ui.ctx().request_repaint_after(Duration::from_millis(100));
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                }
                if ui.button("Stop audio").clicked() {
                    self.production.player.stop();
                    self.production.audio_until = None;
                }
                remove = ui.button("Remove audio").clicked();
            });
        } else {
            ui.label("Import a mono/stereo PCM16 WAV. Audio remains embedded in your project; playback uses the Windows audio device.");
        }
        if remove {
            self.project.extras.production.audio = None;
            self.production.player.stop();
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Event");
            ui.text_edit_singleline(&mut self.production.event_name);
            ui.label("Payload");
            ui.text_edit_singleline(&mut self.production.event_payload);
        });
        if ui.button("Mark current frame").clicked() {
            self.project.extras.production.events.push(EventMarker {
                clip: self.project.rig.active_clip,
                frame: self.frame,
                name: self.production.event_name.clone(),
                payload: self.production.event_payload.clone(),
            });
        }
        let mut remove = None;
        for (i, event) in self.project.extras.production.events.iter_mut().enumerate() {
            ui.push_id(("event", i), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        self.project
                            .rig
                            .clips
                            .get(event.clip)
                            .map_or("Missing clip", |c| c.name.as_str()),
                    );
                    ui.add(
                        egui::DragValue::new(&mut event.frame)
                            .range(1. ..=self.project.rig.clips[event.clip].length as f32)
                            .prefix("Frame "),
                    );
                    ui.text_edit_singleline(&mut event.name);
                    ui.text_edit_singleline(&mut event.payload);
                    if ui.small_button("Go to event").clicked() {
                        self.project.rig.active_clip = event.clip;
                        self.frame = event.frame;
                        self.mode = Mode::Pose;
                    }
                    if ui.small_button("Remove event").clicked() {
                        remove = Some(i);
                    }
                });
            });
        }
        if let Some(i) = remove {
            self.project.extras.production.events.remove(i);
        }
        ui.label("Events are shown during playback and exported with frame timing and collision data in game packages.");
    }
    pub(super) fn production_pixels_boxes(&mut self, ui: &mut egui::Ui) {
        ui.heading("Pixel corrections and collision regions");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Use pixel correction brush").clicked() {
                self.tool = Tool::Pixel;
                self.mode = Mode::Pose;
            }
            ui.color_edit_button_srgba_unmultiplied(&mut self.production.pixel_color);
            ui.add(
                egui::DragValue::new(&mut self.production.brush)
                    .range(1..=8)
                    .prefix("Brush "),
            );
            ui.selectable_value(&mut self.production.paint_mode, PaintMode::Paint, "Paint");
            ui.selectable_value(
                &mut self.production.paint_mode,
                PaintMode::Erase,
                "Erase ink",
            );
            ui.selectable_value(
                &mut self.production.paint_mode,
                PaintMode::Restore,
                "Restore generated",
            );
        });
        if ui.button("Clear corrections on current frame").clicked() {
            self.project
                .extras
                .production
                .corrections
                .retain(|c| c.clip != self.project.rig.active_clip || c.frame != self.frame as u32);
        }
        ui.label("Corrections affect only this clip/frame and keep the rig editable. Erase removes rendered ink; Restore removes the correction itself.");
        ui.separator();
        ui.checkbox(&mut self.production.show_boxes, "Show collision overlays");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Draw collision region on canvas").clicked() {
                self.tool = Tool::Box;
                self.mode = Mode::Pose;
            }
            ui.checkbox(
                &mut self.production.box_attached,
                "Attach new region to selected bone",
            );
            for kind in [
                production::BoxKind::Hurt,
                production::BoxKind::Hit,
                production::BoxKind::Interaction,
            ] {
                ui.selectable_value(&mut self.production.box_kind, kind, format!("{kind:?}"));
            }
            if ui.button("Add default collision region").clicked() {
                let id = self.project.extras.production.next_id();
                self.project.extras.production.boxes.push(BoxTrack {
                    id,
                    clip: self.project.rig.active_clip,
                    end: self.project.rig.active_clip().length,
                    kind: self.production.box_kind,
                    bone: if self.production.box_attached {
                        self.selected
                    } else {
                        None
                    },
                    offset: Vec2::new(8., 8.),
                    ..Default::default()
                });
                self.production.region = Some(id);
            }
        });
        let mut remove = None;
        for (i, b) in self.project.extras.production.boxes.iter_mut().enumerate() {
            ui.push_id(("box", b.id), |ui| {
                egui::CollapsingHeader::new(&b.name)
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.text_edit_singleline(&mut b.name);
                        ui.checkbox(&mut b.enabled, "Enabled region");
                        bone_combo(ui, &self.project.rig, "Region parent", &mut b.bone);
                        clip_combo(ui, &self.project.rig, "Region clip", &mut b.clip);
                        ui.horizontal_wrapped(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut b.start)
                                    .range(1..=self.project.rig.clips[b.clip].length)
                                    .prefix("Start "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut b.end)
                                    .range(b.start..=self.project.rig.clips[b.clip].length)
                                    .prefix("End "),
                            );
                            ui.add(egui::DragValue::new(&mut b.offset.x).prefix("X "));
                            ui.add(egui::DragValue::new(&mut b.offset.y).prefix("Y "));
                            ui.add(
                                egui::DragValue::new(&mut b.size.x)
                                    .range(0.1..=4096.)
                                    .prefix("Width "),
                            );
                            ui.add(
                                egui::DragValue::new(&mut b.size.y)
                                    .range(0.1..=4096.)
                                    .prefix("Height "),
                            );
                            ui.add(egui::DragValue::new(&mut b.angle).prefix("Angle "));
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Key region here").clicked() {
                                if let Err(error) =
                                    self.project.rig.clips[b.clip].release_hold(self.frame)
                                {
                                    self.status = error;
                                    return;
                                }
                                b.keys.retain(|k| k.frame != self.frame);
                                b.keys.push(BoxKey {
                                    frame: self.frame,
                                    offset: b.offset,
                                    size: b.size,
                                    angle: b.angle,
                                    enabled: b.enabled,
                                });
                                b.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                            }
                            if ui.button("Remove collision region").clicked() {
                                remove = Some(i);
                            }
                        });
                        let mut drop_key = None;
                        for (j, k) in b.keys.iter_mut().enumerate() {
                            if !(1. ..=self.project.rig.clips[b.clip].length as f32)
                                .contains(&k.frame)
                            {
                                continue;
                            }
                            let before = k.clone();
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!("Frame {:.2}", k.frame));
                                ui.checkbox(&mut k.enabled, "Active");
                                ui.add(egui::DragValue::new(&mut k.offset.x).prefix("X "));
                                ui.add(egui::DragValue::new(&mut k.offset.y).prefix("Y "));
                                ui.add(
                                    egui::DragValue::new(&mut k.size.x)
                                        .range(0.1..=4096.)
                                        .prefix("W "),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut k.size.y)
                                        .range(0.1..=4096.)
                                        .prefix("H "),
                                );
                                ui.add(egui::DragValue::new(&mut k.angle).prefix("Angle "));
                                if ui.small_button("Delete region key").clicked() {
                                    drop_key = Some(j);
                                }
                            });
                            if *k != before
                                && let Err(error) =
                                    self.project.rig.clips[b.clip].release_hold(k.frame)
                            {
                                *k = before;
                                self.status = error;
                            }
                        }
                        if let Some(j) = drop_key {
                            b.keys.remove(j);
                        }
                    });
            });
        }
        if let Some(i) = remove {
            self.project.extras.production.boxes.remove(i);
        }
        ui.label("Collision overlays do not appear in sprite pixels; exported metadata contains their transformed geometry per frame.");
    }
    pub(super) fn production_export(&mut self, ui: &mut egui::Ui) {
        ui.heading("Godot, Unity and generic game packages");
        self.engine_options(ui);
        ui.checkbox(
            &mut self.exports.overwrite,
            "Replace existing package files",
        );
        if ui
            .add_enabled(
                self.exports.job.is_none(),
                egui::Button::new("Choose folder and export game package…"),
            )
            .clicked()
        {
            self.export_kind(export_ui::Kind::Engine);
        }
        self.export_progress(ui, 220.);
        ui.label("Packages contain rendered PNG sheets, timing, pivots, event markers and animated collision regions. Godot includes a SpriteFrames loader; Unity includes a SpriteRenderer player and editor importer. All graphics/controllers are baked into the pixels. Audio WAV and its start/volume metadata are included when present.");
    }
    pub(super) fn paint_stroke(&mut self, start: Vec2, end: Vec2, record: bool) {
        if !start.is_finite() || !end.is_finite() {
            return;
        }
        let before = self.project.clone();
        let clip_index = self.project.rig.active_clip;
        let start = start - self.project.extras.canvas_origin;
        let end = end - self.project.extras.canvas_origin;
        let width = self.project.rig.width as i32;
        let height = self.project.rig.height as i32;
        let a = Vec2::new(
            start.x.clamp(-8., width as f32 + 8.),
            start.y.clamp(-8., height as f32 + 8.),
        );
        let b = Vec2::new(
            end.x.clamp(-8., width as f32 + 8.),
            end.y.clamp(-8., height as f32 + 8.),
        );
        let steps = (b.x - a.x).abs().max((b.y - a.y).abs()).ceil().min(8192.) as u32;
        let radius = self.production.brush.clamp(1, 8) as i32;
        let color = match self.production.paint_mode {
            PaintMode::Paint => Some(self.production.pixel_color),
            PaintMode::Erase => Some([0; 4]),
            PaintMode::Restore => None,
        };
        let mut pixels = Vec::new();
        for i in 0..=steps {
            let t = i as f32 / steps.max(1) as f32;
            let point = a + (b - a) * t;
            for y in 0..radius {
                for x in 0..radius {
                    let px = point.x.floor() as i32 + x - radius / 2;
                    let py = point.y.floor() as i32 + y - radius / 2;
                    if px >= 0 && py >= 0 && px < width && py < height {
                        pixels.push((px as u32, py as u32));
                    }
                }
            }
        }
        if let Err(e) = production::paint_pixels(
            &mut self.project,
            clip_index,
            self.frame as u32,
            &pixels,
            color,
        ) {
            self.status = format!("Pixel edit exceeds document limits: {e}");
            return;
        }
        if record && self.project != before {
            self.history.record(&before);
        }
        self.changed();
    }
    pub(super) fn draw_collision(&mut self, start: Vec2, end: Vec2) {
        let size = Vec2::new((end.x - start.x).abs(), (end.y - start.y).abs());
        if size.x < 0.5 || size.y < 0.5 {
            return;
        }
        let before = self.project.clone();
        let center = (start + end) * 0.5;
        let bone = if self.production.box_attached {
            self.selected
        } else {
            None
        };
        let world = self.project.rig.world(self.pose().as_ref());
        let (offset, angle) = bone
            .and_then(|id| world.iter().find(|b| b.id == id))
            .map_or((center, 0.), |b| {
                ((center - b.head).rotate(-b.angle), -b.angle)
            });
        let id = self.project.extras.production.next_id();
        self.project.extras.production.boxes.push(BoxTrack {
            id,
            clip: self.project.rig.active_clip,
            bone,
            offset,
            size,
            angle,
            start: 1,
            end: self.project.rig.active_clip().length,
            kind: self.production.box_kind,
            ..Default::default()
        });
        if let Err(e) = self.project.validate() {
            self.project = before;
            self.status = format!("Collision edit exceeds document limits: {e}");
            return;
        }
        self.history.record(&before);
        self.changed();
        self.production.region = Some(id);
    }
}
