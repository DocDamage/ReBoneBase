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
    assert_eq!(file.num_layers(), 7);
    assert_eq!(file.layer(0).name(), "Body");
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
    assert_eq!(parse(&p, false).num_layers(), 5);
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

/// A cel as stored, with its frame z-index.
struct RawCel {
    layer: usize,
    x: i16,
    y: i16,
    z: i16,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
struct RawFile {
    layers: Vec<String>,
    notes: Vec<(usize, String)>,
    frames: Vec<(u16, Vec<RawCel>)>,
}
/// Minimal independent reader for the parts asefile ignores: cel z-index,
/// cel position and layer user data.
fn read_raw(bytes: &[u8]) -> RawFile {
    let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let i16_at = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let mut file = RawFile {
        layers: vec![],
        notes: vec![],
        frames: vec![],
    };
    let mut at = 128;
    for _ in 0..u16_at(6) {
        let size = u32_at(at) as usize;
        let chunks = u32_at(at + 12);
        let duration = u16_at(at + 8);
        let mut chunk = at + 16;
        let mut cels = vec![];
        for _ in 0..chunks {
            let length = u32_at(chunk) as usize;
            let data = chunk + 6;
            match u16_at(chunk + 4) {
                0x2004 => {
                    let n = u16_at(data + 16) as usize;
                    file.layers
                        .push(String::from_utf8(bytes[data + 18..data + 18 + n].to_vec()).unwrap());
                }
                0x2020 => {
                    let n = u16_at(data + 4) as usize;
                    file.notes.push((
                        file.layers.len() - 1,
                        String::from_utf8(bytes[data + 6..data + 6 + n].to_vec()).unwrap(),
                    ));
                }
                0x2005 => {
                    assert_eq!(u16_at(data + 7), 2);
                    assert_eq!(bytes[data + 6], 255);
                    let mut pixels = vec![];
                    std::io::Read::read_to_end(
                        &mut flate2::read::ZlibDecoder::new(&bytes[data + 20..chunk + length]),
                        &mut pixels,
                    )
                    .unwrap();
                    let cel = RawCel {
                        layer: u16_at(data) as usize,
                        x: i16_at(data + 2),
                        y: i16_at(data + 4),
                        z: i16_at(data + 9),
                        width: u16_at(data + 16) as u32,
                        height: u16_at(data + 18) as u32,
                        pixels,
                    };
                    assert_eq!(cel.pixels.len() as u32, cel.width * cel.height * 4);
                    cels.push(cel);
                }
                _ => {}
            }
            chunk += length;
        }
        assert_eq!(chunk, at + size);
        file.frames.push((duration, cels));
        at += size;
    }
    file
}
/// Straight-alpha "over", as the native renderer composites.
fn over(dst: &mut image::Rgba<u8>, src: [u8; 4]) {
    if src[3] == 0 {
        return;
    }
    let a = src[3] as f32 / 255.;
    let b = dst[3] as f32 / 255.;
    let alpha = a + b * (1. - a);
    for i in 0..3 {
        dst[i] = ((src[i] as f32 * a + dst[i] as f32 * b * (1. - a)) / alpha).round() as u8;
    }
    dst[3] = (alpha * 255.).round() as u8;
}
/// Flatten one frame the way Aseprite orders cels: layer index plus z-index.
fn flatten(file: &RawFile, frame: usize, width: u32, height: u32) -> image::RgbaImage {
    let mut cels: Vec<&RawCel> = file.frames[frame].1.iter().collect();
    cels.sort_by_key(|c| (c.layer as i32 + c.z as i32, c.z));
    let mut out = image::RgbaImage::new(width, height);
    for c in cels {
        for y in 0..c.height {
            for x in 0..c.width {
                let i = ((y * c.width + x) * 4) as usize;
                over(
                    out.get_pixel_mut(c.x as u32 + x, c.y as u32 + y),
                    c.pixels[i..i + 4].try_into().unwrap(),
                );
            }
        }
    }
    out
}
fn assert_flattens_to_native(p: &Project, all: bool) -> RawFile {
    let mut prepared = p.clone();
    rebonebase::canvas::grow_all(&mut prepared).unwrap();
    let file = read_raw(&aseprite::encode_project(p, all).unwrap());
    let clips: Vec<usize> = if all {
        (0..p.rig.clips.len()).collect()
    } else {
        vec![p.rig.active_clip]
    };
    let mut q = prepared.clone();
    let mut index = 0;
    for clip in clips {
        q.rig.active_clip = clip;
        for frame in 1..=q.rig.clips[clip].length {
            assert_eq!(
                file.frames[index].0 as u32,
                q.rig.active_clip().duration_ms(frame)
            );
            same_pixels(
                &flatten(&file, index, q.rig.width, q.rig.height),
                &q.render_at(frame as f32),
            );
            index += 1;
        }
    }
    assert_eq!(index, file.frames.len());
    file
}

#[test]
fn production_scenes_export_one_layer_per_component_and_flatten_to_native_frames() {
    let mut p = scene();
    p.rig.active_clip = 0;
    let file = assert_flattens_to_native(&p, true);
    assert_eq!(
        file.layers,
        [
            "Body",
            "Mesh: Deformable artwork",
            "Actor: Actor",
            "Arm",
            "Prop: Prop",
            "Face: Eyes / mouth",
            "Pixel corrections"
        ]
    );
    // The actor is the only merged component, and the file says so.
    assert_eq!(file.notes.len(), 1);
    assert_eq!(file.notes[0].0, 2);
    let (_, report) = aseprite::encode_project_report(&p, true).unwrap();
    assert_eq!(report.layers, file.layers);
    assert_eq!(report.flattened, [file.notes[0].1.clone()]);
    // Cels are cropped and the face track appears only in its own clip.
    assert!(
        file.frames[0]
            .1
            .iter()
            .all(|c| c.width < p.rig.width || c.height < p.rig.height)
    );
    assert!(
        file.frames[3..]
            .iter()
            .all(|f| f.1.iter().all(|c| c.layer != 5))
    );
    // An independent reader agrees on layers, frames and tags.
    let ase = parse(&p, true);
    assert_eq!(ase.num_layers(), 7);
    assert_eq!(ase.num_frames(), 6);
    assert_eq!(ase.num_tags(), 2);
    assert_eq!(ase.tag(1).name(), "Back");
}

#[test]
fn animated_draw_order_semi_transparent_art_and_erased_pixels_flatten_exactly() {
    let mut p = scene();
    let (body, arm) = (p.rig.parts[0].id, p.rig.parts[1].id);
    p.rig.parts[0].z = 0;
    p.rig.parts[1].z = 1;
    // The arm passes behind the body on frame 2 and in front again on 3.
    for (frame, z) in [(1., 1.), (2., -1.), (3., 1.)] {
        p.rig.clips[0].set_key(arm, Channel::Z, frame, z).unwrap();
    }
    p.rig.clips[0].set_key(body, Channel::Z, 1., 0.).unwrap();
    Arc::make_mut(&mut p.extras.production.assets[0].image.pixels)
        .chunks_mut(4)
        .for_each(|px| px[3] = 128);
    // Erase a pixel and paint a translucent one over the art.
    p.extras.production.corrections[0]
        .pixels
        .extend([(30, [0, 0, 0, 0]), (31, [10, 200, 10, 90])]);
    p.validate().unwrap();
    let file = assert_flattens_to_native(&p, false);
    assert!(file.frames[1].1.iter().any(|c| c.z != 0));
}

#[test]
fn scenes_over_the_layered_budget_merge_into_one_reported_layer() {
    let mut p = scene();
    p.rig.width = 2300;
    p.rig.height = 2300;
    let prop = p.extras.production.attachments[0].clone();
    for id in 20..26 {
        p.extras
            .production
            .attachments
            .push(Attachment { id, ..prop.clone() });
        p.extras.production.looks[0].attachments.push(id);
    }
    p.validate().unwrap();
    let (bytes, report) = aseprite::encode_project_report(&p, false).unwrap();
    assert_eq!(report.layers, ["Composited scene"]);
    assert!(report.flattened[0].starts_with("Whole scene"));
    assert_eq!(read_raw(&bytes).layers, ["Composited scene"]);
}

#[test]
fn the_export_job_reports_merged_components_in_its_summary() {
    use rebonebase::export::{self, ExportRequest, ExportSummary, Target};
    let p = scene();
    let request = ExportRequest {
        target: Target::Aseprite { all_clips: false },
        destination: "scene.aseprite".into(),
        overwrite: false,
        frame: 1.,
        pose: None,
    };
    let (files, notes) =
        export::render_with_notes(&p, &request, &rebonebase::jobs::Progress::default()).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(notes.len(), 1);
    assert!(notes[0].starts_with("Actor: Actor"));
    let summary = ExportSummary {
        files: vec!["scene.aseprite".into()],
        bytes: 1,
        notes,
    };
    assert!(
        summary
            .describe(std::path::Path::new("scene.aseprite"))
            .contains("merged into this layer")
    );
}

#[test]
fn rendered_3d_characters_get_their_own_layer_beside_2d_artwork() {
    let mut p = scene();
    let mut c = rebonebase::character3d::load(std::path::Path::new(
        "src/character3d/fixtures/textured_skin.glb",
    ))
    .unwrap();
    c.name = "Hero".into();
    c.z = 50;
    p.extras.characters3d.push(c);
    rebonebase::canvas::grow_all(&mut p).unwrap();
    p.validate().unwrap();
    let file = assert_flattens_to_native(&p, false);
    assert_eq!(file.layers[file.layers.len() - 2], "3D: Hero");
    let layer = file.layers.len() - 2;
    assert!(
        file.frames
            .iter()
            .any(|f| f.1.iter().any(|c| c.layer == layer))
    );
}
