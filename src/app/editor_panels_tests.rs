use super::*;
fn render(app: &mut ReboneApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(280., 1000.))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.inspector_ui(ui));
        },
    )
}
fn click(app: &mut ReboneApp, ctx: &egui::Context, label: &str) {
    let output = render(app, ctx, vec![]);
    let position = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                let point = text.pos + text.galley.size() * 0.5;
                shape.clip_rect.contains(point).then_some(point)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("Control not visible: {label}"));
    for pressed in [true, false] {
        render(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}
#[test]
fn hierarchy_search_retains_ancestors_and_ignores_stored_order_and_collapsed_state() {
    let mut rig = project::templates().remove(0);
    let id = rig
        .bones
        .iter()
        .find(|bone| bone.name == "forearm_L")
        .unwrap()
        .id;
    let mut path = vec![id];
    while let Some(parent) = rig.bone(*path.last().unwrap()).unwrap().parent {
        path.push(parent);
    }
    path.reverse();
    let collapsed = BTreeSet::from([path[0]]);
    rig.bones.reverse();
    let visible = hierarchy_rows(&rig, "  FOREARM_l  ", &collapsed);
    assert_eq!(visible.iter().map(|row| row.id).collect::<Vec<_>>(), path);
    assert!(visible.last().unwrap().matches);
    assert!(
        visible
            .iter()
            .take(visible.len() - 1)
            .all(|row| !row.matches)
    );
    assert!(
        !hierarchy_rows(&rig, "", &collapsed)
            .iter()
            .any(|row| row.id == id)
    );
    assert!(hierarchy_rows(&rig, "no such bone", &collapsed).is_empty());
    assert!(collapsed.contains(&path[0]));
}
#[test]
fn revealing_a_filtered_bone_restores_context_without_editing_the_document() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|bone| bone.name == "forearm_L")
        .unwrap()
        .id;
    app.select_bone(id, false);
    app.panels.search = "unrelated".into();
    app.panels
        .collapsed
        .extend(app.project.rig.bones.iter().map(|bone| bone.id));
    let before = app.project.clone();
    app.reveal_selected_bone(true);
    assert!(app.panels.search.is_empty());
    assert!(
        hierarchy_rows(&app.project.rig, "", &app.panels.collapsed)
            .iter()
            .any(|row| row.id == id)
    );
    assert_eq!(app.project, before);
    assert_eq!(app.selected, Some(id));
    assert!(!app.history.can_undo());
}
#[test]
fn inspector_navigation_and_tool_settings_do_not_create_document_edits() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let id = app.project.rig.bones[0].id;
    app.select_bone(id, false);
    app.mode = Mode::Pose;
    let bone = app.project.rig.bone_mut(id).unwrap();
    bone.rotation_min = Some(-10.);
    bone.rotation_max = Some(10.);
    app.project
        .rig
        .active_clip_mut()
        .set_key(id, Channel::Rotation, 1., 30.)
        .unwrap();
    app.project.validate().unwrap();
    let before = app.project.clone();
    for tab in [
        InspectorTab::Selection,
        InspectorTab::Appearance,
        InspectorTab::Project,
    ] {
        app.panels.inspector = tab;
        render(&mut app, &ctx, vec![]);
    }
    app.panels.inspector = InspectorTab::Selection;
    for tool in [Tool::Pixel, Tool::Box, Tool::Select] {
        app.tool = tool;
        render(&mut app, &ctx, vec![]);
    }
    assert_eq!(app.project, before);
    assert!(!app.history.can_undo());
}
#[test]
fn inspector_lock_and_pose_reset_are_undoable_and_locked_bones_are_protected() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let id = app.project.rig.bones[0].id;
    app.select_bone(id, false);
    let original = app.project.clone();
    click(&mut app, &ctx, "Lock bone");
    assert!(app.project.rig.bone(id).unwrap().locked);
    app.key_selected();
    assert!(app.project.rig.active_clip().tracks.is_empty());
    app.undo();
    assert_eq!(app.project, original);
    assert!(!app.history.can_undo());
    app.mode = Mode::Pose;
    app.project
        .rig
        .active_clip_mut()
        .set_key(id, Channel::Rotation, 1., 35.)
        .unwrap();
    let keyed = app.project.clone();
    click(&mut app, &ctx, "Reset pose here");
    assert_eq!(
        app.project
            .rig
            .active_clip()
            .sample(1.)
            .get(&id)
            .unwrap()
            .rotation,
        0.
    );
    app.project.validate().unwrap();
    app.undo();
    assert_eq!(app.project, keyed);
    assert!(!app.history.can_undo());
}
#[test]
fn released_hidden_timeline_gestures_are_cleared_and_undo_stops_transport() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.studio.key_drag = Some(studio::KeyDrag {
        base: app.project.clone(),
        selected: vec![],
        start: 1.,
        offset: 0.,
        recorded: false,
    });
    app.studio.preferences.workspace.timeline.dock = rebonebase::workspace::Dock::Hidden;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1280., 800.))),
            ..Default::default()
        },
        |ctx| app.show(ctx),
    );
    assert!(app.studio.key_drag.is_none());
    let original = app.project.clone();
    app.record();
    app.project.rig.name = "Edited".into();
    app.playing = true;
    app.mode = Mode::Rest;
    app.undo();
    assert_eq!(app.project, original);
    assert!(!app.playing);
    assert!(app.mode == Mode::Rest);
}
