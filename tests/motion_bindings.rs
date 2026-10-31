use rebonebase::{
    character3d::{self, Playback3d},
    model::Rig,
    production::{self, ClipEdit},
    project::Project,
};
fn scene() -> Project {
    let mut p = Project::new(Rig::new("bound motions", 64, 64));
    p.rig.active_clip_mut().length = 8;
    p.rig.active_clip_mut().fps = 8.;
    let mut c = character3d::load(std::path::Path::new(
        "src/character3d/fixtures/textured_skin.glb",
    ))
    .unwrap();
    c.looped = false;
    c.animation = None;
    c.clips.insert(
        0,
        Playback3d {
            animation: Some(0),
            speed: 1.,
            looped: false,
            offset_ms: 0.,
        },
    );
    p.extras.show_2d = false;
    p.extras.characters3d.push(c);
    p
}
#[test]
fn each_timeline_owns_its_3d_motion_and_survives_persistence() {
    let mut p = scene();
    p.validate().unwrap();
    let moving = p.render_at(5.);
    production::edit_clip(&mut p, ClipEdit::Duplicate(0)).unwrap();
    p.extras.characters3d[0]
        .clips
        .get_mut(&1)
        .unwrap()
        .animation = None;
    p.rig.active_clip = 1;
    assert_ne!(moving, p.render_at(5.));
    p.rig.active_clip = 0;
    assert_eq!(moving, p.render_at(5.));
    let q: Project = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    q.validate().unwrap();
    assert_eq!(q, p);
}
#[test]
fn temporal_clip_edits_preserve_3d_frames_and_remap_bindings() {
    let p = scene();
    let expected = p.render_at(5.);
    let mut trim = p.clone();
    production::edit_clip(
        &mut trim,
        ClipEdit::Trim {
            index: 0,
            start: 5,
            end: 8,
        },
    )
    .unwrap();
    assert_eq!(expected, trim.render_at(1.));
    let mut split = p.clone();
    production::edit_clip(&mut split, ClipEdit::Split { index: 0, at: 5 }).unwrap();
    split.rig.active_clip = 1;
    assert_eq!(expected, split.render_at(1.));
    production::edit_clip(&mut split, ClipEdit::Move { from: 1, to: 0 }).unwrap();
    split.rig.active_clip = 0;
    assert_eq!(expected, split.render_at(1.));
    production::edit_clip(&mut split, ClipEdit::Delete(1)).unwrap();
    assert_eq!(split.extras.characters3d[0].clips.len(), 1);
    production::edit_clip(
        &mut split,
        ClipEdit::Resize {
            index: 0,
            length: 6,
        },
    )
    .unwrap();
    assert_eq!(expected, split.render_at(1.));
}
#[test]
fn invalid_bindings_reject_before_render() {
    let mut p = scene();
    p.extras.characters3d[0]
        .clips
        .get_mut(&0)
        .unwrap()
        .offset_ms = f64::NAN;
    assert!(p.validate().is_err());
    p.extras.characters3d[0].clips.clear();
    p.extras.characters3d[0]
        .clips
        .insert(31, Playback3d::default());
    assert!(p.validate().is_err());
}
#[test]
fn split_and_trim_use_authored_frame_durations() {
    let mut p = scene();
    p.rig.active_clip_mut().durations.insert(1, 20);
    p.rig.active_clip_mut().durations.insert(2, 30);
    let expected = p.render_at(5.);
    production::edit_clip(
        &mut p,
        ClipEdit::Trim {
            index: 0,
            start: 5,
            end: 8,
        },
    )
    .unwrap();
    assert!(
        expected == p.render_at(1.),
        "Trim must preserve the 3D pose with per-frame durations"
    );
}
#[test]
fn retiming_trimmed_motion_does_not_change_its_first_pose() {
    let mut p = scene();
    p.extras.characters3d[0].clips.get_mut(&0).unwrap().speed = 0.5;
    let expected = p.render_at(5.);
    production::edit_clip(
        &mut p,
        ClipEdit::Trim {
            index: 0,
            start: 5,
            end: 8,
        },
    )
    .unwrap();
    p.extras.characters3d[0].clips.get_mut(&0).unwrap().speed = 2.;
    assert!(
        expected == p.render_at(1.),
        "Retiming should keep the trimmed animation start"
    );
}
