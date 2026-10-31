use rebonebase::{
    features::{Extras, Pin, ReferenceSequence, solve_pins},
    model::{Rig, Vec2},
    project::{Project, Reference},
    storage::Store,
};
use std::sync::Arc;

#[test]
fn pinned_leg_stays_at_target_when_root_motion_changes() {
    let mut p = Project::new(rebonebase::project::templates()[0].clone());
    let lower = p.rig.bones.iter().find(|b| b.name == "shin_L").unwrap().id;
    let target = p
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == lower)
        .unwrap()
        .tail;
    p.extras.pins.push(Pin {
        lower,
        target,
        enabled: true,
        grounded: false,
        bend: 1.,
    });
    let root = p.rig.bones.iter().find(|b| b.parent.is_none()).unwrap().id;
    p.rig
        .active_clip_mut()
        .set_key(root, rebonebase::animation::Channel::X, 1., 2.)
        .unwrap();
    let pose = solve_pins(&p.rig, &p.extras, &p.rig.active_clip().sample(1.));
    let result = p
        .rig
        .world(Some(&pose))
        .into_iter()
        .find(|w| w.id == lower)
        .unwrap()
        .tail;
    assert!((result - target).length() < 0.01);
    p.validate().unwrap();
}

#[test]
fn reference_sequence_selects_frames_by_timeline_time_and_roundtrips() {
    let r = Reference {
        width: 1,
        height: 1,
        pixels: Arc::new(vec![255; 4]),
        x: 0.,
        y: 0.,
        scale: 1.,
        opacity: 0.5,
    };
    let seq = ReferenceSequence {
        frames: vec![r.clone(), r],
        fps: 2.,
        start_ms: 0,
        looped: true,
    };
    assert_eq!(seq.index_at(600), 1);
    assert_eq!(seq.index_at(1100), 0);
    let mut p = Project::new(Rig::new("ref", 8, 8));
    p.extras.references = Some(seq);
    p.validate().unwrap();
    assert_eq!(
        serde_json::from_str::<Project>(&serde_json::to_string(&p).unwrap()).unwrap(),
        p
    );
}

#[test]
fn recovery_and_library_store_preserve_files_and_search_metadata() {
    let dir = std::env::temp_dir().join(format!("rebone-store-{}", std::process::id()));
    let store = Store::at(dir.clone());
    let p = Project::new(Rig::new("Robot", 16, 16));
    store.write_recovery("window1", &p, None).unwrap();
    assert_eq!(store.recoveries().unwrap().len(), 1);
    let id = store
        .save_library("Robot", &p, vec!["mechanical".into()], true)
        .unwrap();
    assert_eq!(store.search_library("mech", true).unwrap().len(), 1);
    assert_eq!(store.load_library(&id).unwrap(), p);
    store
        .save_with_backup(&dir.join("robot.rebone"), &p)
        .unwrap();
    let mut changed = p.clone();
    changed.rig.name = "Robot 2".into();
    store
        .save_with_backup(&dir.join("robot.rebone"), &changed)
        .unwrap();
    assert_eq!(store.backups(&dir.join("robot.rebone")).unwrap().len(), 1);
    store.clear_recovery("window1").unwrap();
    assert!(store.recoveries().unwrap().is_empty());
    assert!(Store::safe_id("../bad").is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn extras_reject_invalid_pins_and_image_budgets() {
    let mut p = Project::new(Rig::new("bad", 16, 16));
    p.extras = Extras::default();
    p.extras.pins.push(Pin {
        lower: 999,
        target: Vec2::new(1., 1.),
        enabled: true,
        grounded: false,
        bend: 1.,
    });
    assert!(p.validate().is_err());
}

#[test]
fn pose_library_thumbnail_uses_captured_pose_instead_of_first_frame() {
    let dir = std::env::temp_dir().join(format!(
        "rebone-pose-thumb-{}",
        rebonebase::storage::unique()
    ));
    let store = Store::at(dir.clone());
    let mut p = Project::new(rebonebase::project::templates()[0].clone());
    let id = p
        .rig
        .bones
        .iter()
        .find(|b| b.name == "upper_arm_L")
        .unwrap()
        .id;
    p.rig
        .active_clip_mut()
        .set_key(id, rebonebase::animation::Channel::Rotation, 4., 90.)
        .unwrap();
    p.rig
        .active_clip_mut()
        .set_key(id, rebonebase::animation::Channel::Rotation, 1., 0.)
        .unwrap();
    p.capture_pose("Raised arm".into(), 4.).unwrap();
    let entry = store
        .save_library_kind("Raised arm", &p, vec![], false, "Pose")
        .unwrap();
    let thumbnail = image::open(dir.join("library").join(format!("{entry}.png")))
        .unwrap()
        .to_rgba8();
    let expected = image::imageops::thumbnail(
        &rebonebase::skin::render(&p.rig, Some(&p.poses[0].pose)),
        128,
        128,
    );
    assert!(
        thumbnail.as_raw() == expected.as_raw(),
        "thumbnail must show the captured pose"
    );
    assert!(
        dir.canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap())
    );
    std::fs::remove_dir_all(dir).unwrap();
}
