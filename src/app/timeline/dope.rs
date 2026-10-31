use super::*;

const LABEL_WIDTH: f32 = 125.;
const HEADER_HEIGHT: f32 = 24.;
const ROW_HEIGHT: f32 = 23.;

struct Row {
    id: u32,
    name: String,
    group: bool,
}
#[derive(Clone, Copy)]
struct TimeScale {
    origin: f32,
    cell: f32,
    length: u32,
}
impl TimeScale {
    fn x(self, frame: f32) -> f32 { self.origin + (frame - 0.5) * self.cell }
    fn frame(self, x: f32) -> f32 { (x - self.origin) / self.cell + 0.5 }
    fn bounded_frame(self, x: f32) -> f32 { self.frame(x).round().clamp(1., self.length as f32) }
    fn visible(self, rect: Rect) -> std::ops::RangeInclusive<u32> {
        let first = ((rect.left() - self.origin) / self.cell).floor() as u32 + 1;
        let last = ((rect.right() - self.origin) / self.cell).ceil() as u32 + 1;
        first.clamp(1, self.length)..=last.clamp(1, self.length)
    }
}
impl ReboneApp {
    fn dope_rows(&self) -> Vec<Row> {
        let clip = self.project.rig.active_clip();
        let query = self.timeline.search.trim().to_lowercase();
        let selected = self.selected_bones();
        let animated = |id| clip.tracks.iter().any(|t| t.bone == id && !t.keys.is_empty());
        let accepts = |id, name: &str, selected| name.to_lowercase().contains(&query)
            && match self.timeline.filter {
                TrackFilter::All => true,
                TrackFilter::Selected => selected,
                TrackFilter::Animated => animated(id),
            };
        let mut rows: Vec<_> = self.project.rig.bones.iter()
            .filter(|b| accepts(b.id, &b.name, selected.contains(&b.id)))
            .map(|b| Row { id: b.id, name: b.name.clone(), group: false }).collect();
        rows.extend(self.project.rig.parts.iter()
            .filter(|p| clip.tracks.iter().any(|t| t.bone == p.id))
            .filter(|p| accepts(p.id, &p.name, self.project.rig.bones.iter().any(|b| b.part == p.id && selected.contains(&b.id))))
            .map(|p| Row { id: p.id, name: format!("Group · {}", p.name), group: true }));
        rows
    }
    pub(in crate::app) fn editable_dope(&mut self, ui: &mut egui::Ui) {
        let length = self.project.rig.active_clip().length.max(1);
        let rows = self.dope_rows();
        let width = ui.available_width();
        let cell = if self.timeline.fit {
            ((width - LABEL_WIDTH - 12.).max(1.) / length as f32).clamp(0.001, 96.)
        } else { 24. * self.timeline.zoom };
        if self.timeline.fit { self.timeline.zoom = cell / 24.; }
        let scroll_x = if self.timeline.reveal_playhead {
            ((self.frame - 0.5) * cell - (width - LABEL_WIDTH) * 0.5).max(0.)
        } else { self.timeline.scroll_x };
        self.timeline.reveal_playhead = false;
        let height = ui.available_height().max(40.);
        let output = egui::ScrollArea::both()
            .id_salt("dope_sheet")
            .scroll_source(egui::scroll_area::ScrollSource { drag: false, ..Default::default() })
            .horizontal_scroll_offset(scroll_x)
            .max_height(height)
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let (rect, _) = ui.allocate_exact_size(
                    EVec2::new(LABEL_WIDTH + cell * length as f32, HEADER_HEIGHT + rows.len().max(1) as f32 * ROW_HEIGHT),
                    egui::Sense::hover(),
                );
                let visible = Rect::from_min_size(rect.min + viewport.min.to_vec2(), viewport.size()).intersect(ui.clip_rect());
                let labels = Rect::from_min_max(visible.min, Pos2::new((visible.left() + LABEL_WIDTH).min(visible.right()), visible.bottom()));
                let header = Rect::from_min_max(Pos2::new(labels.right(), visible.top()), Pos2::new(visible.right(), visible.top() + HEADER_HEIGHT));
                let time_rect = Rect::from_min_max(header.min, visible.max);
                let scale = TimeScale { origin: rect.left() + LABEL_WIDTH, cell, length };
                let p = ui.painter_at(time_rect);
                p.rect_filled(time_rect, 3., Color32::from_rgb(20, 26, 32));
                let playhead = scale.x(self.frame);
                p.rect_filled(Rect::from_min_max(Pos2::new(playhead - cell * 0.5, visible.top()), Pos2::new(playhead + cell * 0.5, visible.bottom())), 0., Color32::from_rgb(39, 74, 65));
                let tick_step = [1, 2, 5, 10, 20, 50, 100, 200].into_iter().find(|step| *step as f32 * cell >= 24.).unwrap_or(200);
                for frame in scale.visible(time_rect) {
                    if cell >= 8. || (frame - 1) % tick_step == 0 {
                        let x = scale.x(frame as f32) - cell * 0.5;
                        p.line_segment([Pos2::new(x, visible.top()), Pos2::new(x, visible.bottom())], Stroke::new(1., Color32::from_rgb(40, 48, 55)));
                    }
                }
                let rows_clip = Rect::from_min_max(Pos2::new(visible.left(), header.bottom()), visible.max);
                self.paint_dope_rows(ui, &rows, rect, labels, rows_clip, scale);
                p.line_segment([Pos2::new(playhead, header.bottom()), Pos2::new(playhead, visible.bottom())], Stroke::new(1., ACCENT));
                let ruler = ui.painter_at(header);
                ruler.rect_filled(header, 0., Color32::from_rgb(27, 34, 42));
                for frame in scale.visible(time_rect) {
                    if frame == 1 || (frame - 1) % tick_step == 0 {
                        ruler.text(Pos2::new(scale.x(frame as f32), header.center().y), egui::Align2::CENTER_CENTER, frame.to_string(), egui::FontId::monospace(10.), DIM);
                    }
                }
                ruler.add(egui::Shape::convex_polygon(vec![Pos2::new(playhead - 4., header.top()), Pos2::new(playhead + 4., header.top()), Pos2::new(playhead, header.top() + 5.)], ACCENT, Stroke::NONE));
                let label_header = Rect::from_min_max(labels.min, Pos2::new(labels.right(), header.bottom()));
                ui.painter_at(label_header).rect_filled(label_header, 0., Color32::from_rgb(27, 34, 42));
                ui.painter_at(label_header).text(Pos2::new(labels.left() + 8., header.center().y), egui::Align2::LEFT_CENTER, format!("{} tracks", rows.len()), egui::FontId::proportional(11.), DIM);
                if rows.is_empty() {
                    p.text(Pos2::new(time_rect.left() + 12., header.bottom() + ROW_HEIGHT * 0.5), egui::Align2::LEFT_CENTER, "No matching tracks", egui::FontId::proportional(11.), DIM);
                }
                let response = ui.interact(time_rect, ui.id().with("time_grid"), egui::Sense::click_and_drag());
                self.interact_dope(ui, response, &rows, rect, header, scale);
            });
        self.timeline.scroll_x = output.state.offset.x;
    }
    fn paint_dope_rows(&mut self, ui: &mut egui::Ui, rows: &[Row], content: Rect, labels: Rect, clip: Rect, scale: TimeScale) {
        let label_clip = labels.intersect(clip);
        let time_clip = Rect::from_min_max(Pos2::new(labels.right(), clip.top()), clip.max);
        let lp = ui.painter_at(label_clip);
        let p = ui.painter_at(time_clip);
        lp.rect_filled(label_clip, 0., Color32::from_rgb(25, 32, 39));
        for (index, row) in rows.iter().enumerate() {
            let y = content.top() + HEADER_HEIGHT + index as f32 * ROW_HEIGHT;
            let row_rect = Rect::from_min_size(Pos2::new(labels.left(), y), EVec2::new(labels.width(), ROW_HEIGHT)).intersect(label_clip);
            if row_rect.height() <= 0. { continue; }
            let selected = self.studio.selection.contains(&row.id) || self.selected == Some(row.id);
            if selected {
                lp.rect_filled(row_rect, 0., Color32::from_rgb(39, 74, 65));
                p.rect_filled(Rect::from_min_size(Pos2::new(time_clip.left(), y), EVec2::new(time_clip.width(), ROW_HEIGHT)), 0., Color32::from_rgba_unmultiplied(39, 74, 65, 55));
            }
            let center_y = y + ROW_HEIGHT * 0.5;
            lp.text(Pos2::new(labels.left() + 8., center_y), egui::Align2::LEFT_CENTER, &row.name, egui::FontId::proportional(11.), if row.group { Color32::from_rgb(224, 185, 113) } else if selected { ACCENT } else { Color32::LIGHT_GRAY });
            let label_response = ui.interact(row_rect, ui.id().with(("track_label", row.id)), if row.group { egui::Sense::hover() } else { egui::Sense::click() });
            if row.group { label_response.on_hover_text("Group visibility / depth tracks"); }
            else if label_response.on_hover_text(format!("{} · Click to select; Shift-click to add or remove", row.name)).clicked() {
                self.select_bone(row.id, ui.input(|i| i.modifiers.shift));
                self.changed();
            }
            for track in self.project.rig.active_clip().tracks.iter().filter(|t| t.bone == row.id) {
                for key in track.keys.iter().filter(|key| (1. ..=scale.length as f32).contains(&key.frame)) {
                    let x = scale.x(key.frame);
                    if x < time_clip.left() - 5. || x > time_clip.right() + 5. { continue; }
                    let selected = self.studio.keys.contains(&KeySelection { bone: row.id, channel: track.channel, frame: key.frame });
                    p.add(egui::Shape::convex_polygon(vec![Pos2::new(x, center_y - 4.), Pos2::new(x + 4., center_y), Pos2::new(x, center_y + 4.), Pos2::new(x - 4., center_y)], if selected { Color32::from_rgb(255, 209, 105) } else { ACCENT }, Stroke::NONE));
                }
            }
        }
    }
    fn interact_dope(&mut self, ui: &egui::Ui, response: egui::Response, rows: &[Row], content: Rect, header: Rect, scale: TimeScale) {
        let at = response.interact_pointer_pos();
        if (response.clicked() || response.drag_started()) && let Some(pos) = at {
            let origin = if response.drag_started() { ui.input(|i| i.pointer.press_origin()).unwrap_or(pos) } else { pos };
            let frame = scale.frame(origin.x).clamp(1., scale.length as f32);
            let row_index = ((origin.y - content.top() - HEADER_HEIGHT) / ROW_HEIGHT).floor();
            let row = if origin.y >= header.bottom() && row_index >= 0. { rows.get(row_index as usize) } else { None };
            let hits: Vec<_> = row.map_or_else(Vec::new, |row| self.project.rig.active_clip().tracks.iter()
                .filter(|t| t.bone == row.id).flat_map(|t| t.keys.iter()
                    .filter(|k| (1. ..=scale.length as f32).contains(&k.frame) && (scale.x(k.frame) - origin.x).abs() < 7.)
                    .map(|k| KeySelection { bone: t.bone, channel: t.channel, frame: k.frame })).collect());
            if response.clicked() || !hits.iter().any(|k| self.studio.keys.contains(k)) {
                let shift = ui.input(|i| i.modifiers.shift);
                if !shift { self.studio.keys.clear(); }
                for hit in &hits {
                    if shift && self.studio.keys.contains(hit) { self.studio.keys.retain(|key| key != hit); }
                    else if !self.studio.keys.contains(hit) { self.studio.keys.push(*hit); }
                }
            }
            self.seek_frame(frame.round());
            if response.drag_started() && !hits.is_empty() {
                self.studio.key_drag = Some(studio::KeyDrag { base: self.project.clone(), selected: self.studio.keys.clone(), start: frame, offset: 0., recorded: false });
            }
        }
        if response.dragged() && let Some(pos) = at && let Some(mut drag) = self.studio.key_drag.take() {
            let offset = (scale.frame(pos.x) - drag.start).round();
            let mut candidate = drag.base.clone();
            match editing::retime_keys(candidate.rig.active_clip_mut(), &drag.selected, offset, 1., drag.start) {
                Ok(()) => {
                    if offset != 0. && !drag.recorded { self.history.record(&drag.base); drag.recorded = true; }
                    self.project = candidate;
                    self.studio.keys = drag.selected.iter().map(|key| KeySelection { frame: key.frame + offset, ..*key }).collect();
                    drag.offset = offset;
                    self.changed();
                }
                Err(e) => self.status = e,
            }
            self.studio.key_drag = Some(drag);
        }
        if response.dragged() && self.studio.key_drag.is_none() && let Some(pos) = at {
            self.seek_frame(scale.bounded_frame(pos.x));
        }
        if response.drag_stopped() { self.studio.key_drag = None; }
    }
}
