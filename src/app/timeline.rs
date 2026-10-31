use super::*;
use rebonebase::editing::{self, KeySelection};

mod clip_controls;
mod dope;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Default, PartialEq)]
enum TimelineTab {
    #[default]
    DopeSheet,
    Curves,
}
#[derive(Clone, Copy, Default, PartialEq)]
enum TrackFilter {
    #[default]
    All,
    Selected,
    Animated,
}
pub(super) struct TimelineUi {
    tab: TimelineTab,
    filter: TrackFilter,
    search: String,
    zoom: f32,
    fit: bool,
    scroll_x: f32,
    reveal_playhead: bool,
    metadata_control: Option<egui::Id>,
}
impl Default for TimelineUi {
    fn default() -> Self {
        Self {
            tab: TimelineTab::DopeSheet,
            filter: TrackFilter::All,
            search: String::new(),
            zoom: 1.,
            fit: false,
            scroll_x: 0.,
            reveal_playhead: false,
            metadata_control: None,
        }
    }
}
impl ReboneApp {
    /// All timeline navigation shares the playback and audio clock boundary.
    pub(super) fn seek_frame(&mut self, frame: f32) {
        self.production.player.stop();
        self.production.audio_until = None;
        self.production.audio_error = false;
        self.playing = false;
        self.frame = if frame.is_finite() { frame } else { 1. }
            .clamp(1., self.project.rig.active_clip().length.max(1) as f32);
        self.mode = Mode::Pose;
        if matches!(self.tool, Tool::Bone | Tool::Radius | Tool::Bulge) {
            self.tool = Tool::Select;
        }
        self.last_tick = Instant::now();
        self.sync_playback_time();
        self.changed();
    }
    fn sync_playback_time(&mut self) {
        let clip = self.project.rig.active_clip();
        self.studio.playback_ms = clip
            .frame_sequence()
            .into_iter()
            .take_while(|f| *f != self.frame as u32)
            .map(|f| clip.duration_ms(f) as f64)
            .sum();
    }
    pub(super) fn activate_clip(&mut self, index: usize) {
        if index >= self.project.rig.clips.len() {
            return;
        }
        self.project.rig.active_clip = index;
        self.studio.keys.clear();
        self.studio.key_drag = None;
        self.studio.curve_drag = None;
        self.studio.curve_history_frame = false;
        self.timeline.metadata_control = None;
        self.timeline.reveal_playhead = true;
        self.production.last_event = None;
        self.seek_frame(1.);
    }
    pub(super) fn seek_key(&mut self, forward: bool) {
        let clip = self.project.rig.active_clip();
        let next = clip.tracks.iter().flat_map(|t| &t.keys)
            .map(|k| k.frame)
            .filter(|f| (1. ..=clip.length as f32).contains(f))
            .filter(|f| if forward { *f > self.frame + 0.001 } else { *f < self.frame - 0.001 })
            .min_by(|a, b| if forward { a.total_cmp(b) } else { b.total_cmp(a) });
        self.seek_frame(next.unwrap_or(self.frame));
        self.timeline.reveal_playhead = true;
    }
    pub(super) fn toggle_play(&mut self) {
        let playing = !self.playing;
        let frame = if playing
            && self.project.rig.active_clip().playback == rebonebase::animation::Playback::Reverse
            && self.frame <= 1.
        {
            self.project.rig.active_clip().length as f32
        } else {
            self.frame
        };
        self.seek_frame(frame);
        self.playing = playing;
    }
    pub(super) fn timeline_ui(&mut self, ui: &mut egui::Ui) {
        ui.push_id("animation_timeline", |ui| {
            self.timeline_transport(ui);
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.timeline.tab, TimelineTab::DopeSheet, "Dope Sheet");
                ui.selectable_value(&mut self.timeline.tab, TimelineTab::Curves, "Curves");
                ui.separator();
                self.timeline_key_actions(ui);
                if ui.add_enabled(self.selected.is_some(), egui::Button::new("+ Key pose")).clicked() {
                    self.key_selected();
                }
                if ui.button("Templates…").on_hover_text("Animation templates").clicked() {
                    self.open_production_tab(production_ui::Tab::Motion);
                }
            });
            match self.timeline.tab {
                TimelineTab::DopeSheet => {
                    self.timeline_view_controls(ui);
                    self.editable_dope(ui);
                }
                TimelineTab::Curves => self.timeline_curves(ui),
            }
        });
    }
    fn timeline_key_actions(&mut self, ui: &mut egui::Ui) {
        let selected = !self.studio.keys.is_empty();
        ui.menu_button(format!("Keys · {}", self.studio.keys.len()), |ui| {
            if ui.button("Select all visible keys").clicked() { self.select_all_timeline_keys(); ui.close(); }
            if ui.add_enabled(selected, egui::Button::new("Clear selection")).clicked() { self.studio.keys.clear(); ui.close(); }
            ui.separator();
            egui::ComboBox::from_id_salt("key_easing").width(110.)
                .selected_text(format!("{:?}", self.easing)).show_ui(ui, |ui| {
                    for ease in [Ease::Linear, Ease::Ease, Ease::EaseIn, Ease::EaseOut, Ease::Back, Ease::Bounce, Ease::Step, Ease::Cubic] {
                        ui.selectable_value(&mut self.easing, ease, format!("{ease:?}"));
                    }
                });
            if ui.button("Apply easing").on_hover_text("Apply to selected keys, or keys at the playhead when no keys are selected").clicked() {
                let before = self.project.clone();
                for track in &mut self.project.rig.active_clip_mut().tracks {
                    for key in &mut track.keys {
                        let at_playhead = (key.frame - self.frame).abs() < 0.01 && self.selected.is_none_or(|id| id == track.bone);
                        if if selected { self.studio.keys.contains(&KeySelection { bone: track.bone, channel: track.channel, frame: key.frame }) } else { at_playhead } {
                            key.easing = self.easing;
                        }
                    }
                }
                if before != self.project { self.history.record(&before); self.changed(); }
                ui.close();
            }
            if ui.add_enabled(self.selected.is_some(), egui::Button::new("Remove keys here")).clicked() {
                let before = self.project.clone();
                for track in &mut self.project.rig.active_clip_mut().tracks {
                    if Some(track.bone) == self.selected { track.keys.retain(|key| (key.frame - self.frame).abs() > 0.01); }
                }
                if before != self.project { self.history.record(&before); self.studio.keys.clear(); self.changed(); }
                ui.close();
            }
        });
        if ui.add_enabled(selected, egui::Button::new("Copy").small()).clicked() { self.copy_timeline_keys(); }
        if ui.add_enabled(self.studio.clipboard.as_ref().is_some_and(|c| !c.keys.is_empty()), egui::Button::new("Paste").small()).on_hover_text("Paste keys at the playhead").clicked() { self.paste_timeline_keys(true); }
        if ui.add_enabled(selected, egui::Button::new("Delete keys").small()).clicked() { self.delete_timeline_keys(true); }
    }
    fn timeline_view_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Tracks").small().color(DIM));
            for (filter, name) in [(TrackFilter::All, "All"), (TrackFilter::Selected, "Selected"), (TrackFilter::Animated, "Animated")] {
                ui.selectable_value(&mut self.timeline.filter, filter, name);
            }
            ui.add(egui::TextEdit::singleline(&mut self.timeline.search).desired_width(125.).hint_text("Find track…"));
            if ui.small_button("−").on_hover_text("Zoom out in time").clicked() {
                self.timeline.zoom = (self.timeline.zoom / 1.25).max(1. / 24_000.); self.timeline.fit = false;
            }
            ui.label(RichText::new(if self.timeline.fit { "Fit".into() } else { format!("{:.0}%", self.timeline.zoom * 100.) }).small());
            if ui.small_button("+").on_hover_text("Zoom in time").clicked() {
                self.timeline.zoom = (self.timeline.zoom * 1.25).min(4.); self.timeline.fit = false;
            }
            if ui.small_button("Fit").on_hover_text("Fit the complete clip to the available width").clicked() {
                self.timeline.fit = true; self.timeline.scroll_x = 0.; self.timeline.reveal_playhead = true;
            }
            if ui.small_button("Show playhead").clicked() { self.timeline.reveal_playhead = true; }
        });
    }
    fn timeline_curves(&mut self, ui: &mut egui::Ui) {
        let before = self.project.clone();
        self.studio.curve_history_frame = false;
        egui::ScrollArea::vertical().id_salt("timeline_curve_scroll").show(ui, |ui| {
            ui.push_id("timeline_curves", |ui| self.curve_editor(ui));
        });
        self.commit_studio_edit(before);
    }
    pub(super) fn select_all_timeline_keys(&mut self) {
        let clip = self.project.rig.active_clip();
        self.studio.keys = clip.tracks.iter().flat_map(|t| t.keys.iter()
            .filter(|k| (1. ..=clip.length as f32).contains(&k.frame))
            .map(|k| KeySelection { bone: t.bone, channel: t.channel, frame: k.frame })).collect();
    }
    pub(super) fn delete_timeline_keys(&mut self, record: bool) {
        let before = self.project.clone();
        editing::delete_keys(self.project.rig.active_clip_mut(), &self.studio.keys);
        self.studio.keys.clear();
        self.studio.key_drag = None;
        self.studio.curve_drag = None;
        if before != self.project {
            if record { self.history.record(&before); }
            self.changed();
        }
    }
}
