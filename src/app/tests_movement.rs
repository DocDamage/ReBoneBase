use super::*;

fn begin(app: &mut ReboneApp, tool: Tool, id: Option<u32>, start: Vec2, tail: bool) {
    app.tool = tool;
    app.gesture = Some(Gesture {
        base: app.project.clone(),
        start,
        id,
        tail,
        tool,
        recorded: false,
    });
}
fn close(a: Vec2, b: Vec2) {
    assert!((a - b).length() < 0.001, "{a:?} != {b:?}");
}
fn root_ids(app: &ReboneApp) -> Vec<u32> {
    app.project
        .rig
        .bones
        .iter()
        .filter(|b| b.parent.is_none())
        .map(|b| b.id)
        .collect()
}

#[test]
fn ordinary_root_drags_never_transform_the_whole_mannequin() {
    for mode in [Mode::Rest, Mode::Pose] {
        for tool in [Tool::Select, Tool::IK] {
            for tail in [false, true] {
                let ctx = egui::Context::default();
                let mut app = ReboneApp::with_context(&ctx);
                app.mode = mode;
                let id = root_ids(&app)[0];
                let w = app
                    .project
                    .rig
                    .world(None)
                    .into_iter()
                    .find(|w| w.id == id)
                    .unwrap();
                let start = if tail {
                    w.tail
                } else {
                    w.head + Vec2::new(0., -2.)
                };
                let before = app.project.clone();
                begin(&mut app, tool, Some(id), start, tail);
                app.drag(start + Vec2::new(4., 3.), false);
                assert_eq!(app.project, before, "Root drag changed the whole rig");
            }
        }
    }
}

#[test]
fn torso_ik_cannot_rotate_or_translate_the_root() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.mode = Mode::Pose;
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "chest")
        .unwrap()
        .id;
    let world = app.project.rig.world(None);
    let w = world.iter().find(|w| w.id == id).unwrap();
    let start = w.tail;
    begin(&mut app, Tool::IK, Some(id), start, true);
    app.drag(start + Vec2::new(4., 2.), false);
    let pose = app.project.rig.active_clip().sample(1.);
    for root in root_ids(&app) {
        assert!(
            !pose.contains_key(&root),
            "Torso IK wrote keys on the rig root"
        );
    }
    assert!(pose[&id].rotation.abs() > 0.1);
}

#[test]
fn loose_limb_drags_rotate_at_the_shoulder_without_detaching_it() {
    for tool in [Tool::Select, Tool::IK] {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        app.mode = Mode::Pose;
        let id = app
            .project
            .rig
            .bones
            .iter()
            .find(|b| b.name == "upper_arm_L")
            .unwrap()
            .id;
        let world = app.project.rig.world(None);
        let w = world.iter().find(|w| w.id == id).unwrap();
        let head = w.head;
        let start = w.head + Vec2::new((w.tail.x - w.head.x) * 0.6, (w.tail.y - w.head.y) * 0.6);
        begin(&mut app, tool, Some(id), start, false);
        app.drag(start + Vec2::new(5., -2.), false);
        let pose = app.project.rig.active_clip().sample(1.);
        let after = app.project.rig.world(Some(&pose));
        close(after.iter().find(|w| w.id == id).unwrap().head, head);
        assert!(pose[&id].rotation.abs() > 0.1);
        assert_eq!(pose[&id].x, 0.);
        assert_eq!(pose[&id].y, 0.);
        for root in root_ids(&app) {
            assert!(!pose.contains_key(&root));
        }
    }
}

#[test]
fn move_rig_translates_every_root_and_mirror_axis_with_one_undo() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.project
        .rig
        .add_bone(Vec2::new(8., 8.), Vec2::new(8., 12.), None)
        .unwrap();
    let before = app.project.clone();
    let world = app.project.rig.world(None);
    let start = Vec2::new(2., 2.);
    let delta = Vec2::new(6., -3.);
    begin(&mut app, Tool::MoveRig, None, start, false);
    app.drag(start + Vec2::new(3., 1.), false);
    app.drag(start + delta, false);
    app.gesture = None;
    for (a, b) in world.iter().zip(app.project.rig.world(None)) {
        close(b.head, a.head + delta);
        close(b.tail, a.tail + delta);
    }
    assert_eq!(app.project.rig.mirror.x, before.rig.mirror.x + delta.x);
    app.project.validate().unwrap();
    app.undo();
    assert_eq!(app.project, before);
    app.redo();
    assert_ne!(app.project, before);
    app.undo();
    assert_eq!(app.project, before);
}

#[test]
fn move_rig_in_pose_mode_keys_only_root_offsets_and_keeps_rest_geometry() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.mode = Mode::Pose;
    app.frame = 3.;
    app.project
        .rig
        .add_bone(Vec2::new(8., 8.), Vec2::new(8., 12.), None)
        .unwrap();
    for id in root_ids(&app) {
        app.project
            .rig
            .active_clip_mut()
            .set_key(id, Channel::X, 1., 2.)
            .unwrap();
        app.project
            .rig
            .active_clip_mut()
            .set_key(id, Channel::Y, 1., -1.)
            .unwrap();
    }
    let before = app.project.clone();
    let pose = app.pose().unwrap();
    let world = app.project.rig.world(Some(&pose));
    let delta = Vec2::new(5., 4.);
    begin(&mut app, Tool::MoveRig, None, Vec2::new(0., 0.), false);
    app.drag(delta, false);
    app.gesture = None;
    assert_eq!(app.project.rig.bones, before.rig.bones);
    assert_eq!(app.project.rig.mirror, before.rig.mirror);
    for (a, b) in world.iter().zip(app.project.rig.world(app.pose().as_ref())) {
        close(b.head, a.head + delta);
        close(b.tail, a.tail + delta);
    }
    let roots = root_ids(&app);
    assert!(
        app.project
            .rig
            .active_clip()
            .tracks
            .iter()
            .all(|t| roots.contains(&t.bone) && matches!(t.channel, Channel::X | Channel::Y))
    );
    app.project.validate().unwrap();
    app.undo();
    assert_eq!(app.project, before);
}

#[test]
fn a_joint_pivot_press_selects_without_an_arbitrary_fk_jump() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.mode = Mode::Pose;
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "upper_arm_L")
        .unwrap()
        .id;
    let w = app
        .project
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == id)
        .unwrap();
    let before = app.project.clone();
    begin(&mut app, Tool::Select, Some(id), w.head, false);
    app.drag(w.head + Vec2::new(5., 2.), false);
    assert_eq!(app.project, before);
}

#[test]
fn move_rig_shortcut_and_canvas_gesture_work_from_empty_space() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let pass = |app: &mut ReboneApp, events: Vec<egui::Event>| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1280., 720.))),
                events,
                ..Default::default()
            },
            |ctx| {
                app.keyboard(ctx);
                app.show(ctx)
            },
        );
    };
    pass(&mut app, vec![]);
    pass(&mut app, vec![]);
    pass(
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::M,
            physical_key: Some(egui::Key::M),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.tool == Tool::MoveRig);
    let world = app.project.rig.world(None);
    let before = app.project.clone();
    let start = app.last_canvas.left_top() + EVec2::new(25., 30.);
    let to = start + EVec2::new(24., 16.);
    let delta = Vec2::new(24. / app.zoom, 16. / app.zoom);
    let pointer = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    pass(
        &mut app,
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
    );
    pass(&mut app, vec![egui::Event::PointerMoved(to)]);
    pass(&mut app, vec![pointer(to, false)]);
    assert!(app.selected.is_none());
    for (a, b) in world.iter().zip(app.project.rig.world(None)) {
        close(b.head, a.head + delta);
    }
    app.undo();
    assert_eq!(app.project, before);
}

#[test]
fn history_navigation_finishes_an_active_move_rig_gesture() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let before = app.project.clone();
    begin(&mut app, Tool::MoveRig, None, Vec2::new(0., 0.), false);
    app.drag(Vec2::new(3., 4.), false);
    let moved = app.project.clone();
    assert!(app.gesture.is_some());
    app.undo();
    assert!(app.gesture.is_none());
    assert_eq!(app.project, before);
    app.drag(Vec2::new(8., 9.), false);
    assert_eq!(app.project, before);
    app.redo();
    assert_eq!(app.project, moved);
    begin(&mut app, Tool::MoveRig, None, Vec2::new(0., 0.), false);
    app.redo();
    assert!(app.gesture.is_none());
}

#[test]
fn rest_loose_limb_and_head_body_drags_keep_their_authored_joint_offsets() {
    for name in ["upper_arm_L", "head"] {
        for tool in [Tool::Select, Tool::IK] {
            let ctx = egui::Context::default();
            let mut app = ReboneApp::with_context(&ctx);
            app.mode = Mode::Rest;
            app.zoom = 8.;
            let bone = app
                .project
                .rig
                .bones
                .iter()
                .find(|b| b.name == name)
                .unwrap()
                .clone();
            let world = app.project.rig.world(None);
            let w = world.iter().find(|w| w.id == bone.id).unwrap();
            let start = w.head + (w.tail - w.head) * 0.6;
            let original_head = w.head;
            app.selected = Some(bone.id);
            app.studio.selection = [bone.id].into_iter().collect();
            begin(&mut app, tool, Some(bone.id), start, false);
            app.drag(start + Vec2::new(-7., -11.), false);
            close(
                app.project
                    .rig
                    .world(None)
                    .iter()
                    .find(|w| w.id == bone.id)
                    .unwrap()
                    .head,
                original_head,
            );
            let after = app.project.rig.bone(bone.id).unwrap();
            assert_eq!(after.parent, bone.parent);
            assert_eq!(after.connected, bone.connected);
            assert!((after.x - bone.x).abs() < 0.001 && (after.y - bone.y).abs() < 0.001);
            assert!((after.length - bone.length).abs() < 0.001);
            if let Some(twin) = bone.twin {
                close(
                    app.project
                        .rig
                        .world(None)
                        .iter()
                        .find(|w| w.id == twin)
                        .unwrap()
                        .head,
                    world.iter().find(|w| w.id == twin).unwrap().head,
                );
            }
            app.project.validate().unwrap();
        }
    }
}

#[test]
fn rest_connected_body_drags_rotate_without_shortening_and_tail_drags_still_resize() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.mode = Mode::Rest;
    app.project.rig.mirror.on = false;
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "forearm_L")
        .unwrap()
        .id;
    app.selected = Some(id);
    app.studio.selection = [id].into_iter().collect();
    let w = app
        .project
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == id)
        .unwrap();
    let length = app.project.rig.bone(id).unwrap().length;
    let start = w.head + (w.tail - w.head) * 0.55;
    begin(&mut app, Tool::Select, Some(id), start, false);
    app.drag(start + Vec2::new(4., -3.), false);
    close(
        app.project
            .rig
            .world(None)
            .iter()
            .find(|w| w.id == id)
            .unwrap()
            .head,
        w.head,
    );
    assert!((app.project.rig.bone(id).unwrap().length - length).abs() < 0.001);
    let tail = app
        .project
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == id)
        .unwrap()
        .tail;
    begin(&mut app, Tool::Select, Some(id), tail, true);
    app.drag(tail + Vec2::new(3., 4.), false);
    close(
        app.project
            .rig
            .world(None)
            .iter()
            .find(|w| w.id == id)
            .unwrap()
            .head,
        w.head,
    );
    assert!((app.project.rig.bone(id).unwrap().length - length).abs() > 0.1);
    app.project.validate().unwrap();
}

#[test]
fn multi_selection_rest_drag_rotates_each_subtree_about_its_anchored_head() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.mode = Mode::Rest;
    let ids: Vec<_> = app
        .project
        .rig
        .bones
        .iter()
        .filter(|b| b.name == "upper_arm_L" || b.name == "upper_arm_R")
        .map(|b| b.id)
        .collect();
    app.studio.selection = ids.iter().copied().collect();
    app.selected = Some(ids[0]);
    let before = app.project.rig.world(None);
    let w = before.iter().find(|w| w.id == ids[0]).unwrap();
    let start = w.head + (w.tail - w.head) * 0.6;
    begin(&mut app, Tool::Select, Some(ids[0]), start, false);
    app.drag(start + Vec2::new(4., -2.), false);
    let after = app.project.rig.world(None);
    for id in ids {
        close(
            after.iter().find(|w| w.id == id).unwrap().head,
            before.iter().find(|w| w.id == id).unwrap().head,
        );
    }
    app.project.validate().unwrap();
}

#[test]
fn rest_joint_pivot_press_does_not_relocate_the_joint_or_create_undo() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.mode = Mode::Rest;
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "upper_arm_L")
        .unwrap()
        .id;
    let head = app
        .project
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == id)
        .unwrap()
        .head;
    let before = app.project.clone();
    begin(&mut app, Tool::Select, Some(id), head, false);
    app.drag(head + Vec2::new(8., 4.), false);
    assert_eq!(app.project, before);
    assert!(!app.history.undo(&mut app.project));
}

#[test]
fn real_pointer_rest_drag_rotates_upper_arm_while_shoulder_stays_attached() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.mode = Mode::Rest;
    app.tool = Tool::Select;
    let pass = |app: &mut ReboneApp, events| {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1280., 720.))),
                events,
                ..Default::default()
            },
            |ctx| {
                app.keyboard(ctx);
                app.show(ctx);
            },
        );
    };
    pass(&mut app, vec![]);
    pass(&mut app, vec![]);
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "upper_arm_L")
        .unwrap()
        .id;
    let w = app
        .project
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == id)
        .unwrap();
    let start_world = w.head + (w.tail - w.head) * 0.6;
    let origin = app.last_canvas.center() + app.pan
        - EVec2::new(app.project.rig.width as f32, app.project.rig.height as f32) * app.zoom * 0.5;
    let start = origin + EVec2::new(start_world.x, start_world.y) * app.zoom;
    let end = start + EVec2::new(-28., -32.);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    pass(
        &mut app,
        vec![egui::Event::PointerMoved(start), button(start, true)],
    );
    pass(&mut app, vec![egui::Event::PointerMoved(end)]);
    assert_eq!(app.gesture.as_ref().unwrap().id, Some(id));
    pass(&mut app, vec![button(end, false)]);
    close(
        app.project
            .rig
            .world(None)
            .iter()
            .find(|w| w.id == id)
            .unwrap()
            .head,
        w.head,
    );
}
