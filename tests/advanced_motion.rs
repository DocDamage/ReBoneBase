use rebonebase::{
    advanced_motion::{
        self as advanced, Chain, ContactKey, ContactTrack, Layer, MotionTools, Spring, WalkSettings,
    },
    animation::{Channel, Ease, Pose},
    model::{Rig, Vec2},
};

fn chain() -> (Rig, Vec<u32>) {
    let mut rig = Rig::new("Chain", 64, 64);
    let root = rig
        .add_bone(Vec2::new(2., 10.), Vec2::new(5., 10.), None)
        .unwrap();
    let a = rig
        .add_bone(Vec2::new(5., 10.), Vec2::new(10., 10.), Some(root))
        .unwrap();
    let b = rig
        .add_bone(Vec2::new(10., 10.), Vec2::new(15., 10.), Some(a))
        .unwrap();
    let c = rig
        .add_bone(Vec2::new(15., 10.), Vec2::new(20., 10.), Some(b))
        .unwrap();
    (rig, vec![a, b, c])
}
fn close(a: Vec2, b: Vec2) {
    assert!((a - b).length() < 0.05, "{a:?} != {b:?}");
}

#[test]
fn ccd_reaches_with_three_bones_and_never_moves_the_root_or_lengths() {
    let (mut rig, ids) = chain();
    let before = rig.clone();
    let config = Chain {
        bones: ids.clone(),
        target: Vec2::new(13., 17.),
        iterations: 40,
        ..Default::default()
    };
    let pose = advanced::solve_chain(&rig, &Pose::new(), &config);
    let world = rig.world(Some(&pose));
    close(world.last().unwrap().tail, config.target);
    assert!(!pose.contains_key(&rig.bones[0].id));
    assert_eq!(rig, before);
    rig.bone_mut(ids[1]).unwrap().locked = true;
    rig.bone_mut(ids[0]).unwrap().rotation_min = Some(-10.);
    rig.bone_mut(ids[0]).unwrap().rotation_max = Some(10.);
    let pose = advanced::solve_chain(&rig, &Pose::new(), &config);
    assert!(!pose.contains_key(&ids[1]));
    assert!((-10. ..=10.).contains(&pose[&ids[0]].rotation));
    let zero = Chain {
        weight: 0.,
        ..config
    };
    assert!(advanced::solve_chain(&rig, &Pose::new(), &zero).is_empty());
}

#[test]
fn contacts_blend_into_ik_then_release_to_original_fk() {
    let (rig, ids) = chain();
    let track = ContactTrack {
        lower: ids[1],
        keys: vec![
            ContactKey {
                frame: 1.,
                target: Vec2::new(11., 16.),
                weight: 1.,
                ..Default::default()
            },
            ContactKey {
                frame: 5.,
                target: Vec2::new(11., 16.),
                weight: 0.,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let tools = MotionTools {
        contacts: vec![track],
        ..Default::default()
    };
    tools.validate(&rig).unwrap();
    let solved = advanced::evaluate_constraints(&rig, &tools, 1., &Pose::new());
    let world = rig.world(Some(&solved));
    close(
        world.iter().find(|w| w.id == ids[1]).unwrap().tail,
        Vec2::new(11., 16.),
    );
    assert_eq!(
        advanced::evaluate_constraints(&rig, &tools, 5., &Pose::new()),
        Pose::new()
    );
    assert!(!solved.contains_key(&rig.bones[0].id));
}

#[test]
fn masks_and_channel_specific_layers_preserve_unanimated_offsets() {
    let (mut rig, ids) = chain();
    rig.clips.push(rig.clips[0].clone());
    rig.clips[1]
        .set_key(ids[0], Channel::Rotation, 1., 40.)
        .unwrap();
    rig.clips[1]
        .set_key(ids[1], Channel::Rotation, 1., 60.)
        .unwrap();
    let mut base = Pose::new();
    base.entry(ids[0]).or_default().x = 3.;
    base.get_mut(&ids[0]).unwrap().rotation = 10.;
    let mut tools = MotionTools {
        layers: vec![Layer {
            clip: 1,
            weight: 0.5,
            mask: vec![ids[0]],
            ..Default::default()
        }],
        ..Default::default()
    };
    let pose = advanced::layers(&rig, &tools, 1., &base);
    assert_eq!(pose[&ids[0]].rotation, 25.);
    assert_eq!(pose[&ids[0]].x, 3.);
    assert!(!pose.contains_key(&ids[1]));
    tools.layers[0].additive = true;
    assert_eq!(
        advanced::layers(&rig, &tools, 1., &base)[&ids[0]].rotation,
        30.
    );
    tools.remap_clips(&[Some(0), None]);
    assert!(tools.layers.is_empty());
}

#[test]
fn spring_simulation_is_deterministic_for_scrubbing_and_can_be_baked() {
    let (mut rig, ids) = chain();
    rig.clips[0]
        .set_key(ids[2], Channel::Rotation, 1., 0.)
        .unwrap();
    rig.clips[0]
        .set_key(ids[2], Channel::Rotation, 2., 60.)
        .unwrap();
    let tools = MotionTools {
        springs: vec![Spring {
            bone: ids[2],
            stiffness: 25.,
            damping: 5.,
            wind: 10.,
            ..Default::default()
        }],
        ..Default::default()
    };
    tools.validate(&rig).unwrap();
    let base = rig.clips[0].sample(6.);
    let first = advanced::evaluate_constraints(&rig, &tools, 6., &base);
    let _ = advanced::evaluate_constraints(&rig, &tools, 2., &rig.clips[0].sample(2.));
    assert_eq!(
        advanced::evaluate_constraints(&rig, &tools, 6., &base),
        first
    );
    assert!((first[&ids[2]].rotation - base[&ids[2]].rotation).abs() > 0.01);
    let old_bones = rig.bones.clone();
    advanced::bake_pose(&mut rig, &first, 6.).unwrap();
    assert_eq!(rig.bones, old_bones);
    assert!((rig.clips[0].sample(6.)[&ids[2]].rotation - first[&ids[2]].rotation).abs() < 0.001);
}

#[test]
fn generated_walk_has_a_full_gait_and_ground_contacts_and_optional_travel() {
    let mut rig = rebonebase::project::templates()[0].clone();
    let settings = WalkSettings {
        root_travel: true,
        frames: 24,
        ..Default::default()
    };
    let index = advanced::generate_walk(&mut rig, &settings).unwrap();
    assert_eq!(rig.active_clip, index);
    assert_eq!(rig.clips[index].length, 24);
    assert!(rig.clips[index].tracks.len() > 12);
    rig.validate().unwrap();
    let root = rig.bones.iter().find(|b| b.parent.is_none()).unwrap().id;
    let first = rig.clips[index].sample(1.);
    let middle = rig.clips[index].sample(12.);
    assert!(middle[&root].x > first[&root].x);
    let shin = rig.bones.iter().find(|b| b.name == "shin_L").unwrap().id;
    let a = rig
        .world(Some(&first))
        .into_iter()
        .find(|w| w.id == shin)
        .unwrap();
    for frame in 2..=14 {
        let b = rig
            .world(Some(&rig.clips[index].sample(frame as f32)))
            .into_iter()
            .find(|w| w.id == shin)
            .unwrap();
        close(a.tail, b.tail);
    }
    let mut unsupported = Rig::new("No legs", 32, 32);
    let before = unsupported.clone();
    assert!(advanced::generate_walk(&mut unsupported, &WalkSettings::default()).is_err());
    assert_eq!(unsupported, before);
}

#[test]
fn loop_cleanup_matches_pose_and_velocity_and_mirroring_swaps_both_sides() {
    let mut rig = rebonebase::project::templates()[0].clone();
    let left = rig.bones.iter().find(|b| b.name == "forearm_L").unwrap().id;
    let right = rig.bone(left).unwrap().twin.unwrap();
    rig.clips[0].looped = false;
    rig.clips[0]
        .set_key(left, Channel::Rotation, 1., 10.)
        .unwrap();
    rig.clips[0]
        .set_key(left, Channel::Rotation, 12., 70.)
        .unwrap();
    advanced::smooth_loop(&mut rig, 0, 4, true).unwrap();
    let clip = &rig.clips[0];
    let track = clip
        .tracks
        .iter()
        .find(|t| t.bone == left && t.channel == Channel::Rotation)
        .unwrap();
    assert!((track.keys[0].value - track.keys.last().unwrap().value).abs() < 0.001);
    assert_eq!(
        track.keys[0].tangent_out,
        track.keys.last().unwrap().tangent_in
    );
    assert_eq!(track.keys[0].easing, Ease::Cubic);
    let mirrored = advanced::mirror_clip(&mut rig, 0).unwrap();
    assert!((rig.clips[mirrored].sample(1.)[&right].rotation + 10.).abs() < 0.01);
    rig.validate().unwrap();
}

#[test]
fn transitions_blend_angles_short_way_and_create_valid_transactional_clips() {
    let (mut rig, ids) = chain();
    let mut from = Pose::new();
    from.entry(ids[0]).or_default().rotation = 170.;
    let mut to = Pose::new();
    to.entry(ids[0]).or_default().rotation = -170.;
    let mid = advanced::transition_pose(&rig, &from, &to, 0.5);
    assert!((mid[&ids[0]].rotation - 180.).abs() < 0.01);
    rig.clips[0]
        .set_key(ids[0], Channel::Rotation, 12., 30.)
        .unwrap();
    let mut second = rig.clips[0].clone();
    second.set_key(ids[0], Channel::Rotation, 1., -30.).unwrap();
    rig.clips.push(second);
    let created = advanced::create_transition(&mut rig, 0, 1, 6).unwrap();
    assert_eq!(rig.clips[created].length, 6);
    assert!((rig.clips[created].sample(1.)[&ids[0]].rotation - 30.).abs() < 0.01);
    assert!((rig.clips[created].sample(6.)[&ids[0]].rotation + 30.).abs() < 0.01);
    let before = rig.clone();
    assert!(advanced::create_transition(&mut rig, 0, 99, 6).is_err());
    assert_eq!(rig, before);
}

#[test]
fn controller_metadata_is_bounded_and_backward_compatible() {
    let (rig, ids) = chain();
    let empty: MotionTools = serde_json::from_str("{}").unwrap();
    empty.validate(&rig).unwrap();
    let bad = MotionTools {
        chains: vec![Chain {
            bones: vec![rig.bones[0].id, ids[0]],
            ..Default::default()
        }],
        ..Default::default()
    };
    assert!(bad.validate(&rig).is_err());
    let bad = MotionTools {
        springs: vec![Spring {
            bone: ids[0],
            stiffness: f32::NAN,
            ..Default::default()
        }],
        ..Default::default()
    };
    assert!(bad.validate(&rig).is_err());
}

#[test]
fn contact_timing_and_clip_ownership_survive_remapping() {
    let (mut rig, ids) = chain();
    rig.clips.push(rig.clips[0].clone());
    let track = ContactTrack {
        lower: ids[1],
        clip: Some(1),
        keys: vec![
            ContactKey {
                frame: 2.,
                target: Vec2::new(11., 16.),
                weight: 0.,
                ..Default::default()
            },
            ContactKey {
                frame: 4.,
                target: Vec2::new(11., 16.),
                weight: 1.,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let mut tools = MotionTools {
        contacts: vec![track],
        ..Default::default()
    };
    assert!(advanced::evaluate_constraints(&rig, &tools, 4., &Pose::new()).is_empty());
    rig.active_clip = 1;
    assert!(advanced::evaluate_constraints(&rig, &tools, 1., &Pose::new()).is_empty());
    let half = advanced::evaluate_constraints(&rig, &tools, 3., &Pose::new());
    let full = advanced::evaluate_constraints(&rig, &tools, 4., &Pose::new());
    assert!((half[&ids[0]].rotation * 2. - full[&ids[0]].rotation).abs() < 0.001);
    assert_eq!(
        advanced::evaluate_constraints(&rig, &tools, 6., &Pose::new()),
        full
    );
    tools.remap_clips(&[None, Some(0)]);
    assert_eq!(tools.contacts[0].clip, Some(0));
    tools.remap_clips(&[None]);
    assert!(tools.contacts.is_empty());
}

#[test]
fn chain_handles_unreachable_folded_targets_and_length_locks() {
    let (mut rig, ids) = chain();
    for b in &mut rig.bones {
        b.length_locked = true;
    }
    let mut pose = Pose::new();
    pose.entry(ids[1]).or_default().rotation = 180.;
    let config = Chain {
        bones: ids.clone(),
        target: Vec2::new(12., 12.),
        iterations: 64,
        ..Default::default()
    };
    let solved = advanced::solve_chain(&rig, &pose, &config);
    close(rig.world(Some(&solved)).last().unwrap().tail, config.target);
    let far = Chain {
        target: Vec2::new(500., 100.),
        ..config
    };
    let solved = advanced::solve_chain(&rig, &Pose::new(), &far);
    for w in rig.world(Some(&solved)) {
        let b = rig.bone(w.id).unwrap();
        assert!(((w.tail - w.head).length() - b.length).abs() < 0.001);
    }
    assert!(!solved.contains_key(&rig.bones[0].id));
}

#[test]
fn all_layer_channels_have_neutral_additive_values_and_disabled_layers_do_nothing() {
    let (mut rig, ids) = chain();
    let part = rig.add_part("Layer part", [30, 40, 50, 255]);
    rig.clips.push(rig.clips[0].clone());
    let clip = &mut rig.clips[1];
    for (ch, value) in [(Channel::X, 4.), (Channel::Y, -2.), (Channel::Scale, 2.)] {
        clip.set_key(ids[0], ch, 1., value).unwrap();
    }
    clip.set_key(part, Channel::Z, 1., 7.).unwrap();
    clip.set_key(part, Channel::Visibility, 1., 0.).unwrap();
    let mut tools = MotionTools {
        layers: vec![Layer {
            clip: 1,
            weight: 0.5,
            additive: true,
            ..Default::default()
        }],
        ..Default::default()
    };
    let p = advanced::layers(&rig, &tools, 1., &Pose::new());
    assert_eq!(p[&ids[0]].x, 2.);
    assert_eq!(p[&ids[0]].y, -1.);
    assert_eq!(p[&ids[0]].scale, 1.5);
    assert_eq!(p[&part].z, Some((rig.parts[0].z as f32 + 7.) / 2.));
    assert_eq!(p[&part].visibility, Some(0.));
    rig.bone_mut(ids[0]).unwrap().length_locked = true;
    assert_eq!(
        advanced::layers(&rig, &tools, 1., &Pose::new())[&ids[0]].scale,
        1.
    );
    tools.layers[0].enabled = false;
    assert!(advanced::layers(&rig, &tools, 1., &Pose::new()).is_empty());
}

#[test]
fn spring_damping_regimes_are_finite_under_long_holds_and_joint_limits() {
    for damping in [0., 10., 40.] {
        let (mut rig, ids) = chain();
        rig.clips[0].durations.insert(1, 60000);
        rig.clips[0]
            .set_key(ids[0], Channel::Rotation, 2., 90.)
            .unwrap();
        rig.bone_mut(ids[0]).unwrap().rotation_min = Some(-20.);
        rig.bone_mut(ids[0]).unwrap().rotation_max = Some(20.);
        let tools = MotionTools {
            springs: vec![Spring {
                bone: ids[0],
                stiffness: 25.,
                damping,
                gravity: 50.,
                wind: 10.,
                ..Default::default()
            }],
            ..Default::default()
        };
        let p = advanced::evaluate_constraints(&rig, &tools, 8.5, &rig.clips[0].sample(8.5));
        assert!(p[&ids[0]].rotation.is_finite());
        assert!((-20. ..=20.).contains(&p[&ids[0]].rotation));
        rig.bone_mut(ids[0]).unwrap().locked = true;
        let base = rig.clips[0].sample(8.5);
        assert_eq!(
            advanced::evaluate_constraints(&rig, &tools, 8.5, &base),
            base
        );
    }
}

#[test]
fn contacts_remain_authoritative_over_secondary_springs() {
    let (rig, ids) = chain();
    let tools = MotionTools {
        contacts: vec![ContactTrack {
            lower: ids[1],
            keys: vec![ContactKey {
                target: Vec2::new(11., 16.),
                ..Default::default()
            }],
            ..Default::default()
        }],
        springs: vec![Spring {
            bone: ids[1],
            gravity: 80.,
            ..Default::default()
        }],
        ..Default::default()
    };
    let p = advanced::evaluate_constraints(&rig, &tools, 8., &Pose::new());
    close(
        rig.world(Some(&p))
            .iter()
            .find(|w| w.id == ids[1])
            .unwrap()
            .tail,
        Vec2::new(11., 16.),
    );
}

#[test]
fn baking_rejects_missing_owners_nonfinite_values_and_out_of_range_times_atomically() {
    let (mut rig, ids) = chain();
    let before = rig.clone();
    let mut p = Pose::new();
    p.entry(ids[0]).or_default().rotation = 20.;
    p.entry(99999).or_default().x = 1.;
    assert!(advanced::bake_pose(&mut rig, &p, 1.).is_err());
    assert_eq!(rig, before);
    p.remove(&99999);
    p.get_mut(&ids[0]).unwrap().rotation = f32::NAN;
    assert!(advanced::bake_pose(&mut rig, &p, 1.).is_err());
    assert_eq!(rig, before);
    p.get_mut(&ids[0]).unwrap().rotation = 20.;
    assert!(advanced::bake_pose(&mut rig, &p, 0.).is_err());
    assert_eq!(rig, before);
}

#[test]
fn run_generator_supports_in_place_and_rejects_locked_or_invalid_limbs() {
    let mut rig = rebonebase::project::templates()[0].clone();
    let index = advanced::generate_walk(
        &mut rig,
        &WalkSettings {
            run: true,
            name: "Run".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(rig.clips[index].looped);
    let root = rig.bones.iter().find(|b| b.parent.is_none()).unwrap().id;
    for frame in 1..=rig.clips[index].length {
        assert_eq!(rig.clips[index].sample(frame as f32)[&root].x, 0.);
    }
    rig.validate().unwrap();
    let shin = rig.bones.iter().find(|b| b.name == "shin_L").unwrap().id;
    rig.bone_mut(shin).unwrap().locked = true;
    let before = rig.clone();
    assert!(advanced::generate_walk(&mut rig, &WalkSettings::default()).is_err());
    assert_eq!(rig, before);
    assert!(
        advanced::generate_walk(
            &mut rig,
            &WalkSettings {
                cadence: f32::NAN,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert_eq!(rig, before);
}

#[test]
fn mirroring_reflects_animated_root_loose_joints_and_part_visibility_with_timing() {
    let mut rig = rebonebase::project::templates()[0].clone();
    let root = rig.bones.iter().find(|b| b.parent.is_none()).unwrap().id;
    let left = rig
        .bones
        .iter()
        .find(|b| b.name == "upper_arm_L")
        .unwrap()
        .id;
    let right = rig.bone(left).unwrap().twin.unwrap();
    let part = rig.bone(left).unwrap().part;
    let otherpart = rig.bone(right).unwrap().part;
    for (id, ch, value) in [
        (root, Channel::X, 3.),
        (left, Channel::X, 2.),
        (left, Channel::Rotation, 15.),
        (left, Channel::Scale, 1.2),
        (part, Channel::Visibility, 0.),
    ] {
        rig.clips[0].set_key(id, ch, 1., value).unwrap();
    }
    rig.clips[0].durations.insert(1, 123);
    rig.clips[0].playback = rebonebase::animation::Playback::Reverse;
    let source = rig.world(Some(&rig.clips[0].sample(1.)));
    let index = advanced::mirror_clip(&mut rig, 0).unwrap();
    assert_eq!(rig.clips[index].durations.get(&1), Some(&123));
    assert_eq!(
        rig.clips[index].playback,
        rebonebase::animation::Playback::Reverse
    );
    let pose = rig.clips[index].sample(1.);
    let mirrored = rig.world(Some(&pose));
    for b in &rig.bones {
        let target = b.twin.unwrap_or(b.id);
        let from = source.iter().find(|w| w.id == b.id).unwrap();
        let to = mirrored.iter().find(|w| w.id == target).unwrap();
        close(
            to.head,
            Vec2::new(2. * rig.mirror.x - from.head.x, from.head.y),
        );
        close(
            to.tail,
            Vec2::new(2. * rig.mirror.x - from.tail.x, from.tail.y),
        );
    }
    assert_eq!(pose[&otherpart].visibility, Some(0.));
}

#[test]
fn controller_documents_reject_bad_channels_keys_and_references() {
    let (rig, ids) = chain();
    let bads = vec![
        MotionTools {
            layers: vec![Layer {
                clip: 5,
                ..Default::default()
            }],
            ..Default::default()
        },
        MotionTools {
            layers: vec![Layer {
                mask: vec![99999],
                ..Default::default()
            }],
            ..Default::default()
        },
        MotionTools {
            contacts: vec![ContactTrack {
                lower: ids[1],
                keys: vec![ContactKey::default(), ContactKey::default()],
                ..Default::default()
            }],
            ..Default::default()
        },
        MotionTools {
            springs: vec![
                Spring {
                    bone: ids[0],
                    ..Default::default()
                };
                2
            ],
            ..Default::default()
        },
        MotionTools {
            chains: vec![Chain {
                bones: vec![ids[0], ids[2]],
                ..Default::default()
            }],
            ..Default::default()
        },
    ];
    for bad in bads {
        assert!(bad.validate(&rig).is_err());
        assert_eq!(
            advanced::evaluate_constraints(&rig, &bad, 2., &Pose::new()),
            Pose::new()
        );
    }
}

#[test]
fn clip_authoring_failures_preserve_the_entire_document() {
    let (mut rig, ids) = chain();
    let before = rig.clone();
    assert!(advanced::smooth_loop(&mut rig, 99, 3, true).is_err());
    assert!(advanced::smooth_loop(&mut rig, 0, 12, true).is_err());
    assert!(advanced::create_transition(&mut rig, 0, 0, 1).is_err());
    assert!(advanced::mirror_clip(&mut rig, 99).is_err());
    assert_eq!(rig, before);
    rig.clips[0]
        .set_key(ids[0], Channel::Rotation, 1., 30.)
        .unwrap();
    while rig.clips.len() < 32 {
        rig.clips.push(rig.clips[0].clone());
    }
    let before = rig.clone();
    assert!(advanced::mirror_clip(&mut rig, 0).is_err());
    assert!(advanced::create_transition(&mut rig, 0, 1, 4).is_err());
    assert_eq!(rig, before);
}

#[test]
fn contact_bend_switches_on_its_key_and_chain_capture_preserves_exact_fk() {
    let (rig, ids) = chain();
    let tools = MotionTools {
        contacts: vec![ContactTrack {
            lower: ids[1],
            keys: vec![
                ContactKey {
                    target: Vec2::new(11., 16.),
                    bend: 1.,
                    ..Default::default()
                },
                ContactKey {
                    frame: 3.,
                    target: Vec2::new(11., 16.),
                    bend: -1.,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let before = advanced::evaluate_constraints(&rig, &tools, 2.99, &Pose::new());
    let on_key = advanced::evaluate_constraints(&rig, &tools, 3., &Pose::new());
    assert!(before[&ids[1]].rotation > 0.);
    assert!(on_key[&ids[1]].rotation < 0.);
    let base = Pose::new();
    let target = rig.world(Some(&base)).last().unwrap().tail;
    assert_eq!(
        advanced::solve_chain(
            &rig,
            &base,
            &Chain {
                bones: ids,
                target,
                ..Default::default()
            }
        ),
        base
    );
}

#[test]
fn layer_and_spring_aggregate_work_is_bounded_before_evaluation() {
    let (mut rig, ids) = chain();
    rig.clips[0].length = 240;
    for id in ids.iter().copied() {
        for channel in [Channel::X, Channel::Y, Channel::Rotation, Channel::Scale] {
            for frame in 1..=240 {
                rig.clips[0]
                    .set_key(
                        id,
                        channel,
                        frame as f32,
                        if channel == Channel::Scale { 1. } else { 0. },
                    )
                    .unwrap();
            }
        }
    }
    let tools = MotionTools {
        layers: vec![Layer::default(); 32],
        springs: vec![Spring {
            bone: ids[0],
            ..Default::default()
        }],
        ..Default::default()
    };
    rig.validate().unwrap();
    assert!(
        tools.validate(&rig).is_err(),
        "replaying 32 dense layers over 240 spring frames must exceed the work budget"
    );
}

#[test]
fn empty_pose_bakes_still_validate_time_and_active_clip_without_panicking() {
    let (mut rig, _) = chain();
    let before = rig.clone();
    assert!(advanced::bake_pose(&mut rig, &Pose::new(), 0.).is_err());
    assert_eq!(rig, before);
    rig.active_clip = 100;
    assert!(advanced::bake_pose(&mut rig, &Pose::new(), 1.).is_err());
}

#[test]
fn metadata_limits_reject_hostile_controller_documents() {
    let (rig, ids) = chain();
    let chain_config = Chain {
        bones: ids.clone(),
        ..Default::default()
    };
    for bad in [
        MotionTools {
            chains: vec![chain_config; 33],
            ..Default::default()
        },
        MotionTools {
            contacts: vec![ContactTrack::default(); 129],
            ..Default::default()
        },
        MotionTools {
            layers: vec![Layer::default(); 33],
            ..Default::default()
        },
        MotionTools {
            springs: vec![Spring::default(); 65],
            ..Default::default()
        },
        MotionTools {
            contacts: vec![ContactTrack {
                lower: ids[1],
                keys: vec![ContactKey {
                    weight: -0.5,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        },
        MotionTools {
            contacts: vec![ContactTrack {
                lower: ids[0],
                ..Default::default()
            }],
            ..Default::default()
        },
        MotionTools {
            layers: vec![Layer {
                offset: 241.,
                ..Default::default()
            }],
            ..Default::default()
        },
    ] {
        assert!(bad.validate(&rig).is_err());
    }
}
