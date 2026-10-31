use rebonebase::{
    animation::Pose,
    model::{Rig, Vec2},
    production::{self, MeshKey, SpriteAsset},
    project::{Project, Reference},
};
use std::sync::Arc;

fn fixture(columns: u32, alpha: u8) -> Project {
    let mut rig = Rig::new("Mesh bind", 32, 32);
    let root = rig
        .add_bone(Vec2::new(4., 10.), Vec2::new(10., 10.), None)
        .unwrap();
    let mut p = Project::new(rig);
    p.extras.production.assets.push(SpriteAsset {
        id: 1,
        name: "Artwork".into(),
        image: Reference {
            width: 2,
            height: 2,
            pixels: Arc::new([20, 70, 220, alpha].repeat(4)),
            x: 0.,
            y: 0.,
            scale: 1.,
            opacity: 1.,
        },
    });
    p.extras.production.meshes.push(
        production::grid_mesh(
            &p.rig,
            2,
            1,
            Vec2::new(4., 8.),
            Vec2::new(4., 4.),
            columns,
            columns,
            &[root],
        )
        .unwrap(),
    );
    p
}
fn close(a: Vec2, b: Vec2) {
    assert!((a - b).length() < 0.001, "{a:?} != {b:?}");
}
#[test]
fn grown_canvas_subtracts_its_origin_once_after_near_normalized_mesh_weights() {
    let mut p = fixture(1, 255);
    for v in &mut p.extras.production.meshes[0].vertices {
        v.weights[0].weight = 0.9995;
    }
    p.rig.bones[0].x -= 4000.;
    p.extras.canvas_origin = Vec2::new(-4000., 0.);
    p.validate().unwrap();
    let image = p.render_at(1.);
    assert_eq!(image.get_pixel(9, 10).0, [20, 70, 220, 255]);
    assert_eq!(image.get_pixel(4, 10).0, [0; 4]);
}

#[test]
fn changing_the_rest_root_translates_bound_artwork_and_preserves_the_pose_pivot() {
    let mut p = fixture(1, 255);
    let root = p.rig.bones[0].id;
    let mesh = &p.extras.production.meshes[0];
    let original = production::mesh_vertices(&p.rig, None, mesh, 1.);
    let bind_head = p.rig.world(None)[0].head;
    p.rig.bone_mut(root).unwrap().x += 5.;
    p.rig.bone_mut(root).unwrap().y -= 2.;
    for (a, b) in production::mesh_vertices(&p.rig, None, mesh, 1.)
        .iter()
        .zip(&original)
    {
        close(*a, *b + Vec2::new(5., -2.));
    }
    let mut pose = Pose::new();
    pose.entry(root).or_default().rotation = 90.;
    let head = p.rig.world(Some(&pose))[0].head;
    for (a, b) in production::mesh_vertices(&p.rig, Some(&pose), mesh, 1.)
        .iter()
        .zip(&original)
    {
        close(*a, head + (*b - bind_head).rotate(90.));
    }
}

#[test]
fn grid_triangle_edges_composite_translucent_pixels_exactly_once() {
    for columns in [1, 2, 4] {
        for alpha in [128, 255] {
            let p = fixture(columns, alpha);
            let image = p.render_at(1.);
            for y in 0..32 {
                for x in 0..32 {
                    let expected = if (4..8).contains(&x) && (8..12).contains(&y) {
                        [20, 70, 220, alpha]
                    } else {
                        [0; 4]
                    };
                    assert_eq!(
                        image.get_pixel(x, y).0,
                        expected,
                        "grid {columns}, alpha {alpha}, pixel ({x},{y})"
                    );
                }
            }
        }
    }
}

#[test]
fn independent_translucent_meshes_still_composite_over_each_other() {
    let mut p = fixture(1, 128);
    let mut second = p.extras.production.meshes[0].clone();
    second.id = 3;
    p.extras.production.meshes.push(second);
    let image = p.render_at(1.);
    for y in 8..12 {
        for x in 4..8 {
            assert_eq!(image.get_pixel(x, y)[3], 192);
        }
    }
}

#[test]
fn two_bone_weights_use_original_bind_transforms_after_rest_length_and_position_edits() {
    let mut rig = Rig::new("Two bindings", 32, 32);
    let a = rig
        .add_bone(Vec2::new(4., 4.), Vec2::new(10., 4.), None)
        .unwrap();
    let b = rig
        .add_bone(Vec2::new(4., 10.), Vec2::new(10., 10.), None)
        .unwrap();
    let original = rig.world(None);
    let mesh = production::grid_mesh(
        &rig,
        2,
        1,
        Vec2::new(4., 5.),
        Vec2::new(4., 4.),
        1,
        1,
        &[a, b],
    )
    .unwrap();
    assert!(mesh.vertices.iter().all(|v| v.weights.len() == 2));
    rig.bone_mut(b).unwrap().x += 5.;
    rig.bone_mut(b).unwrap().length *= 2.;
    let mut pose = Pose::new();
    pose.entry(a).or_default().rotation = 90.;
    let world = rig.world(Some(&pose));
    let deformed = production::mesh_vertices(&rig, Some(&pose), &mesh, 1.);
    for (v, actual) in mesh.vertices.iter().zip(deformed) {
        let mut expected = Vec2::default();
        for w in &v.weights {
            let from = original.iter().find(|v| v.id == w.bone).unwrap();
            let to = world.iter().find(|v| v.id == w.bone).unwrap();
            let ratio = (to.tail - to.head).length() / (from.tail - from.head).length();
            expected = expected
                + (to.head + ((v.rest - from.head).rotate(-from.angle) * ratio).rotate(to.angle))
                    * w.weight;
        }
        close(actual, expected);
    }
}

#[test]
fn deformation_keys_interpolate_after_skinning_and_disable_for_other_clips() {
    let mut p = fixture(2, 255);
    let id = p.rig.bones[0].id;
    let mesh = &mut p.extras.production.meshes[0];
    let count = mesh.vertices.len();
    mesh.keys = vec![
        MeshKey {
            frame: 1.,
            offsets: vec![Vec2::default(); count],
        },
        MeshKey {
            frame: 3.,
            offsets: vec![Vec2::new(2., 4.); count],
        },
    ];
    let mut pose = Pose::new();
    pose.entry(id).or_default().rotation = 90.;
    let base = production::mesh_vertices(&p.rig, Some(&pose), mesh, 0.);
    let middle = production::mesh_vertices(&p.rig, Some(&pose), mesh, 2.);
    let held = production::mesh_vertices(&p.rig, Some(&pose), mesh, 8.);
    for ((base, middle), held) in base.into_iter().zip(middle).zip(held) {
        close(middle, base + Vec2::new(1., 2.));
        close(held, base + Vec2::new(2., 4.));
    }
    p.validate().unwrap();
}

#[test]
fn malformed_grid_inputs_are_rejected_before_creating_an_invalid_mesh() {
    let p = fixture(1, 255);
    let id = p.rig.bones[0].id;
    assert!(
        production::grid_mesh(
            &p.rig,
            2,
            1,
            Vec2::new(100000., 100000.),
            Vec2::new(1., 1.),
            1,
            1,
            &[id]
        )
        .is_err()
    );
    assert!(
        production::grid_mesh(
            &p.rig,
            2,
            1,
            Vec2::default(),
            Vec2::new(1., 1.),
            0,
            1,
            &[id]
        )
        .is_err()
    );
    assert!(
        production::grid_mesh(
            &p.rig,
            2,
            1,
            Vec2::default(),
            Vec2::new(1., 1.),
            1,
            1,
            &[999]
        )
        .is_err()
    );
}

#[test]
fn checkerboard_uvs_are_continuous_across_grid_seams_and_reversed_winding() {
    for columns in [1, 2, 4] {
        let mut p = fixture(columns, 128);
        let colors = [
            [255, 0, 0, 128],
            [0, 255, 0, 128],
            [0, 0, 255, 128],
            [255, 255, 0, 128],
        ];
        p.extras.production.assets[0].image.pixels = Arc::new(colors.concat());
        for reverse in [false, true] {
            if reverse {
                for t in &mut p.extras.production.meshes[0].triangles {
                    t.swap(1, 2);
                }
            }
            let image = p.render_at(1.);
            for y in 8..12 {
                for x in 4..8 {
                    let index = ((y - 8) / 2 * 2 + (x - 4) / 2) as usize;
                    assert_eq!(
                        image.get_pixel(x, y).0,
                        colors[index],
                        "grid {columns}, reverse {reverse}, pixel ({x},{y})"
                    );
                }
            }
        }
    }
}

#[test]
fn old_projects_without_bind_snapshots_load_and_keep_legacy_rest_fallback() {
    let p = fixture(1, 255);
    let mut value = serde_json::to_value(&p).unwrap();
    value["extras"]["production"]["meshes"][0]
        .as_object_mut()
        .unwrap()
        .remove("bind");
    let old: Project = serde_json::from_value(value).unwrap();
    old.validate().unwrap();
    let m = &old.extras.production.meshes[0];
    assert!(m.bind.is_empty());
    let points = production::mesh_vertices(&old.rig, None, m, 1.);
    for (a, b) in points.iter().zip(&m.vertices) {
        close(*a, b.rest);
    }
    let roundtrip: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(p, roundtrip);
    assert!(!roundtrip.extras.production.meshes[0].bind.is_empty());
}

#[test]
fn binding_snapshots_reject_missing_references_partial_maps_and_nonfinite_data() {
    let p = fixture(1, 255);
    let id = p.rig.bones[0].id;
    for invalid in [
        production::BindTransform {
            head: Vec2::new(f32::INFINITY, 0.),
            angle: 0.,
            length: 6.,
        },
        production::BindTransform {
            head: Vec2::default(),
            angle: f32::NAN,
            length: 6.,
        },
        production::BindTransform {
            head: Vec2::default(),
            angle: 0.,
            length: 0.,
        },
    ] {
        let mut bad = p.clone();
        bad.extras.production.meshes[0].bind.insert(id, invalid);
        assert!(bad.validate().is_err());
    }
    let mut bad = p.clone();
    let binding = bad.extras.production.meshes[0].bind[&id];
    bad.extras.production.meshes[0].bind.insert(999, binding);
    assert!(bad.validate().is_err());
    let mut two = p.clone();
    let other = two
        .rig
        .add_bone(Vec2::new(2., 2.), Vec2::new(5., 2.), None)
        .unwrap();
    two.extras.production.meshes[0].vertices[0].weights = vec![production::Weight {
        bone: other,
        weight: 1.,
    }];
    assert!(two.validate().is_err());
}

#[test]
fn grid_binding_deduplicates_input_bones_and_is_repeatable() {
    let p = fixture(1, 255);
    let id = p.rig.bones[0].id;
    let mesh = production::grid_mesh(
        &p.rig,
        2,
        1,
        Vec2::new(4., 8.),
        Vec2::new(4., 4.),
        1,
        1,
        &[id, id],
    )
    .unwrap();
    assert_eq!(mesh, p.extras.production.meshes[0]);
    assert!(mesh.vertices.iter().all(|v| v.weights.len() == 1));
}
