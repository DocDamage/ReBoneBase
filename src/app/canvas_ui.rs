use super::*;
impl ReboneApp {
    pub(super) fn canvas_center(&self) -> EVec2 {
        EVec2::new(
            self.project.extras.canvas_origin.x + self.project.rig.width as f32 * 0.5,
            self.project.extras.canvas_origin.y + self.project.rig.height as f32 * 0.5,
        )
    }
    pub(super) fn rebase_canvas_camera(&mut self, previous: EVec2) {
        let delta = (self.canvas_center() - previous) * self.zoom;
        self.pan += delta;
    }
    pub(super) fn sync_canvas(&mut self) {
        let origin = self.project.extras.canvas_origin;
        let size = Vec2::new(
            self.project.rig.width as f32,
            self.project.rig.height as f32,
        );
        let pose = self.pose();
        let complete = self.gesture.is_none()
            && self.studio.key_drag.is_none()
            && self.studio.curve_drag.is_none();
        if complete
            && !self.production.transition_preview
            && self.frame.fract() == 0.
            && self.canvas_snapshot.as_ref() == Some(&self.project)
        {
            return;
        }
        let result = if complete && self.canvas_snapshot.as_ref() != Some(&self.project) {
            rebonebase::canvas::grow_all(&mut self.project)
        } else {
            rebonebase::canvas::grow(&mut self.project, self.frame, pose.as_ref())
        };
        match result {
            Ok(true) => {
                let next = self.project.extras.canvas_origin;
                let next_size = Vec2::new(
                    self.project.rig.width as f32,
                    self.project.rig.height as f32,
                );
                let shift = (next + next_size * 0.5) - (origin + size * 0.5);
                self.pan += EVec2::new(shift.x, shift.y) * self.zoom;
                if let Some(g) = &mut self.gesture {
                    let _ = rebonebase::canvas::set_bounds(
                        &mut g.base,
                        next,
                        self.project.rig.width,
                        self.project.rig.height,
                    );
                }
                self.changed();
            }
            Err(e) => self.status = format!("Canvas growth: {e}"),
            Ok(false) => {}
        }
        if complete {
            self.canvas_snapshot = Some(self.project.clone());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn undo_and_redo_of_canvas_growth_keep_the_world_camera_steady() {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        app.fit = false;
        let screen = |app: &ReboneApp| {
            app.pan
                - EVec2::new(
                    app.project.extras.canvas_origin.x + app.project.rig.width as f32 * 0.5,
                    app.project.extras.canvas_origin.y + app.project.rig.height as f32 * 0.5,
                ) * app.zoom
        };
        let original = screen(&app);
        app.record();
        app.project.rig.bones[0].x -= 60.;
        app.sync_canvas();
        assert!((screen(&app) - original).length() < 0.001);
        app.undo();
        assert!(
            (screen(&app) - original).length() < 0.001,
            "Undo moved the world camera"
        );
        app.redo();
        assert!(
            (screen(&app) - original).length() < 0.001,
            "Redo moved the world camera"
        );
    }
    #[test]
    fn growth_preserves_screen_joint_positions_and_gesture_bases_and_corrected_pixels() {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        app.fit = false;
        let root = app
            .project
            .rig
            .bones
            .iter()
            .find(|b| b.parent.is_none())
            .unwrap()
            .id;
        app.project.rig.bone_mut(root).unwrap().x = -20.;
        let w = app.project.rig.world(None)[0].head;
        let screen = |app: &ReboneApp| {
            app.pan
                + EVec2::new(
                    w.x - app.project.extras.canvas_origin.x - app.project.rig.width as f32 * 0.5,
                    w.y - app.project.extras.canvas_origin.y - app.project.rig.height as f32 * 0.5,
                ) * app.zoom
        };
        let before = screen(&app);
        app.gesture = Some(Gesture {
            base: app.project.clone(),
            start: w,
            id: Some(root),
            tail: false,
            tool: Tool::MoveRig,
            recorded: false,
        });
        app.sync_canvas();
        assert!(app.project.extras.canvas_origin.x < 0.);
        assert!((screen(&app) - before).length() < 0.001);
        assert_eq!(
            app.gesture.as_ref().unwrap().base.extras.canvas_origin,
            app.project.extras.canvas_origin
        );
    }
}
