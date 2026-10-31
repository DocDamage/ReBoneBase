use rebonebase::{
    model::Rig,
    project::{History, Project, gif, load, save, sheet},
};

#[test]
fn history_undo_redo_and_branching() {
    let mut h = History::default();
    let mut p = Project::new(Rig::new("first", 64, 64));
    h.record(&p);
    p.rig.name = "second".into();
    assert!(h.undo(&mut p));
    assert_eq!(p.rig.name, "first");
    assert!(h.redo(&mut p));
    assert_eq!(p.rig.name, "second");
    h.undo(&mut p);
    h.record(&p);
    p.rig.name = "branch".into();
    assert!(!h.redo(&mut p));
}

#[test]
fn project_roundtrip_and_failed_open_preserves_existing_file() {
    let path = std::env::temp_dir().join(format!("rebonebase-test-{}.rebone", std::process::id()));
    let p = Project::new(Rig::new("roundtrip", 64, 64));
    save(&path, &p).unwrap();
    assert_eq!(load(&path).unwrap().rig, p.rig);
    let mut invalid = p.clone();
    invalid.rig.width = 0;
    assert!(save(&path, &invalid).is_err());
    assert_eq!(load(&path).unwrap().rig.name, "roundtrip");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn animation_exports_have_expected_dimensions_and_gif_frames() {
    let mut p = Project::new(Rig::new("export", 16, 16));
    p.rig.clips[0].length = 3;
    let img = sheet(&p.rig, 2).unwrap();
    assert_eq!(img.dimensions(), (32, 32));
    let bytes = gif(&p.rig).unwrap();
    assert_eq!(&bytes[..6], b"GIF89a");
    use image::AnimationDecoder;
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(decoder.into_frames().count(), 3);
}

#[test]
fn oversized_export_is_rejected_before_allocating() {
    let mut rig = Rig::new("huge", 2048, 2048);
    rig.clips[0].length = 1000;
    assert!(sheet(&rig, 32).is_err());
    assert!(gif(&rig).is_err());
}

#[test]
fn reference_buffers_share_history_storage_and_save_compactly() {
    use std::sync::Arc;
    let mut p = Project::new(Rig::new("ref", 16, 16));
    p.reference = Some(rebonebase::project::Reference {
        width: 16,
        height: 16,
        pixels: Arc::new(vec![255; 16 * 16 * 4]),
        x: 0.,
        y: 0.,
        scale: 1.,
        opacity: 0.5,
    });
    let copy = p.clone();
    assert!(Arc::ptr_eq(
        &p.reference.as_ref().unwrap().pixels,
        &copy.reference.as_ref().unwrap().pixels
    ));
    let json = serde_json::to_vec(&p).unwrap();
    let document: serde_json::Value = serde_json::from_slice(&json).unwrap();
    let pixels = document["reference"]["pixels"].as_str().unwrap();
    // Check the compact image representation independently of project headers.
    assert_eq!(pixels.len(), (16usize * 16 * 4).div_ceil(3) * 4);
    let decoded: Project = serde_json::from_slice(&json).unwrap();
    assert_eq!(decoded, p);
    decoded.validate().unwrap();
}

#[test]
fn pose_library_roundtrips_and_invalid_application_is_atomic() {
    use rebonebase::{animation::Channel, model::Vec2};
    let mut p = Project::new(Rig::new("pose", 32, 32));
    let id = p
        .rig
        .add_bone(Vec2::new(8., 8.), Vec2::new(8., 20.), None)
        .unwrap();
    p.rig
        .active_clip_mut()
        .set_key(id, Channel::Rotation, 1., 45.)
        .unwrap();
    p.capture_pose("Raised".into(), 1.).unwrap();
    let before = p.clone();
    assert!(p.apply_pose(0, 0., false).is_err());
    assert_eq!(p, before);
    p.apply_pose(0, 2., false).unwrap();
    assert_eq!(p.rig.active_clip().sample(2.)[&id].rotation, 45.);
    p.validate().unwrap();
    assert_eq!(
        serde_json::from_str::<Project>(&serde_json::to_string(&p).unwrap()).unwrap(),
        p
    );
}

#[test]
fn mirrored_pose_keeps_loose_children_at_their_authored_anchors() {
    use rebonebase::animation::Channel;
    let mut p = Project::new(rebonebase::project::templates()[0].clone());
    let left = p
        .rig
        .bones
        .iter()
        .find(|b| b.name == "upper_arm_L")
        .unwrap()
        .id;
    let right = p.rig.bone(left).unwrap().twin.unwrap();
    p.rig
        .active_clip_mut()
        .set_key(left, Channel::X, 1., 1.)
        .unwrap();
    p.rig
        .active_clip_mut()
        .set_key(left, Channel::Y, 1., 2.)
        .unwrap();
    let source_pose = p.pose_at(1.);
    let source = p
        .rig
        .world(Some(&source_pose))
        .into_iter()
        .find(|w| w.id == left)
        .unwrap();
    p.capture_pose("Offset".into(), 1.).unwrap();
    p.apply_pose(0, 2., true).unwrap();
    let pose = p.pose_at(2.);
    let target = p
        .rig
        .world(Some(&pose))
        .into_iter()
        .find(|w| w.id == right)
        .unwrap();
    assert!((target.head.x - (64. - source.head.x)).abs() < 0.001);
    assert!((target.head.y - source.head.y).abs() < 0.001);
}
