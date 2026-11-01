//! Deterministic parts of the performance work: shared background jobs,
//! repeated-frame reuse, bounded undo memory and the unified export path.
use rebonebase::{
    aseprite,
    batch::{self, ExportFormat, ExportPreset},
    export::{self, ExportRequest, Target},
    jobs::{Job, Outcome, Progress},
    perf::{self, Scale},
    production::engine_export::{self, EngineTarget},
    project::{self, History, Project, Reference},
};
use std::{path::PathBuf, sync::Arc};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rebone-perf-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn request(target: Target, destination: PathBuf) -> ExportRequest {
    ExportRequest {
        target,
        destination,
        overwrite: false,
        frame: 3.,
        pose: None,
    }
}

fn png(image: &image::RgbaImage) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn unified_single_file_targets_match_the_existing_encoders() {
    let p = perf::large_project(Scale::SMALL).unwrap();
    let render = |target| {
        let files =
            export::render(&p, &request(target, "out".into()), &Progress::default()).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, PathBuf::from("out"));
        files.into_iter().next().unwrap().1
    };
    let sheet = project::sheet_project(&p, 4).unwrap();
    assert_eq!(
        render(Target::Sheet {
            columns: 4,
            indexed: false
        }),
        png(&sheet)
    );
    assert_eq!(
        render(Target::Sheet {
            columns: 4,
            indexed: true
        }),
        batch::indexed_png(&sheet, &p.extras.palette).unwrap()
    );
    assert_eq!(render(Target::Gif), project::gif_project(&p).unwrap());
    for all_clips in [false, true] {
        assert_eq!(
            render(Target::Aseprite { all_clips }),
            aseprite::encode_project(&p, all_clips).unwrap()
        );
    }
    // Animate mode passes the displayed pose; Rig mode passes none (rest pose).
    let mut grown = p.clone();
    rebonebase::canvas::grow_all(&mut grown).unwrap();
    let mut posed = request(Target::Frame { indexed: false }, "out".into());
    posed.pose = Some(p.pose_at(3.));
    let files = export::render(&p, &posed, &Progress::default()).unwrap();
    assert_eq!(files[0].1, png(&grown.try_render_at(3.).unwrap()));
    assert_eq!(
        render(Target::Frame { indexed: false }),
        png(&grown.try_render_pose(3., None).unwrap())
    );
}

#[test]
fn batch_and_package_reuse_one_canvas_envelope_without_changing_output() {
    let p = perf::large_project(Scale::SMALL).unwrap();
    let preset = ExportPreset {
        columns: 4,
        ..Default::default()
    };
    let files = batch::plan(&p, &preset, &Progress::default()).unwrap();
    for (i, clip) in p.rig.clips.iter().enumerate() {
        let mut single = p.clone();
        single.rig.active_clip = i;
        let expected = png(&project::sheet_project(&single, 4).unwrap());
        let name = format!("{:02}-{}.png", i + 1, clip.name.replace(' ', "_"));
        let (_, bytes) = files
            .iter()
            .find(|(path, _)| path.to_string_lossy() == name)
            .unwrap_or_else(|| panic!("{name} missing from {files:?}"));
        assert_eq!(bytes, &expected, "{name}");
    }
    let package =
        engine_export::plan_package(&p, EngineTarget::Godot, &Progress::default()).unwrap();
    let mut first = p.clone();
    first.rig.active_clip = 0;
    let sheet = png(&project::sheet_project(&first, 8).unwrap());
    assert_eq!(
        package
            .iter()
            .find(|(path, _)| path.to_string_lossy() == "clip-01.png")
            .unwrap()
            .1,
        sheet
    );
}

#[test]
fn background_export_reports_progress_and_writes_through_the_transaction() {
    let p = perf::large_project(Scale::SMALL).unwrap();
    let folder = temp("batch");
    let preset = ExportPreset {
        format: ExportFormat::Frames,
        ..Default::default()
    };
    let expected_frames = Target::Batch(preset.clone()).frame_count(&p);
    let mut req = request(Target::Batch(preset), folder.clone());
    req.overwrite = true;
    let job = Job::spawn("batch", move |progress| {
        let summary = export::run(&p, &req, progress)?;
        Ok((summary, progress.snapshot()))
    });
    let Outcome::Finished((summary, snapshot)) = job.wait() else {
        panic!("export did not finish");
    };
    assert_eq!(snapshot.total, expected_frames);
    assert_eq!(snapshot.done, snapshot.total);
    assert!(!snapshot.cancellable, "the job committed before writing");
    assert_eq!(snapshot.stage, "Writing files");
    // One PNG per sequence frame plus atlas.json.
    assert_eq!(summary.files.len() as u64, expected_frames + 1);
    for file in &summary.files {
        assert!(file.is_file(), "{}", file.display());
    }
    assert!(
        summary
            .describe(&folder)
            .contains(&format!("{} files", summary.files.len()))
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn cancelled_exports_write_nothing() {
    let p = perf::large_project(Scale::SMALL).unwrap();
    let folder = temp("cancel");
    let file = folder.join("sheet.png");
    for target in [
        Target::Sheet {
            columns: 8,
            indexed: false,
        },
        Target::Engine(EngineTarget::Unity),
    ] {
        let destination = if target.writes_folder() {
            folder.join("package")
        } else {
            file.clone()
        };
        let req = request(target, destination.clone());
        let project = p.clone();
        let progress = Progress::default();
        progress.cancel();
        let error = export::run(&project, &req, &progress).unwrap_err();
        assert!(rebonebase::jobs::is_cancelled(&error), "{error:#}");
        assert!(!destination.exists());
    }
    assert!(
        perf::cancel_latency(&perf::large_project(Scale::LARGE).unwrap()).unwrap()
            < std::time::Duration::from_secs(5)
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn history_keeps_more_small_steps_and_fewer_large_ones() {
    let small = perf::large_project(Scale::SMALL).unwrap();
    let (history, _) = perf::record_edits(&small, History::MAX_STEPS + 50);
    assert_eq!(history.undo_steps(), History::MAX_STEPS);
    let large = perf::large_project(Scale::LARGE).unwrap();
    let per_step = large.snapshot_bytes();
    let budget = per_step * 20;
    let mut history = History::with_budget(budget);
    let mut current = large.clone();
    let bone = current.rig.bones[0].id;
    for i in 0..60 {
        history.record(&current);
        current
            .rig
            .active_clip_mut()
            .set_key(bone, rebonebase::animation::Channel::X, 1., i as f32)
            .unwrap();
    }
    assert!(history.undo_steps() >= History::MIN_STEPS);
    assert!(history.undo_steps() < 60);
    assert!(history.bytes() <= budget + per_step, "{}", history.bytes());
    // Undo and redo stay exact and keep accounting balanced.
    let latest = current.clone();
    let steps = history.undo_steps();
    for _ in 0..steps {
        assert!(history.undo(&mut current));
    }
    assert!(!history.can_undo());
    for _ in 0..steps {
        assert!(history.redo(&mut current));
    }
    assert_eq!(current, latest);
    history.record(&current);
    assert!(!history.can_redo());
}

#[test]
fn snapshots_do_not_charge_shared_image_memory() {
    let mut p = Project::new(project::templates()[0].clone());
    let before = p.snapshot_bytes();
    p.reference = Some(Reference {
        width: 1024,
        height: 1024,
        pixels: Arc::new(vec![255; 1024 * 1024 * 4]),
        x: 0.,
        y: 0.,
        scale: 1.,
        opacity: 1.,
    });
    assert!(p.snapshot_bytes() - before < 1024);
    let large = perf::large_project(Scale::LARGE).unwrap().snapshot_bytes();
    assert!(large > before * 10, "keys are charged: {large} vs {before}");
}

#[test]
fn frame_counts_describe_the_rendered_sequence() {
    let p = perf::large_project(Scale::SMALL).unwrap();
    // Clip 2 is active and plays ping-pong: 12 forward + 10 back.
    assert_eq!(Target::Gif.frame_count(&p), 22);
    assert_eq!(Target::Aseprite { all_clips: false }.frame_count(&p), 12);
    assert_eq!(Target::Aseprite { all_clips: true }.frame_count(&p), 36);
    assert_eq!(
        Target::Engine(EngineTarget::Generic).frame_count(&p),
        12 + 22 + 12
    );
    assert_eq!(Target::Frame { indexed: true }.frame_count(&p), 1);
    assert_eq!(Target::Gif.extension(), Some("gif"));
    assert!(Target::Batch(ExportPreset::default()).writes_folder());
}
