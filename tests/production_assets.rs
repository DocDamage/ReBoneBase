use rebonebase::{
    model::{Rig, Vec2},
    production::*,
    project::{Project, Reference},
};
use std::sync::Arc;
#[test]
fn scene_actor_placement_and_prop_animation_are_clip_specific() {
    let mut p = project();
    p.rig.clips.push(Default::default());
    let mut child = project();
    child.extras.production.assets.push(SpriteAsset {
        id: 1,
        name: "ink".into(),
        image: image([255, 0, 0, 255]),
    });
    child.extras.production.attachments.push(Attachment {
        id: 2,
        asset: 1,
        offset: Vec2::new(1., 1.),
        ..Default::default()
    });
    p.extras.production.actors.push(SceneActor {
        id: 3,
        project: Box::new(child),
        keys: vec![
            ActorKey {
                clip: 0,
                frame: 1.,
                offset: Vec2::default(),
                scale: 1.,
                visible: true,
            },
            ActorKey {
                clip: 0,
                frame: 3.,
                offset: Vec2::new(5., 0.),
                scale: 1.,
                visible: true,
            },
        ],
        ..Default::default()
    });
    assert!(p.render_at(1.).as_raw() != p.render_at(3.).as_raw());
    p.rig.active_clip = 1;
    assert_eq!(p.render_at(3.).get_pixel(1, 1).0, [255, 0, 0, 255]);
    p.extras.production.assets.push(SpriteAsset {
        id: 10,
        name: "prop".into(),
        image: image([0, 0, 255, 255]),
    });
    p.extras.production.attachments.push(Attachment {
        id: 11,
        asset: 10,
        offset: Vec2::new(10., 10.),
        clip: Some(0),
        keys: vec![AttachmentKey {
            frame: 1.,
            bone: None,
            offset: Vec2::new(5., 5.),
            angle: 0.,
            asset: 10,
            visible: true,
        }],
        ..Default::default()
    });
    assert_eq!(p.render_at(1.).get_pixel(10, 10).0, [0, 0, 255, 255]);
    p.validate().unwrap();
}
fn image(color: [u8; 4]) -> Reference {
    Reference {
        width: 2,
        height: 2,
        pixels: Arc::new(color.repeat(4)),
        x: 0.,
        y: 0.,
        scale: 1.,
        opacity: 1.,
    }
}
fn project() -> Project {
    let mut r = Rig::new("Art", 16, 16);
    r.add_bone(Vec2::new(4., 8.), Vec2::new(8., 8.), None)
        .unwrap();
    Project::new(r)
}
#[test]
fn baked_project_releases_ik_back_to_neutral_fk() {
    let mut p = Project::new(rebonebase::project::templates()[0].clone());
    let id = p
        .rig
        .bones
        .iter()
        .find(|b| b.name == "forearm_L")
        .unwrap()
        .id;
    let target = p.rig.world(None).iter().find(|w| w.id == id).unwrap().tail + Vec2::new(-4., -6.);
    p.extras
        .production
        .motion
        .contacts
        .push(rebonebase::advanced_motion::ContactTrack {
            lower: id,
            clip: Some(0),
            keys: vec![
                rebonebase::advanced_motion::ContactKey {
                    frame: 1.,
                    target,
                    weight: 1.,
                    bend: 1.,
                },
                rebonebase::advanced_motion::ContactKey {
                    frame: 5.,
                    target,
                    weight: 0.,
                    bend: 1.,
                },
            ],
            enabled: true,
        });
    let expected = p.rig.world(Some(&p.pose_at(5.)));
    let baked = p.baked_rig().unwrap();
    let actual = baked.world(Some(&baked.active_clip().sample(5.)));
    for (a, b) in actual.iter().zip(expected) {
        assert!(
            (a.tail - b.tail).length() < 0.001,
            "{} did not release",
            a.id
        );
    }
}
#[test]
fn attachment_parent_switch_and_face_keys_render_at_their_keyed_positions() {
    let mut p = project();
    p.extras.production.assets.push(SpriteAsset {
        id: 1,
        name: "red".into(),
        image: image([255, 0, 0, 255]),
    });
    p.extras.production.assets.push(SpriteAsset {
        id: 2,
        name: "blue".into(),
        image: image([0, 0, 255, 255]),
    });
    p.extras.production.attachments.push(Attachment {
        id: 3,
        asset: 1,
        bone: Some(p.rig.bones[0].id),
        offset: Vec2::new(1., 0.),
        ..Default::default()
    });
    let before = p.render_at(1.);
    assert_eq!(before.get_pixel(5, 8).0, [255, 0, 0, 255]);
    switch_attachment_parent(&mut p, 3, None, 1.).unwrap();
    assert!(p.render_at(1.).as_raw() == before.as_raw());
    p.extras.production.faces.push(FaceTrack {
        id: 4,
        bone: None,
        offset: Vec2::new(10., 5.),
        keys: vec![
            FaceKey {
                frame: 1.,
                asset: 1,
                offset: Vec2::default(),
            },
            FaceKey {
                frame: 3.,
                asset: 2,
                offset: Vec2::new(1., 0.),
            },
        ],
        ..Default::default()
    });
    assert_eq!(p.render_at(3.).get_pixel(11, 5).0, [0, 0, 255, 255]);
    p.validate().unwrap();
}
#[test]
fn weighted_mesh_follows_bones_and_pixel_corrections_override_only_their_frame() {
    let mut p = project();
    let id = p.rig.bones[0].id;
    p.extras.production.assets.push(SpriteAsset {
        id: 1,
        name: "skin".into(),
        image: image([20, 200, 70, 255]),
    });
    let mesh = grid_mesh(
        &p.rig,
        2,
        1,
        Vec2::new(3., 6.),
        Vec2::new(4., 4.),
        2,
        2,
        &[id],
    )
    .unwrap();
    p.extras.production.meshes.push(mesh);
    assert!(p.render_at(1.).pixels().any(|v| v[3] > 0));
    p.rig
        .active_clip_mut()
        .set_key(id, rebonebase::animation::Channel::X, 1., 0.)
        .unwrap();
    p.rig
        .active_clip_mut()
        .set_key(id, rebonebase::animation::Channel::X, 2., 3.)
        .unwrap();
    assert!(
        p.render_at(1.).as_raw() != p.render_at(2.).as_raw(),
        "mesh should move with keyed bone translation"
    );
    paint_pixel(&mut p, 0, 2, 1, 1, Some([255, 0, 255, 255])).unwrap();
    assert_eq!(p.render_at(2.).get_pixel(1, 1).0, [255, 0, 255, 255]);
    assert_ne!(p.render_at(1.).get_pixel(1, 1).0, [255, 0, 255, 255]);
    paint_pixel(&mut p, 0, 2, 1, 1, None).unwrap();
    assert_ne!(p.render_at(2.).get_pixel(1, 1).0, [255, 0, 255, 255]);
    p.validate().unwrap();
}
#[test]
fn looks_and_scene_actors_preserve_independent_visibility_timing_and_project_roundtrip() {
    let mut p = project();
    p.extras.production.assets.push(SpriteAsset {
        id: 1,
        name: "prop".into(),
        image: image([255, 0, 0, 255]),
    });
    p.extras.production.attachments.push(Attachment {
        id: 2,
        asset: 1,
        offset: Vec2::new(3., 3.),
        ..Default::default()
    });
    let original = p.render_at(1.);
    p.extras.production.looks.push(Look {
        name: "Hidden".into(),
        ..Default::default()
    });
    p.extras.production.active_look = Some(0);
    assert!(p.render_at(1.).pixels().all(|v| v[3] == 0));
    p.extras.production.active_look = None;
    let mut actor = p.clone();
    actor.extras.production.actors.clear();
    p.extras.production.actors.push(SceneActor {
        id: 8,
        name: "Friend".into(),
        project: Box::new(actor),
        offset: Vec2::new(6., 0.),
        ..Default::default()
    });
    assert_ne!(original.as_raw(), p.render_at(1.).as_raw());
    p.validate().unwrap();
    let restored: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(restored, p);
}
#[test]
fn collision_shapes_are_timed_and_bone_attached_and_invalid_external_data_is_rejected() {
    let mut p = project();
    p.extras.production.boxes.push(BoxTrack {
        id: 9,
        bone: Some(p.rig.bones[0].id),
        start: 2,
        end: 4,
        offset: Vec2::new(2., 0.),
        size: Vec2::new(3., 2.),
        ..Default::default()
    });
    assert!(collision_shapes(&p, 1.).is_empty());
    assert_eq!(collision_shapes(&p, 2.).len(), 1);
    p.extras.production.attachments.push(Attachment {
        asset: 999,
        ..Default::default()
    });
    assert!(p.validate().is_err());
}
