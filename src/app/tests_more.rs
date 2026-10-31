use super::*;

#[test]
fn selecting_a_bone_keeps_the_canvas_position_stable() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    pass(&mut app, &ctx, vec![]);
    pass(&mut app, &ctx, vec![]);
    let before = app.last_canvas;
    app.selected = Some(app.project.rig.bones[4].id);
    pass(&mut app, &ctx, vec![]);
    pass(&mut app, &ctx, vec![]);
    assert!(
        (before.center().y - app.last_canvas.center().y).abs() < 1.,
        "Canvas moved when timeline row count changed: {before:?} -> {:?}",
        app.last_canvas
    );
}
fn pass(app: &mut ReboneApp, ctx: &egui::Context, events: Vec<egui::Event>) {
    let modifiers = events
        .iter()
        .find_map(|event| {
            if let egui::Event::Key { modifiers, .. } = event {
                Some(*modifiers)
            } else {
                None
            }
        })
        .unwrap_or_default();
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1280., 720.))),
        events,
        modifiers,
        ..Default::default()
    };
    let _ = ctx.run(input, |ctx| {
        app.keyboard(ctx);
        app.show(ctx)
    });
}
#[test]
fn native_panels_render_reference_poses_and_motion_mapping() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.selected = Some(app.project.rig.bones[4].id);
    app.grid = true;
    app.project.reference = Some(project::Reference {
        width: 2,
        height: 2,
        pixels: std::sync::Arc::new(vec![255; 16]),
        x: 0.,
        y: 0.,
        scale: 1.,
        opacity: 0.5,
    });
    app.project.capture_pose("Test pose".into(), 1.).unwrap();
    pass(&mut app, &ctx, vec![]);
    pass(&mut app, &ctx, vec![]);
    app.mode = Mode::Pose;
    app.onion = true;
    app.changed();
    pass(&mut app, &ctx, vec![]);
    assert!(app.skin_tex.is_some());
    assert!(app.onion_tex.is_some());
    assert!(app.ref_tex.is_some());
    let motion=Motion::from_bvh("HIERARCHY ROOT hips { OFFSET 0 0 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation JOINT chest { OFFSET 0 10 0 CHANNELS 3 Zrotation Xrotation Yrotation End Site { OFFSET 0 10 0 } } } MOTION Frames: 1 Frame Time: 0.1 0 0 0 0 0 0 0 0 0").unwrap();
    app.motion_setup = Some(MotionSetup::new(motion, &app.project.rig));
    pass(&mut app, &ctx, vec![]);
    pass(&mut app, &ctx, vec![]);
    assert!(app.motion_setup.is_some());
    app.motion_setup = None;
    app.import_notes = Some(vec!["An unsupported source field was skipped".into()]);
    app.help = true;
    pass(&mut app, &ctx, vec![]);
    assert!(app.import_notes.is_some());
    app.pending = Some(Pending::New);
    pass(&mut app, &ctx, vec![]);
    assert!(app.pending.is_some());
}
#[test]
fn shortcuts_edit_scrub_key_and_restore_a_document() {
    fn key(app: &mut ReboneApp, ctx: &egui::Context, k: egui::Key, mods: egui::Modifiers) {
        for pressed in [true, false] {
            pass(
                app,
                ctx,
                vec![egui::Event::Key {
                    key: k,
                    physical_key: Some(k),
                    pressed,
                    repeat: false,
                    modifiers: mods,
                }],
            );
        }
    }
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    pass(&mut app, &ctx, vec![]);
    for k in [
        egui::Key::B,
        egui::Key::R,
        egui::Key::V,
        egui::Key::F,
        egui::Key::Num0,
        egui::Key::I,
        egui::Key::ArrowRight,
        egui::Key::ArrowLeft,
        egui::Key::Space,
        egui::Key::Space,
    ] {
        key(&mut app, &ctx, k, egui::Modifiers::NONE);
    }
    app.selected = Some(app.project.rig.bones[4].id);
    key(&mut app, &ctx, egui::Key::K, egui::Modifiers::NONE);
    assert!(!app.project.rig.active_clip().tracks.is_empty());
    let command = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    key(&mut app, &ctx, egui::Key::Z, command);
    assert!(app.project.rig.active_clip().tracks.is_empty());
    key(&mut app, &ctx, egui::Key::Y, command);
    assert!(!app.project.rig.active_clip().tracks.is_empty());
    let count = app.project.rig.bones.len();
    key(&mut app, &ctx, egui::Key::Delete, egui::Modifiers::NONE);
    assert!(app.project.rig.bones.len() < count);
    key(&mut app, &ctx, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!app.playing);
    key(&mut app, &ctx, egui::Key::N, command);
    assert!(app.pending.is_some());
}
#[test]
fn radius_bulge_and_fk_pose_gestures_remain_undoable() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "upper_arm_L")
        .unwrap()
        .id;
    let original = app.project.clone();
    let w = app
        .project
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == id)
        .unwrap();
    for tool in [Tool::Radius, Tool::Bulge, Tool::Select] {
        app.gesture = Some(Gesture {
            base: app.project.clone(),
            start: w.tail,
            id: Some(id),
            tail: true,
            tool,
            recorded: false,
        });
        app.drag(w.tail + Vec2::new(5., 3.), false);
        app.gesture = None;
        app.project.validate().unwrap();
        app.undo();
        assert_eq!(app.project, original);
    }
    app.mode = Mode::Pose;
    let start = w.head + Vec2::new((w.tail.x - w.head.x) * 0.6, (w.tail.y - w.head.y) * 0.6);
    app.gesture = Some(Gesture {
        base: app.project.clone(),
        start,
        id: Some(id),
        tail: false,
        tool: Tool::Select,
        recorded: false,
    });
    app.drag(start + Vec2::new(3., 1.), false);
    app.gesture = None;
    let pose = app.project.rig.active_clip().sample(1.);
    assert!(pose[&id].rotation.abs() > 0.1);
    assert_eq!(pose[&id].x, 0.);
    assert_eq!(pose[&id].y, 0.);
    app.project.capture_pose("Offset".into(), 1.).unwrap();
    app.selected = Some(id);
    app.delete();
    app.project.validate().unwrap();
}
#[test]
fn saved_project_reopens_and_failed_open_keeps_current_work() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let path = std::env::temp_dir().join(format!("rebonebase-ui-{}.rebone", std::process::id()));
    app.path = Some(path.clone());
    assert!(app.save(false));
    assert!(!app.dirty());
    app.project.rig.name = "Working edit".into();
    app.open(path.clone());
    assert_eq!(app.project.rig.name, "Humanoid");
    app.project.rig.name = "Keep me".into();
    app.open(path.with_extension("missing"));
    assert_eq!(app.project.rig.name, "Keep me");
    app.record();
    app.project.rig.width = 0;
    pass(&mut app, &ctx, vec![]);
    assert_eq!(app.project.rig.width, 64);
    std::fs::remove_file(path).unwrap();
}
