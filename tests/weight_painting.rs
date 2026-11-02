use rebonebase::{
    model::{Rig, Vec2},
    production::{
        self, Mesh,
        weights::{self, Brush, BrushMode, Falloff, RebindMode},
    },
};

fn rig_and_mesh() -> (Rig, Mesh, u32, u32) {
    let mut rig = Rig::new("Weights", 64, 64);
    let a = rig
        .add_bone(Vec2::new(4., 4.), Vec2::new(20., 4.), None)
        .unwrap();
    let b = rig
        .add_bone(Vec2::new(4., 30.), Vec2::new(20., 30.), None)
        .unwrap();
    let mesh = production::grid_mesh(
        &rig,
        1,
        1,
        Vec2::new(4., 4.),
        Vec2::new(16., 26.),
        2,
        2,
        &[a],
    )
    .unwrap();
    (rig, mesh, a, b)
}

fn assert_normalized(mesh: &Mesh) {
    for v in &mesh.vertices {
        let sum: f32 = v.weights.iter().map(|w| w.weight).sum();
        assert!((sum - 1.).abs() < 0.0001, "{:?}", v.weights);
        assert!(v.weights.len() <= weights::MAX_INFLUENCES);
        assert!(v.weights.iter().all(|w| w.weight > 0.));
    }
}

fn rest_points(rig: &Rig, mesh: &Mesh) -> Vec<Vec2> {
    production::mesh_vertices(rig, None, mesh, 0.)
}

#[test]
fn brush_falloff_shapes_strength_inside_the_radius_only() {
    let mut brush = Brush {
        mode: BrushMode::Add,
        radius: 10.,
        strength: 0.5,
        falloff: Falloff::Constant,
    };
    assert_eq!(brush.factor(0.), 0.5);
    assert_eq!(brush.factor(9.9), 0.5);
    assert_eq!(brush.factor(10.1), 0.);
    brush.falloff = Falloff::Linear;
    assert!((brush.factor(5.) - 0.25).abs() < 1e-6);
    brush.falloff = Falloff::Smooth;
    assert!((brush.factor(5.) - 0.25).abs() < 1e-6);
    assert!(brush.factor(2.) > 0.4 && brush.factor(8.) < 0.1);
    assert_eq!(brush.factor(f32::NAN), 0.);
}

#[test]
fn add_and_subtract_paint_a_new_influence_and_keep_vertices_normalized() {
    let (rig, mut mesh, a, b) = rig_and_mesh();
    let rest = rig.world(None);
    let points = rest_points(&rig, &mesh);
    let brush = Brush {
        mode: BrushMode::Add,
        radius: 3.,
        strength: 0.4,
        falloff: Falloff::Constant,
    };
    // Bottom-left vertex sits at (4, 30), on bone b.
    let changed =
        weights::paint(&mut mesh, &rest, &points, Vec2::new(4., 30.), &brush, b, 4).unwrap();
    assert_eq!(changed, 1);
    let v = &mesh.vertices[6];
    assert!((weights::weight_of(v, b) - 0.4).abs() < 1e-5);
    assert!((weights::weight_of(v, a) - 0.6).abs() < 1e-5);
    // The new influence gets a bind snapshot so the mesh stays valid.
    assert!(mesh.bind.contains_key(&b));
    assert_normalized(&mesh);
    let subtract = Brush {
        mode: BrushMode::Subtract,
        strength: 1.,
        ..brush
    };
    weights::paint(
        &mut mesh,
        &rest,
        &points,
        Vec2::new(4., 30.),
        &subtract,
        b,
        4,
    )
    .unwrap();
    assert_eq!(mesh.vertices[6].weights.len(), 1);
    assert_eq!(weights::weight_of(&mesh.vertices[6], a), 1.);
    // Subtracting the only influence has nowhere to send the weight.
    let before = mesh.clone();
    weights::paint(
        &mut mesh,
        &rest,
        &points,
        Vec2::new(4., 30.),
        &subtract,
        a,
        4,
    )
    .unwrap();
    assert_eq!(mesh, before);
}

#[test]
fn locked_influences_keep_their_weight_while_others_are_painted() {
    let (rig, mut mesh, a, b) = rig_and_mesh();
    let rest = rig.world(None);
    let mut third = rig.clone();
    let c = third
        .add_bone(Vec2::new(30., 4.), Vec2::new(40., 4.), None)
        .unwrap();
    let rest3 = third.world(None);
    weights::assign(&mut mesh, &rest, &[0], b, 0.3, 4).unwrap();
    mesh.locked.push(b);
    let points = rest_points(&rig, &mesh);
    let brush = Brush {
        mode: BrushMode::Add,
        radius: 1.,
        strength: 1.,
        falloff: Falloff::Constant,
    };
    weights::paint(&mut mesh, &rest3, &points, points[0], &brush, c, 4).unwrap();
    let v = &mesh.vertices[0];
    assert!((weights::weight_of(v, b) - 0.3).abs() < 1e-5);
    assert!((weights::weight_of(v, c) - 0.7).abs() < 1e-5);
    assert_eq!(weights::weight_of(v, a), 0.);
    // A locked bone cannot be painted or assigned.
    let before = mesh.clone();
    weights::paint(&mut mesh, &rest3, &points, points[0], &brush, b, 4).unwrap();
    assert_eq!(mesh, before);
    assert!(weights::assign(&mut mesh, &rest3, &[0], b, 1., 4).is_err());
    assert_normalized(&mesh);
}

#[test]
fn smoothing_blends_toward_neighbors_and_the_limit_drops_small_influences() {
    let (rig, mut mesh, a, b) = rig_and_mesh();
    let rest = rig.world(None);
    // Middle row (vertices 3..6) fully on b; others on a.
    weights::assign(&mut mesh, &rest, &[3, 4, 5], b, 1., 4).unwrap();
    let points = rest_points(&rig, &mesh);
    let brush = Brush {
        mode: BrushMode::Smooth,
        radius: 1.,
        strength: 1.,
        falloff: Falloff::Constant,
    };
    weights::paint(&mut mesh, &rest, &points, points[4], &brush, a, 4).unwrap();
    let center = &mesh.vertices[4];
    let wa = weights::weight_of(center, a);
    assert!(wa > 0.2 && wa < 0.8, "{wa}");
    assert_normalized(&mesh);
    // Only the brushed vertex changed.
    assert_eq!(weights::weight_of(&mesh.vertices[3], b), 1.);
    weights::normalize(&mut mesh, 1);
    assert_eq!(mesh.vertices[4].weights.len(), 1);
    assert_normalized(&mesh);
}

#[test]
fn bulk_assignment_sets_every_selected_vertex_once() {
    let (rig, mut mesh, a, b) = rig_and_mesh();
    let rest = rig.world(None);
    let changed = weights::assign(&mut mesh, &rest, &[0, 1, 1, 2], b, 0.25, 4).unwrap();
    assert_eq!(changed, 3);
    for i in 0..3 {
        assert!((weights::weight_of(&mesh.vertices[i], b) - 0.25).abs() < 1e-5);
        assert!((weights::weight_of(&mesh.vertices[i], a) - 0.75).abs() < 1e-5);
    }
    assert_eq!(weights::weight_of(&mesh.vertices[3], b), 0.);
    assert!(weights::assign(&mut mesh, &rest, &[99], b, 1., 4).is_err());
    assert_eq!(weights::influences(&mesh), vec![a, b]);
}

#[test]
fn normalize_repairs_drift_and_applies_the_influence_limit() {
    let (rig, mut mesh, _a, b) = rig_and_mesh();
    let rest = rig.world(None);
    weights::assign(&mut mesh, &rest, &[0], b, 0.4, 4).unwrap();
    for w in &mut mesh.vertices[0].weights {
        w.weight *= 3.;
    }
    assert!(weights::normalize(&mut mesh, 4) >= 1);
    assert!((weights::weight_of(&mesh.vertices[0], b) - 0.4).abs() < 1e-5);
    assert_normalized(&mesh);
    weights::normalize(&mut mesh, 1);
    assert_eq!(weights::weight_of(&mesh.vertices[0], b), 0.);
    assert_normalized(&mesh);
}

#[test]
fn rebind_keeps_the_displayed_shape_or_returns_artwork_to_its_drawn_place() {
    let (mut rig, mesh, a, _b) = rig_and_mesh();
    rig.bone_mut(a).unwrap().x += 6.;
    let displayed = rest_points(&rig, &mesh);
    assert!((displayed[0] - mesh.vertices[0].rest).length() > 5.);
    let keep = weights::rebind(&rig, &mesh, RebindMode::KeepShape).unwrap();
    for (p, v) in rest_points(&rig, &keep).iter().zip(&displayed) {
        assert!((*p - *v).length() < 1e-3);
    }
    let original = weights::rebind(&rig, &mesh, RebindMode::OriginalPlacement).unwrap();
    for (p, v) in rest_points(&rig, &original).iter().zip(&mesh.vertices) {
        assert!((*p - v.rest).length() < 1e-3);
    }
    let rest = rig.world(None);
    let head = rest.iter().find(|w| w.id == a).unwrap().head;
    assert_eq!(keep.bind[&a].head, head);
    assert_eq!(original.bind[&a].head, head);
}
