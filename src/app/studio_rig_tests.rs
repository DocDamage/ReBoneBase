use super::*;

fn app() -> (ReboneApp, egui::Context) {
    let ctx = egui::Context::default();
    (ReboneApp::with_context(&ctx), ctx)
}
fn select(app: &mut ReboneApp, name: &str) -> u32 {
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == name)
        .unwrap()
        .id;
    app.selected = Some(id);
    app.studio.selection = [id].into_iter().collect();
    id
}
fn click(app: &mut ReboneApp, ctx: &egui::Context, label: &str) {
    super::super::tests_studio::click(app, ctx, studio::Tab::Rig, label);
}

#[test]
fn reparenting_an_attached_lower_bone_is_blocked_without_losing_its_pin() {
    let (mut app, ctx) = app();
    let lower = select(&mut app, "shin_L");
    app.add_pin(lower, false);
    app.mode = Mode::Rest;
    app.studio.parent = None;
    let original = app
        .project
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == lower)
        .unwrap();
    app.studio.parent = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "upper_arm_L")
        .map(|b| b.id);
    click(&mut app, &ctx, "Reparent");
    assert!(app.project.rig.bone(lower).unwrap().connected);
    assert_eq!(app.project.extras.pins.len(), 1);
    app.project.validate().unwrap();
    let after = app
        .project
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == lower)
        .unwrap();
    assert!((after.head - original.head).length() < 0.001);
    assert!((after.tail - original.tail).length() < 0.001);
}

#[test]
fn moving_pinned_rest_joints_is_blocked_and_preserves_the_pin() {
    let (mut app, ctx) = app();
    let lower = select(&mut app, "shin_L");
    app.add_pin(lower, false);
    app.mode = Mode::Rest;
    app.studio.group_delta = Vec2::new(2., 0.);
    click(&mut app, &ctx, "Move rest joints");
    assert!(app.project.rig.bone(lower).unwrap().connected);
    assert_eq!(app.project.extras.pins.len(), 1);
    app.project.validate().unwrap();
}

#[test]
fn lock_selection_constraints_and_preferred_bend_follow_actual_widget_clicks() {
    let (mut app, ctx) = app();
    click(&mut app, &ctx, "Select all unlocked");
    assert_eq!(app.studio.selection.len(), app.project.rig.bones.len());
    click(&mut app, &ctx, "Lock selected");
    assert!(app.project.rig.bones.iter().all(|b| b.locked));
    click(&mut app, &ctx, "Unlock all");
    assert!(app.project.rig.bones.iter().all(|b| !b.locked));
    click(&mut app, &ctx, "Clear selection");
    assert!(app.selected.is_none());
    let id = select(&mut app, "upper_arm_L");
    click(&mut app, &ctx, "Lock length");
    click(&mut app, &ctx, "Limit pose rotation");
    click(&mut app, &ctx, "Negative");
    let bone = app.project.rig.bone(id).unwrap();
    assert!(bone.length_locked);
    assert_eq!(
        (bone.rotation_min, bone.rotation_max),
        (Some(-90.), Some(90.))
    );
    assert_eq!(bone.bend, -1.);
    click(&mut app, &ctx, "Locked for editing");
    assert!(app.project.rig.bone(id).unwrap().locked);
    let before = app.project.clone();
    app.studio.group_angle = 20.;
    click(&mut app, &ctx, "Rotate at selected joints");
    assert_eq!(app.project, before);
    click(&mut app, &ctx, "Unlock all");
    click(&mut app, &ctx, "Limit pose rotation");
    assert_eq!(app.project.rig.bone(id).unwrap().rotation_min, None);
    app.project.validate().unwrap();
}

#[test]
fn pins_ground_contact_bend_flip_and_removal_are_editable_from_rig_tools() {
    let (mut app, ctx) = app();
    let foot = select(&mut app, "foot_L");
    let lower = app.project.rig.bone(foot).unwrap().parent.unwrap();
    click(&mut app, &ctx, "Pin selected limb end");
    assert_eq!(app.project.extras.pins[0].lower, lower);
    click(&mut app, &ctx, "Pin selected limb end");
    assert_eq!(app.project.extras.pins.len(), 1);
    click(&mut app, &ctx, "Show ground and constrain grounded pins");
    assert!(app.project.extras.ground.is_some());
    click(&mut app, &ctx, "Grounded");
    assert!(app.project.extras.pins[0].grounded);
    click(&mut app, &ctx, "Flip bend");
    assert_eq!(app.project.extras.pins[0].bend, -1.);
    click(&mut app, &ctx, "Plant both feet");
    assert_eq!(app.project.extras.pins.len(), 2);
    assert!(app.project.extras.pins.iter().all(|p| p.grounded));
    while !app.project.extras.pins.is_empty() {
        click(&mut app, &ctx, "Remove");
    }
    click(&mut app, &ctx, "Show ground and constrain grounded pins");
    assert!(app.project.extras.ground.is_none());
    app.project.validate().unwrap();
}

#[test]
fn authored_roots_can_connect_but_existing_joints_cannot_detach_and_rotations_stay_anchored() {
    let (mut app, ctx) = app();
    let lower = select(&mut app, "shin_L");
    let original_parent = app.project.rig.bone(lower).unwrap().parent.unwrap();
    click(&mut app, &ctx, "Detach");
    assert_eq!(
        app.project.rig.bone(lower).unwrap().parent,
        Some(original_parent)
    );
    let head = app
        .project
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == lower)
        .unwrap()
        .head;
    let new = app
        .project
        .rig
        .add_bone(head, head + Vec2::new(3., 0.), None)
        .unwrap();
    app.selected = Some(new);
    app.studio.selection = [new].into_iter().collect();
    app.studio.parent = Some(original_parent);
    click(&mut app, &ctx, "Connect");
    assert!(app.project.rig.bone(new).unwrap().connected);
    assert_eq!(
        app.project.rig.bone(new).unwrap().parent,
        Some(original_parent)
    );
    select(&mut app, "shin_L");
    let angle = app.project.rig.bone(lower).unwrap().angle;
    app.studio.group_angle = 15.;
    click(&mut app, &ctx, "Rotate at selected joints");
    assert!((app.project.rig.bone(lower).unwrap().angle - angle - 15.).abs() < 0.001);
    app.mode = Mode::Pose;
    click(&mut app, &ctx, "Rotate at selected joints");
    assert!((app.project.rig.active_clip().sample(app.frame)[&lower].rotation - 15.).abs() < 0.001);
    select(&mut app, "hips");
    let before = app.project.clone();
    click(&mut app, &ctx, "Pin selected limb end");
    assert!(app.project.extras.pins.is_empty());
    click(&mut app, &ctx, "Rotate at selected joints");
    assert_eq!(app.project, before);
    app.project.validate().unwrap();
}
