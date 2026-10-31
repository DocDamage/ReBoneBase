use rebonebase::{
    model::{Rig, Vec2},
    motion::{Motion, Projection},
};

const BVH: &str = "HIERARCHY ROOT hips { OFFSET 0 0 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation JOINT chest { OFFSET 0 10 0 CHANNELS 3 Zrotation Xrotation Yrotation End Site { OFFSET 0 10 0 } } } MOTION Frames: 2 Frame Time: 0.1 0 0 0 0 0 0 0 0 0 2 0 0 90 0 0 0 0 0";
#[test]
fn bvh_projection_and_retarget_keys_are_visible_and_valid() {
    let motion = Motion::from_bvh(BVH).unwrap();
    assert_eq!(motion.frame_count(), 2);
    let mut rig = Rig::new("test", 64, 64);
    let hips = rig
        .add_bone(Vec2::new(32., 40.), Vec2::new(32., 30.), None)
        .unwrap();
    rig.bone_mut(hips).unwrap().name = "hips".into();
    let chest = rig
        .add_bone(Vec2::new(32., 30.), Vec2::new(32., 20.), Some(hips))
        .unwrap();
    rig.bone_mut(chest).unwrap().name = "chest".into();
    let report = motion.retarget(&mut rig, Projection::Front, 1.).unwrap();
    assert!(report.mapped >= 2);
    let pose = rig.active_clip().sample(2.);
    assert!(pose.get(&hips).unwrap().rotation.abs() > 80.);
    rig.validate().unwrap();
}
#[test]
fn malformed_truncated_and_hostile_bvh_fail_without_panicking() {
    for text in [
        "",
        "HIERARCHY ROOT x {",
        "HIERARCHY ROOT x { OFFSET NaN 0 0 CHANNELS 0 } MOTION Frames: 2 Frame Time: 0.1",
        &BVH[..BVH.len() - 12],
    ] {
        assert!(Motion::from_bvh(text).is_err());
    }
}
#[test]
fn gltf_embedded_animation_is_sampled_and_external_paths_are_rejected() {
    use base64::Engine as _;
    let values: [f32; 10] = [
        0.,
        1.,
        0.,
        0.,
        0.,
        1.,
        0.,
        0.,
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    ];
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let uri = format!(
        "data:application/octet-stream;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    );
    let data = serde_json::json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":40,"uri":uri}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":8},{"buffer":0,"byteOffset":8,"byteLength":32}],"accessors":[{"bufferView":0,"componentType":5126,"count":2,"type":"SCALAR","min":[0.],"max":[1.]},{"bufferView":1,"componentType":5126,"count":2,"type":"VEC4"}],"nodes":[{"name":"hips","children":[1]},{"name":"chest","translation":[0.,10.,0.],"children":[2]},{"name":"head","translation":[0.,10.,0.]}],"animations":[{"samplers":[{"input":0,"output":1,"interpolation":"LINEAR"}],"channels":[{"sampler":0,"target":{"node":0,"path":"rotation"}}]}]});
    let motion = Motion::from_gltf(&serde_json::to_vec(&data).unwrap(), None).unwrap();
    assert!(motion.frame_count() > 1);
    let mut bad = data;
    bad["buffers"][0]["uri"] = serde_json::json!("../private.bin");
    assert!(
        Motion::from_gltf(
            &serde_json::to_vec(&bad).unwrap(),
            Some(std::path::Path::new("."))
        )
        .is_err()
    );
}

fn multi_animation_gltf(reordered: bool) -> Vec<u8> {
    use base64::Engine as _;
    let q = std::f32::consts::FRAC_1_SQRT_2;
    let values: [f32; 18] = [
        0., 1., 0., 0., 0., 1., 0., 0., q, q, 0., 0., 0., 1., 0., 0., -q, q,
    ];
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let nodes = if reordered {
        serde_json::json!([{"name":"chest","translation":[0.,10.,0.],"children":[2]},{"name":"hips","children":[0]},{"name":"head","translation":[0.,10.,0.]}])
    } else {
        serde_json::json!([{"name":"hips","children":[1]},{"name":"chest","translation":[0.,10.,0.],"children":[2]},{"name":"head","translation":[0.,10.,0.]}])
    };
    let root = usize::from(reordered);
    serde_json::to_vec(&serde_json::json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":72,"uri":format!("data:application/octet-stream;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes))}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":8},{"buffer":0,"byteOffset":8,"byteLength":32},{"buffer":0,"byteOffset":40,"byteLength":32}],"accessors":[{"bufferView":0,"componentType":5126,"count":2,"type":"SCALAR","min":[0.],"max":[1.]},{"bufferView":1,"componentType":5126,"count":2,"type":"VEC4"},{"bufferView":2,"componentType":5126,"count":2,"type":"VEC4"}],"nodes":nodes,"animations":[{"name":"Wave","samplers":[{"input":0,"output":1}],"channels":[{"sampler":0,"target":{"node":root,"path":"rotation"}}]},{"name":"Bow","samplers":[{"input":0,"output":2}],"channels":[{"sampler":0,"target":{"node":root,"path":"rotation"}}]}]})).unwrap()
}
fn preview_target() -> Rig {
    let mut rig = Rig::new("Preview", 64, 64);
    let part = rig.add_part("Body", [210, 60, 30, 255]);
    let hips = rig
        .add_bone(Vec2::new(32., 40.), Vec2::new(32., 30.), None)
        .unwrap();
    rig.bone_mut(hips).unwrap().name = "hips".into();
    rig.bone_mut(hips).unwrap().part = part;
    let chest = rig
        .add_bone(Vec2::new(32., 30.), Vec2::new(32., 20.), Some(hips))
        .unwrap();
    rig.bone_mut(chest).unwrap().name = "chest".into();
    rig.bone_mut(chest).unwrap().part = part;
    rig
}

#[test]
fn named_gltf_animations_select_distinct_motion_and_reject_bad_indices() {
    let data = multi_animation_gltf(false);
    let first = Motion::from_gltf_animation(&data, None, 0).unwrap();
    let second = Motion::from_gltf_animation(&data, None, 1).unwrap();
    assert_eq!(first.name, "Wave");
    assert_eq!(second.name, "Bow");
    assert!(Motion::from_gltf_animation(&data, None, 2).is_err());
    let mut a = preview_target();
    let mut b = a.clone();
    first.retarget(&mut a, Projection::Front, 1.).unwrap();
    second.retarget(&mut b, Projection::Front, 1.).unwrap();
    let root = a.bones[0].id;
    let f = a.active_clip().length as f32;
    assert!(
        (a.active_clip().sample(f)[&root].rotation - b.active_clip().sample(f)[&root].rotation)
            .abs()
            > 100.
    );
    let path = std::env::temp_dir().join(format!("rebone-animations-{}.gltf", std::process::id()));
    std::fs::write(&path, &data).unwrap();
    assert_eq!(Motion::animation_names(&path).unwrap(), vec!["Wave", "Bow"]);
    assert!(
        Motion::load_animation(&path, 1)
            .unwrap()
            .name
            .contains("Bow")
    );
    assert!(Motion::load_animation(&path, 9).is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn retarget_profiles_resolve_names_after_source_nodes_are_reordered() {
    use rebonebase::motion::RetargetProfile;
    let rig = preview_target();
    let first = Motion::from_gltf_animation(&multi_animation_gltf(false), None, 0).unwrap();
    let reordered = Motion::from_gltf_animation(&multi_animation_gltf(true), None, 0).unwrap();
    let profile = RetargetProfile::from_mapping(
        &rig,
        &first,
        "Humanoid",
        Projection::Side,
        2.,
        &first.mapping(&rig),
    )
    .unwrap();
    let profile: RetargetProfile =
        serde_json::from_str(&serde_json::to_string(&profile).unwrap()).unwrap();
    let map = profile.resolve(&rig, &reordered);
    let names = reordered.joint_names();
    for (bone, mapped) in rig.bones.iter().zip(map) {
        assert_eq!(bone.name, names[mapped.unwrap()]);
    }
    assert_eq!(profile.projection, Projection::Side);
    assert_eq!(profile.scale, 2.);
    assert!(
        RetargetProfile::from_mapping(
            &rig,
            &first,
            "Bad",
            Projection::Front,
            f32::NAN,
            &first.mapping(&rig)
        )
        .is_err()
    );
}

#[test]
fn actual_mannequin_preview_matches_accepted_import_without_mutating_project() {
    let motion = Motion::from_gltf_animation(&multi_animation_gltf(false), None, 1).unwrap();
    let rig = preview_target();
    let before = rig.clone();
    let map = motion.mapping(&rig);
    let frame = motion.frame_count() - 1;
    let preview = motion
        .preview_rig(&rig, Projection::Front, 1., &map, frame)
        .unwrap();
    assert!(preview.pixels().any(|p| p[3] > 0));
    assert_eq!(rig, before);
    let mut accepted = rig.clone();
    motion
        .retarget_mapped(&mut accepted, Projection::Front, 1., &map)
        .unwrap();
    assert_eq!(
        preview,
        rebonebase::skin::render(
            &accepted,
            Some(&accepted.active_clip().sample((frame + 1) as f32))
        )
    );
    assert!(
        motion
            .preview_rig(&rig, Projection::Front, 1., &map, usize::MAX)
            .is_err()
    );
    assert_eq!(rig, before);
}

#[test]
fn saved_profiles_do_not_remap_ambiguous_or_missing_target_names() {
    use rebonebase::motion::RetargetProfile;
    let motion = Motion::from_gltf_animation(&multi_animation_gltf(false), None, 0).unwrap();
    let mut rig = preview_target();
    let profile = RetargetProfile::from_mapping(
        &rig,
        &motion,
        "Profile",
        Projection::Front,
        1.,
        &motion.mapping(&rig),
    )
    .unwrap();
    rig.bones[1].name = "hips".into();
    assert!(
        RetargetProfile::from_mapping(
            &rig,
            &motion,
            "Ambiguous",
            Projection::Front,
            1.,
            &[Some(0), None]
        )
        .is_err()
    );
    assert!(profile.resolve(&rig, &motion).iter().all(Option::is_none));
}

#[test]
fn bvh_and_static_gltf_expose_one_selectable_motion() {
    let path = std::env::temp_dir().join(format!("rebone-selection-{}.bvh", std::process::id()));
    std::fs::write(&path, BVH).unwrap();
    assert_eq!(Motion::animation_names(&path).unwrap().len(), 1);
    assert!(Motion::load_animation(&path, 0).is_ok());
    assert!(Motion::load_animation(&path, 1).is_err());
    std::fs::remove_file(path).unwrap();
    let data = br#"{"asset":{"version":"2.0"},"nodes":[{"name":"hips"}]}"#;
    assert_eq!(
        Motion::from_gltf_animation(data, None, 0)
            .unwrap()
            .frame_count(),
        1
    );
    assert!(Motion::from_gltf_animation(data, None, 1).is_err());
}
