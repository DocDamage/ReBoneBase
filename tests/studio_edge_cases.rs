use rebonebase::{
    animation::{Channel, Playback},
    aseprite, batch,
    features::{self, Pin, ReferenceSequence},
    project::{self, Project, Reference},
    skin,
};
use std::{io::Cursor, path::PathBuf, sync::Arc};

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rebone-edge-{}-{}",
            std::process::id(),
            rebonebase::storage::unique()
        ));
        std::fs::create_dir(&path).unwrap();
        assert!(
            path.canonicalize()
                .unwrap()
                .starts_with(std::env::temp_dir().canonicalize().unwrap())
        );
        Self(path)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        if self
            .0
            .canonicalize()
            .is_ok_and(|p| p.starts_with(std::env::temp_dir().canonicalize().unwrap()))
        {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn transparent_pixels_do_not_turn_opaque_when_transparent_swatch_has_hidden_rgb() {
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 0]));
    let png = batch::indexed_png(&image, &[[0, 0, 0, 255], [255, 255, 255, 0]]).unwrap();
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert!(decoded.pixels().all(|p| p[3] == 0));
}

#[test]
fn visible_pixels_do_not_disappear_into_a_transparent_swatches_hidden_rgb() {
    let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    let bytes = batch::indexed_png(&image, &[[0, 0, 0, 255], [255, 255, 255, 0]]).unwrap();
    assert_eq!(
        image::load_from_memory(&bytes)
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)[3],
        255
    );
    assert!(batch::indexed_png(&image, &[[255, 255, 255, 0]]).is_err());
}

#[test]
fn atomic_write_never_removes_a_temp_file_owned_by_another_writer() {
    let dir = TestDir::new();
    let path = dir.0.join("project.rebone");
    let other_temp = dir
        .0
        .join(format!(".project.rebone.{}.tmp", std::process::id()));
    std::fs::write(&other_temp, b"another writer's unfinished data").unwrap();
    let _ = project::atomic_write(&path, b"new data");
    assert_eq!(
        std::fs::read(&other_temp).unwrap(),
        b"another writer's unfinished data"
    );
}

#[test]
fn persistent_pins_preserve_locked_joint_pose_and_respect_angle_limits() {
    let mut p = Project::new(project::templates()[0].clone());
    let lower = p.rig.bones.iter().find(|b| b.name == "shin_L").unwrap().id;
    let upper = p.rig.bone(lower).unwrap().parent.unwrap();
    let target = p
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == lower)
        .unwrap()
        .tail
        + rebonebase::model::Vec2::new(4., -3.);
    p.extras.pins.push(Pin {
        lower,
        target,
        enabled: true,
        grounded: false,
        bend: -1.,
    });
    p.rig.bone_mut(upper).unwrap().locked = true;
    p.rig.bone_mut(lower).unwrap().locked = true;
    let original = p.rig.active_clip().sample(1.);
    let solved = features::solve_pins(&p.rig, &p.extras, &original);
    assert_eq!(solved, original);
    p.rig.bone_mut(upper).unwrap().locked = false;
    p.rig.bone_mut(lower).unwrap().locked = false;
    p.rig.bone_mut(lower).unwrap().rotation_min = Some(-10.);
    p.rig.bone_mut(lower).unwrap().rotation_max = Some(10.);
    let solved = features::solve_pins(&p.rig, &p.extras, &original);
    assert!((-10. ..=10.).contains(&solved[&lower].rotation));
    p.validate().unwrap();
}

#[test]
fn aseprite_all_clips_roundtrip_names_directions_holds_and_frame_pixels() {
    let mut rig = project::templates()[0].clone();
    rig.clips[0].name = "Walk".into();
    rig.clips[0].length = 2;
    rig.clips[0].looped = false;
    rig.clips[0].durations.insert(2, 250);
    let mut second = rig.clips[0].clone();
    second.name = "Return".into();
    second.length = 3;
    second.looped = true;
    second.playback = Playback::Reverse;
    second.durations = [(1, 70), (2, 80), (3, 90)].into_iter().collect();
    let root = rig.bones.iter().find(|b| b.parent.is_none()).unwrap().id;
    second.set_key(root, Channel::X, 3., 3.).unwrap();
    rig.clips.push(second);
    let file =
        asefile::AsepriteFile::read(Cursor::new(aseprite::encode_all(&rig).unwrap())).unwrap();
    assert_eq!(file.num_frames(), 5);
    assert_eq!(file.num_tags(), 2);
    assert_eq!(
        (
            file.tag(0).name(),
            file.tag(0).from_frame(),
            file.tag(0).to_frame()
        ),
        ("Walk", 0, 1)
    );
    assert_eq!(file.tag(0).repeat().unwrap().get(), 1);
    assert_eq!(
        (
            file.tag(1).name(),
            file.tag(1).from_frame(),
            file.tag(1).to_frame()
        ),
        ("Return", 2, 4)
    );
    assert_eq!(
        file.tag(1).animation_direction(),
        asefile::AnimationDirection::Reverse
    );
    assert_eq!(
        (0..5).map(|f| file.frame(f).duration()).collect::<Vec<_>>(),
        vec![100, 250, 70, 80, 90]
    );
    for (output, (clip, frame)) in [(0, 1), (0, 2), (1, 1), (1, 2), (1, 3)]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            file.frame(output as u32).image().as_raw(),
            skin::render(&rig, Some(&rig.clips[clip].sample(frame as f32))).as_raw()
        );
    }
}

#[test]
fn sequence_validation_counts_shared_image_buffers_for_every_frame() {
    let frame = Reference {
        width: 1280,
        height: 1280,
        pixels: Arc::new(vec![0; 1280 * 1280 * 4]),
        x: 0.,
        y: 0.,
        scale: 1.,
        opacity: 0.5,
    };
    let mut p = Project::new(rebonebase::model::Rig::new("Budget", 32, 32));
    p.extras.references = Some(ReferenceSequence {
        frames: vec![frame; 11],
        fps: 12.,
        start_ms: 0,
        looped: true,
    });
    assert!(p.validate().is_err());
}

#[test]
fn grounded_leg_targets_the_visible_sole_instead_of_sinking_the_ankle() {
    let mut p = Project::new(project::templates()[0].clone());
    let lower = p.rig.bones.iter().find(|b| b.name == "shin_L").unwrap().id;
    let target = p
        .rig
        .world(None)
        .into_iter()
        .find(|w| w.id == lower)
        .unwrap()
        .tail;
    p.extras.ground = Some(60.);
    p.extras.pins.push(Pin {
        lower,
        target,
        enabled: true,
        grounded: true,
        bend: 1.,
    });
    let solved = features::solve_pins(&p.rig, &p.extras, &p.rig.active_clip().sample(1.));
    let world = p.rig.world(Some(&solved));
    let feet: Vec<_> = p
        .rig
        .bones
        .iter()
        .filter(|b| b.parent == Some(lower) && b.connected)
        .collect();
    assert!(!feet.is_empty());
    let sole = feet
        .iter()
        .flat_map(|b| {
            let w = world.iter().find(|w| w.id == b.id).unwrap();
            [w.head.y + b.radius_head, w.tail.y + b.radius_tail]
        })
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (sole - 60.).abs() < 0.1,
        "Visible sole ended at {sole}, not reachable floor 60"
    );
}
