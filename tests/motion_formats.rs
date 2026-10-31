use glam::{Quat, Vec3};
use rebonebase::{
    character3d::{Model3d, Node3d, Transform3d},
    motion::Motion,
};
use std::path::Path;

fn target() -> Model3d {
    Model3d {
        nodes: vec![
            Node3d {
                name: "hips".into(),
                parent: None,
                transform: Transform3d {
                    rotation: Quat::from_rotation_y(0.4).to_array(),
                    ..Default::default()
                },
            },
            Node3d {
                name: "chest".into(),
                parent: Some(0),
                transform: Transform3d {
                    translation: [0., 2., 0.],
                    rotation: Quat::from_rotation_x(0.3).to_array(),
                    ..Default::default()
                },
            },
        ],
        ..Default::default()
    }
}
const UNITY: &str = "%YAML 1.1\n%TAG !u! tag:unity3d.com,2011:\n--- !u!74 &7400000\nAnimationClip:\n  m_Name: Turn\n  m_Compressed: 0\n  m_RotationCurves:\n  - curve:\n      m_Curve:\n      - time: 0\n        value: {x: 0, y: 0, z: 0, w: 1}\n      - time: 1\n        value: {x: 0, y: 0, z: 0.70710678, w: 0.70710678}\n    path: hips\n  m_PositionCurves:\n  - curve:\n      m_Curve:\n      - time: 0\n        value: {x: 0, y: 0, z: 0}\n      - time: 1\n        value: {x: 1, y: 0, z: 0}\n    path: hips\n  - curve:\n      m_Curve:\n      - time: 0\n        value: {x: 0, y: 1, z: 0}\n      - time: 1\n        value: {x: 0, y: 1, z: 0}\n    path: hips/chest\n";
#[test]
fn unity_transform_curves_retarget_without_detaching_target_joints() {
    let m = Motion::from_unity_anim(UNITY).unwrap();
    assert!((m.duration() - 1.).abs() < 0.001);
    assert_eq!(m.joint_names(), vec!["hips", "chest"]);
    let t = target();
    let map = m.mapping_3d(&t);
    let a = m.retarget_3d(&t, &map, false).unwrap();
    assert_eq!(a.frames.last().unwrap()[1].translation, [0., 2., 0.]);
    assert_eq!(a.frames.last().unwrap()[0].translation, [0.; 3]);
    let root = Quat::from_array(a.frames.last().unwrap()[0].rotation);
    let expected = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
        * Quat::from_array(t.nodes[0].transform.rotation);
    assert!(root.angle_between(expected) < 0.001);
    let travel = m.retarget_3d(&t, &map, true).unwrap();
    assert!((travel.frames.last().unwrap()[0].translation[0] - 2.).abs() < 0.01);
    assert!(m.retarget_3d(&t, &[None, None], false).is_err());
}
#[test]
fn native_fbx_motion_does_not_need_a_mesh() {
    let path = Path::new("src/character3d/fixtures/skinned_ascii.fbx");
    let names = Motion::animation_names(path).unwrap();
    assert!(!names.is_empty());
    let m = Motion::load_animation(path, 0).unwrap();
    assert!(m.frame_count() > 1);
    assert!(m.joint_names().iter().any(|n| n == "Joint"));
    let a = m
        .retarget_3d(&target(), &[Some(0), Some(1)], false)
        .unwrap();
    assert!(a.frames.len() > 1);
    assert!(Motion::load_animation(path, 99).is_err());
    let bytes = std::fs::read(path).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let mut depth = 0i32;
    let mut lines = vec![];
    for line in text.lines() {
        let s = line.trim();
        if depth > 0 {
            depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
            continue;
        }
        if ["Geometry:", "Material:", "Deformer:", "Model: 1000,"]
            .iter()
            .any(|p| s.starts_with(p))
        {
            depth = line.matches('{').count() as i32 - line.matches('}').count() as i32;
            continue;
        }
        if s.starts_with("C:")
            && s.split(',')
                .skip(1)
                .take(2)
                .filter_map(|v| v.trim().parse::<u64>().ok())
                .any(|id| [100, 350, 500, 501, 502, 1000].contains(&id))
        {
            continue;
        }
        lines.push(line);
    }
    let motion_only = lines.join("\n");
    assert!(!motion_only.contains("Geometry:"));
    let motion = Motion::from_fbx(motion_only.as_bytes(), 0).unwrap();
    let names = motion.joint_names();
    let root = names.iter().position(|n| n == "Root").unwrap();
    let joint = names.iter().position(|n| n == "Joint").unwrap();
    let a = motion
        .retarget_3d(&target(), &[Some(root), Some(joint)], false)
        .unwrap();
    assert!(
        Quat::from_array(a.frames[0][1].rotation)
            .angle_between(Quat::from_array(a.frames.last().unwrap()[1].rotation))
            > 1.
    );
    let mut rig = rebonebase::model::Rig::new("2D FBX", 64, 64);
    let id = rig
        .add_bone(
            rebonebase::model::Vec2::new(20., 20.),
            rebonebase::model::Vec2::new(20., 10.),
            None,
        )
        .unwrap();
    rig.bone_mut(id).unwrap().name = "Joint".into();
    motion
        .retarget(&mut rig, rebonebase::motion::Projection::Front, 1.)
        .unwrap();
    assert!(
        rig.active_clip().sample(rig.active_clip().length as f32)[&id]
            .rotation
            .abs()
            > 45.
    );
}
#[test]
fn unity_invalid_data_and_unreal_binary_have_actionable_errors() {
    for text in [
        "\0Unity binary",
        "AnimationClip:\n  m_Compressed: 1\n",
        "AnimationClip:\n  m_MuscleClip: {}\n",
        &UNITY.replace("time: 1", "time: NaN"),
        &UNITY.replace("value: {x: 0, y: 0, z: 0, w: 1}", "value: *recursive"),
    ] {
        assert!(Motion::from_unity_anim(text).is_err());
    }
    let p = std::env::temp_dir().join(format!("rebone-motion-{}.uasset", std::process::id()));
    std::fs::write(&p, b"unreal").unwrap();
    let err = Motion::load(&p).err().unwrap().to_string();
    assert!(err.contains("Unreal") && err.contains("FBX"));
    std::fs::remove_file(p).unwrap();
}
#[test]
fn humanoid_names_match_unity_mixamo_and_manny() {
    let bvh = "HIERARCHY ROOT pelvis { OFFSET 0 0 0 CHANNELS 3 Xrotation Yrotation Zrotation JOINT upperarm_l { OFFSET 1 1 0 CHANNELS 3 Xrotation Yrotation Zrotation JOINT lowerarm_l { OFFSET 1 0 0 CHANNELS 3 Xrotation Yrotation Zrotation End Site { OFFSET 1 0 0 } } } } MOTION Frames: 1 Frame Time: 0.1 0 0 0 0 0 0 0 0 0";
    let m = Motion::from_bvh(bvh).unwrap();
    let mut t = target();
    t.nodes[0].name = "Hips".into();
    t.nodes[1].name = "mixamorig:LeftArm".into();
    t.nodes.push(Node3d {
        name: "LeftLowerArm".into(),
        parent: Some(1),
        transform: Transform3d::default(),
    });
    assert_eq!(m.mapping_3d(&t), vec![Some(0), Some(1), Some(2)]);
    let a = m.retarget_3d(&t, &m.mapping_3d(&t), false).unwrap();
    assert!(
        (Quat::from_array(a.frames[0][0].rotation) * Vec3::Y
            - Quat::from_array(t.nodes[0].transform.rotation) * Vec3::Y)
            .length()
            < 0.001
    );
}

#[test]
fn unity_empty_path_is_the_parent_of_all_relative_joint_paths() {
    let text = UNITY.replace(
        "path: hips\n  m_PositionCurves:",
        "path: \n  m_PositionCurves:",
    );
    let m = Motion::from_unity_anim(&text).unwrap();
    let names = m.joint_names();
    let hips = names.iter().position(|n| n == "hips").unwrap();
    // The animated root turns the hips->chest vector from up to left.
    let preview = m.preview(rebonebase::motion::Projection::Front, m.frame_count() - 1);
    assert!(
        preview
            .iter()
            .any(|(a, b)| (b.x - a.x) < -0.99 && (b.y - a.y).abs() < 0.01)
    );
    assert_eq!(hips, 1);
}

#[test]
fn bvh_bind_pose_is_offsets_not_first_animated_pose_and_unmapped_parents_are_kept() {
    let bvh = "HIERARCHY ROOT hips { OFFSET 0 0 0 CHANNELS 3 Xrotation Yrotation Zrotation JOINT chest { OFFSET 0 1 0 CHANNELS 3 Xrotation Yrotation Zrotation End Site { OFFSET 0 1 0 } } } MOTION Frames: 2 Frame Time: 0.1 0 0 90 0 0 0 0 0 180 0 0 0";
    let m = Motion::from_bvh(bvh).unwrap();
    let t = target();
    let a = m.retarget_3d(&t, &[Some(0), Some(1)], false).unwrap();
    let expected = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
        * Quat::from_array(t.nodes[0].transform.rotation);
    assert!(Quat::from_array(a.frames[0][0].rotation).angle_between(expected) < 0.001);
    assert!(
        Quat::from_array(a.frames[0][1].rotation)
            .angle_between(Quat::from_array(t.nodes[1].transform.rotation))
            < 0.001
    );
    let a = m.retarget_3d(&t, &[None, Some(1)], false).unwrap();
    assert_eq!(a.frames[0][0].rotation, t.nodes[0].transform.rotation);
}

#[test]
fn unity_euler_and_long_clip_resampling_preserve_duration() {
    let text = UNITY
        .replace("m_RotationCurves:", "m_EulerCurves:")
        .replace("{x: 0, y: 0, z: 0, w: 1}", "{x: 0, y: 0, z: 0}")
        .replace(
            "{x: 0, y: 0, z: 0.70710678, w: 0.70710678}",
            "{x: 0, y: 0, z: 90}",
        )
        .replace("time: 1", "time: 20");
    let m = Motion::from_unity_anim(&text).unwrap();
    assert_eq!(m.frame_count(), 240);
    assert!((m.duration() - 20.).abs() < 0.001);
    let a = m
        .retarget_3d(&target(), &[Some(0), Some(1)], false)
        .unwrap();
    let expected = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
        * Quat::from_array(target().nodes[0].transform.rotation);
    assert!(Quat::from_array(a.frames.last().unwrap()[0].rotation).angle_between(expected) < 0.001);
}

#[test]
fn unity_position_tangents_sample_cubic_and_stepped_curves() {
    use rebonebase::motion::Projection;
    let text=UNITY.replace("value: {x: 0, y: 0, z: 0}\n","value: {x: 0, y: 0, z: 0}\n        inSlope: {x: 0, y: 0, z: 0}\n        outSlope: {x: 0, y: 0, z: 0}\n").replace("value: {x: 1, y: 0, z: 0}\n","value: {x: 1, y: 0, z: 0}\n        inSlope: {x: 0, y: 0, z: 0}\n        outSlope: {x: 0, y: 0, z: 0}\n");
    let m = Motion::from_unity_anim(&text).unwrap();
    assert!((m.preview(Projection::Front, 6)[0].0.x - 0.104).abs() < 0.0001);
    let step = text.replace(
        "outSlope: {x: 0, y: 0, z: 0}",
        "outSlope: {x: .inf, y: .inf, z: .inf}",
    );
    let m = Motion::from_unity_anim(&step).unwrap();
    assert_eq!(m.preview(Projection::Front, 15)[0].0.x, 0.);
    assert!(
        Motion::from_unity_anim(&text.replace("outSlope:", "weightedMode: 1\n        outSlope:"))
            .is_err()
    );
}

#[test]
fn unity_file_dispatch_and_invalid_key_structure_are_bounded() {
    let p = std::env::temp_dir().join(format!("rebone-transform-{}.anim", std::process::id()));
    std::fs::write(&p, UNITY).unwrap();
    assert_eq!(Motion::animation_names(&p).unwrap(), vec!["Turn"]);
    assert_eq!(Motion::load(&p).unwrap().name, "Turn");
    assert!(Motion::load_animation(&p, 1).is_err());
    std::fs::remove_file(p).unwrap();
    for bad in [
        UNITY.replace("time: 1", "time: 0"),
        UNITY.replace("time: 1", "time: -1"),
        UNITY.replace("path: hips\n", "path: ../hips\n"),
        UNITY.replace(
            "value: {x: 0, y: 0, z: 0, w: 1}",
            "value: {x: 0, y: 0, z: 0, w: 0}",
        ),
        UNITY.replace("path: hips/chest", "path: *alias"),
    ] {
        assert!(Motion::from_unity_anim(&bad).is_err());
    }
}

#[test]
fn manny_spine_and_clavicle_map_to_unity_humanoid_roles() {
    let bvh = "HIERARCHY ROOT spine_01 { OFFSET 0 0 0 CHANNELS 3 Xrotation Yrotation Zrotation JOINT clavicle_l { OFFSET 1 1 0 CHANNELS 3 Xrotation Yrotation Zrotation JOINT spine_03 { OFFSET 0 1 0 CHANNELS 3 Xrotation Yrotation Zrotation End Site { OFFSET 0 1 0 } } } } MOTION Frames: 1 Frame Time: 0.1 0 0 0 0 0 0 0 0 0";
    let m = Motion::from_bvh(bvh).unwrap();
    let mut t = target();
    t.nodes[0].name = "Spine".into();
    t.nodes[1].name = "LeftShoulder".into();
    t.nodes.push(Node3d {
        name: "UpperChest".into(),
        parent: Some(0),
        transform: Transform3d::default(),
    });
    assert_eq!(m.mapping_3d(&t), vec![Some(0), Some(1), Some(2)]);
}

#[test]
fn gltf_motion_small_duration_and_percent_encoded_local_buffers_work() {
    use base64::Engine as _;
    let mut doc: serde_json::Value = serde_json::from_slice(
        &std::fs::read("src/character3d/fixtures/textured_skin.gltf").unwrap(),
    )
    .unwrap();
    let input = doc["animations"][0]["samplers"][0]["input"]
        .as_u64()
        .unwrap() as usize;
    let accessor = &doc["accessors"][input];
    let view = accessor["bufferView"].as_u64().unwrap() as usize;
    let offset = doc["bufferViews"][view]["byteOffset"].as_u64().unwrap_or(0) as usize
        + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
    let uri = doc["buffers"][0]["uri"].as_str().unwrap();
    let mut data = base64::engine::general_purpose::STANDARD
        .decode(uri.split_once(',').unwrap().1)
        .unwrap();
    data[offset + 4..offset + 8].copy_from_slice(&0.001f32.to_le_bytes());
    doc["buffers"][0]["uri"] = format!(
        "data:application/octet-stream;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&data)
    )
    .into();
    let m = Motion::from_gltf(&serde_json::to_vec(&doc).unwrap(), None).unwrap();
    assert!((m.duration() - 0.001).abs() < 0.00001);
    assert_eq!(m.frame_count(), 2);
    let dir = std::env::temp_dir().join(format!("rebone-motion-uri-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("motion data.bin"), data).unwrap();
    doc["buffers"][0]["uri"] = "motion%20data.bin".into();
    assert!(Motion::from_gltf(&serde_json::to_vec(&doc).unwrap(), Some(&dir)).is_ok());
    doc["buffers"][0]["uri"] = "%2e%2e/private.bin".into();
    assert!(Motion::from_gltf(&serde_json::to_vec(&doc).unwrap(), Some(&dir)).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unity_rotation_only_child_paths_drive_2d_fk() {
    use rebonebase::{
        model::{Rig, Vec2},
        motion::Projection,
    };
    let mut text = UNITY
        .split("  m_PositionCurves:")
        .next()
        .unwrap()
        .to_owned();
    text.push_str("  - curve:\n      m_Curve:\n      - time: 0\n        value: {x: 0, y: 0, z: 0, w: 1}\n      - time: 1\n        value: {x: 0, y: 0, z: 0, w: 1}\n    path: hips/chest\n");
    let m = Motion::from_unity_anim(&text).unwrap();
    let mut rig = Rig::new("Rotation only", 64, 64);
    let hips = rig
        .add_bone(Vec2::new(32., 40.), Vec2::new(32., 30.), None)
        .unwrap();
    rig.bone_mut(hips).unwrap().name = "hips".into();
    let chest = rig
        .add_bone(Vec2::new(32., 30.), Vec2::new(32., 20.), Some(hips))
        .unwrap();
    rig.bone_mut(chest).unwrap().name = "chest".into();
    m.retarget(&mut rig, Projection::Front, 1.).unwrap();
    assert!(
        rig.active_clip().sample(rig.active_clip().length as f32)[&hips]
            .rotation
            .abs()
            > 80.
    );
}
