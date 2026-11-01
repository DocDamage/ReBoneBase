use rebonebase::{
    animation::Playback,
    batch::{self, ExportFormat, ExportPreset},
    jobs::Progress,
    model::{Rig, Vec2},
    production::{
        BoxTrack, EventMarker,
        engine_export::{EngineTarget, plan_package},
    },
    project::Project,
    sprite_package::{self, FORMAT, FORMAT_VERSION, PageKind, SpritePackage},
};
use std::path::PathBuf;

fn actor() -> Project {
    let mut rig = Rig::new("Actor", 12, 10);
    let part = rig.add_part("Body", [200, 30, 40, 255]);
    let bone = rig
        .add_bone(Vec2::new(3., 5.), Vec2::new(9., 5.), None)
        .unwrap();
    rig.bone_mut(bone).unwrap().part = part;
    rig.clips[0].name = "Walk".into();
    rig.clips[0].length = 4;
    rig.clips[0].durations.insert(2, 370);
    let mut back = rig.clips[0].clone();
    back.name = "Walk".into();
    back.playback = Playback::PingPong;
    back.looped = false;
    back.durations.insert(3, 45);
    rig.clips.push(back);
    let mut p = Project::new(rig);
    p.extras.production.pivot = Vec2::new(0.25, 0.75);
    for (clip, frame, name) in [(0, 2., "step"), (0, 2., "dust"), (1, 3., "turn")] {
        p.extras.production.events.push(EventMarker {
            clip,
            frame,
            name: name.into(),
            payload: format!("{clip}"),
        });
    }
    p.extras.production.boxes.push(BoxTrack {
        id: 7,
        end: 4,
        ..Default::default()
    });
    p
}
fn file<'a>(files: &'a [(PathBuf, Vec<u8>)], name: &str) -> &'a [u8] {
    &files
        .iter()
        .find(|(p, _)| p == &PathBuf::from(name))
        .unwrap()
        .1
}
fn engine(p: &Project) -> (Vec<(PathBuf, Vec<u8>)>, SpritePackage) {
    let files = plan_package(p, EngineTarget::Generic, &Progress::default()).unwrap();
    let package = sprite_package::parse(file(&files, "package.json")).unwrap();
    (files, package)
}

#[test]
fn both_exporters_write_the_current_versioned_format() {
    let p = actor();
    let (files, package) = engine(&p);
    let raw: serde_json::Value = serde_json::from_slice(file(&files, "package.json")).unwrap();
    assert_eq!(raw["format"], FORMAT);
    assert_eq!(raw["format_version"], FORMAT_VERSION);
    // The export format is versioned apart from the native project schema.
    assert!(raw.get("version").is_none());
    // Export grows the canvas; the pivot is stored in pixels of that canvas.
    let mut grown = p.clone();
    rebonebase::canvas::grow_all(&mut grown).unwrap();
    let size = &package.frame_size;
    assert_eq!((size.w, size.h), (grown.rig.width, grown.rig.height));
    let pivot = grown.extras.production.pivot;
    assert_eq!(
        package.pivot,
        Vec2::new(pivot.x * size.w as f32, pivot.y * size.h as f32)
    );
    assert_eq!(package.pivot_normalized(), pivot);
    assert_eq!(package.clips[0].animation, "Walk");
    assert_eq!(package.clips[1].animation, "Walk (2)");
    assert_eq!(package.clips[1].id, "clip-02");

    let preset = ExportPreset::default();
    let atlas = batch::plan(&p, &preset, &Progress::default()).unwrap();
    let batch = sprite_package::parse(file(&atlas, "atlas.json")).unwrap();
    assert_eq!(batch.format_version, FORMAT_VERSION);
    assert_eq!(batch.pivot, package.pivot);
    assert_eq!(batch.clips.len(), package.clips.len());
    for (a, b) in batch.clips.iter().zip(&package.clips) {
        assert_eq!(
            (&a.id, &a.animation, a.looped, a.fps),
            (&b.id, &b.animation, b.looped, b.fps)
        );
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
        }
    }
}

#[test]
fn timing_events_and_collisions_match_the_project() {
    let p = actor();
    let (_, package) = engine(&p);
    let walk = &package.clips[0];
    let sources: Vec<_> = walk.frames.iter().map(|f| f.source_frame).collect();
    assert_eq!(sources, [1, 2, 3, 4]);
    let holds: Vec<_> = walk.frames.iter().map(|f| f.duration_ms).collect();
    let default = p.rig.clips[0].duration_ms(1);
    assert_eq!(holds, [default, 370, default, default]);
    let names: Vec<_> = walk.frames[1]
        .events
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, ["step", "dust"], "events keep authored order");
    assert!(
        walk.frames
            .iter()
            .all(|f| f.boxes.len() == 1 && f.boxes[0].id == 7)
    );
    let pingpong = &package.clips[1];
    let sources: Vec<_> = pingpong.frames.iter().map(|f| f.source_frame).collect();
    assert_eq!(sources, [1, 2, 3, 4, 3, 2]);
    for f in &pingpong.frames {
        assert_eq!(f.events.len(), usize::from(f.source_frame == 3));
        assert_eq!(f.duration_ms, p.rig.clips[1].duration_ms(f.source_frame));
        assert_eq!(
            f.source_time_ms,
            p.rig.clips[1].source_time_ms(f.source_frame as f32)
        );
    }
}

#[test]
fn every_frame_reconstructs_from_its_page_pixel_for_pixel() {
    let p = actor();
    let (files, package) = engine(&p);
    let mut grown = p.clone();
    rebonebase::canvas::grow_all(&mut grown).unwrap();
    for (index, clip) in package.clips.iter().enumerate() {
        grown.rig.active_clip = index;
        for frame in &clip.frames {
            let page = &package.pages[frame.page];
            let image = image::load_from_memory(file(&files, &page.file))
                .unwrap()
                .to_rgba8();
            assert_eq!(image.dimensions(), (page.w, page.h));
            let r = frame.rect.unwrap();
            let mut rebuilt = image::RgbaImage::new(frame.source_size.w, frame.source_size.h);
            let crop = image::imageops::crop_imm(&image, r.x, r.y, r.w, r.h).to_image();
            image::imageops::replace(
                &mut rebuilt,
                &crop,
                frame.trim.x as i64,
                frame.trim.y as i64,
            );
            assert_eq!(rebuilt, grown.render_at(frame.source_frame as f32));
        }
    }
}

#[test]
fn batch_animation_and_frame_files_are_described_as_pages() {
    let p = actor();
    for (format, kind, pages) in [
        (ExportFormat::Gif, PageKind::Animation, 1),
        (ExportFormat::Frames, PageKind::Atlas, 4),
        (ExportFormat::IndexedSheet, PageKind::Atlas, 1),
    ] {
        let preset = ExportPreset {
            format,
            all_clips: false,
            ..Default::default()
        };
        let files = batch::plan(&p, &preset, &Progress::default()).unwrap();
        let package = sprite_package::parse(file(&files, "atlas.json")).unwrap();
        assert_eq!(package.pages.len(), pages, "{format:?}");
        assert!(package.pages.iter().all(|page| page.kind == kind));
        for page in &package.pages {
            assert!(files.iter().any(|(f, _)| f == &PathBuf::from(&page.file)));
        }
        assert_eq!(package.clips[0].frames[1].events.len(), 2);
    }
}

#[test]
fn version_one_engine_packages_upgrade_without_losing_metadata() {
    let box_json = serde_json::json!({"id":7,"name":"Hurt","kind":"Hurt","center":{"x":2.,"y":3.},"size":{"x":4.,"y":2.},"angle":0.,"corners":[{"x":0.,"y":2.},{"x":4.,"y":2.},{"x":4.,"y":4.},{"x":0.,"y":4.}]});
    let v1 = serde_json::json!({"version":1,"generator":"ReBoneBase Production","name":"Old","width":8,"height":6,"pivot":{"x":0.5,"y":1.0},"canvas_origin":{"x":1.,"y":2.},"coordinates":"pixels, top-left origin, y down; angles degrees clockwise","audio_file":"reference.wav","audio_start_ms":250,"audio_volume":0.5,"audio_enabled":true,"clips":[
        {"id":"clip-01","animation":"Idle","name":"Idle","looped":false,"fps":8.0,"file":"clip-01.png","frames":[
            {"source_frame":1,"source_time_ms":0,"duration_ms":125,"x":0,"y":0,"w":8,"h":6,"events":[],"boxes":[box_json]},
            {"source_frame":2,"source_time_ms":125,"duration_ms":300,"x":8,"y":0,"w":8,"h":6,"events":[{"clip":0,"frame":2.0,"name":"blink","payload":"x"}],"boxes":[]}]}]});
    let package = sprite_package::parse(&serde_json::to_vec(&v1).unwrap()).unwrap();
    assert_eq!(package.format_version, FORMAT_VERSION);
    assert_eq!(package.pivot, Vec2::new(4., 6.));
    assert_eq!(package.canvas_origin, Vec2::new(1., 2.));
    assert_eq!(package.pages[0].file, "clip-01.png");
    assert_eq!((package.pages[0].w, package.pages[0].h), (16, 6));
    let clip = &package.clips[0];
    assert_eq!(
        (clip.animation.as_str(), clip.looped, clip.fps),
        ("Idle", false, 8.)
    );
    assert_eq!(clip.frames[1].rect.unwrap().x, 8);
    assert_eq!(clip.frames[1].duration_ms, 300);
    assert_eq!(clip.frames[1].source_time_ms, 125);
    assert_eq!(clip.frames[1].events[0].name, "blink");
    assert_eq!(clip.frames[0].boxes[0].id, 7);
    assert_eq!(clip.frames[0].boxes[0].corners[2], Vec2::new(4., 4.));
    let audio = package.audio.unwrap();
    assert_eq!(
        (audio.start_ms, audio.volume, audio.enabled),
        (250, 0.5, true)
    );
}

#[test]
fn unversioned_batch_atlases_upgrade() {
    let v0 = serde_json::json!({"app":"ReBoneBase","clips":[
        {"name":"Run","width":8,"height":8,"fps":12.0,"loop":true,"playback":"Reverse","frames":[
            {"file":"01-Run.png","source_frame":2,"source_time_ms":83,"frame":{"x":0,"y":0,"w":8,"h":8},"duration":90,"events":[],"boxes":[]},
            {"file":"01-Run.png","source_frame":1,"source_time_ms":0,"frame":{"x":8,"y":0,"w":8,"h":8},"duration":83,"events":[],"boxes":[]}]},
        {"name":"Run","width":8,"height":8,"fps":12.0,"loop":false,"playback":"Forward","frames":[
            {"file":"02-Run.gif","source_frame":1,"source_time_ms":0,"duration":83,"events":[],"boxes":[]}]}]});
    let package = sprite_package::parse(&serde_json::to_vec(&v0).unwrap()).unwrap();
    assert_eq!(package.pages.len(), 2);
    assert_eq!((package.pages[0].w, package.pages[0].h), (16, 8));
    assert_eq!(package.pages[1].kind, PageKind::Animation);
    assert_eq!(package.clips[0].playback, Playback::Reverse);
    assert_eq!(package.clips[0].frames[0].duration_ms, 90);
    assert_eq!(package.clips[1].animation, "Run (2)");
    assert!(package.clips[1].frames[0].rect.is_none());
}

#[test]
fn newer_or_foreign_documents_are_rejected() {
    let p = actor();
    let (files, _) = engine(&p);
    let mut raw: serde_json::Value = serde_json::from_slice(file(&files, "package.json")).unwrap();
    raw["format_version"] = (FORMAT_VERSION + 1).into();
    let err = sprite_package::parse(&serde_json::to_vec(&raw).unwrap()).unwrap_err();
    assert!(err.to_string().contains("newer"), "{err}");
    assert!(sprite_package::parse(br#"{"version":2,"clips":[]}"#).is_err());
    assert!(sprite_package::parse(b"not json").is_err());
    raw["format_version"] = FORMAT_VERSION.into();
    raw["clips"][0]["frames"][0]["rect"]["w"] = 100_000.into();
    assert!(sprite_package::parse(&serde_json::to_vec(&raw).unwrap()).is_err());
}
