use rebonebase::{
    character3d,
    model::{Rig, Vec2},
    project::{self, Project},
};
#[test]
fn imported_3d_characters_render_animate_roundtrip_and_export_in_the_existing_project() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/character3d/fixtures/textured_skin.glb");
    let mut c = character3d::load(&path).unwrap();
    c.id = 1;
    c.scale = 16.;
    c.position = Vec2::new(24., 40.);
    c.yaw = 0.;
    c.pitch = 0.;
    c.animation = Some(0);
    let mut p = Project::new(Rig::new("3D scene", 64, 64));
    p.rig.active_clip_mut().length = 4;
    p.rig.active_clip_mut().fps = 4.;
    p.extras.show_2d = false;
    p.extras.characters3d.push(c);
    p.validate().unwrap();
    let first = p.render_at(1.);
    assert!(first.pixels().any(|v| v.0[3] > 0));
    assert_ne!(first, p.render_at(3.));
    let json = serde_json::to_vec(&p).unwrap();
    let round: Project = serde_json::from_slice(&json).unwrap();
    round.validate().unwrap();
    assert_eq!(p, round);
    assert_eq!(round.render_at(1.), first);
    let mut prepared = p.clone();
    rebonebase::canvas::grow_all(&mut prepared).unwrap();
    assert_eq!(
        project::sheet_project(&p, 2).unwrap().dimensions(),
        (prepared.rig.width * 2, prepared.rig.height * 2)
    );
    let ase = rebonebase::aseprite::encode_project(&p, false).unwrap();
    let decoded = asefile::AsepriteFile::read(std::io::Cursor::new(ase)).unwrap();
    assert_eq!(decoded.num_frames(), 4);
    assert_eq!(
        decoded.frame(0).image().as_raw(),
        prepared.render_at(1.).as_raw()
    );
    p.extras.characters3d[0].joints.insert(1, [0., 0., 45.]);
    assert_ne!(p.render_at(1.), first);
}
#[test]
fn invalid_3d_instances_and_scene_work_are_rejected_before_rendering() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/character3d/fixtures/textured.obj");
    let c = character3d::load(&path).unwrap();
    let mut p = Project::new(Rig::new("3D", 64, 64));
    p.extras.characters3d.push(c);
    p.validate().unwrap();
    p.extras.characters3d[0].scale = f32::NAN;
    assert!(p.validate().is_err());
    p.extras.characters3d[0].scale = 1.;
    p.extras.characters3d.push(p.extras.characters3d[0].clone());
    assert!(p.validate().is_err());
}
