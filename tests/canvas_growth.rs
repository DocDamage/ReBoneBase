use rebonebase::{
    canvas,
    model::{Bone, Part, Rig, Vec2},
    production,
    project::Project,
};
fn document() -> Project {
    let mut r = Rig::new("Canvas", 32, 32);
    r.parts.push(Part {
        id: 100,
        color: [255, 0, 0, 255],
        ..Default::default()
    });
    r.bones.push(Bone {
        id: 1,
        x: 12.,
        y: 12.,
        length: 8.,
        radius_head: 2.,
        radius_tail: 2.,
        part: 100,
        ..Default::default()
    });
    Project::new(r)
}
#[test]
fn canvas_grows_in_every_direction_without_relocating_joints_or_existing_pixels() {
    let mut p = document();
    production::paint_pixel(&mut p, 0, 1, 5, 6, Some([0, 255, 0, 255])).unwrap();
    p.rig.bones[0].x = -20.;
    p.rig.bones[0].y = 45.;
    let before = p.rig.world(None);
    assert!(canvas::grow(&mut p, 1., None).unwrap());
    assert!(p.extras.canvas_origin.x < -22. && p.rig.height > 48);
    assert_eq!(p.rig.world(None), before);
    let image = p.render_pose(1., None);
    let x = (5. - p.extras.canvas_origin.x) as u32;
    let y = (6. - p.extras.canvas_origin.y) as u32;
    assert_eq!(image.get_pixel(x, y).0, [0, 255, 0, 255]);
    assert!(image.pixels().any(|v| v.0 == [255, 0, 0, 255]));
    assert!(!canvas::grow(&mut p, 1., None).unwrap());
    let round: Project = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    assert_eq!(round, p);
    p.validate().unwrap();
}
#[test]
fn animation_envelope_grows_once_for_all_frames_and_stays_fixed_during_export() {
    use rebonebase::animation::Channel;
    let mut p = document();
    p.rig.active_clip_mut().length = 4;
    p.rig
        .active_clip_mut()
        .set_key(1, Channel::X, 4., 65.)
        .unwrap();
    assert!(canvas::grow_all(&mut p).unwrap());
    assert!(p.rig.width > 85);
    assert_eq!(p.render_at(1.).dimensions(), p.render_at(4.).dimensions());
    assert!(!canvas::grow_all(&mut p).unwrap());
    assert!(p.render_at(4.).pixels().any(|v| v.0[3] > 0));
}
#[test]
fn unbounded_growth_fails_atomically_and_old_documents_default_to_automatic_canvas() {
    let mut p = document();
    p.rig.bones[0].x = 10000.;
    let before = p.clone();
    assert!(canvas::grow(&mut p, 1., None).is_err());
    assert_eq!(p, before);
    let old = serde_json::json!({"version":1,"rig":document().rig});
    let p: Project = serde_json::from_value(old).unwrap();
    assert!(p.extras.auto_canvas);
    assert_eq!(p.extras.canvas_origin, Vec2::default());
}
#[test]
fn exported_sheets_grow_to_include_the_full_animation_without_mutating_the_document() {
    let mut p = document();
    p.rig.bones[0].x = -20.;
    let before = p.clone();
    let sheet = rebonebase::project::sheet_project(&p, 1).unwrap();
    assert!(sheet.width() > 32);
    assert!(sheet.pixels().any(|v| v.0[3] > 0));
    assert_eq!(p, before);
}
#[test]
fn actor_pixel_corrections_outside_its_skeleton_are_part_of_the_parent_envelope() {
    let mut p = document();
    let mut actor = document();
    production::paint_pixel(&mut actor, 0, 1, 31, 31, Some([0, 255, 0, 255])).unwrap();
    p.extras.production.actors.push(production::SceneActor {
        id: 1,
        project: Box::new(actor),
        offset: Vec2::new(40., 0.),
        ..Default::default()
    });
    canvas::grow_all(&mut p).unwrap();
    assert!(p.rig.width > 72);
    let img = p.render_at(1.);
    let origin = p.extras.canvas_origin;
    assert_eq!(
        img.get_pixel((71. - origin.x) as u32, (31. - origin.y) as u32)
            .0,
        [0, 255, 0, 255]
    );
}
#[test]
fn one_sided_growth_keeps_the_game_export_pivot_at_its_original_world_anchor() {
    let mut p = document();
    canvas::set_bounds(&mut p, Vec2::new(-20., -10.), 52, 64).unwrap();
    let pivot = p.extras.production.pivot;
    let world = p.extras.canvas_origin
        + Vec2::new(p.rig.width as f32 * pivot.x, p.rig.height as f32 * pivot.y);
    assert!(
        (world - Vec2::new(16., 32.)).length() < 0.001,
        "Export anchor moved to {world:?}"
    );
}
#[test]
fn imported_actors_grow_their_own_raster_before_the_parent_scene_composites_them() {
    let mut p = document();
    let mut actor = document();
    actor.rig.bones[0].x = -10.;
    actor.rig.parts[0].color = [0, 255, 0, 255];
    p.extras.production.actors.push(production::SceneActor {
        id: 1,
        project: Box::new(actor),
        offset: Vec2::new(40., 0.),
        ..Default::default()
    });
    canvas::grow_all(&mut p).unwrap();
    let origin = p.extras.canvas_origin;
    assert_eq!(
        p.render_at(1.)
            .get_pixel((30. - origin.x) as u32, (12. - origin.y) as u32)
            .0,
        [0, 255, 0, 255]
    );
}
