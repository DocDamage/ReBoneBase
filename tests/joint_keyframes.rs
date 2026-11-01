use rebonebase::{
    character3d::{self, JointClipboard, JointKeyRef, Playback3d, tracks},
    directions::{self, Count, DirectionSet, Source},
    export::{self, ExportRequest, Target},
    jobs::Progress,
    model::Rig,
    production::{self, ClipEdit},
    project::Project,
};
const JOINT: usize = 1;
fn scene() -> Project {
    let mut p = Project::new(Rig::new("keyed joints", 64, 64));
    p.rig.active_clip_mut().length = 8;
    p.rig.active_clip_mut().fps = 8.;
    let mut c = character3d::load(std::path::Path::new(
        "src/character3d/fixtures/textured_skin.glb",
    ))
    .unwrap();
    c.clips.insert(
        0,
        Playback3d {
            animation: Some(0),
            speed: 1.,
            looped: false,
            offset_ms: 0.,
        },
    );
    p.extras.show_2d = false;
    p.extras.characters3d.push(c);
    p
}
fn keyed(keys: &[(f32, [f32; 3])]) -> Project {
    let mut p = scene();
    for (frame, rotation) in keys {
        p.extras.characters3d[0]
            .set_joint_key(0, JOINT, *frame, *rotation)
            .unwrap();
    }
    p.validate().unwrap();
    p
}
fn frame_png(p: &Project, frame: f32) -> Vec<u8> {
    let request = ExportRequest {
        target: Target::Frame { indexed: false },
        destination: "frame.png".into(),
        overwrite: false,
        frame,
        pose: None,
    };
    export::render(p, &request, &Progress::default()).unwrap()[0]
        .1
        .clone()
}
fn camera_views(p: &Project, frame: f32) -> Vec<image::RgbaImage> {
    let set = DirectionSet {
        count: Count::Four,
        start_angle: 0.,
        source: Source::Camera {
            facing: directions::FACING_VIEWER,
        },
    };
    directions::preview(p, &set, 0, frame)
        .unwrap()
        .into_iter()
        .map(|(_, image)| image)
        .collect()
}
fn degrees_between(a: [f32; 3], b: [f32; 3]) -> f32 {
    tracks::quat(a).angle_between(tracks::quat(b)).to_degrees()
}

#[test]
fn interpolation_takes_the_short_way_across_180_degrees() {
    let p = keyed(&[(1., [0., 0., 170.]), (5., [0., 0., -170.])]);
    let c = &p.extras.characters3d[0];
    let mid = c.sample_joint(0, JOINT, 3.);
    assert!(
        degrees_between(mid, [0., 0., 180.]) < 0.01,
        "halfway between 170° and -170° is 180°, got {mid:?}"
    );
    let quarter = c.sample_joint(0, JOINT, 2.);
    assert!(
        degrees_between(quarter, [0., 0., 175.]) < 0.01,
        "{quarter:?}"
    );
    // Values hold outside the keyed range.
    assert!(degrees_between(c.sample_joint(0, JOINT, 0.), [0., 0., 170.]) < 0.01);
    assert!(degrees_between(c.sample_joint(0, JOINT, 8.), [0., 0., -170.]) < 0.01);
    // Unkeyed joints and clips are uncorrected.
    assert_eq!(c.sample_joint(0, 0, 3.), [0.; 3]);
    assert_eq!(c.sample_joint(1, JOINT, 3.), [0.; 3]);
}

#[test]
fn scrubbing_between_corrections_is_smooth() {
    let p = keyed(&[(2., [0., 0., 0.]), (6., [0., 0., 60.])]);
    let c = &p.extras.characters3d[0];
    let samples: Vec<_> = (0..=40)
        .map(|i| c.sample_joint(0, JOINT, 2. + i as f32 * 0.1)[2])
        .collect();
    assert!(samples.windows(2).all(|w| w[1] >= w[0] - 1e-4));
    assert!(samples.windows(2).all(|w| (w[1] - w[0]).abs() < 2.));
    assert!((samples[20] - 30.).abs() < 0.01);
    // Rendering a fractional frame lands between the neighbouring frames.
    assert_ne!(p.render_at(3.), p.render_at(3.5));
    assert_ne!(p.render_at(3.5), p.render_at(4.));
}

#[test]
fn projects_without_keys_look_the_same_and_load_unchanged() {
    let p = scene();
    let mut json = serde_json::to_value(&p).unwrap();
    // A project saved before keyed corrections has no `tracks` field.
    json["extras"]["characters3d"][0]
        .as_object_mut()
        .unwrap()
        .remove("tracks")
        .unwrap();
    let old: Project = serde_json::from_value(json).unwrap();
    old.validate().unwrap();
    assert_eq!(old, p);
    for frame in [1., 3.5, 8.] {
        assert_eq!(old.render_at(frame), p.render_at(frame));
    }
    // An empty key list is the same as none.
    let mut cleared = keyed(&[(4., [0., 0., 40.])]);
    cleared.extras.characters3d[0].clear_joint_track(0, JOINT);
    assert_eq!(cleared, p);
}

#[test]
fn corrections_are_additive_over_imported_motion() {
    let p = keyed(&[(1., [0., 0., 35.]), (8., [0., 0., 35.])]);
    let mut offset = scene();
    offset.extras.characters3d[0]
        .joints
        .insert(JOINT, [0., 0., 35.]);
    let plain = scene();
    for frame in [1., 4., 6.5] {
        assert_eq!(p.render_at(frame), offset.render_at(frame));
        assert_ne!(p.render_at(frame), plain.render_at(frame));
    }
    // Imported motion still plays underneath the constant correction.
    assert_ne!(p.render_at(1.), p.render_at(6.));
}

#[test]
fn limbs_stay_attached_to_their_parents() {
    let p = keyed(&[(3., [0., 0., 70.])]);
    let plain = scene();
    let at = |p: &Project| {
        let (c, time) = p.extras.characters3d[0].for_clip(0, 3., 375.);
        c.joint_positions(time).unwrap()
    };
    let (keyed, plain) = (at(&p), at(&plain));
    // Rotating a joint moves its children around it, never the joint itself.
    assert!((keyed[JOINT] - plain[JOINT]).length() < 1e-3);
    let parent = p.extras.characters3d[0].model.nodes[JOINT].parent;
    if let Some(parent) = parent {
        assert!((keyed[parent] - plain[parent]).length() < 1e-3);
    }
}

#[test]
fn canvas_frame_export_and_direction_export_agree() {
    let p = keyed(&[(2., [0., 0., -20.]), (6., [10., 0., 45.])]);
    let mut offset = scene();
    // At frame 6 the keyed correction equals a whole-clip offset.
    offset.extras.characters3d[0]
        .joints
        .insert(JOINT, [10., 0., 45.]);
    assert_eq!(p.render_at(6.), offset.render_at(6.));
    assert_eq!(frame_png(&p, 6.), frame_png(&offset, 6.));
    assert_ne!(frame_png(&p, 6.), frame_png(&scene(), 6.));
    assert_eq!(camera_views(&p, 6.), camera_views(&offset, 6.));
    assert_ne!(camera_views(&p, 6.), camera_views(&scene(), 6.));
    // The full direction package renders the same keyed frames.
    let export = directions::DirectionExport {
        set: DirectionSet {
            count: Count::Four,
            start_angle: 0.,
            source: Source::Camera {
                facing: directions::FACING_VIEWER,
            },
        },
        clips: vec![0],
        engine: production::engine_export::EngineTarget::Generic,
        atlas: Default::default(),
    };
    let files = |p: &Project| directions::plan(p, &export, &Progress::default()).unwrap();
    assert_ne!(files(&p), files(&scene()));
}

#[test]
fn save_and_load_preserve_keys() {
    let mut p = keyed(&[(2., [5., -10., 20.]), (7., [0., 30., -90.])]);
    p.extras.characters3d[0].set_joint_easing(
        0,
        &[JointKeyRef {
            joint: JOINT,
            frame: 2.,
        }],
        rebonebase::animation::Ease::EaseIn,
    );
    let q: Project = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    q.validate().unwrap();
    assert_eq!(q, p);
    assert_eq!(q.render_at(4.5), p.render_at(4.5));
}

#[test]
fn clip_edits_keep_corrections_on_the_same_frames() {
    let p = keyed(&[
        (2., [0., 0., 50.]),
        (5., [0., 0., -40.]),
        (8., [0., 0., 10.]),
    ]);
    let expected = p.render_at(6.);
    let mut trim = p.clone();
    production::edit_clip(
        &mut trim,
        ClipEdit::Trim {
            index: 0,
            start: 4,
            end: 8,
        },
    )
    .unwrap();
    assert_eq!(expected, trim.render_at(3.));
    let mut split = p.clone();
    production::edit_clip(&mut split, ClipEdit::Split { index: 0, at: 4 }).unwrap();
    split.rig.active_clip = 1;
    assert_eq!(expected, split.render_at(3.));
    production::edit_clip(&mut split, ClipEdit::Move { from: 1, to: 0 }).unwrap();
    split.rig.active_clip = 0;
    assert_eq!(expected, split.render_at(3.));
    let mut duplicate = p.clone();
    production::edit_clip(&mut duplicate, ClipEdit::Duplicate(0)).unwrap();
    duplicate.rig.active_clip = 1;
    assert_eq!(expected, duplicate.render_at(6.));
    // Each copy is independent.
    duplicate.extras.characters3d[0].clear_joint_track(1, JOINT);
    assert_ne!(expected, duplicate.render_at(6.));
    duplicate.rig.active_clip = 0;
    assert_eq!(expected, duplicate.render_at(6.));
    production::edit_clip(&mut duplicate, ClipEdit::Delete(0)).unwrap();
    assert!(duplicate.extras.characters3d[0].tracks.is_empty());
}

#[test]
fn keys_move_copy_paste_and_reject_collisions() {
    let mut p = keyed(&[(2., [0., 0., 20.]), (4., [0., 0., 40.])]);
    let c = &mut p.extras.characters3d[0];
    let first = JointKeyRef {
        joint: JOINT,
        frame: 2.,
    };
    let before = c.clone();
    assert!(c.move_joint_keys(0, &[first], 2., 8).is_err());
    assert!(c.move_joint_keys(0, &[first], -2., 8).is_err());
    assert_eq!(*c, before, "a rejected move changes nothing");
    c.move_joint_keys(0, &[first], 4., 8).unwrap();
    let frames: Vec<_> = c.joint_keys(0, JOINT).iter().map(|k| k.frame).collect();
    assert_eq!(frames, [4., 6.]);
    let copied: JointClipboard = c.copy_joint_keys(
        0,
        &[
            JointKeyRef {
                joint: JOINT,
                frame: 4.,
            },
            JointKeyRef {
                joint: JOINT,
                frame: 6.,
            },
        ],
    );
    let pasted = c.paste_joint_keys(3, &copied, 1.).unwrap();
    assert_eq!(pasted.len(), 2);
    assert_eq!(c.joint_keys(3, JOINT)[1].frame, 3.);
    assert_eq!(c.joint_keys(3, JOINT)[1].rotation, [0., 0., 20.]);
    c.delete_joint_keys(3, &pasted);
    assert!(!c.tracks.contains_key(&3));
    assert!(c.set_joint_key(0, 9999, 1., [0.; 3]).is_err());
    assert!(c.set_joint_key(0, JOINT, f32::NAN, [0.; 3]).is_err());
}

#[test]
fn invalid_keys_are_rejected_before_rendering() {
    let mut p = keyed(&[(2., [0., 0., 20.])]);
    p.extras.characters3d[0]
        .tracks
        .get_mut(&0)
        .unwrap()
        .get_mut(&JOINT)
        .unwrap()[0]
        .rotation[0] = f32::NAN;
    assert!(p.validate().is_err());
    let mut p = keyed(&[(2., [0., 0., 20.])]);
    let keys = p.extras.characters3d[0]
        .tracks
        .get_mut(&0)
        .unwrap()
        .remove(&JOINT)
        .unwrap();
    p.extras.characters3d[0]
        .tracks
        .get_mut(&0)
        .unwrap()
        .insert(10_000, keys);
    assert!(p.validate().is_err());
}
