use rebonebase::{
    animation::{Channel, Playback},
    atlas::{self, AtlasOptions, Packer},
    batch::{self, ExportFormat, ExportPreset},
    jobs::Progress,
    model::{Rig, Vec2},
    production::{
        BoxTrack, EventMarker,
        engine_export::{EngineTarget, plan_package, plan_package_with},
    },
    project::Project,
    sprite_package::{self, SpritePackage},
};
use std::{collections::HashMap, path::PathBuf};

/// A small figure moving across a mostly empty canvas.
fn sparse() -> Project {
    let mut rig = Rig::new("Sparse", 96, 64);
    let part = rig.add_part("Body", [40, 200, 90, 255]);
    let arm = rig.add_part("Arm", [230, 60, 40, 255]);
    let root = rig
        .add_bone(Vec2::new(20., 40.), Vec2::new(28., 40.), None)
        .unwrap();
    let child = rig
        .add_bone(Vec2::new(28., 40.), Vec2::new(32., 36.), Some(root))
        .unwrap();
    rig.bone_mut(root).unwrap().part = part;
    rig.bone_mut(child).unwrap().part = arm;
    let clip = &mut rig.clips[0];
    clip.name = "Wave".into();
    clip.length = 6;
    clip.playback = Playback::PingPong;
    clip.durations.insert(3, 250);
    clip.set_key(child, Channel::Rotation, 1., 0.).unwrap();
    clip.set_key(child, Channel::Rotation, 6., 70.).unwrap();
    let mut idle = rig.clips[0].clone();
    idle.name = "Idle".into();
    idle.playback = Playback::Forward;
    idle.keys_mut_clear();
    rig.clips.push(idle);
    let mut p = Project::new(rig);
    p.extras.production.pivot = Vec2::new(0.3, 0.8);
    p.extras.production.events.push(EventMarker {
        clip: 0,
        frame: 2.,
        name: "wave".into(),
        payload: String::new(),
    });
    p.extras.production.boxes.push(BoxTrack {
        id: 3,
        bone: Some(child),
        end: 6,
        size: Vec2::new(5., 3.),
        ..Default::default()
    });
    p
}
trait ClearKeys {
    fn keys_mut_clear(&mut self);
}
impl ClearKeys for rebonebase::animation::Clip {
    /// Hold the rest pose so every frame renders the same image.
    fn keys_mut_clear(&mut self) {
        let len = self.length;
        *self = rebonebase::animation::Clip {
            name: self.name.clone(),
            length: len,
            ..Default::default()
        };
    }
}

fn packed(options: AtlasOptions) -> AtlasOptions {
    AtlasOptions {
        packed: true,
        ..options
    }
}
fn package(p: &Project, layout: AtlasOptions) -> (HashMap<PathBuf, Vec<u8>>, SpritePackage) {
    let files: HashMap<_, _> =
        plan_package_with(p, EngineTarget::Generic, &layout, &Progress::default())
            .unwrap()
            .into_iter()
            .collect();
    let package = sprite_package::parse(&files[&PathBuf::from("package.json")]).unwrap();
    (files, package)
}
fn pages(files: &HashMap<PathBuf, Vec<u8>>, package: &SpritePackage) -> Vec<image::RgbaImage> {
    package
        .pages
        .iter()
        .map(|page| {
            let image = image::load_from_memory(&files[&PathBuf::from(&page.file)])
                .unwrap()
                .to_rgba8();
            assert_eq!(image.dimensions(), (page.w, page.h));
            image
        })
        .collect()
}
/// Rebuild every frame from its page and compare with a direct render.
fn assert_reconstructs(p: &Project, files: &HashMap<PathBuf, Vec<u8>>, package: &SpritePackage) {
    let images = pages(files, package);
    let mut grown = p.clone();
    rebonebase::canvas::grow_all(&mut grown).unwrap();
    for (index, clip) in package.clips.iter().enumerate() {
        grown.rig.active_clip = index;
        for (n, frame) in clip.frames.iter().enumerate() {
            let r = frame.rect.unwrap();
            assert!(r.w > 0 && r.h > 0, "frames never have empty regions");
            let crop =
                image::imageops::crop_imm(&images[frame.page], r.x, r.y, r.w, r.h).to_image();
            let mut rebuilt = image::RgbaImage::new(frame.source_size.w, frame.source_size.h);
            image::imageops::replace(
                &mut rebuilt,
                &crop,
                frame.trim.x as i64,
                frame.trim.y as i64,
            );
            assert!(
                rebuilt == grown.render_at(frame.source_frame as f32),
                "{} frame {n} differs",
                clip.animation
            );
        }
    }
}
fn page_area(package: &SpritePackage) -> u64 {
    package.pages.iter().map(|p| p.w as u64 * p.h as u64).sum()
}

#[test]
fn packed_frames_reconstruct_pixel_for_pixel_under_every_option() {
    let p = sparse();
    for options in [
        AtlasOptions::default(),
        AtlasOptions {
            trim: false,
            ..Default::default()
        },
        AtlasOptions {
            reuse: false,
            padding: 0,
            ..Default::default()
        },
        AtlasOptions {
            padding: 5,
            extrude: 2,
            ..Default::default()
        },
        AtlasOptions {
            max_size: 64,
            extrude: 1,
            ..Default::default()
        },
    ] {
        let (files, package) = package(&p, packed(options));
        assert_reconstructs(&p, &files, &package);
        for page in &package.pages {
            assert!(
                page.w <= options.max_size && page.h <= options.max_size,
                "{options:?}"
            );
        }
    }
}

#[test]
fn sparse_animations_use_less_atlas_space_than_grids() {
    let p = sparse();
    let (_, grid) = package(&p, AtlasOptions::default());
    let (_, tight) = package(&p, packed(AtlasOptions::default()));
    assert!(
        page_area(&tight) * 4 < page_area(&grid),
        "packed {} px vs grid {} px",
        page_area(&tight),
        page_area(&grid)
    );
    let (_, untrimmed) = package(
        &p,
        packed(AtlasOptions {
            trim: false,
            ..Default::default()
        }),
    );
    assert!(page_area(&tight) < page_area(&untrimmed));
}

#[test]
fn reused_images_keep_separate_timing_events_and_regions() {
    let p = sparse();
    let (_, grid) = package(&p, AtlasOptions::default());
    let (_, shared) = package(&p, packed(AtlasOptions::default()));
    let (_, separate) = package(
        &p,
        packed(AtlasOptions {
            reuse: false,
            ..Default::default()
        }),
    );
    // Ping-pong revisits frames 2–5 and Idle holds one image for six frames.
    let wave = &shared.clips[0];
    assert_eq!(wave.frames.len(), 10);
    assert_eq!(wave.frames[1].rect, wave.frames[9].rect);
    assert_eq!(wave.frames[1].page, wave.frames[9].page);
    let idle = &shared.clips[1];
    assert!(idle.frames.iter().all(|f| f.rect == idle.frames[0].rect));
    let distinct = |package: &SpritePackage| {
        let mut rects: Vec<_> = package
            .clips
            .iter()
            .flat_map(|c| &c.frames)
            .map(|f| (f.page, f.rect.map(|r| (r.x, r.y))))
            .collect();
        rects.sort();
        rects.dedup();
        rects.len()
    };
    assert!(distinct(&shared) < distinct(&separate));
    assert!(page_area(&shared) < page_area(&separate));
    // Sharing pixels never changes what each frame means.
    for (a, b) in [&shared, &separate]
        .into_iter()
        .flat_map(|pk| pk.clips.iter().zip(&grid.clips))
    {
        assert_eq!(a.frames.len(), b.frames.len());
        for (x, y) in a.frames.iter().zip(&b.frames) {
            assert_eq!(
                (
                    x.source_frame,
                    x.source_time_ms,
                    x.duration_ms,
                    &x.events,
                    &x.boxes
                ),
                (
                    y.source_frame,
                    y.source_time_ms,
                    y.duration_ms,
                    &y.events,
                    &y.boxes
                )
            );
            assert_eq!(x.source_size, y.source_size);
        }
    }
    // Both passes over source frame 2 keep their own copy of its event.
    assert_eq!(wave.frames[1].events.len(), 1);
    assert_eq!(wave.frames[9].events.len(), 1);
    assert!(wave.frames[3].events.is_empty());
    assert_eq!(wave.frames[2].duration_ms, 250);
    assert_eq!(shared.pivot, grid.pivot);
}

#[test]
fn small_texture_limits_spill_onto_additional_pages() {
    let p = sparse();
    let options = packed(AtlasOptions {
        reuse: false,
        trim: false,
        max_size: 128,
        padding: 1,
        ..Default::default()
    });
    let (files, package) = package(&p, options);
    assert!(package.pages.len() > 1, "{} pages", package.pages.len());
    assert!(package.pages.iter().all(|p| p.w <= 128 && p.h <= 128));
    assert!(files.contains_key(&PathBuf::from("atlas-02.png")));
    assert_reconstructs(&p, &files, &package);
    let too_small = packed(AtlasOptions {
        max_size: 32,
        trim: false,
        ..Default::default()
    });
    let err =
        plan_package_with(&p, EngineTarget::Godot, &too_small, &Progress::default()).unwrap_err();
    assert!(err.to_string().contains("does not fit"), "{err}");
}

#[test]
fn packed_images_are_padded_and_extruded() {
    let mut a = image::RgbaImage::new(7, 5);
    for (x, y, px) in a.enumerate_pixels_mut() {
        *px = image::Rgba([x as u8 * 30, y as u8 * 40, 200, 255]);
    }
    let b = image::RgbaImage::from_pixel(4, 9, image::Rgba([9, 8, 7, 255]));
    let options = AtlasOptions {
        packed: true,
        trim: false,
        reuse: true,
        padding: 3,
        extrude: 2,
        max_size: 64,
    };
    let mut packer = Packer::new(options).unwrap();
    let ids = [packer.add(&a), packer.add(&b), packer.add(&a)];
    assert_eq!(packer.unique_images(), 2);
    let atlas = packer.finish(&Progress::default()).unwrap();
    let (pa, pb) = (atlas.frames[ids[0]], atlas.frames[ids[1]]);
    assert_eq!(pa, atlas.frames[ids[2]]);
    let page = &atlas.pages[0];
    // Extruded borders repeat the nearest edge pixel.
    for (place, src) in [(pa, &a), (pb, &b)] {
        let r = place.rect;
        for dy in -2i32..r.h as i32 + 2 {
            for dx in -2i32..r.w as i32 + 2 {
                let sx = dx.clamp(0, r.w as i32 - 1) as u32;
                let sy = dy.clamp(0, r.h as i32 - 1) as u32;
                let px = page.get_pixel((r.x as i32 + dx) as u32, (r.y as i32 + dy) as u32);
                assert_eq!(px, src.get_pixel(sx, sy));
            }
        }
    }
    // Extruded footprints stay at least `padding` apart.
    let grow = |r: sprite_package::Rect| (r.x - 2, r.y - 2, r.x + r.w + 2, r.y + r.h + 2);
    let (a0, b0) = (grow(pa.rect), grow(pb.rect));
    let gap_x = (b0.0 as i64 - a0.2 as i64).max(a0.0 as i64 - b0.2 as i64);
    let gap_y = (b0.1 as i64 - a0.3 as i64).max(a0.1 as i64 - b0.3 as i64);
    assert!(gap_x >= 3 || gap_y >= 3, "gap {gap_x} {gap_y}");
}

#[test]
fn blank_frames_trim_to_one_transparent_pixel() {
    let mut packer = Packer::new(packed(AtlasOptions::default())).unwrap();
    let blank = packer.add(&image::RgbaImage::new(40, 30));
    let blank_again = packer.add(&image::RgbaImage::new(20, 10));
    let mut dot = image::RgbaImage::new(40, 30);
    dot.put_pixel(33, 7, image::Rgba([1, 2, 3, 4]));
    let dot = packer.add(&dot);
    let atlas = packer.finish(&Progress::default()).unwrap();
    let b = atlas.frames[blank];
    assert_eq!((b.rect.w, b.rect.h), (1, 1));
    assert_eq!(b, atlas.frames[blank_again]);
    assert_eq!(atlas.pages[b.page].get_pixel(b.rect.x, b.rect.y)[3], 0);
    let d = atlas.frames[dot];
    assert_eq!((d.trim.x, d.trim.y, d.rect.w, d.rect.h), (33, 7, 1, 1));
    assert_eq!(
        rebonebase::atlas::opaque_bounds(&image::RgbaImage::new(2, 2)),
        None
    );
}

#[test]
fn many_random_sizes_pack_without_overlap_inside_the_limit() {
    let mut seed = 0x2545_f491_u32;
    let mut next = |n: u32| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed % n
    };
    let options = AtlasOptions {
        packed: true,
        trim: false,
        reuse: false,
        padding: 1,
        extrude: 1,
        max_size: 128,
    };
    let mut packer = Packer::new(options).unwrap();
    for i in 0..300 {
        let (w, h) = (1 + next(60), 1 + next(60));
        let image =
            image::RgbaImage::from_pixel(w, h, image::Rgba([i as u8, (i >> 8) as u8, 1, 255]));
        packer.add(&image);
    }
    let atlas = packer.finish(&Progress::default()).unwrap();
    assert!(atlas.pages.len() > 1);
    for page in &atlas.pages {
        assert!(page.width() <= 128 && page.height() <= 128);
    }
    for (i, a) in atlas.frames.iter().enumerate() {
        for b in &atlas.frames[i + 1..] {
            if a.page != b.page {
                continue;
            }
            let (ra, rb) = (a.rect, b.rect);
            // Two 1 px extruded borders plus 1 px of padding separate images.
            let gap = 3;
            let apart = ra.x + ra.w + gap <= rb.x
                || rb.x + rb.w + gap <= ra.x
                || ra.y + ra.h + gap <= rb.y
                || rb.y + rb.h + gap <= ra.y;
            assert!(apart, "{ra:?} and {rb:?} overlap or touch");
        }
    }
}

#[test]
fn batch_presets_pack_sheets_and_old_presets_keep_grids() {
    let p = sparse();
    let old: ExportPreset =
        serde_json::from_str(r#"{"name":"Old","format":"Sheet","columns":4}"#).unwrap();
    assert!(!old.atlas.packed);
    let grid = batch::plan(&p, &old, &Progress::default()).unwrap();
    assert!(grid.iter().any(|(f, _)| f == &PathBuf::from("01-Wave.png")));
    for format in [ExportFormat::Sheet, ExportFormat::IndexedSheet] {
        let preset = ExportPreset {
            format,
            prefix: "hero_".into(),
            atlas: packed(AtlasOptions::default()),
            ..Default::default()
        };
        let files: HashMap<_, _> = batch::plan(&p, &preset, &Progress::default())
            .unwrap()
            .into_iter()
            .collect();
        let package = sprite_package::parse(&files[&PathBuf::from("atlas.json")]).unwrap();
        assert_eq!(package.pages.len(), 1);
        assert_eq!(package.pages[0].file, "hero_atlas-01.png");
        // Indexed pages are palette-quantized, so only RGBA compares exactly.
        if format == ExportFormat::Sheet {
            assert_reconstructs(&p, &files, &package);
        } else {
            pages(&files, &package);
        }
    }
    // Packing applies to sheets only; GIF output is unchanged.
    let gif = ExportPreset {
        format: ExportFormat::Gif,
        atlas: packed(AtlasOptions::default()),
        ..Default::default()
    };
    let files = batch::plan(&p, &gif, &Progress::default()).unwrap();
    assert!(
        files
            .iter()
            .any(|(f, _)| f.extension().is_some_and(|e| e == "gif"))
    );
    let _ = plan_package(&p, EngineTarget::Unity, &Progress::default()).unwrap();
    assert!(
        atlas::AtlasOptions {
            max_size: 9000,
            ..Default::default()
        }
        .validate()
        .is_err()
    );
}
