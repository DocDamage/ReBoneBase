use rebonebase::{
    animation::{Channel, Playback},
    aseprite,
    batch::{self, ExportFormat, ExportPreset},
    model::{Rig, Vec2},
    production::{
        self, Attachment, BoxTrack, Correction, EventMarker, FaceKey, FaceTrack, Look, SceneActor,
        SpriteAsset,
    },
    project::{self, Project, Reference},
};
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn scene() -> Project {
    let mut rig = Rig::new("Production", 24, 24);
    let body = rig.add_part("Body", [240, 20, 30, 255]);
    let arm = rig.add_part("Arm", [20, 40, 240, 255]);
    let root = rig
        .add_bone(Vec2::new(7., 12.), Vec2::new(15., 12.), None)
        .unwrap();
    let child = rig
        .add_bone(Vec2::new(15., 12.), Vec2::new(18., 16.), Some(root))
        .unwrap();
    rig.bone_mut(root).unwrap().part = body;
    rig.bone_mut(child).unwrap().part = arm;
    rig.clips[0].length = 3;
    rig.clips[0].name = "Forward".into();
    rig.clips[0].durations.insert(2, 370);
    rig.clips[0]
        .set_key(child, Channel::Rotation, 3., 15.)
        .unwrap();
    let mut back = rig.clips[0].clone();
    back.name = "Back".into();
    back.playback = Playback::Reverse;
    back.looped = false;
    back.durations.insert(1, 120);
    rig.clips.push(back);
    let mut p = Project::new(rig);
    let asset = |id, color| SpriteAsset {
        id,
        name: format!("Asset {id}"),
        image: Reference {
            width: 2,
            height: 2,
            pixels: Arc::new([color; 4].concat()),
            x: 0.,
            y: 0.,
            scale: 1.,
            opacity: 1.,
        },
    };
    p.extras.production.assets = vec![asset(1, [255, 255, 0, 255]), asset(2, [0, 255, 255, 255])];
    p.extras.production.attachments.push(Attachment {
        id: 3,
        asset: 1,
        offset: Vec2::new(1., 1.),
        ..Default::default()
    });
    p.extras.production.faces.push(FaceTrack {
        id: 4,
        clip: 0,
        offset: Vec2::new(3., 1.),
        keys: vec![FaceKey {
            frame: 1.,
            asset: 2,
            offset: Vec2::default(),
        }],
        ..Default::default()
    });
    p.extras.production.meshes.push(
        production::grid_mesh(
            &p.rig,
            5,
            2,
            Vec2::new(12., 1.),
            Vec2::new(4., 4.),
            1,
            1,
            &[root],
        )
        .unwrap(),
    );
    let mut actor_rig = Rig::new("Other actor", 4, 4);
    let part = actor_rig.add_part("Other", [100, 30, 150, 255]);
    let b = actor_rig
        .add_bone(Vec2::new(1., 1.), Vec2::new(3., 1.), None)
        .unwrap();
    actor_rig.bone_mut(b).unwrap().part = part;
    p.extras.production.actors.push(SceneActor {
        id: 6,
        project: Box::new(Project::new(actor_rig)),
        offset: Vec2::new(18., 17.),
        ..Default::default()
    });
    p.extras.production.looks.push(Look {
        name: "Green outfit".into(),
        colors: [(body, [30, 240, 40, 255])].into(),
        attachments: vec![3],
        meshes: vec![5],
    });
    p.extras.production.active_look = Some(0);
    p.extras.production.corrections.push(Correction {
        clip: 0,
        frame: 1,
        pixels: [(0, [255, 0, 255, 255])].into(),
    });
    p.extras.production.boxes.push(BoxTrack {
        id: 7,
        end: 3,
        ..Default::default()
    });
    p.extras.production.events.push(EventMarker {
        clip: 0,
        frame: 2.,
        name: "Step".into(),
        payload: "left".into(),
    });
    rebonebase::canvas::grow_all(&mut p).unwrap();
    p.validate().unwrap();
    p
}
fn parse(p: &Project, all: bool) -> asefile::AsepriteFile {
    asefile::AsepriteFile::read(Cursor::new(aseprite::encode_project(p, all).unwrap())).unwrap()
}
fn folder(label: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "rebone-production-export-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn same_pixels(a: &image::RgbaImage, b: &image::RgbaImage) {
    assert_eq!(a.dimensions(), b.dimensions());
    if let Some((i, (left, right))) = a
        .pixels()
        .zip(b.pixels())
        .enumerate()
        .find(|(_, (a, b))| a != b)
    {
        panic!(
            "pixel ({}, {}) differs: {left:?} != {right:?}",
            i as u32 % a.width(),
            i as u32 / a.width()
        );
    }
}

#[test]
fn aseprite_scene_layer_preserves_every_rendered_feature_and_frame_hold() {
    let mut p = scene();
    rebonebase::canvas::grow_all(&mut p).unwrap();
    let file = parse(&p, false);
    assert_eq!(file.num_layers(), 1);
    assert_eq!(file.layer(0).name(), "Composited scene");
    assert_eq!(file.num_frames(), 3);
    for f in 0..3 {
        assert_eq!(
            file.frame(f).duration(),
            p.rig.active_clip().duration_ms(f + 1)
        );
        assert_eq!(
            file.frame(f).image().as_raw(),
            p.render_at((f + 1) as f32).as_raw()
        );
    }
}

#[test]
fn aseprite_scene_all_clips_preserve_tags_directions_and_canonical_frame_pixels() {
    let mut p = scene();
    p.rig.active_clip = 1;
    let file = parse(&p, true);
    assert_eq!(file.num_frames(), 6);
    assert_eq!(file.num_tags(), 2);
    assert_eq!(
        (
            file.tag(0).name(),
            file.tag(0).from_frame(),
            file.tag(0).to_frame()
        ),
        ("Forward", 0, 2)
    );
    assert_eq!(
        (
            file.tag(1).name(),
            file.tag(1).from_frame(),
            file.tag(1).to_frame()
        ),
        ("Back", 3, 5)
    );
    assert_eq!(
        file.tag(1).animation_direction(),
        asefile::AnimationDirection::Reverse
    );
    assert_eq!(file.tag(1).repeat().unwrap().get(), 1);
    for index in 0..2 {
        p.rig.active_clip = index;
        for frame in 1..=3 {
            let f = file.frame((index * 3) as u32 + frame - 1);
            assert_eq!(f.duration(), p.rig.active_clip().duration_ms(frame));
            assert_eq!(f.image().as_raw(), p.render_at(frame as f32).as_raw());
        }
    }
}

#[test]
fn projects_without_art_keep_editable_group_layers_and_baked_motion() {
    let mut p = scene();
    p.extras.production = Default::default();
    let child = p.rig.bones[1].id;
    p.extras
        .production
        .motion
        .springs
        .push(rebonebase::advanced_motion::Spring {
            bone: child,
            gravity: 40.,
            ..Default::default()
        });
    rebonebase::canvas::grow_all(&mut p).unwrap();
    let file = parse(&p, true);
    assert_eq!(file.num_layers(), 2);
    assert_eq!(file.layer(0).name(), "Body");
    assert_eq!(file.layer(1).name(), "Arm");
    for index in 0..2 {
        p.rig.active_clip = index;
        for frame in 1..=3 {
            assert_eq!(
                file.frame(index as u32 * 3 + frame - 1).image().as_raw(),
                p.render_at(frame as f32).as_raw()
            );
        }
    }
}

#[test]
fn scene_export_does_not_require_a_skin_group_and_rejects_aggregate_work() {
    let mut p = scene();
    p.rig.parts.clear();
    for b in &mut p.rig.bones {
        b.part = 0;
    }
    p.extras.production.looks.clear();
    p.extras.production.active_look = None;
    assert_eq!(parse(&p, false).num_layers(), 1);
    p.rig.width = 512;
    p.rig.height = 512;
    while p.rig.clips.len() < 32 {
        p.rig.clips.push(p.rig.clips[0].clone());
    }
    assert!(aseprite::encode_project(&p, true).is_err());
    p.rig.width = 0;
    assert!(aseprite::encode_project(&p, false).is_err());
}

#[test]
fn batch_raster_formats_and_metadata_use_the_rendered_project() {
    for format in [
        ExportFormat::Sheet,
        ExportFormat::IndexedSheet,
        ExportFormat::Frames,
        ExportFormat::Aseprite,
        ExportFormat::Gif,
    ] {
        let mut p = scene();
        p.extras.palette.clear();
        let dir = folder(&format!("{format:?}"));
        let preset = ExportPreset {
            format,
            columns: 2,
            all_clips: false,
            ..Default::default()
        };
        let report = batch::export(&p, &dir, &preset, false).unwrap();
        let atlas: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("atlas.json")).unwrap()).unwrap();
        let clip = &atlas["clips"][0];
        assert_eq!(clip["frames"].as_array().unwrap().len(), 3);
        assert_eq!(clip["frames"][1]["events"][0]["name"], "Step");
        assert_eq!(clip["frames"][0]["boxes"][0]["id"], 7);
        let file = report
            .files
            .iter()
            .find(|f| f.extension().is_some_and(|e| e != "json"))
            .unwrap();
        match format {
            ExportFormat::Sheet | ExportFormat::IndexedSheet => same_pixels(
                &image::open(file).unwrap().to_rgba8(),
                &project::sheet_project(&p, 2).unwrap(),
            ),
            ExportFormat::Frames => {
                same_pixels(&image::open(file).unwrap().to_rgba8(), &p.render_at(1.))
            }
            ExportFormat::Aseprite => {
                let file =
                    asefile::AsepriteFile::read(Cursor::new(std::fs::read(file).unwrap())).unwrap();
                assert_eq!(file.frame(0).image().as_raw(), p.render_at(1.).as_raw());
            }
            ExportFormat::Gif => {
                use image::AnimationDecoder;
                let frames =
                    image::codecs::gif::GifDecoder::new(Cursor::new(std::fs::read(file).unwrap()))
                        .unwrap()
                        .into_frames()
                        .collect_frames()
                        .unwrap();
                assert_eq!(frames.len(), 3);
                same_pixels(frames[0].buffer(), &p.render_at(1.));
                assert_eq!(frames[1].delay().numer_denom_ms(), (370, 1));
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn batch_all_clips_checks_the_aggregate_budget_before_writing_files() {
    let mut p = scene();
    p.rig.width = 512;
    p.rig.height = 512;
    while p.rig.clips.len() < 32 {
        p.rig.clips.push(p.rig.clips[0].clone());
    }
    let dir = folder("budget");
    assert!(batch::export(&p, &dir, &ExportPreset::default(), false).is_err());
    assert!(!dir.exists() || std::fs::read_dir(&dir).unwrap().next().is_none());
    if dir.exists() {
        std::fs::remove_dir_all(dir).unwrap();
    }
}
