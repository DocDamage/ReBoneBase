use rebonebase::{
    advanced_motion::{ContactKey, ContactTrack, Layer, Spring},
    animation::{Channel, Ease},
    model::{Rig, Vec2},
    production::{
        self, ActorKey, Attachment, AttachmentKey, BoxKey, BoxTrack, ClipEdit, Correction,
        EventMarker, FaceKey, FaceTrack, MeshKey, SceneActor, SpriteAsset,
    },
    project::{Project, Reference},
};
use std::sync::Arc;

fn fixture() -> Project {
    let mut rig = Rig::new("Temporal metadata", 32, 32);
    let part = rig.add_part("Body", [30, 180, 100, 255]);
    let root = rig
        .add_bone(Vec2::new(4., 14.), Vec2::new(8., 14.), None)
        .unwrap();
    let a = rig
        .add_bone(Vec2::new(8., 14.), Vec2::new(12., 14.), Some(root))
        .unwrap();
    let b = rig
        .add_bone(Vec2::new(12., 14.), Vec2::new(16., 14.), Some(a))
        .unwrap();
    for bone in &mut rig.bones {
        bone.part = part;
        bone.radius_head = 1.;
        bone.radius_tail = 1.;
    }
    rig.clips[0].length = 6;
    rig.clips[0].name = "Source".into();
    rig.clips[0].durations.insert(3, 330);
    rig.clips[0].set_key(root, Channel::X, 1., 0.).unwrap();
    rig.clips[0].set_key(root, Channel::X, 6., 3.).unwrap();
    rig.clips[0]
        .set_key(b, Channel::Rotation, 1., -20.)
        .unwrap();
    rig.clips[0].set_key(b, Channel::Rotation, 6., 30.).unwrap();
    rig.clips[0].tracks[0].keys[0].easing = Ease::EaseIn;
    let mut other = rig.clips[0].clone();
    other.length = 4;
    other.name = "Other".into();
    other.tracks.clear();
    other.durations.clear();
    rig.clips.push(other);
    let mut p = Project::new(rig);
    let image = Reference {
        width: 2,
        height: 2,
        pixels: Arc::new([255, 240, 0, 255].repeat(4)),
        x: 0.,
        y: 0.,
        scale: 1.,
        opacity: 1.,
    };
    p.extras.production.assets.push(SpriteAsset {
        id: 1,
        name: "Sprite".into(),
        image,
    });
    p.extras.production.attachments.push(Attachment {
        id: 2,
        asset: 1,
        clip: Some(0),
        keys: vec![
            AttachmentKey {
                frame: 2.,
                bone: None,
                offset: Vec2::new(2., 1.),
                angle: 0.,
                asset: 1,
                visible: true,
            },
            AttachmentKey {
                frame: 5.,
                bone: None,
                offset: Vec2::new(7., 1.),
                angle: 0.,
                asset: 1,
                visible: true,
            },
        ],
        ..Default::default()
    });
    let mut mesh = production::grid_mesh(
        &p.rig,
        3,
        1,
        Vec2::new(12., 3.),
        Vec2::new(4., 4.),
        1,
        1,
        &[a],
    )
    .unwrap();
    mesh.keys = vec![
        MeshKey {
            frame: 1.,
            offsets: vec![Vec2::default(); 4],
        },
        MeshKey {
            frame: 6.,
            offsets: vec![Vec2::new(3., 0.); 4],
        },
    ];
    p.extras.production.meshes.push(mesh);
    p.extras.production.faces.push(FaceTrack {
        id: 4,
        clip: 0,
        offset: Vec2::new(2., 5.),
        keys: vec![
            FaceKey {
                frame: 2.,
                asset: 1,
                offset: Vec2::default(),
            },
            FaceKey {
                frame: 5.,
                asset: 1,
                offset: Vec2::new(3., 0.),
            },
        ],
        ..Default::default()
    });
    p.extras.production.boxes.push(BoxTrack {
        id: 5,
        clip: 0,
        start: 2,
        end: 6,
        keys: vec![
            BoxKey {
                frame: 2.,
                offset: Vec2::new(1., 2.),
                size: Vec2::new(4., 4.),
                angle: 0.,
                enabled: true,
            },
            BoxKey {
                frame: 6.,
                offset: Vec2::new(5., 2.),
                size: Vec2::new(4., 4.),
                angle: 40.,
                enabled: true,
            },
        ],
        ..Default::default()
    });
    p.extras.production.events.push(EventMarker {
        clip: 0,
        frame: 3.,
        name: "Step".into(),
        payload: "left".into(),
    });
    p.extras.production.corrections.push(Correction {
        clip: 0,
        frame: 4,
        pixels: [(0, [255, 0, 255, 255])].into(),
    });
    let mut actor_rig = Rig::new("Actor", 6, 4);
    let part = actor_rig.add_part("Actor", [80, 30, 240, 255]);
    let bone = actor_rig
        .add_bone(Vec2::new(1., 1.), Vec2::new(3., 1.), None)
        .unwrap();
    actor_rig.bone_mut(bone).unwrap().part = part;
    actor_rig.bone_mut(bone).unwrap().radius_head = 0.5;
    actor_rig.bone_mut(bone).unwrap().radius_tail = 0.5;
    actor_rig.clips[0].length = 6;
    actor_rig.clips[0]
        .set_key(bone, Channel::X, 1., 0.)
        .unwrap();
    actor_rig.clips[0]
        .set_key(bone, Channel::X, 6., 2.)
        .unwrap();
    p.extras.production.actors.push(SceneActor {
        id: 6,
        project: Box::new(Project::new(actor_rig)),
        offset: Vec2::new(20., 24.),
        keys: vec![
            ActorKey {
                clip: 0,
                frame: 2.,
                offset: Vec2::new(20., 24.),
                scale: 1.,
                visible: true,
            },
            ActorKey {
                clip: 0,
                frame: 6.,
                offset: Vec2::new(24., 24.),
                scale: 1.,
                visible: true,
            },
            ActorKey {
                clip: 1,
                frame: 1.,
                offset: Vec2::new(3., 24.),
                scale: 1.,
                visible: true,
            },
        ],
        ..Default::default()
    });
    p.extras.production.motion.contacts.push(ContactTrack {
        lower: b,
        clip: Some(0),
        keys: vec![
            ContactKey {
                target: Vec2::new(14., 17.),
                ..Default::default()
            },
            ContactKey {
                frame: 6.,
                target: Vec2::new(14., 17.),
                weight: 0.,
                ..Default::default()
            },
        ],
        ..Default::default()
    });
    p.validate().unwrap();
    p
}
fn same_frames(a: &Project, clip_a: usize, start: u32, b: &Project, clip_b: usize, length: u32) {
    let mut a = a.clone();
    a.rig.active_clip = clip_a;
    let mut b = b.clone();
    b.rig.active_clip = clip_b;
    for f in 1..=length {
        let from = (start + f - 1) as f32;
        let to = f as f32;
        let left_world = a.rig.world(Some(&a.pose_at(from)));
        let right_world = b.rig.world(Some(&b.pose_at(to)));
        for (l, r) in left_world.iter().zip(&right_world) {
            assert!(
                (l.head - r.head).length() < 0.001 && (l.tail - r.tail).length() < 0.001,
                "frame {f}: {l:?} != {r:?}"
            );
        }
        let left = a.render_at(from);
        let right = b.render_at(to);
        if let Some((i, (l, r))) = left
            .pixels()
            .zip(right.pixels())
            .enumerate()
            .find(|(_, (l, r))| l != r)
        {
            panic!("frame {f}, pixel {} differs: {l:?} != {r:?}", i);
        }
    }
}

#[test]
fn duplicate_copies_owned_state_with_fresh_ids_without_duplicating_shared_artwork() {
    let mut p = fixture();
    let before = p.clone();
    production::edit_clip(&mut p, ClipEdit::Duplicate(0)).unwrap();
    p.validate().unwrap();
    assert_eq!(p.rig.active_clip, 1);
    assert_eq!(p.rig.clips.len(), 3);
    assert_eq!(p.extras.production.attachments.len(), 1);
    assert_eq!(p.extras.production.meshes.len(), 1);
    assert_eq!(p.extras.production.actors.len(), 1);
    assert_eq!(p.extras.production.faces.len(), 2);
    assert_eq!(p.extras.production.boxes.len(), 2);
    assert_ne!(
        p.extras.production.faces[0].id,
        p.extras.production.faces[1].id
    );
    assert_ne!(
        p.extras.production.boxes[0].id,
        p.extras.production.boxes[1].id
    );
    same_frames(&before, 0, 1, &p, 1, 6);
    same_frames(&before, 1, 1, &p, 2, 4);
}

#[test]
fn trim_keeps_rendered_integer_frames_boundary_holds_durations_and_metadata_aligned() {
    let mut p = fixture();
    let before = p.clone();
    production::edit_clip(
        &mut p,
        ClipEdit::Trim {
            index: 0,
            start: 3,
            end: 5,
        },
    )
    .unwrap();
    p.validate().unwrap();
    assert_eq!(p.rig.clips[0].length, 3);
    assert_eq!(p.rig.clips[0].duration_ms(1), 330);
    same_frames(&before, 0, 3, &p, 0, 3);
    same_frames(&before, 1, 1, &p, 1, 4);
    assert_eq!(p.extras.production.events[0].frame, 1.);
    assert_eq!(p.extras.production.corrections[0].frame, 2);
    assert_eq!(
        (
            p.extras.production.boxes[0].start,
            p.extras.production.boxes[0].end
        ),
        (1, 3)
    );
}

#[test]
fn split_preserves_both_ranges_and_creates_independent_prop_and_mesh_key_sets() {
    let mut p = fixture();
    let before = p.clone();
    production::edit_clip(&mut p, ClipEdit::Split { index: 0, at: 4 }).unwrap();
    p.validate().unwrap();
    assert_eq!(p.rig.active_clip, 1);
    same_frames(&before, 0, 1, &p, 0, 3);
    same_frames(&before, 0, 4, &p, 1, 3);
    same_frames(&before, 1, 1, &p, 2, 4);
    let prop = production::prop_keys_mut(&mut p.extras.production.attachments[0], 1, 3);
    prop[0].offset.x += 3.;
    let mesh = production::mesh_keys_mut(&mut p.extras.production.meshes[0], 1, 3);
    mesh[0].offsets[0].x += 2.;
    same_frames(&before, 0, 1, &p, 0, 3);
    p.validate().unwrap();
}

#[test]
fn resize_shrinks_all_owned_keys_then_extends_with_held_displayed_pose() {
    let mut p = fixture();
    let before = p.clone();
    production::edit_clip(
        &mut p,
        ClipEdit::Resize {
            index: 0,
            length: 3,
        },
    )
    .unwrap();
    p.validate().unwrap();
    same_frames(&before, 0, 1, &p, 0, 3);
    assert!(p.extras.production.corrections.is_empty());
    let pose = p.pose_at(3.);
    production::edit_clip(
        &mut p,
        ClipEdit::Resize {
            index: 0,
            length: 7,
        },
    )
    .unwrap();
    p.validate().unwrap();
    assert_eq!(p.pose_at(7.), pose);
}

#[test]
fn delete_and_move_remap_every_reference_and_keep_globally_visible_artwork() {
    let mut p = fixture();
    production::edit_clip(&mut p, ClipEdit::Duplicate(0)).unwrap();
    production::edit_clip(&mut p, ClipEdit::Move { from: 1, to: 2 }).unwrap();
    p.validate().unwrap();
    assert!(p.extras.production.faces.iter().any(|f| f.clip == 2));
    production::edit_clip(&mut p, ClipEdit::Delete(0)).unwrap();
    p.validate().unwrap();
    assert_eq!(p.extras.production.meshes.len(), 1);
    assert_eq!(p.extras.production.attachments.len(), 1);
    assert!(p.extras.production.faces.iter().all(|f| f.clip == 1));
    assert!(
        p.extras
            .production
            .motion
            .contacts
            .iter()
            .all(|c| c.clip == Some(1))
    );
}

#[test]
fn temporal_edits_bake_layers_and_spring_history_without_double_application() {
    let mut p = fixture();
    let bone = p.rig.bones[2].id;
    p.extras.production.motion.contacts.clear();
    p.extras.production.motion.layers.push(Layer {
        clip: 1,
        weight: 0.3,
        ..Default::default()
    });
    p.extras.production.motion.springs.push(Spring {
        bone,
        gravity: 45.,
        wind: 10.,
        ..Default::default()
    });
    let before = p.clone();
    production::edit_clip(&mut p, ClipEdit::Split { index: 0, at: 4 }).unwrap();
    same_frames(&before, 0, 1, &p, 0, 3);
    same_frames(&before, 0, 4, &p, 1, 3);
    p.validate().unwrap();
}

#[test]
fn invalid_operations_are_atomic_including_metadata_capacity_failures() {
    let mut p = fixture();
    for operation in [
        ClipEdit::Delete(99),
        ClipEdit::Duplicate(99),
        ClipEdit::Move { from: 0, to: 9 },
        ClipEdit::Trim {
            index: 0,
            start: 5,
            end: 2,
        },
        ClipEdit::Split { index: 0, at: 1 },
        ClipEdit::Resize {
            index: 0,
            length: 0,
        },
        ClipEdit::Resize {
            index: 0,
            length: 241,
        },
    ] {
        let before = p.clone();
        assert!(production::edit_clip(&mut p, operation).is_err());
        assert_eq!(p, before);
    }
    while p.extras.production.faces.len() < 32 {
        let mut f = p.extras.production.faces[0].clone();
        f.id = p.extras.production.next_id();
        p.extras.production.faces.push(f);
    }
    let before = p.clone();
    assert!(production::edit_clip(&mut p, ClipEdit::Duplicate(0)).is_err());
    assert_eq!(p, before);
}

#[test]
fn newly_initialized_prop_overrides_keep_sparse_keys_and_hold_new_states() {
    let mut p = fixture();
    let keys = production::prop_keys_mut(&mut p.extras.production.attachments[0], 0, 6);
    keys.push(AttachmentKey {
        frame: 3.,
        bone: None,
        offset: Vec2::new(10., 1.),
        angle: 0.,
        asset: 1,
        visible: true,
    });
    keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    assert_eq!(p.render_at(4.).get_pixel(10, 1).0, [255, 240, 0, 255]);
    assert!(production::prop_keys(&p.extras.production.attachments[0], 0).len() < 6);
    p.validate().unwrap();
}

#[test]
fn global_contacts_and_props_remain_unchanged_in_other_clips_after_a_trim() {
    let mut p = fixture();
    p.extras.production.attachments[0].clip = None;
    p.extras.production.motion.contacts[0].clip = None;
    let before = p.clone();
    production::edit_clip(
        &mut p,
        ClipEdit::Trim {
            index: 0,
            start: 3,
            end: 5,
        },
    )
    .unwrap();
    same_frames(&before, 0, 3, &p, 0, 3);
    same_frames(&before, 1, 1, &p, 1, 4);
    p.validate().unwrap();
}

#[test]
fn mesh_override_helpers_preserve_future_interpolation_at_the_clip_boundary() {
    let mut p = fixture();
    let original = p.extras.production.meshes[0].clone();
    let mesh = &mut p.extras.production.meshes[0];
    production::mesh_keys_mut(mesh, 0, 4);
    assert_eq!(production::mesh_keys(mesh, 0).last().unwrap().frame, 4.);
    for frame in 1..=4 {
        let a = production::mesh_offsets(&original, 0, frame as f32);
        let b = production::mesh_offsets(mesh, 0, frame as f32);
        for (a, b) in a.iter().zip(b) {
            assert!((*a - b).length() < 0.0001);
        }
    }
    let frame = production::mesh_offsets(&original, 0, 3.);
    assert_eq!(frame.len(), original.vertices.len());
    assert!(frame.iter().all(|p| (p.x - 1.2).abs() < 0.0001));
}

#[test]
fn pruning_deleted_bones_also_checks_prop_override_timelines() {
    let mut p = fixture();
    production::edit_clip(&mut p, ClipEdit::Duplicate(0)).unwrap();
    let bone = p.rig.bones[2].id;
    production::prop_keys_mut(&mut p.extras.production.attachments[0], 1, 6)[0].bone = Some(bone);
    p.rig.delete_bone(bone);
    p.extras.production.prune_bones(&p.rig);
    assert!(p.extras.production.attachments.is_empty());
    p.validate().unwrap();
}

#[test]
fn single_frame_trim_drops_out_of_range_events_corrections_and_regions() {
    let mut p = fixture();
    let before = p.clone();
    production::edit_clip(
        &mut p,
        ClipEdit::Trim {
            index: 0,
            start: 1,
            end: 1,
        },
    )
    .unwrap();
    same_frames(&before, 0, 1, &p, 0, 1);
    assert!(p.extras.production.events.is_empty());
    assert!(p.extras.production.corrections.is_empty());
    assert!(p.extras.production.boxes.is_empty());
    p.validate().unwrap();
}

#[test]
fn malformed_clip_overrides_are_rejected_at_the_project_boundary() {
    let p = fixture();
    let mut bad = p.clone();
    bad.extras.production.baked_motion.insert(99);
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.extras.production.attachments[0]
        .clip_keys
        .insert(99, vec![]);
    assert!(bad.validate().is_err());
    let mut bad = p.clone();
    bad.extras.production.meshes[0].clip_keys.insert(
        0,
        vec![MeshKey {
            frame: 1.,
            offsets: vec![],
        }],
    );
    assert!(bad.validate().is_err());
    let mut bad = p;
    bad.extras.production.actors[0]
        .clip_time_ms
        .insert(0, f64::NAN);
    assert!(bad.validate().is_err());
}

#[test]
fn mirrored_project_clip_reflects_evaluated_fk_and_keeps_shared_art_and_metadata() {
    let mut p = Project::new(rebonebase::project::templates()[0].clone());
    let left = p
        .rig
        .bones
        .iter()
        .find(|b| b.name == "forearm_L")
        .unwrap()
        .id;
    p.rig.clips[0]
        .set_key(left, Channel::Rotation, 1., 25.)
        .unwrap();
    p.rig.clips[0]
        .set_key(left, Channel::Rotation, 12., -35.)
        .unwrap();
    p.extras.production.motion.springs.push(Spring {
        bone: left,
        gravity: 30.,
        ..Default::default()
    });
    production::edit_clip(
        &mut p,
        ClipEdit::Trim {
            index: 0,
            start: 3,
            end: 9,
        },
    )
    .unwrap();
    let source = p.clone();
    production::mirror_project_clip(&mut p, 0).unwrap();
    let mirrored = p.rig.active_clip;
    assert!(p.extras.production.baked_motion.contains(&mirrored));
    for frame in 1..=7 {
        let a = source.rig.world(Some(&source.pose_at(frame as f32)));
        let b = p.rig.world(Some(&p.pose_at(frame as f32)));
        for bone in &source.rig.bones {
            let from = a.iter().find(|w| w.id == bone.id).unwrap();
            let to = b
                .iter()
                .find(|w| w.id == bone.twin.unwrap_or(bone.id))
                .unwrap();
            assert!(
                (to.head - Vec2::new(2. * p.rig.mirror.x - from.head.x, from.head.y)).length()
                    < 0.001
            );
            assert!(
                (to.tail - Vec2::new(2. * p.rig.mirror.x - from.tail.x, from.tail.y)).length()
                    < 0.001
            );
        }
    }
    let mut art = fixture();
    let before = art.extras.production.clone();
    production::mirror_project_clip(&mut art, 0).unwrap();
    let index = art.rig.active_clip;
    assert_eq!(
        art.extras.production.attachments.len(),
        before.attachments.len()
    );
    assert_eq!(art.extras.production.meshes.len(), before.meshes.len());
    assert!(art.extras.production.faces.iter().any(|f| f.clip == index));
    assert!(art.extras.production.events.iter().any(|e| e.clip == index));
    let c = art
        .extras
        .production
        .motion
        .contacts
        .iter()
        .find(|c| c.clip == Some(index))
        .unwrap();
    assert_eq!(
        c.keys[0].target.x,
        2. * art.rig.mirror.x - before.motion.contacts[0].keys[0].target.x
    );
    assert_eq!(c.keys[0].bend, -before.motion.contacts[0].keys[0].bend);
    art.validate().unwrap();
}

#[test]
fn project_transition_uses_displayed_playback_endpoints_without_reapplying_controllers() {
    let mut p = fixture();
    p.extras.production.motion.springs.push(Spring {
        bone: p.rig.bones[2].id,
        gravity: 30.,
        ..Default::default()
    });
    p.rig.clips[1]
        .set_key(p.rig.bones[1].id, Channel::Rotation, 1., 40.)
        .unwrap();
    p.extras.production.motion.layers.push(Layer {
        clip: 1,
        weight: 0.2,
        ..Default::default()
    });
    let source = p.clone();
    let from = source.complete_pose_at(6.);
    let mut to_source = source.clone();
    to_source.rig.active_clip = 1;
    let to = to_source.complete_pose_at(1.);
    production::transition_project_clip(&mut p, 0, 1, 5).unwrap();
    let index = p.rig.active_clip;
    assert!(p.extras.production.baked_motion.contains(&index));
    assert_eq!(p.rig.active_clip().length, 5);
    for (frame, expected) in [(1., from), (5., to)] {
        let a = source.rig.world(Some(&expected));
        let b = p.rig.world(Some(&p.pose_at(frame)));
        for (a, b) in a.iter().zip(b) {
            assert!((a.head - b.head).length() < 0.001 && (a.tail - b.tail).length() < 0.001);
        }
    }
    assert!(p.extras.production.faces.iter().any(|f| f.clip == index));
    p.validate().unwrap();
    let before = p.clone();
    assert!(production::mirror_project_clip(&mut p, 99).is_err());
    assert!(production::transition_project_clip(&mut p, 0, 99, 4).is_err());
    assert_eq!(p, before);
}

#[test]
fn controller_pruning_removes_broken_chains_contacts_and_root_springs() {
    let mut p = fixture();
    let upper = p.rig.bones[1].id;
    let lower = p.rig.bones[2].id;
    p.extras
        .production
        .motion
        .chains
        .push(rebonebase::advanced_motion::Chain {
            bones: vec![upper, lower],
            ..Default::default()
        });
    p.extras.production.motion.springs.push(Spring {
        bone: lower,
        ..Default::default()
    });
    p.rig.bone_mut(lower).unwrap().connected = false;
    p.extras.production.prune_bones(&p.rig);
    assert!(p.extras.production.motion.contacts.is_empty());
    assert!(p.extras.production.motion.chains.is_empty());
    assert_eq!(p.extras.production.motion.springs.len(), 1);
    p.rig.bone_mut(lower).unwrap().parent = None;
    p.extras.production.prune_bones(&p.rig);
    assert!(p.extras.production.motion.springs.is_empty());
    p.validate().unwrap();
}

#[test]
fn deleting_last_masked_bone_never_turns_a_layer_into_global_animation() {
    let mut p = fixture();
    let bone = p.rig.bones[2].id;
    let part = p.rig.parts[0].id;
    p.extras.production.motion.layers.push(Layer {
        name: "Only deleted bone".into(),
        clip: 1,
        mask: vec![bone],
        ..Default::default()
    });
    p.extras.production.motion.layers.push(Layer {
        name: "Part channels".into(),
        clip: 1,
        mask: vec![part],
        ..Default::default()
    });
    p.rig.delete_bone(bone);
    p.extras.production.prune_bones(&p.rig);
    assert!(
        p.extras
            .production
            .motion
            .layers
            .iter()
            .find(|l| l.name == "Only deleted bone")
            .is_none_or(|l| !l.enabled)
    );
    assert_eq!(
        p.extras
            .production
            .motion
            .layers
            .iter()
            .find(|l| l.name == "Part channels")
            .unwrap()
            .mask,
        vec![part]
    );
    p.validate().unwrap();
}
