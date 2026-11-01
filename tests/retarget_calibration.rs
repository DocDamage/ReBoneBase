use glam::{EulerRot, Quat, Vec3};
use rebonebase::{
    animation_import::import_calibrated,
    character3d::{Animation3d, Character, Model3d, Node3d, Transform3d},
    model::Rig,
    motion::{
        Motion, Projection,
        calibration::{Calibration3d, ReferencePose, RootScale, RootTravel, UpAxis},
    },
    project::Project,
};
use std::sync::Arc;

/// One walk-like clip. `leg_first` swaps the sibling order, which changes the
/// joint indices but not the motion.
fn bvh(leg_first: bool) -> Motion {
    let chest = "  JOINT chest\n  {\n    OFFSET 0 10 0\n    CHANNELS 3 Zrotation Xrotation Yrotation\n    End Site\n    {\n      OFFSET 0 5 0\n    }\n  }\n";
    let leg = "  JOINT leg\n  {\n    OFFSET 2 -1 0\n    CHANNELS 3 Zrotation Xrotation Yrotation\n    End Site\n    {\n      OFFSET 0 -8 0\n    }\n  }\n";
    let (first, second) = if leg_first {
        (leg, chest)
    } else {
        (chest, leg)
    };
    let mut text = format!(
        "HIERARCHY\nROOT hips\n{{\n  OFFSET 0 0 0\n  CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation\n{first}{second}}}\nMOTION\nFrames: 9\nFrame Time: 0.0416667\n"
    );
    for f in 0..9 {
        let t = f as f32 / 8.;
        let hips = [6. * t, 0.5 * (t * 6.).sin(), 0., 15. * t, 0., 0.];
        let chest = [0., 30. * t, 0.];
        let leg = [-40. * t, 10. * t, 0.];
        let (a, b) = if leg_first {
            (leg, chest)
        } else {
            (chest, leg)
        };
        let values: Vec<String> = hips
            .iter()
            .chain(&a)
            .chain(&b)
            .map(|v| format!("{v:.5}"))
            .collect();
        text += &values.join(" ");
        text += "\n";
    }
    Motion::from_bvh(&text).unwrap()
}
/// Differently named and proportioned target.
fn target() -> Model3d {
    let node = |name: &str, parent, translation, rotation: Quat| Node3d {
        name: name.into(),
        parent,
        transform: Transform3d {
            translation,
            rotation: rotation.to_array(),
            ..Default::default()
        },
    };
    Model3d {
        nodes: vec![
            node("Pelvis", None, [0., 1., 0.], Quat::from_rotation_y(0.2)),
            node("Torso", Some(0), [0., 0.5, 0.], Quat::IDENTITY),
            node(
                "Thigh",
                Some(0),
                [0.15, -0.1, 0.],
                Quat::from_rotation_z(0.1),
            ),
        ],
        ..Default::default()
    }
}
fn calibration() -> Calibration3d {
    let mut c = Calibration3d {
        name: "Walk source".into(),
        ..Default::default()
    };
    for (t, s) in [("Pelvis", "hips"), ("Torso", "chest"), ("Thigh", "leg")] {
        c.set_source(t, Some(s));
    }
    c
}
fn retarget(m: &Motion, c: &Calibration3d) -> Animation3d {
    let t = target();
    let (mapping, report) = c.resolve(&t, m);
    assert_eq!(report.mapped, 3, "{:?}", report.lines());
    m.retarget_3d_calibrated(&t, &mapping, c).unwrap()
}
fn close(a: &Animation3d, b: &Animation3d) -> bool {
    a.frames.len() == b.frames.len()
        && a.frames.iter().zip(&b.frames).all(|(x, y)| {
            x.iter().zip(y).all(|(p, q)| {
                // angle_between is NaN for identical quaternions, so compare dots.
                Quat::from_array(p.rotation)
                    .dot(Quat::from_array(q.rotation))
                    .abs()
                    > 1. - 1e-6
                    && Vec3::from_array(p.translation).distance(Vec3::from_array(q.translation))
                        < 1e-4
            })
        })
}
fn project() -> Project {
    let mut p = Project::new(Rig::new("calibrated", 64, 64));
    p.extras.show_2d = false;
    p.extras.characters3d.push(Character {
        model: Arc::new(target()),
        ..Default::default()
    });
    p
}

#[test]
fn one_calibration_imports_reordered_sources_identically() {
    let (a, b) = (bvh(false), bvh(true));
    assert_ne!(a.joint_names(), b.joint_names(), "joint order differs");
    let c = calibration();
    assert!(close(&retarget(&a, &c), &retarget(&b, &c)));
    // Index mappings do not survive the reorder.
    let t = target();
    let (indices, _) = c.resolve(&t, &a);
    assert!(!close(
        &a.retarget_3d(&t, &indices, false).unwrap(),
        &b.retarget_3d(&t, &indices, false).unwrap()
    ));
    // A batch applies the same calibration to every clip, and matches preview.
    let mut c = c;
    c.facing = 30.;
    c.root_travel = RootTravel::Ground;
    c.offsets.insert("Torso".into(), [0., 0., 12.]);
    let mut preview = project();
    import_calibrated(
        &mut preview,
        1,
        std::slice::from_ref(&a),
        &c,
        false,
        Projection::Front,
    )
    .unwrap();
    let mut batch = project();
    let (clips, reports) =
        import_calibrated(&mut batch, 1, &[a, b], &c, false, Projection::Front).unwrap();
    assert_eq!(clips.len(), 2);
    assert!(
        reports
            .iter()
            .all(|r| r.mapped == 3 && r.missing.is_empty())
    );
    let anims = &batch.extras.characters3d[0].model.animations;
    let shown = &preview.extras.characters3d[0].model.animations[0];
    assert!(close(&anims[0], shown) && close(&anims[1], shown));
}

#[test]
fn reports_name_missing_ambiguous_stale_and_unmapped_joints() {
    let mut c = calibration();
    c.set_source("Thigh", Some("knee"));
    c.set_source("Gone", Some("hips"));
    c.mapping.remove("Torso");
    let mut t = target();
    t.nodes.push(Node3d {
        name: "Pelvis".into(),
        parent: Some(0),
        transform: Default::default(),
    });
    let (mapping, report) = c.resolve(&t, &bvh(false));
    assert_eq!(mapping, [None, None, None, None]);
    assert_eq!(report.mapped, 0);
    assert_eq!(report.missing, [("Thigh".to_string(), "knee".to_string())]);
    assert_eq!(report.ambiguous, ["Pelvis"]);
    assert_eq!(report.stale, ["Gone"]);
    assert_eq!(report.unmapped, ["Torso"]);
    let lines = report.lines().join("\n");
    for word in ["knee", "Pelvis", "Gone", "Torso"] {
        assert!(lines.contains(word), "{lines}");
    }
    let mut p = project();
    let before = p.clone();
    let err = import_calibrated(
        &mut p,
        1,
        &[bvh(false)],
        &Calibration3d::default(),
        false,
        Projection::Front,
    )
    .unwrap_err();
    assert!(err.to_string().contains("no calibrated joints"), "{err}");
    assert_eq!(p, before, "a failed batch leaves the project unchanged");
}

#[test]
fn calibration_keeps_target_limb_lengths_and_attachments() {
    let mut c = calibration();
    c.facing = 70.;
    c.up = UpAxis::Z;
    c.root_travel = RootTravel::Full;
    c.root_scale = RootScale::Fixed(0.5);
    c.offsets.insert("Thigh".into(), [20., -10., 5.]);
    c.reference = ReferencePose::Frame(3);
    let a = retarget(&bvh(true), &c);
    let t = target();
    for frame in &a.frames {
        for (i, (got, rest)) in frame.iter().zip(&t.nodes).enumerate() {
            assert_eq!(got.scale, rest.transform.scale);
            if i > 0 {
                assert_eq!(got.translation, rest.transform.translation);
            }
        }
    }
}

#[test]
fn facing_and_up_axis_rotate_source_motion_into_target_space() {
    let m = bvh(false);
    let base = calibration();
    let plain = retarget(&m, &base);
    let t = target();
    let rest = Quat::from_array(t.nodes[0].transform.rotation);
    let last = |a: &Animation3d| Quat::from_array(a.frames.last().unwrap()[0].rotation);
    // The source rolls the hips 15° about Z.
    let roll = Quat::from_rotation_z(15f32.to_radians());
    assert!(last(&plain).angle_between(roll * rest) < 1e-3);
    for (up, facing) in [(UpAxis::Y, 90.), (UpAxis::Z, 0.), (UpAxis::Z, -45.)] {
        let c = Calibration3d {
            up,
            facing,
            ..base.clone()
        };
        let space = Quat::from_rotation_y(f32::to_radians(facing))
            * match up {
                UpAxis::Y => Quat::IDENTITY,
                UpAxis::Z => Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            };
        let expected = space * roll * space.inverse() * rest;
        assert!(last(&retarget(&m, &c)).angle_between(expected) < 1e-3);
    }
}

#[test]
fn root_travel_and_scale_policies() {
    let m = bvh(false);
    let travel = |c: &Calibration3d| {
        let a = retarget(&m, c);
        let first = Vec3::from_array(a.frames[0][0].translation);
        a.frames
            .iter()
            .map(|f| Vec3::from_array(f[0].translation) - first)
            .collect::<Vec<_>>()
    };
    let with = |root_travel, root_scale| Calibration3d {
        root_travel,
        root_scale,
        ..calibration()
    };
    assert!(
        travel(&with(RootTravel::Off, RootScale::Auto))
            .iter()
            .all(|d| d.length() < 1e-6)
    );
    let full = travel(&with(RootTravel::Full, RootScale::Fixed(0.1)));
    assert!((full[8].x - 0.6).abs() < 1e-4, "{:?}", full[8]);
    assert!(full.iter().any(|d| d.y.abs() > 0.01), "source bob is kept");
    let ground = travel(&with(RootTravel::Ground, RootScale::Fixed(0.1)));
    assert!(
        ground
            .iter()
            .zip(&full)
            .all(|(g, f)| g.y == 0. && (g.x - f.x).abs() < 1e-5)
    );
    let vertical = travel(&with(RootTravel::Vertical, RootScale::Fixed(0.1)));
    assert!(
        vertical
            .iter()
            .zip(&full)
            .all(|(v, f)| v.x == 0. && v.z == 0. && (v.y - f.y).abs() < 1e-5)
    );
    let doubled = travel(&with(RootTravel::Full, RootScale::Fixed(0.2)));
    assert!(
        doubled
            .iter()
            .zip(&full)
            .all(|(d, f)| (*d - *f * 2.).length() < 1e-4)
    );
    // Facing turns the travel direction too.
    let turned = travel(&Calibration3d {
        facing: 90.,
        ..with(RootTravel::Ground, RootScale::Fixed(0.1))
    });
    assert!(
        (turned[8] - Vec3::new(0., 0., -0.6)).length() < 1e-4,
        "{:?}",
        turned[8]
    );
}

#[test]
fn reference_frame_and_joint_offsets_calibrate_the_rest_pose() {
    let m = bvh(true);
    let t = target();
    let c = Calibration3d {
        reference: ReferencePose::Frame(8),
        ..calibration()
    };
    // The reference frame maps exactly onto the target's rest pose.
    let a = retarget(&m, &c);
    for (got, rest) in a.frames[8].iter().zip(&t.nodes) {
        assert!(
            Quat::from_array(got.rotation).angle_between(Quat::from_array(rest.transform.rotation))
                < 1e-3
        );
    }
    // A reference past the clip end uses its last frame.
    let late = Calibration3d {
        reference: ReferencePose::Frame(500),
        ..c.clone()
    };
    assert!(close(&a, &retarget(&m, &late)));
    let mut offset = c.clone();
    offset.offsets.insert("Torso".into(), [0., 0., 25.]);
    let b = retarget(&m, &offset);
    let extra = Quat::from_euler(EulerRot::XYZ, 0., 0., 25f32.to_radians());
    for (x, y) in a.frames.iter().zip(&b.frames) {
        let expect = Quat::from_array(x[1].rotation) * extra;
        assert!(Quat::from_array(y[1].rotation).angle_between(expect) < 1e-3);
        assert_eq!(x[0], y[0], "other joints are unaffected");
        assert_eq!(x[2], y[2]);
    }
}

#[test]
fn calibrations_round_trip_and_reject_invalid_values() {
    let mut c = calibration();
    c.reference = ReferencePose::Frame(2);
    c.up = UpAxis::Z;
    c.facing = -90.;
    c.root_travel = RootTravel::Vertical;
    c.root_scale = RootScale::Fixed(0.25);
    c.offsets.insert("Thigh".into(), [1., 2., 3.]);
    let back: Calibration3d = serde_json::from_slice(&serde_json::to_vec(&c).unwrap()).unwrap();
    assert_eq!(back, c);
    back.validate().unwrap();
    // Older or partial files fill in defaults.
    let partial: Calibration3d =
        serde_json::from_str(r#"{"name":"x","mapping":{"Pelvis":"hips"}}"#).unwrap();
    assert_eq!(partial.root_travel, RootTravel::Off);
    for bad in [
        Calibration3d {
            facing: f32::NAN,
            ..c.clone()
        },
        Calibration3d {
            root_scale: RootScale::Fixed(0.),
            ..c.clone()
        },
    ] {
        assert!(bad.validate().is_err());
    }
    // Automatic calibration stores the canonical name matches.
    let auto = Calibration3d::automatic(&target(), &bvh(false));
    assert_eq!(auto.mapping.get("Pelvis").map(String::as_str), Some("hips"));
}
