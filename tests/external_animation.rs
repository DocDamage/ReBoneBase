use rebonebase::{
    character3d,
    model::Rig,
    motion::{Motion, Projection},
    project::Project,
};
#[test]
fn external_motion_import_is_atomic_keeps_mesh_and_creates_independent_clips() {
    let mut p = Project::new(Rig::new("animation", 64, 64));
    let c = character3d::load(std::path::Path::new(
        "src/character3d/fixtures/textured_skin.glb",
    ))
    .unwrap();
    p.extras.characters3d.push(c);
    let source = Motion::from_gltf(
        include_bytes!("../src/character3d/fixtures/textured_skin.glb"),
        None,
    )
    .unwrap();
    let mapping = source.mapping_3d(&p.extras.characters3d[0].model);
    let before = p.clone();
    assert!(
        rebonebase::animation_import::import_character_motions(
            &mut p,
            99,
            std::slice::from_ref(&source),
            &mapping,
            false,
            false,
            Projection::Front
        )
        .is_err()
    );
    assert_eq!(p, before);
    let indexes = rebonebase::animation_import::import_character_motions(
        &mut p,
        1,
        &[source.clone(), source],
        &mapping,
        false,
        false,
        Projection::Front,
    )
    .unwrap();
    assert_eq!(indexes, vec![1, 2]);
    assert_eq!(
        p.extras.characters3d[0].model.primitives,
        before.extras.characters3d[0].model.primitives
    );
    assert_eq!(p.extras.characters3d[0].model.animations.len(), 3);
    assert_ne!(
        p.extras.characters3d[0].clips[&1].animation,
        p.extras.characters3d[0].clips[&2].animation
    );
    p.validate().unwrap();
    let encoded = serde_json::to_vec(&p).unwrap();
    let restored: Project = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(restored, p);
}
#[test]
fn one_import_drives_3d_and_attached_2d_skeleton_and_rolls_back_on_unmapped_2d() {
    use rebonebase::model::Vec2;
    let mut p = Project::new(Rig::new("both", 64, 64));
    let c = character3d::load(std::path::Path::new(
        "src/character3d/fixtures/textured_skin.glb",
    ))
    .unwrap();
    let source = Motion::from_gltf(
        include_bytes!("../src/character3d/fixtures/textured_skin.glb"),
        None,
    )
    .unwrap();
    let mapping = source.mapping_3d(&c.model);
    p.extras.characters3d.push(c);
    let before = p.clone();
    assert!(
        rebonebase::animation_import::import_character_motions(
            &mut p,
            1,
            std::slice::from_ref(&source),
            &mapping,
            false,
            true,
            Projection::Front
        )
        .is_err()
    );
    assert_eq!(p, before);
    let names = source.joint_names();
    let root = p
        .rig
        .add_bone(Vec2::new(32., 40.), Vec2::new(32., 30.), None)
        .unwrap();
    p.rig.bone_mut(root).unwrap().name = names[0].clone();
    let child = p
        .rig
        .add_bone(Vec2::new(32., 30.), Vec2::new(32., 20.), Some(root))
        .unwrap();
    p.rig.bone_mut(child).unwrap().name = names[1].clone();
    let bones = p.rig.bones.clone();
    p.extras.show_2d = false;
    rebonebase::animation_import::import_character_motions(
        &mut p,
        1,
        &[source],
        &mapping,
        false,
        true,
        Projection::Front,
    )
    .unwrap();
    assert!(
        p.extras.show_2d,
        "Importing both targets should show the 2D rig"
    );
    assert_eq!(p.rig.bones, bones);
    assert!(!p.rig.active_clip().tracks.is_empty());
    assert!(
        p.extras.characters3d[0]
            .clips
            .contains_key(&p.rig.active_clip)
    );
    for f in 1..=p.rig.active_clip().length {
        let pose = p.pose_at(f as f32);
        assert!(pose.values().all(|b| b.x == 0. && b.y == 0.));
    }
}
#[test]
fn imported_open_motion_keeps_its_final_sample_on_the_timeline() {
    let mut p = Project::new(Rig::new("endpoint", 64, 64));
    p.extras.show_2d = false;
    let c = character3d::load(std::path::Path::new(
        "src/character3d/fixtures/textured_skin.glb",
    ))
    .unwrap();
    let motion = Motion::from_gltf(
        include_bytes!("../src/character3d/fixtures/textured_skin.glb"),
        None,
    )
    .unwrap();
    let mapping = motion.mapping_3d(&c.model);
    p.extras.characters3d.push(c);
    rebonebase::animation_import::import_character_motions(
        &mut p,
        1,
        &[motion],
        &mapping,
        false,
        false,
        Projection::Front,
    )
    .unwrap();
    let binding = &p.extras.characters3d[0].clips[&p.rig.active_clip];
    assert!(
        !binding.looped,
        "Timeline looping should control repeated playback"
    );
    let (c, _) = p.extras.characters3d[0].for_clip(p.rig.active_clip, 0.);
    let duration = c.model.animations[binding.animation.unwrap()].duration as f64 * 1000.;
    let expected = c
        .render(p.rig.width, p.rig.height, p.extras.canvas_origin, duration)
        .unwrap();
    assert!(
        expected == p.render_at(p.rig.active_clip().length as f32),
        "Final rendered pose must match the imported source endpoint"
    );
}
