use base64::{Engine as _, engine::general_purpose::STANDARD};
use rebonebase::{
    character3d::{self, Character},
    model::Vec2,
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/character3d/fixtures")
        .join(name)
}
fn setup(mut c: Character) -> Character {
    c.position = Vec2::new(16., 28.);
    c.scale = 10.;
    c.looped = false;
    c
}

#[test]
fn textured_gltf_and_glb_render_and_embed_without_external_runtime_dependencies() {
    for file in ["textured_skin.gltf", "textured_skin.glb"] {
        let c = setup(character3d::load(&fixture(file)).unwrap());
        c.validate().unwrap();
        assert_eq!(c.model.triangle_count(), 2);
        assert_eq!(c.model.animations[0].name, "Arm turn");
        let image = c.render(32, 32, Vec2::default(), 0.).unwrap();
        assert!(image.pixels().any(|p| p.0 == [255, 0, 0, 255]));
        assert!(image.pixels().any(|p| p.0 == [0, 255, 0, 255]));
        assert!(image.pixels().any(|p| p.0 == [0, 0, 255, 255]));
        assert!(c.embedded_bytes() > 100);
        let cloned = c.clone();
        assert!(Arc::ptr_eq(&c.model, &cloned.model));
        let restored: Character =
            serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(c, restored);
        assert_eq!(image, restored.render(32, 32, Vec2::default(), 0.).unwrap());
    }
}

#[test]
fn native_obj_mtl_keeps_texture_coordinates_and_materials() {
    let c = setup(character3d::load(&fixture("textured.obj")).unwrap());
    assert_eq!(c.model.triangle_count(), 2);
    let image = c.render(32, 32, Vec2::default(), 0.).unwrap();
    assert!(image.pixels().any(|p| p.0 == [255, 0, 0, 255]));
    assert!(image.pixels().any(|p| p.0 == [0, 255, 0, 255]));
    c.validate().unwrap();
}

#[test]
fn native_fbx_skin_and_authored_animation_are_imported_and_evaluated() {
    let c = setup(character3d::load(&fixture("skinned_ascii.fbx")).unwrap());
    assert_eq!(c.model.triangle_count(), 2);
    assert!(!c.model.animations.is_empty());
    let first = c.render(32, 32, Vec2::default(), 0.).unwrap();
    let next = c.render(32, 32, Vec2::default(), 750.).unwrap();
    assert!(first.pixels().any(|p| p[3] > 0));
    assert_ne!(first, next);
    c.validate().unwrap();
}

#[test]
fn animated_and_manual_joint_rotations_keep_joint_heads_in_the_hierarchy() {
    let mut c = setup(character3d::load(&fixture("textured_skin.glb")).unwrap());
    let first = c.render(32, 32, Vec2::default(), 0.).unwrap();
    assert_ne!(first, c.render(32, 32, Vec2::default(), 750.).unwrap());
    c.animation = None;
    let index = c
        .model
        .nodes
        .iter()
        .position(|n| n.name == "Joint")
        .unwrap();
    let heads = c.joint_positions(0.).unwrap();
    c.joints.insert(index, [0., 0., 90.]);
    let posed = c.joint_positions(0.).unwrap();
    assert!((heads[index] - posed[index]).length() < 0.001);
    assert!((heads[0] - posed[0]).length() < 0.001);
    assert_ne!(first, c.render(32, 32, Vec2::default(), 0.).unwrap());
}

fn depth_scene(reverse: bool) -> Vec<u8> {
    let positions: [f32; 18] = [
        -1., 0., 1., 1., 0., 1., 0., 2., 1., -1., 0., 0., 1., 0., 0., 0., 2., 0.,
    ];
    let data: Vec<_> = positions.into_iter().flat_map(f32::to_le_bytes).collect();
    let primitives = if reverse { vec![1, 0] } else { vec![0, 1] }
        .into_iter()
        .map(|i| serde_json::json!({"attributes":{"POSITION":i},"material":i}))
        .collect::<Vec<_>>();
    serde_json::to_vec(&serde_json::json!({"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],"buffers":[{"byteLength":data.len(),"uri":format!("data:application/octet-stream;base64,{}",STANDARD.encode(data))}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[-1,0,1],"max":[1,2,1]},{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3","min":[-1,0,0],"max":[1,2,0]}],"meshes":[{"primitives":primitives}],"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[1,0,0,1]}},{"pbrMetallicRoughness":{"baseColorFactor":[0,0,1,1]}}]})).unwrap()
}
#[test]
fn z_buffer_keeps_front_triangles_independent_of_mesh_order() {
    let a = setup(character3d::from_gltf(&depth_scene(false), None).unwrap())
        .render(32, 32, Vec2::default(), 0.)
        .unwrap();
    let b = setup(character3d::from_gltf(&depth_scene(true), None).unwrap())
        .render(32, 32, Vec2::default(), 0.)
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.get_pixel(16, 18).0, [255, 0, 0, 255]);
}

#[test]
fn origin_camera_and_bounds_use_canvas_world_coordinates() {
    let mut c = setup(character3d::load(&fixture("textured_skin.glb")).unwrap());
    let original = c.render(32, 32, Vec2::default(), 0.).unwrap();
    let shifted = c.render(32, 32, Vec2::new(-4., -3.), 0.).unwrap();
    for y in 0..29 {
        for x in 0..28 {
            assert_eq!(original.get_pixel(x, y), shifted.get_pixel(x + 4, y + 3));
        }
    }
    let (a, b) = c.bounds(0.).unwrap();
    c.yaw = 60.;
    let (corner, end) = c.bounds(0.).unwrap();
    assert!(end.x - corner.x < b.x - a.x);
    assert!(c.render_work(32, 32) > 0);
    c.validate_render(32, 32).unwrap();
    assert!(c.validate_render(100000, 100000).is_err());
}

#[test]
fn malformed_assets_and_external_resource_escapes_are_rejected() {
    assert!(character3d::from_gltf(b"{}", None).is_err());
    assert!(character3d::from_fbx(b"not FBX", None).is_err());
    assert!(character3d::from_obj(b"v 0 0 0\nf 99 98 97", None).is_err());
    let mut doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture("textured_skin.gltf")).unwrap()).unwrap();
    for path in [
        "../outside.bin",
        "https://example.invalid/secret.bin",
        "C:/private/data.bin",
    ] {
        doc["buffers"][0]["uri"] = path.into();
        assert!(
            character3d::from_gltf(
                &serde_json::to_vec(&doc).unwrap(),
                fixture("textured_skin.gltf").parent()
            )
            .is_err()
        );
    }
    assert!(
        character3d::from_obj(
            b"mtllib ../private.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3",
            fixture("textured.obj").parent()
        )
        .is_err()
    );
    let mut c = setup(character3d::load(&fixture("textured_skin.glb")).unwrap());
    c.joints.insert(999, [0., 0., 0.]);
    assert!(c.validate().is_err());
    c.joints.clear();
    c.scale = f32::NAN;
    assert!(c.render(32, 32, Vec2::default(), 0.).is_err());
}

#[test]
fn normalized_model_rejects_overflowing_quaternions_and_interpolated_scale_chains() {
    use rebonebase::character3d::{Animation3d, Node3d, Transform3d};
    let mut c = Character::default();
    Arc::make_mut(&mut c.model).nodes[0].transform.rotation = [1e30; 4];
    assert!(c.validate().is_err());
    let mut c = Character::default();
    let m = Arc::make_mut(&mut c.model);
    for i in 1..17 {
        m.nodes.push(Node3d {
            name: format!("Joint{i}"),
            parent: Some(i - 1),
            transform: Transform3d::default(),
        });
    }
    m.primitives[0].node = 16;
    let frames = (0..2)
        .map(|frame| {
            (0..17)
                .map(|i| Transform3d {
                    scale: [if (i + frame) % 2 == 0 { 1000. } else { 0.001 }; 3],
                    ..Default::default()
                })
                .collect()
        })
        .collect();
    m.animations.push(Animation3d {
        name: "Scale overflow between safe endpoints".into(),
        duration: 1.,
        times: vec![0., 1.],
        frames,
    });
    assert!(c.validate().is_err());
}

#[test]
fn transparent_shared_edges_composite_once_and_alpha_mask_uses_cutoff() {
    use rebonebase::character3d::AlphaMode;
    let mut c = setup(character3d::load(&fixture("textured_skin.glb")).unwrap());
    c.animation = None;
    let m = Arc::make_mut(&mut c.model);
    m.primitives[0].positions[2][1] = 1.;
    m.primitives[0].positions[3][1] = 1.;
    m.primitives[0].material.alpha = AlphaMode::Blend;
    m.textures[0].pixels = [20, 70, 220, 128].repeat(4);
    let image = c.render(32, 32, Vec2::default(), 0.).unwrap();
    for y in 18..28 {
        for x in 11..21 {
            assert_eq!(
                image.get_pixel(x, y).0,
                [20, 70, 220, 128],
                "shared edge at({x},{y})"
            );
        }
    }
    Arc::make_mut(&mut c.model).primitives[0].material.alpha = AlphaMode::Mask;
    Arc::make_mut(&mut c.model).primitives[0].material.cutoff = 0.6;
    assert!(
        c.render(32, 32, Vec2::default(), 0.)
            .unwrap()
            .pixels()
            .all(|p| p[3] == 0)
    );
    Arc::make_mut(&mut c.model).primitives[0].material.cutoff = 0.4;
    assert_eq!(
        c.render(32, 32, Vec2::default(), 0.)
            .unwrap()
            .get_pixel(16, 22)[3],
        255
    );
}

#[test]
fn hidden_models_have_no_render_work_and_large_finite_timeline_values_stay_safe() {
    let mut c = setup(character3d::load(&fixture("textured_skin.glb")).unwrap());
    c.looped = true;
    assert!(c.render(32, 32, Vec2::default(), f64::MAX).is_ok());
    c.enabled = false;
    assert_eq!(c.render_work(4096, 4096), 0);
    c.validate_render(4096, 4096).unwrap();
    assert!(c.bounds(0.).is_none());
    assert!(
        c.render(32, 32, Vec2::default(), 0.)
            .unwrap()
            .pixels()
            .all(|p| p[3] == 0)
    );
}

#[test]
fn gltf_percent_encoded_local_names_work_but_encoded_traversal_does_not() {
    let dir = std::env::temp_dir().join(format!("rebone-gltf-local-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture("textured_skin.gltf")).unwrap()).unwrap();
    let uri = doc["buffers"][0]["uri"].as_str().unwrap();
    let bytes = STANDARD.decode(uri.split_once(',').unwrap().1).unwrap();
    std::fs::write(dir.join("mesh data.bin"), bytes).unwrap();
    doc["buffers"][0]["uri"] = "mesh%20data.bin".into();
    assert!(character3d::from_gltf(&serde_json::to_vec(&doc).unwrap(), Some(&dir)).is_ok());
    doc["buffers"][0]["uri"] = "%2e%2e/mesh%20data.bin".into();
    assert!(character3d::from_gltf(&serde_json::to_vec(&doc).unwrap(), Some(&dir)).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn extra_gltf_skin_influences_are_reported_once() {
    let mut doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture("textured_skin.gltf")).unwrap()).unwrap();
    let attrs = &mut doc["meshes"][0]["primitives"][0]["attributes"];
    attrs["JOINTS_1"] = attrs["JOINTS_0"].clone();
    attrs["WEIGHTS_1"] = attrs["WEIGHTS_0"].clone();
    let c = character3d::from_gltf(
        &serde_json::to_vec(&doc).unwrap(),
        fixture("textured_skin.gltf").parent(),
    )
    .unwrap();
    assert_eq!(
        c.model
            .warnings
            .iter()
            .filter(|w| w.contains("four") && w.contains("influences"))
            .count(),
        1
    );
}
