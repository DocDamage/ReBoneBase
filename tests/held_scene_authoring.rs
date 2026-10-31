use rebonebase::{
    animation::{Channel, ClipSource},
    model::{Rig, Vec2},
    production::{
        self, Attachment, AttachmentKey, ClipEdit, SpriteAsset, paint_pixel, paint_pixels,
        switch_attachment_parent,
    },
    project::{Project, Reference},
};
use std::sync::Arc;

fn held_prop() -> (Project, u32) {
    let mut rig = Rig::new("Held artwork", 32, 24);
    let old_parent = rig
        .add_bone(Vec2::new(5., 8.), Vec2::new(10., 8.), None)
        .unwrap();
    let new_parent = rig
        .add_bone(Vec2::new(20., 8.), Vec2::new(25., 8.), None)
        .unwrap();
    rig.clips[0].length = 8;
    rig.clips[0].looped = false;
    for (bone, end) in [(old_parent, 14.), (new_parent, -7.)] {
        rig.clips[0].set_key(bone, Channel::X, 1., 0.).unwrap();
        rig.clips[0].set_key(bone, Channel::X, 8., end).unwrap();
    }
    let mut project = Project::new(rig);
    project.extras.auto_canvas = false;
    project.extras.show_2d = false;
    for (id, color) in [(1, [255, 80, 10, 255]), (2, [10, 80, 255, 255])] {
        project.extras.production.assets.push(SpriteAsset {
            id,
            name: format!("Color {id}"),
            image: Reference {
                width: 1,
                height: 1,
                pixels: Arc::new(color.to_vec()),
                x: 0.,
                y: 0.,
                scale: 1.,
                opacity: 1.,
            },
        });
    }
    project.extras.production.attachments.push(Attachment {
        id: 3,
        name: "Lantern".into(),
        asset: 1,
        bone: Some(old_parent),
        offset: Vec2::new(2., 0.),
        clip: Some(0),
        keys: vec![
            AttachmentKey {
                frame: 1.,
                bone: Some(old_parent),
                offset: Vec2::new(2., 0.),
                angle: 0.,
                asset: 1,
                visible: true,
            },
            AttachmentKey {
                frame: 6.,
                bone: Some(old_parent),
                offset: Vec2::new(6., 0.),
                angle: 0.,
                asset: 2,
                visible: true,
            },
        ],
        ..Default::default()
    });
    for length in [3, 7] {
        production::edit_clip(&mut project, ClipEdit::Resize { index: 0, length }).unwrap();
    }
    project.validate().unwrap();
    (project, new_parent)
}

#[test]
fn switching_parent_in_a_held_extension_preserves_the_displayed_prop() {
    let (mut project, parent) = held_prop();
    let before = project.try_render_at(6.).unwrap();
    let old_world = project.rig.world(Some(&project.pose_at(6.)));
    assert_eq!(project.rig.clips[0].evaluation_frame(6.), 3.);
    assert!(before.pixels().any(|pixel| pixel[3] > 0));

    switch_attachment_parent(&mut project, 3, Some(parent), 6.).unwrap();

    assert_eq!(project.rig.clips[0].evaluation_frame(6.), 6.);
    let new_world = project.rig.world(Some(&project.pose_at(6.)));
    assert_ne!(
        old_world
            .iter()
            .find(|bone| bone.id == parent)
            .unwrap()
            .head,
        new_world
            .iter()
            .find(|bone| bone.id == parent)
            .unwrap()
            .head
    );
    assert_eq!(project.try_render_at(6.).unwrap(), before);
    let key = production::prop_keys(&project.extras.production.attachments[0], 0)
        .iter()
        .find(|key| key.frame == 6.)
        .unwrap();
    assert_eq!(key.bone, Some(parent));
    assert_eq!(key.asset, 1);
    project.validate().unwrap();
}

#[test]
fn painting_a_held_extension_releases_its_clock_and_displays_the_new_correction() {
    let (mut project, _) = held_prop();
    let before = project.clone();
    paint_pixel(&mut project, 0, 6, 0, 0, None).unwrap();
    assert_eq!(project, before);

    paint_pixel(&mut project, 0, 6, 0, 0, Some([220, 0, 200, 255])).unwrap();
    assert_eq!(project.rig.clips[0].evaluation_frame(6.), 6.);
    assert_eq!(
        project.try_render_at(6.).unwrap().get_pixel(0, 0).0,
        [220, 0, 200, 255]
    );
    assert_eq!(project.try_render_at(5.).unwrap().get_pixel(0, 0).0, [0; 4]);
    paint_pixel(&mut project, 0, 6, 0, 0, None).unwrap();
    assert_eq!(project.try_render_at(6.).unwrap().get_pixel(0, 0).0, [0; 4]);
}

#[test]
fn failed_parent_switch_or_clock_release_leaves_the_project_unchanged() {
    let (mut project, parent) = held_prop();
    let before = project.clone();
    assert!(switch_attachment_parent(&mut project, 999, Some(parent), 6.).is_err());
    assert_eq!(project, before);

    project.rig.clips[0].tracks.clear();
    project.rig.clips[0].source = Some(ClipSource {
        origin: 4094,
        length: 4096,
        end: 2,
        looped: false,
        durations: Default::default(),
        time_offset_ms: 0,
    });
    project.extras.production.attachments[0].keys.clear();
    project.extras.production.attachments[0].clip_keys.clear();
    project.validate().unwrap();
    let before = project.clone();
    assert!(switch_attachment_parent(&mut project, 3, Some(parent), 6.).is_err());
    assert_eq!(project, before);
    assert!(paint_pixel(&mut project, 0, 6, 0, 0, Some([1; 4])).is_err());
    assert_eq!(project, before);
}

#[test]
fn a_batched_stroke_is_applied_completely_or_leaves_the_project_unchanged() {
    let (mut project, _) = held_prop();
    let before = project.clone();
    assert!(paint_pixels(&mut project, 0, 6, &[(0, 0), (32, 0)], Some([1; 4])).is_err());
    assert_eq!(project, before);
    paint_pixels(
        &mut project,
        0,
        6,
        &[(0, 0), (1, 0), (1, 1)],
        Some([20, 30, 40, 255]),
    )
    .unwrap();
    let rendered = project.try_render_at(6.).unwrap();
    for (x, y) in [(0, 0), (1, 0), (1, 1)] {
        assert_eq!(rendered.get_pixel(x, y).0, [20, 30, 40, 255]);
    }
    assert_eq!(project.extras.production.corrections[0].pixels.len(), 3);
}
