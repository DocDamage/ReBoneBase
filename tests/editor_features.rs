use rebonebase::{
    animation::{Channel, Clip, Ease, Playback},
    editing,
    model::{Rig, Vec2},
    skin,
};

fn chain() -> (Rig, u32, u32, u32) {
    let mut rig = Rig::new("Editable", 32, 32);
    let a = rig
        .add_bone(Vec2::new(4., 4.), Vec2::new(12., 4.), None)
        .unwrap();
    let b = rig
        .add_bone(Vec2::new(12., 4.), Vec2::new(20., 4.), Some(a))
        .unwrap();
    let c = rig
        .add_bone(Vec2::new(20., 4.), Vec2::new(24., 4.), Some(b))
        .unwrap();
    (rig, a, b, c)
}
fn close(a: Vec2, b: Vec2) {
    assert!((a - b).length() < 0.001, "{a:?} != {b:?}");
}

#[test]
fn locked_bones_and_length_and_rotation_constraints_are_enforced() {
    let (mut rig, _, b, _) = chain();
    rig.bone_mut(b).unwrap().locked = true;
    let before = rig.clone();
    assert!(rig.move_tail(b, Vec2::new(20., 10.)).is_err());
    assert_eq!(rig, before);
    let bone = rig.bone_mut(b).unwrap();
    bone.locked = false;
    bone.length_locked = true;
    bone.rotation_min = Some(-20.);
    bone.rotation_max = Some(30.);
    rig.move_tail(b, Vec2::new(12., 30.)).unwrap();
    assert_eq!(rig.bone(b).unwrap().length, 8.);
    rig.clips[0]
        .set_key(b, Channel::Rotation, 1., 100.)
        .unwrap();
    rig.clips[0].set_key(b, Channel::Scale, 1., 3.).unwrap();
    let rest = rig.world(None);
    let posed = rig.world(Some(&rig.clips[0].sample(1.)));
    let index = rig.bones.iter().position(|bone| bone.id == b).unwrap();
    assert!((posed[index].angle - rest[index].angle - 30.).abs() < 0.001);
    assert!(((posed[index].tail - posed[index].head).length() - 8.).abs() < 0.001);
}

#[test]
fn old_json_keeps_default_constraints_and_new_settings_roundtrip() {
    let rig: Rig =
        serde_json::from_str(r#"{"width":32,"height":32,"bones":[{"id":1}],"parts":[]}"#).unwrap();
    assert!(!rig.bones[0].locked);
    assert!(!rig.bones[0].length_locked);
    assert_eq!(rig.bones[0].rotation_min, None);
    let (mut rig, _, b, _) = chain();
    rig.bone_mut(b).unwrap().rotation_min = Some(90.);
    rig.bone_mut(b).unwrap().rotation_max = Some(-90.);
    assert!(rig.validate().is_err());
    rig.bone_mut(b).unwrap().rotation_max = Some(120.);
    rig.clips[0].playback = Playback::PingPong;
    rig.clips[0].durations.insert(2, 350);
    assert_eq!(Rig::from_json(&rig.to_json().unwrap()).unwrap(), rig);
}

#[test]
fn reparent_detach_and_connect_preserve_world_rest_and_reject_cycles_atomically() {
    let (mut rig, a, b, c) = chain();
    let original = rig.world(None);
    editing::reparent(&mut rig, b, None, false).unwrap();
    close(rig.world(None)[1].head, original[1].head);
    close(rig.world(None)[2].tail, original[2].tail);
    editing::reparent(&mut rig, b, Some(a), true).unwrap();
    assert!(rig.bone(b).unwrap().connected);
    let before = rig.clone();
    assert!(editing::reparent(&mut rig, a, Some(c), false).is_err());
    assert_eq!(rig, before);
    assert!(editing::reparent(&mut rig, b, Some(c), true).is_err());
    assert_eq!(rig, before);
}

#[test]
fn group_transform_does_not_double_transform_selected_descendants() {
    let (mut rig, _, b, c) = chain();
    let world = rig.world(None);
    let delta = Vec2::new(3., 2.);
    editing::transform_rest(&mut rig, &[b, c], delta, 0., Vec2::default()).unwrap();
    close(rig.world(None)[1].head, world[1].head + delta);
    close(rig.world(None)[2].tail, world[2].tail + delta);
    close(rig.world(None)[0].head, world[0].head);
    let before = rig.clone();
    let pivot = rig.world(None)[1].head;
    editing::transform_pose(&mut rig, &[b, c], Vec2::default(), 45., pivot, 1.).unwrap();
    let p = rig.clips[0].sample(1.);
    assert!((p[&b].rotation - 45.).abs() < 0.001);
    assert!(!p.contains_key(&c));
    assert_eq!(rig.bones, before.bones);
}

#[test]
fn key_clipboard_retime_and_snap_replace_collisions_and_fail_atomically() {
    let mut clip = Clip::default();
    clip.set_key(1, Channel::X, 1., 10.).unwrap();
    clip.set_key(1, Channel::X, 3., 30.).unwrap();
    clip.set_key(1, Channel::X, 5., 50.).unwrap();
    let selected = [
        editing::KeySelection {
            bone: 1,
            channel: Channel::X,
            frame: 1.,
        },
        editing::KeySelection {
            bone: 1,
            channel: Channel::X,
            frame: 3.,
        },
    ];
    let copy = editing::copy_keys(&clip, &selected);
    editing::paste_keys(&mut clip, &copy, 3.).unwrap();
    assert_eq!(
        clip.tracks[0]
            .keys
            .iter()
            .map(|k| k.value)
            .collect::<Vec<_>>(),
        vec![10., 10., 30.]
    );
    let before = clip.clone();
    assert!(editing::retime_keys(&mut clip, &selected, 20., 1., 1.).is_err());
    assert_eq!(clip, before);
    editing::retime_keys(&mut clip, &selected, 0.5, 1., 1.).unwrap();
    editing::snap_keys(
        &mut clip,
        &[editing::KeySelection {
            bone: 1,
            channel: Channel::X,
            frame: 1.5,
        }],
        1.,
    )
    .unwrap();
    assert!(clip.tracks[0].keys.iter().any(|k| k.frame == 2.));
}

#[test]
fn cubic_tangents_control_channel_curves_without_affecting_old_eases() {
    let mut clip = Clip {
        looped: false,
        ..Default::default()
    };
    clip.set_key(1, Channel::X, 1., 0.).unwrap();
    clip.set_key(1, Channel::X, 11., 10.).unwrap();
    let keys = &mut clip.tracks[0].keys;
    keys[0].easing = Ease::Cubic;
    keys[0].tangent_out = 2.;
    keys[1].tangent_in = 0.;
    assert!((clip.sample(6.)[&1].x - 7.5).abs() < 0.001);
    assert_eq!(clip.sample(1.)[&1].x, 0.);
    assert_eq!(clip.sample(11.)[&1].x, 10.);
}

#[test]
fn clip_management_keeps_active_identity_and_boundary_frames() {
    let (mut rig, a, _, _) = chain();
    rig.clips[0].looped = false;
    rig.clips[0].set_key(a, Channel::X, 1., 0.).unwrap();
    rig.clips[0].set_key(a, Channel::X, 12., 11.).unwrap();
    let copied = editing::duplicate_clip(&mut rig, 0).unwrap();
    assert_eq!(copied, 1);
    assert_eq!(rig.clips.len(), 2);
    editing::move_clip(&mut rig, 1, 0).unwrap();
    assert_eq!(rig.active_clip, 0);
    let source = rig.clips[0].clone();
    editing::trim_clip(&mut rig, 0, 3, 8).unwrap();
    assert_eq!(rig.clips[0].length, 6);
    assert!((rig.clips[0].sample(1.)[&a].x - source.sample(3.)[&a].x).abs() < 0.001);
    let right = editing::split_clip(&mut rig, 0, 4).unwrap();
    assert_eq!(rig.clips[0].length, 3);
    assert_eq!(rig.clips[right].length, 3);
    while rig.clips.len() > 1 {
        editing::delete_clip(&mut rig, 0).unwrap();
    }
    assert!(editing::delete_clip(&mut rig, 0).is_err());
    rig.validate().unwrap();
}

#[test]
fn playback_timing_handles_holds_reverse_pingpong_and_completion() {
    let mut clip = Clip {
        length: 3,
        fps: 10.,
        ..Default::default()
    };
    clip.durations.insert(2, 300);
    assert_eq!(clip.total_duration_ms(), 500);
    assert_eq!(clip.frame_at_ms(150.), (2, false));
    assert_eq!(clip.frame_at_ms(500.), (1, false));
    clip.playback = Playback::Reverse;
    assert_eq!(clip.frame_sequence(), vec![3, 2, 1]);
    assert_eq!(clip.frame_at_ms(0.).0, 3);
    clip.playback = Playback::PingPong;
    assert_eq!(clip.frame_sequence(), vec![1, 2, 3, 2]);
    assert_eq!(clip.total_duration_ms(), 800);
    clip.looped = false;
    assert_eq!(clip.frame_at_ms(801.), (1, true));
}

#[test]
fn root_motion_path_and_in_place_conversion_keep_initial_placement() {
    let (mut rig, a, _, _) = chain();
    rig.clips[0].set_key(a, Channel::X, 1., 2.).unwrap();
    rig.clips[0].set_key(a, Channel::X, 12., 12.).unwrap();
    let path = editing::root_path(&rig, 0, a).unwrap();
    assert_eq!(path.len(), 12);
    close(path[0], Vec2::new(6., 4.));
    editing::make_in_place(&mut rig, 0).unwrap();
    let path = editing::root_path(&rig, 0, a).unwrap();
    for p in path {
        close(p, Vec2::new(6., 4.));
    }
}

#[test]
fn pixel_outlines_and_per_group_settings_preserve_original_fill() {
    let mut image = image::RgbaImage::new(5, 5);
    image.put_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    let outlined = skin::outline(&image, 1, [0, 0, 0, 255]);
    assert_eq!(outlined.get_pixel(2, 2).0, [255, 0, 0, 255]);
    assert_eq!(outlined.get_pixel(1, 2).0, [0, 0, 0, 255]);
    assert_eq!(outlined.get_pixel(0, 0).0, [0, 0, 0, 0]);
    let (mut rig, a, _, _) = chain();
    let part = rig.add_part("Outline", [255, 0, 0, 255]);
    rig.bone_mut(a).unwrap().part = part;
    let plain = skin::render(&rig, None);
    rig.parts[0].outline_width = 1;
    let outlined = skin::render(&rig, None);
    assert!(
        outlined.pixels().filter(|p| p[3] > 0).count()
            > plain.pixels().filter(|p| p[3] > 0).count()
    );
}

#[test]
fn constrained_ik_obeys_joint_limits_and_does_not_key_the_root() {
    let (mut rig, a, b, c) = chain();
    rig.bone_mut(b).unwrap().rotation_min = Some(-10.);
    rig.bone_mut(b).unwrap().rotation_max = Some(10.);
    rig.bone_mut(c).unwrap().rotation_min = Some(0.);
    rig.bone_mut(c).unwrap().rotation_max = Some(45.);
    let mut pose = rebonebase::animation::Pose::new();
    rebonebase::ik::apply_two_bone(&rig, &mut pose, b, c, Vec2::new(12., 14.)).unwrap();
    assert!(!pose.contains_key(&a));
    assert!((-10. ..=10.).contains(&pose[&b].rotation));
    assert!((0. ..=45.).contains(&pose[&c].rotation));
    rig.bone_mut(b).unwrap().locked = true;
    rig.bone_mut(c).unwrap().locked = true;
    let before = pose.clone();
    assert!(rebonebase::ik::apply_two_bone(&rig, &mut pose, b, c, Vec2::new(10., 10.)).is_err());
    assert_eq!(pose, before);
}

#[test]
fn an_ik_chain_with_one_locked_joint_uses_the_remaining_joint() {
    let (mut rig, _, upper, lower) = chain();
    rig.bone_mut(upper).unwrap().locked = true;
    let mut pose = rebonebase::animation::Pose::new();
    let target = Vec2::new(20., 0.);
    rebonebase::ik::apply_two_bone(&rig, &mut pose, upper, lower, target).unwrap();
    close(
        rig.world(Some(&pose))
            .into_iter()
            .find(|w| w.id == lower)
            .unwrap()
            .tail,
        target,
    );
    rig.bone_mut(upper).unwrap().locked = false;
    rig.bone_mut(lower).unwrap().locked = true;
    pose.clear();
    let target = Vec2::new(12., 16.);
    rebonebase::ik::apply_two_bone(&rig, &mut pose, upper, lower, target).unwrap();
    close(
        rig.world(Some(&pose))
            .into_iter()
            .find(|w| w.id == lower)
            .unwrap()
            .tail,
        target,
    );
}
