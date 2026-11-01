use super::*;

/// A tail is picked before a coincident child head; bodies follow endpoints.
pub(super) fn hits(
    rig: &Rig,
    world: &[rebonebase::model::WorldBone],
    at: Vec2,
    tolerance: f32,
) -> Vec<(u32, bool)> {
    let mut hits: Vec<_> = world
        .iter()
        .filter(|w| rig.bone(w.id).is_some_and(|b| !b.locked))
        .filter_map(|w| {
            let tail = (w.tail - at).length();
            let head = (w.head - at).length();
            let d = w.tail - w.head;
            let len = d.x * d.x + d.y * d.y;
            let t = if len > 0. {
                ((at.x - w.head.x) * d.x + (at.y - w.head.y) * d.y) / len
            } else {
                0.
            };
            let body = (at - (w.head + d * t.clamp(0., 1.))).length();
            if tail < tolerance {
                Some((w.id, true, 0, tail))
            } else if head < tolerance {
                Some((w.id, false, 1, head))
            } else if body < tolerance {
                Some((w.id, false, 2, body))
            } else {
                None
            }
        })
        .collect();
    hits.sort_by(|a, b| {
        (a.2 == 2)
            .cmp(&(b.2 == 2))
            .then(a.3.total_cmp(&b.3))
            .then(a.2.cmp(&b.2))
            .then(a.0.cmp(&b.0))
    });
    hits.into_iter()
        .map(|(id, tail, _, _)| (id, tail))
        .collect()
}

impl ReboneApp {
    pub(super) fn pick(
        &mut self,
        world: &[rebonebase::model::WorldBone],
        at: Vec2,
        cycle: bool,
    ) -> Option<(u32, bool)> {
        let candidates = hits(&self.project.rig, world, at, 9. / self.zoom);
        let ids: Vec<_> = candidates.iter().map(|h| h.0).collect();
        if ids != self.studio.pick_ids {
            self.studio.pick_cycle = 0;
            self.studio.pick_ids = ids;
        } else if cycle && !ids.is_empty() {
            self.studio.pick_cycle = (self.studio.pick_cycle + 1) % ids.len();
        }
        candidates.get(self.studio.pick_cycle).copied()
    }
    pub(super) fn finish_marquee(
        &mut self,
        world: &[rebonebase::model::WorldBone],
        additive: bool,
    ) {
        if let Some((a, b)) = self.studio.marquee.take() {
            if !additive {
                self.studio.selection.clear();
            }
            let inside = |p: Vec2| {
                p.x >= a.x.min(b.x)
                    && p.x <= a.x.max(b.x)
                    && p.y >= a.y.min(b.y)
                    && p.y <= a.y.max(b.y)
            };
            for w in world {
                if (inside(w.head) || inside(w.tail))
                    && self.project.rig.bone(w.id).is_some_and(|b| !b.locked)
                {
                    self.studio.selection.insert(w.id);
                }
            }
            self.selected = self.studio.selection.iter().next().copied();
            self.studio.last_primary = self.selected;
        }
    }
    pub(super) fn onion_image(&mut self) -> image::RgbaImage {
        let r = &self.project.rig;
        let settings = &self.project.extras.onion;
        let mut result = image::RgbaImage::new(r.width, r.height);
        for (count, direction, color) in [
            (settings.before, -1., settings.before_color),
            (settings.after, 1., settings.after_color),
        ] {
            for step in (1..=count).rev() {
                let frame = self.frame + direction * step as f32 * settings.spacing as f32;
                if frame < 1. || frame > r.active_clip().length as f32 {
                    continue;
                }
                let img = self.preview.image(&self.project, frame);
                let opacity = settings.opacity / (step as f32).sqrt();
                for (out, src) in result.pixels_mut().zip(img.pixels()) {
                    let alpha = src[3] as f32 / 255. * opacity;
                    let old = out[3] as f32 / 255.;
                    let combined = alpha + old * (1. - alpha);
                    if combined > 0. {
                        for ch in 0..3 {
                            out[ch] = ((color[ch] as f32 * alpha
                                + out[ch] as f32 * old * (1. - alpha))
                                / combined)
                                .round() as u8;
                        }
                        out[3] = (combined * 255.).round() as u8;
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_priority_locks_and_marquee() {
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
        let world = app.project.rig.world(None);
        let at = world.iter().find(|w| w.id == id).unwrap().tail;
        assert_eq!(hits(&app.project.rig, &world, at, 0.1)[0], (id, true));
        app.project.rig.bone_mut(id).unwrap().locked = true;
        assert!(
            !hits(&app.project.rig, &world, at, 0.1)
                .iter()
                .any(|h| h.0 == id)
        );
        let before = app.project.clone();
        app.studio.marquee = Some((Vec2::new(-500., -500.), Vec2::new(500., 500.)));
        app.finish_marquee(&world, false);
        assert_eq!(app.studio.selection.len(), app.project.rig.bones.len() - 1);
        assert_eq!(app.project, before);
    }
    #[test]
    fn onion_respects_both_sides() {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        app.frame = 1.;
        app.project.extras.onion.before = 8;
        app.project.extras.onion.after = 0;
        assert!(app.onion_image().pixels().all(|p| p[3] == 0));
        app.project.extras.onion.after = 1;
        assert!(app.onion_image().pixels().any(|p| p[3] > 0));
    }
}
