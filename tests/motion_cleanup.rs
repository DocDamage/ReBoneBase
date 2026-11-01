use glam::Quat;
use rebonebase::{
    character3d::{
        Animation3d, Character, Model3d, Node3d, Playback3d, Transform3d,
        cleanup::{self, Cleanup, Frames},
    },
    export::{self, ExportRequest, Target},
    jobs::Progress,
    model::Rig,
    project::Project,
};
use std::sync::Arc;

const LENGTH: u32 = 24;
const HIPS: usize = 0;
const THIGH: usize = 1;
const SHIN: usize = 2;
const FOOT: usize = 3;
const SPINE: usize = 4;
const ARM: usize = 5;
const HAND: usize = 6;

/// Deterministic noise in -1..1.
fn noise(i: u32, salt: u32) -> f32 {
    let mut x = i
        .wrapping_mul(747_796_405)
        .wrapping_add(salt.wrapping_mul(2_891_336_453));
    x ^= x >> 13;
    x = x.wrapping_mul(1_274_126_177);
    x ^= x >> 16;
    (x % 20_001) as f32 / 10_000. - 1.
}
fn node(name: &str, parent: Option<usize>, translation: [f32; 3]) -> Node3d {
    Node3d {
        name: name.into(),
        parent,
        transform: Transform3d {
            translation,
            ..Default::default()
        },
    }
}
/// A leg and an arm. The stance foot slides backwards while the body moves
/// forwards, and the arm and thigh carry high-frequency noise.
fn noisy_walk() -> Project {
    let nodes = vec![
        node("hips", None, [0., 1., 0.]),
        node("thigh", Some(HIPS), [0.1, 0., 0.]),
        node("shin", Some(THIGH), [0., -0.45, 0.]),
        node("foot", Some(SHIN), [0., -0.45, 0.]),
        node("spine", Some(HIPS), [0., 0.3, 0.]),
        node("arm", Some(SPINE), [0.2, 0.2, 0.]),
        node("hand", Some(ARM), [0.3, 0., 0.]),
    ];
    let mut frames = vec![];
    for i in 0..LENGTH {
        let phase = i as f32 / LENGTH as f32 * std::f32::consts::TAU;
        let mut pose: Vec<_> = nodes.iter().map(|n| n.transform.clone()).collect();
        pose[HIPS].translation = [0., 0.95, 0.02 * i as f32];
        let thigh = 25f32.to_radians() * phase.sin() + 3f32.to_radians() * noise(i, 1);
        pose[THIGH].rotation = Quat::from_rotation_x(thigh).to_array();
        pose[SHIN].rotation =
            // A walk keeps the stance knee slightly bent.
            Quat::from_rotation_x(-(15f32.to_radians() + (30f32.to_radians() * (phase + 1.).sin()).max(0.)))
                .to_array();
        let arm = 20f32.to_radians() * phase.cos() + 6f32.to_radians() * noise(i, 2);
        pose[ARM].rotation = Quat::from_rotation_z(arm).to_array();
        frames.push(pose);
    }
    let mut cube = Model3d::default().primitives.remove(0);
    cube.node = FOOT;
    for p in &mut cube.positions {
        *p = [p[0] * 0.2, p[1] * 0.1, p[2] * 0.3];
    }
    let mut hand = cube.clone();
    hand.node = HAND;
    let model = Model3d {
        nodes,
        primitives: vec![cube, hand],
        animations: vec![Animation3d {
            name: "Noisy walk".into(),
            duration: (LENGTH - 1) as f32 / 24.,
            times: (0..LENGTH).map(|i| i as f32 / 24.).collect(),
            frames,
        }],
        ..Default::default()
    };
    let mut p = Project::new(Rig::new("cleanup", 96, 96));
    p.rig.active_clip_mut().length = LENGTH;
    p.rig.active_clip_mut().fps = 24.;
    p.extras.show_2d = false;
    let mut c = Character {
        model: Arc::new(model),
        position: rebonebase::model::Vec2::new(48., 80.),
        scale: 40.,
        ..Default::default()
    };
    c.clips.insert(
        0,
        Playback3d {
            animation: Some(0),
            speed: 1.,
            looped: false,
            offset_ms: 0.,
        },
    );
    p.extras.characters3d.push(c);
    p.validate().unwrap();
    p
}
fn locals(p: &Project) -> Frames {
    Frames::new(p, 1).unwrap()
}
fn same_rotation(a: &Transform3d, b: &Transform3d) -> bool {
    cleanup::angle(Quat::from_array(a.rotation), Quat::from_array(b.rotation)) < 1e-3
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

#[test]
fn smoothing_reduces_jitter_only_on_chosen_joints_and_frames() {
    let p = noisy_walk();
    let op = Cleanup::Smooth {
        joints: vec![ARM],
        start: 5,
        end: 18,
        radius: 3,
        strength: 1.,
    };
    let (q, report) = cleanup::apply(&p, 1, &op).unwrap();
    assert!(
        report.after < report.before * 0.4,
        "jitter {} → {}",
        report.before,
        report.after
    );
    assert_eq!(report.keys, 14);
    let (before, after) = (locals(&p), locals(&q));
    for f in 1..=LENGTH as usize {
        for j in 0..7 {
            let touched = j == ARM && (5..=18).contains(&f);
            if !touched {
                assert!(
                    same_rotation(&before.visible[f - 1][j], &after.visible[f - 1][j]),
                    "joint {j} frame {f} changed"
                );
            }
        }
    }
    // Timing and the imported animation are untouched.
    assert_eq!(q.rig, p.rig);
    assert_eq!(
        q.extras.characters3d[0].model,
        p.extras.characters3d[0].model
    );
    // Untouched frames render identically; touched ones agree with export.
    for f in [1., 3., 4., 19., 24.] {
        assert_eq!(q.render_at(f), p.render_at(f));
    }
    assert_ne!(frame_png(&q, 10.), frame_png(&p, 10.));
    assert_eq!(
        frame_png(&q, 10.),
        image_png(&q.render_at(10.)),
        "export matches the canvas"
    );
}
fn image_png(image: &image::RgbaImage) -> Vec<u8> {
    let mut out = std::io::Cursor::new(vec![]);
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn planting_stops_a_sliding_foot_and_aligns_it_to_the_floor() {
    let p = noisy_walk();
    let contacts = cleanup::detect_contacts(&p, 1, FOOT, 0.2, 0.2).unwrap();
    assert!(!contacts.is_empty());
    let (start, end) = (8, 14);
    // The lowest foot height is full extension; a reachable floor sits above it.
    let floor = cleanup::floor_height(&p, 1, FOOT).unwrap() + 0.03;
    let op = Cleanup::Plant {
        foot: FOOT,
        start,
        end,
        blend_in: 2,
        blend_out: 3,
        floor: Some(floor),
    };
    let (q, report) = cleanup::apply(&p, 1, &op).unwrap();
    assert!(
        report.before > 0.05,
        "fixture should slide: {}",
        report.before
    );
    assert!(
        report.after < report.before * 0.05,
        "slide {} → {}",
        report.before,
        report.after
    );
    let after = locals(&q);
    // The hips keep moving, so the last contact frames end at the leg's
    // reach; the report states the remaining distance.
    assert!(report.max_error < 0.02, "{report:?}");
    for f in start..=end {
        let y = after.position(f, FOOT).y;
        assert!((y - floor).abs() < 0.02, "frame {f}: {y} vs floor {floor}");
    }
    // Only the leg changes, and only inside the blend window.
    let before = locals(&p);
    for f in 1..=LENGTH {
        for j in 0..7 {
            let window = (6..=17).contains(&f) && [THIGH, SHIN, FOOT].contains(&j);
            if !window {
                assert!(
                    same_rotation(
                        &before.visible[f as usize - 1][j],
                        &after.visible[f as usize - 1][j]
                    ),
                    "joint {j} frame {f} changed"
                );
            }
        }
    }
    // Rotation-only corrections keep bone lengths, and the foot's world
    // orientation is preserved.
    for f in start..=end {
        for (a, b) in [(THIGH, SHIN), (SHIN, FOOT)] {
            let len = |s: &Frames| (s.position(f, a) - s.position(f, b)).length();
            assert!((len(&before) - len(&after)).abs() < 1e-4);
        }
        let rot = |s: &Frames| {
            let (_, r, _) = s.world[f as usize - 1][FOOT].to_scale_rotation_translation();
            r
        };
        let turned = cleanup::angle(rot(&before), rot(&after));
        assert!(turned < 0.1, "frame {f}: foot turned {turned}°");
    }
    // The blend eases in rather than snapping.
    let jump = |s: &Frames, f: u32| (s.position(f, FOOT) - s.position(f - 1, FOOT)).length();
    let worst = (6..=17).map(|f| jump(&after, f)).fold(0., f32::max);
    let original = (6..=17).map(|f| jump(&before, f)).fold(0., f32::max);
    assert!(
        worst <= original + report.before,
        "no snap larger than the removed slide: {worst} vs {original} + {}",
        report.before
    );
    assert_eq!(q.rig, p.rig);
}

#[test]
fn key_reduction_respects_the_tolerance_and_keeps_other_frames() {
    let p = noisy_walk();
    let (smoothed, _) = cleanup::apply(
        &p,
        1,
        &Cleanup::Smooth {
            joints: vec![ARM, THIGH],
            start: 1,
            end: LENGTH,
            radius: 4,
            strength: 1.,
        },
    )
    .unwrap();
    let dense: usize = smoothed.extras.characters3d[0].tracks[&0]
        .values()
        .map(Vec::len)
        .sum();
    assert_eq!(dense, 2 * LENGTH as usize);
    let (reduced, report) = cleanup::apply(
        &smoothed,
        1,
        &Cleanup::Reduce {
            joints: vec![ARM, THIGH],
            start: 1,
            end: LENGTH,
            tolerance: 2.,
        },
    )
    .unwrap();
    assert!(report.after < report.before * 0.7, "{report:?}");
    assert!(report.max_error <= 2.);
    let (a, b) = (locals(&smoothed), locals(&reduced));
    for f in 0..LENGTH as usize {
        for j in 0..7 {
            let e = cleanup::angle(
                Quat::from_array(a.visible[f][j].rotation),
                Quat::from_array(b.visible[f][j].rotation),
            );
            assert!(e <= 2. + 1e-3, "joint {j} frame {f}: {e}°");
        }
    }
    // A zero tolerance keeps the pose exactly.
    let (exact, _) = cleanup::apply(
        &smoothed,
        1,
        &Cleanup::Reduce {
            joints: vec![ARM],
            start: 1,
            end: LENGTH,
            tolerance: 0.,
        },
    )
    .unwrap();
    let c = locals(&exact);
    for f in 0..LENGTH as usize {
        assert!(same_rotation(&a.visible[f][ARM], &c.visible[f][ARM]));
    }
}

#[test]
fn cleanup_keeps_existing_corrections_outside_its_range() {
    let mut p = noisy_walk();
    let c = &mut p.extras.characters3d[0];
    c.set_joint_key(0, ARM, 2., [0., 0., 30.]).unwrap();
    c.set_joint_key(0, ARM, 22., [0., 0., -30.]).unwrap();
    c.set_joint_easing(
        0,
        &[rebonebase::character3d::JointKeyRef {
            joint: ARM,
            frame: 2.,
        }],
        rebonebase::animation::Ease::EaseIn,
    );
    let (q, _) = cleanup::apply(
        &p,
        1,
        &Cleanup::Smooth {
            joints: vec![ARM],
            start: 9,
            end: 13,
            radius: 2,
            strength: 1.,
        },
    )
    .unwrap();
    let (a, b) = (locals(&p), locals(&q));
    for f in (1..=LENGTH as usize).filter(|f| !(9..=13).contains(f)) {
        assert!(
            same_rotation(&a.visible[f - 1][ARM], &b.visible[f - 1][ARM]),
            "frame {f}"
        );
    }
}

#[test]
fn invalid_requests_change_nothing() {
    let p = noisy_walk();
    for op in [
        Cleanup::Smooth {
            joints: vec![ARM],
            start: 0,
            end: 5,
            radius: 2,
            strength: 1.,
        },
        Cleanup::Smooth {
            joints: vec![99],
            start: 1,
            end: 5,
            radius: 2,
            strength: 1.,
        },
        Cleanup::Plant {
            foot: HIPS,
            start: 1,
            end: 4,
            blend_in: 0,
            blend_out: 0,
            floor: None,
        },
        Cleanup::Reduce {
            joints: vec![ARM],
            start: 3,
            end: 2,
            tolerance: 1.,
        },
    ] {
        assert!(cleanup::apply(&p, 1, &op).is_err(), "{op:?}");
    }
    assert!(
        cleanup::apply(
            &p,
            77,
            &Cleanup::Reduce {
                joints: vec![],
                start: 1,
                end: 2,
                tolerance: 1.
            }
        )
        .is_err()
    );
}
