use rebonebase::{
    animation::Channel,
    atlas::AtlasOptions,
    character3d::{Character, Material3d, Model3d, Primitive3d},
    directions::{self, Count, DirectionExport, DirectionSet, Slot, Source},
    jobs::Progress,
    model::{Rig, Vec2},
    production::{BoxTrack, EventMarker, engine_export::EngineTarget},
    project::Project,
    sprite_package::{self, SpritePackage},
};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

type Files = HashMap<PathBuf, Vec<u8>>;

fn plan(p: &Project, export: &DirectionExport) -> (Files, SpritePackage) {
    let before = serde_json::to_value(p).unwrap();
    let files: Files = directions::plan(p, export, &Progress::default())
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(
        serde_json::to_value(p).unwrap(),
        before,
        "exporting directions must leave the project unchanged"
    );
    let package = sprite_package::parse(&files[&PathBuf::from("package.json")]).unwrap();
    (files, package)
}
fn page(files: &Files, package: &SpritePackage, i: usize) -> image::RgbaImage {
    image::load_from_memory(&files[&PathBuf::from(&package.pages[i].file)])
        .unwrap()
        .to_rgba8()
}
/// The untrimmed source frame `n` of `clip`.
fn frame(files: &Files, package: &SpritePackage, clip: usize, n: usize) -> image::RgbaImage {
    let f = &package.clips[clip].frames[n];
    let r = f.rect.unwrap();
    let crop =
        image::imageops::crop_imm(&page(files, package, f.page), r.x, r.y, r.w, r.h).to_image();
    let mut out = image::RgbaImage::new(f.source_size.w, f.source_size.h);
    image::imageops::replace(&mut out, &crop, f.trim.x as i64, f.trim.y as i64);
    out
}
fn assert_consistent(package: &SpritePackage, count: usize) {
    assert_eq!(package.clips.len() % count, 0);
    for set in package.clips.chunks(count) {
        let first = &set[0];
        for clip in set {
            assert!(clip.direction.is_some());
            assert_eq!(clip.frames.len(), first.frames.len(), "{}", clip.animation);
            assert_eq!(clip.looped, first.looped);
            for (a, b) in clip.frames.iter().zip(&first.frames) {
                assert_eq!(a.duration_ms, b.duration_ms);
                assert_eq!(a.source_size, b.source_size);
            }
        }
    }
}
fn no_clipping(image: &image::RgbaImage) -> bool {
    let (w, h) = image.dimensions();
    image
        .enumerate_pixels()
        .all(|(x, y, p)| p[3] == 0 || (x > 0 && y > 0 && x + 1 < w && y + 1 < h))
}

/// A 2D rig with distinct right- and down-facing clips.
fn flat() -> Project {
    let mut rig = Rig::new("Flat", 40, 32);
    let body = rig.add_part("Body", [60, 120, 220, 255]);
    let hand = rig.add_part("Hand", [240, 200, 40, 255]);
    let root = rig
        .add_bone(Vec2::new(12., 22.), Vec2::new(20., 22.), None)
        .unwrap();
    let arm = rig
        .add_bone(Vec2::new(20., 22.), Vec2::new(26., 18.), Some(root))
        .unwrap();
    rig.bone_mut(root).unwrap().part = body;
    rig.bone_mut(arm).unwrap().part = hand;
    let walk = &mut rig.clips[0];
    walk.name = "Walk right".into();
    walk.length = 4;
    walk.durations.insert(2, 180);
    walk.set_key(arm, Channel::Rotation, 1., 0.).unwrap();
    walk.set_key(arm, Channel::Rotation, 4., 50.).unwrap();
    let mut down = rig.clips[0].clone();
    down.name = "Walk down".into();
    down.set_key(arm, Channel::Rotation, 4., -60.).unwrap();
    let mut up = rig.clips[0].clone();
    up.name = "Walk up".into();
    let short = rebonebase::animation::Clip {
        name: "Short".into(),
        length: 3,
        ..Default::default()
    };
    rig.clips.extend([down, up, short]);
    let mut p = Project::new(rig);
    p.extras.production.pivot = Vec2::new(0.4, 0.9);
    p.extras.production.events.push(EventMarker {
        clip: 0,
        frame: 2.,
        name: "step".into(),
        payload: String::new(),
    });
    p.extras.production.boxes.push(BoxTrack {
        id: 5,
        bone: Some(arm),
        end: 4,
        offset: Vec2::new(3., 0.),
        size: Vec2::new(6., 3.),
        angle: 20.,
        ..Default::default()
    });
    p
}
fn flat_export(slots: Vec<Slot>, count: Count) -> DirectionExport {
    DirectionExport {
        set: DirectionSet {
            count,
            start_angle: 0.,
            source: Source::Clips {
                name: "Walk".into(),
                slots,
            },
        },
        clips: vec![],
        engine: EngineTarget::Generic,
        atlas: AtlasOptions::default(),
    }
}

#[test]
fn compass_presets_name_and_order_directions() {
    let ids = |count, start| {
        directions::directions(count, start)
            .into_iter()
            .map(|d| d.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(Count::Four, 0.), ["e", "s", "w", "n"]);
    assert_eq!(ids(Count::Four, 45.), ["se", "sw", "nw", "ne"]);
    assert_eq!(
        ids(Count::Eight, 0.),
        ["e", "se", "s", "sw", "w", "nw", "n", "ne"]
    );
    assert_eq!(ids(Count::Four, 90.), ["s", "w", "n", "e"]);
    let angles: Vec<_> = directions::directions(Count::Four, -90.)
        .iter()
        .map(|d| d.angle)
        .collect();
    assert_eq!(angles, [270., 0., 90., 180.]);
}

#[test]
fn flat_sets_use_authored_clips_and_explicit_mirrors() {
    let p = flat();
    // e = Walk right, s = Walk down, w = mirror of e, n = Walk up.
    let slots = vec![Slot::Clip(0), Slot::Clip(1), Slot::Mirror(0), Slot::Clip(2)];
    for atlas in [
        AtlasOptions::default(),
        AtlasOptions {
            packed: true,
            ..Default::default()
        },
    ] {
        let mut export = flat_export(slots.clone(), Count::Four);
        export.atlas = atlas;
        let (files, package) = plan(&p, &export);
        assert_consistent(&package, 4);
        let names: Vec<_> = package.clips.iter().map(|c| c.animation.as_str()).collect();
        assert_eq!(names, ["Walk_e", "Walk_s", "Walk_w", "Walk_n"]);
        assert!(package.clips.iter().all(|c| c.name == "Walk"));
        let dirs: Vec<_> = package
            .clips
            .iter()
            .map(|c| c.direction.clone().unwrap().angle)
            .collect();
        assert_eq!(dirs, [0., 90., 180., 270.]);
        // Mirroring centers the canvas on the pivot so the anchor cannot move.
        assert_eq!(package.pivot.x * 2., package.frame_size.w as f32);
        for n in 0..4 {
            let east = frame(&files, &package, 0, n);
            let west = frame(&files, &package, 2, n);
            assert_eq!(west, image::imageops::flip_horizontal(&east), "frame {n}");
            assert!(no_clipping(&east) && no_clipping(&frame(&files, &package, 1, n)));
            let (e, w) = (&package.clips[0].frames[n], &package.clips[2].frames[n]);
            assert_eq!(e.events, w.events);
            assert_eq!(e.boxes.len(), 1);
            let width = package.frame_size.w as f32;
            assert!((w.boxes[0].center.x - (width - e.boxes[0].center.x)).abs() < 1e-3);
            assert_eq!(w.boxes[0].angle, -e.boxes[0].angle);
            assert_eq!(w.boxes[0].center.y, e.boxes[0].center.y);
        }
        assert_eq!(package.clips[2].frames[1].duration_ms, 180);
        assert_eq!(package.clips[2].frames[1].events[0].name, "step");
    }
}

#[test]
fn flat_sets_reject_incomplete_or_inconsistent_assignments() {
    let p = flat();
    let err = |slots, count| {
        directions::plan(&p, &flat_export(slots, count), &Progress::default())
            .unwrap_err()
            .to_string()
    };
    assert!(err(vec![Slot::Clip(0); 3], Count::Four).contains("each of the 4"));
    assert!(
        err(
            vec![Slot::Clip(0), Slot::Empty, Slot::Clip(1), Slot::Clip(2)],
            Count::Four
        )
        .contains("direction s")
    );
    assert!(
        err(
            vec![
                Slot::Clip(0),
                Slot::Mirror(2),
                Slot::Mirror(0),
                Slot::Clip(1)
            ],
            Count::Four
        )
        .contains("must mirror")
    );
    assert!(
        err(
            vec![Slot::Clip(0), Slot::Clip(3), Slot::Clip(1), Slot::Clip(2)],
            Count::Four
        )
        .contains("different frame count")
    );
    let mut fixed = p.clone();
    fixed.extras.auto_canvas = false;
    let slots = vec![Slot::Clip(0), Slot::Clip(1), Slot::Mirror(0), Slot::Clip(2)];
    let e = directions::plan(
        &fixed,
        &flat_export(slots, Count::Four),
        &Progress::default(),
    )
    .unwrap_err();
    assert!(e.to_string().contains("Pivot X"), "{e}");
}

/// A cube with a long red nose along +Z, the model's forward axis.
fn nosed() -> Project {
    let mut p = Project::new(Rig::new("Solid", 48, 48));
    p.extras.show_2d = false;
    let mut model = Model3d::default();
    let cube = model.primitives[0].clone();
    let nose = Primitive3d {
        positions: cube
            .positions
            .iter()
            .map(|[x, y, z]| [x * 0.3, 0.4 + y * 0.3, 0.5 + (z + 0.5) * 1.4])
            .collect(),
        material: Material3d {
            color: [0.9, 0.1, 0.1, 1.],
            ..Default::default()
        },
        ..cube
    };
    model.primitives.push(nose);
    let c = Character {
        position: Vec2::new(24., 36.),
        scale: 10.,
        pitch: 20.,
        model: Arc::new(model),
        ..Default::default()
    };
    p.extras.characters3d.push(c);
    p.rig.clips[0].name = "Turn".into();
    p.rig.clips[0].length = 3;
    p.rig.clips[0].durations.insert(2, 90);
    p
}
fn camera_export(count: Count) -> DirectionExport {
    DirectionExport {
        set: DirectionSet {
            count,
            start_angle: 0.,
            source: Source::Camera {
                facing: directions::FACING_VIEWER,
            },
        },
        clips: vec![0],
        engine: EngineTarget::Godot,
        atlas: AtlasOptions {
            packed: true,
            ..Default::default()
        },
    }
}
fn red_extent(image: &image::RgbaImage) -> (u32, u32) {
    let xs: Vec<u32> = image
        .enumerate_pixels()
        .filter(|(_, _, p)| p[3] > 0 && p[0] > 150 && p[1] < 80)
        .map(|(x, _, _)| x)
        .collect();
    (*xs.iter().min().unwrap(), *xs.iter().max().unwrap())
}

#[test]
fn eight_camera_directions_share_canvas_timing_and_anchor_without_clipping() {
    let p = nosed();
    let (files, package) = plan(&p, &camera_export(Count::Eight));
    assert_eq!(package.clips.len(), 8);
    assert_consistent(&package, 8);
    let ids: Vec<_> = package
        .clips
        .iter()
        .map(|c| c.direction.clone().unwrap().id)
        .collect();
    assert_eq!(ids, ["e", "se", "s", "sw", "w", "nw", "n", "ne"]);
    assert_eq!(package.clips[3].animation, "Turn_sw");
    assert_eq!(package.clips[3].frames[1].duration_ms, 90);
    for (i, clip) in package.clips.iter().enumerate() {
        for n in 0..clip.frames.len() {
            assert!(
                no_clipping(&frame(&files, &package, i, n)),
                "{} {n}",
                clip.animation
            );
        }
    }
    // The shared envelope keeps the character's world anchor on one canvas pixel.
    let views = directions::views(&p, &camera_export(Count::Eight).set).unwrap();
    let first = &views[0].project;
    for v in &views {
        assert_eq!(v.project.extras.canvas_origin, first.extras.canvas_origin);
        assert_eq!(
            (v.project.rig.width, v.project.rig.height),
            (first.rig.width, first.rig.height)
        );
        assert_eq!(
            v.project.extras.production.pivot,
            first.extras.production.pivot
        );
    }
    // The nose points the named way: right for e, left for w.
    let center_x = p.extras.characters3d[0].position.x - first.extras.canvas_origin.x;
    let (e_min, e_max) = red_extent(&frame(&files, &package, 0, 0));
    let (w_min, w_max) = red_extent(&frame(&files, &package, 4, 0));
    assert!(
        e_min as f32 > center_x - 2. && e_max as f32 > center_x + 8.,
        "e {e_min}..{e_max} vs {center_x}"
    );
    assert!(
        (w_max as f32) < center_x + 2. && (w_min as f32) < center_x - 8.,
        "w {w_min}..{w_max} vs {center_x}"
    );
    assert_eq!(
        directions::frame_count(&p, &camera_export(Count::Eight)),
        8 * 3
    );
}

#[test]
fn camera_sets_need_a_3d_character_and_previews_cover_each_direction() {
    let flat = flat();
    let err =
        directions::plan(&flat, &camera_export(Count::Four), &Progress::default()).unwrap_err();
    assert!(err.to_string().contains("3D character"), "{err}");
    let p = nosed();
    let previews = directions::preview(&p, &camera_export(Count::Four).set, 0, 1.).unwrap();
    assert_eq!(previews.len(), 4);
    let size = previews[0].1.dimensions();
    assert!(previews.iter().all(|(_, image)| image.dimensions() == size));
    assert_ne!(previews[0].1, previews[2].1, "east and west views differ");
}
