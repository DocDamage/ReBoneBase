use rebonebase::{
    advanced_motion::Layer,
    animation::Channel,
    model::Vec2,
    project::{self, Project},
};
#[test]
fn authored_limb_translation_tracks_and_layers_cannot_move_attachment_points() {
    let mut p = Project::new(project::templates()[0].clone());
    let arm = p
        .rig
        .bones
        .iter()
        .find(|b| b.name == "upper_arm_R")
        .unwrap()
        .id;
    let root = p.rig.bones.iter().find(|b| b.parent.is_none()).unwrap().id;
    p.rig
        .active_clip_mut()
        .set_key(root, Channel::X, 1., 5.)
        .unwrap();
    let anchored = p.rig.world(Some(&p.pose_at(1.)));
    p.rig
        .active_clip_mut()
        .set_key(arm, Channel::X, 1., 20.)
        .unwrap();
    p.rig
        .active_clip_mut()
        .set_key(arm, Channel::Y, 1., -8.)
        .unwrap();
    let mut layer = p.rig.clips[0].clone();
    layer.name = "Offsets".into();
    p.rig.clips.push(layer);
    p.extras.production.motion.layers.push(Layer {
        clip: 1,
        weight: 1.,
        mask: vec![arm],
        ..Default::default()
    });
    let stored = p.clone();
    let actual = p.rig.world(Some(&p.pose_at(1.)));
    let head = |w: &Vec<rebonebase::model::WorldBone>| w.iter().find(|b| b.id == arm).unwrap().head;
    assert_eq!(head(&actual), head(&anchored));
    assert_eq!(p, stored);
    let raw = p.rig.active_clip().sample(1.);
    assert_eq!((raw[&arm].x, raw[&arm].y), (20., -8.));
    p.extras.production.baked_motion.insert(0);
    assert_eq!(head(&p.rig.world(Some(&p.pose_at(1.)))), head(&anchored));
    assert_ne!(head(&p.rig.world(Some(&raw))), head(&anchored));
    assert_eq!(p.pose_at(1.).get(&root).unwrap().x, 5.);
    assert_ne!(head(&anchored), Vec2::default());
    p.extras.production.motion.layers.clear();
    let baked = p.baked_rig().unwrap();
    assert_eq!(
        head(&baked.world(Some(&baked.active_clip().sample(1.)))),
        head(&anchored)
    );
}
