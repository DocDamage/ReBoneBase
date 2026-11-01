//! Synthetic large projects and timed checks for performance regressions.
//!
//! `cargo run --release --example perf_check -- --check perf/budgets.json`
//! runs every check and fails when one exceeds its budget. Tests use the same
//! fixtures for the deterministic parts: render reuse, memory and cancellation.
use crate::{
    animation::{Channel, Playback},
    batch::{ExportFormat, ExportPreset},
    export::{self, ExportRequest, Target},
    jobs::{Job, Outcome, Progress},
    production::engine_export::{self, EngineTarget},
    project::{self, History, Project},
};
use anyhow::{Result, ensure};
use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Scale {
    pub clips: usize,
    pub frames: u32,
    /// Bones keyed on every frame instead of every fourth frame.
    pub dense_keys: bool,
}
impl Scale {
    pub const SMALL: Self = Self {
        clips: 3,
        frames: 12,
        dense_keys: false,
    };
    pub const LARGE: Self = Self {
        clips: 12,
        frames: 48,
        dense_keys: true,
    };
}

/// A humanoid with many animated clips; every other clip plays ping-pong.
pub fn large_project(scale: Scale) -> Result<Project> {
    let mut p = Project::new(project::templates()[0].clone());
    p.rig.name = "Performance".into();
    let template = p.rig.clips[0].clone();
    p.rig.clips.clear();
    let bones: Vec<u32> = p.rig.bones.iter().map(|b| b.id).collect();
    let step = if scale.dense_keys { 1 } else { 4 };
    for c in 0..scale.clips {
        let mut clip = template.clone();
        clip.name = format!("Clip {}", c + 1);
        clip.length = scale.frames;
        clip.tracks.clear();
        clip.playback = if c % 2 == 0 {
            Playback::Forward
        } else {
            Playback::PingPong
        };
        for (i, bone) in bones.iter().enumerate() {
            for f in (1..=scale.frames).step_by(step) {
                let t = f as f32 / scale.frames as f32 * std::f32::consts::TAU;
                let phase = i as f32 * 0.7 + c as f32 * 0.3;
                clip.set_key(*bone, Channel::Rotation, f as f32, (t + phase).sin() * 25.)
                    .map_err(anyhow::Error::msg)?;
                if scale.dense_keys && i == 0 {
                    clip.set_key(*bone, Channel::Y, f as f32, (t * 2.).sin() * 3.)
                        .map_err(anyhow::Error::msg)?;
                }
            }
        }
        p.rig.clips.push(clip);
    }
    p.rig.active_clip = 1.min(p.rig.clips.len() - 1);
    p.validate()?;
    Ok(p)
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckResult {
    pub name: String,
    pub median_ms: f64,
    pub max_ms: f64,
    pub detail: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub scale: Scale,
    pub runs: usize,
    pub frames: u64,
    pub checks: Vec<CheckResult>,
}

fn time<T>(runs: usize, mut f: impl FnMut() -> Result<T>) -> Result<(f64, f64, T)> {
    let mut samples = vec![];
    let mut last = None;
    for _ in 0..runs.max(1) {
        let start = Instant::now();
        last = Some(f()?);
        samples.push(start.elapsed().as_secs_f64() * 1000.);
    }
    samples.sort_by(f64::total_cmp);
    let median = samples[samples.len() / 2];
    let max = *samples.last().expect("at least one run");
    Ok((median, max, last.expect("at least one run")))
}

/// Run every check `runs` times on a project of the given scale.
pub fn run(scale: Scale, runs: usize) -> Result<Report> {
    let project = large_project(scale)?;
    let frames = Target::Engine(EngineTarget::Generic).frame_count(&project);
    let mut checks = vec![];
    let mut push = |name: &str, (median_ms, max_ms, detail): (f64, f64, String)| {
        checks.push(CheckResult {
            name: name.into(),
            median_ms,
            max_ms,
            detail,
        });
    };

    push(
        "prepare_canvas",
        time(runs, || {
            let p = project::prepare_export(&project)?;
            Ok(format!("{} × {} px", p.rig.width, p.rig.height))
        })?,
    );
    let prepared = project::prepare_export(&project)?;
    let progress = Progress::default();
    push(
        "render_active_sheet",
        time(runs, || {
            let image = project::render_sheet(&prepared, 8, &progress)?;
            Ok(format!("{} × {} px", image.width(), image.height()))
        })?,
    );
    for (name, format) in [
        ("batch_sheets_all_clips", ExportFormat::Sheet),
        ("batch_frames_all_clips", ExportFormat::Frames),
        ("batch_gifs_all_clips", ExportFormat::Gif),
    ] {
        let preset = ExportPreset {
            format,
            ..Default::default()
        };
        push(
            name,
            time(runs, || {
                let files = crate::batch::plan(&project, &preset, &Progress::default())?;
                Ok(describe(&files))
            })?,
        );
    }
    push(
        "aseprite_all_clips",
        time(runs, || {
            let request = ExportRequest {
                target: Target::Aseprite { all_clips: true },
                destination: "all.aseprite".into(),
                overwrite: false,
                frame: 1.,
                pose: None,
            };
            Ok(describe(&export::render(
                &project,
                &request,
                &Progress::default(),
            )?))
        })?,
    );
    push(
        "engine_package",
        time(runs, || {
            let files =
                engine_export::plan_package(&project, EngineTarget::Godot, &Progress::default())?;
            Ok(describe(&files))
        })?,
    );
    push(
        "history_200_edits",
        time(runs, || {
            let (history, _) = record_edits(&project, 200);
            Ok(format!(
                "{} steps kept · {}",
                history.undo_steps(),
                export::human_bytes(history.bytes() as u64)
            ))
        })?,
    );
    push(
        "undo_redo_50",
        time(runs, || {
            let (mut history, mut current) = record_edits(&project, 50);
            for _ in 0..50 {
                history.undo(&mut current);
            }
            for _ in 0..50 {
                history.redo(&mut current);
            }
            Ok(format!("{} steps", history.undo_steps()))
        })?,
    );
    push(
        "cancel_latency",
        time(runs, || {
            let latency = cancel_latency(&project)?;
            Ok(format!("{:.1} ms to stop", latency.as_secs_f64() * 1000.))
        })?,
    );
    Ok(Report {
        scale,
        runs,
        frames,
        checks,
    })
}

fn describe(files: &[(std::path::PathBuf, Vec<u8>)]) -> String {
    let bytes = files.iter().map(|(_, b)| b.len() as u64).sum();
    format!("{} files · {}", files.len(), export::human_bytes(bytes))
}

/// Record `edits` distinct one-key changes, as interactive posing would.
pub fn record_edits(project: &Project, edits: usize) -> (History, Project) {
    let mut history = History::default();
    let mut current = project.clone();
    let bone = current.rig.bones[0].id;
    for i in 0..edits {
        history.record(&current);
        let clip = current.rig.active_clip_mut();
        let frame = (i as u32 % clip.length + 1) as f32;
        let _ = clip.set_key(bone, Channel::Rotation, frame, i as f32);
    }
    (history, current)
}

/// Time from requesting cancellation of a running package export until the
/// worker reports that it stopped.
pub fn cancel_latency(project: &Project) -> Result<Duration> {
    let project = project.clone();
    let job = Job::spawn("cancel", move |progress| {
        engine_export::plan_package(&project, EngineTarget::Generic, progress)
    });
    while job.progress().done == 0 && job.poll().is_none() {
        std::thread::yield_now();
    }
    let start = Instant::now();
    job.cancel();
    let outcome = job.wait();
    ensure!(
        outcome == Outcome::Cancelled,
        "Export finished before it could be cancelled"
    );
    Ok(start.elapsed())
}

/// Checks whose median exceeds the budget, as readable failures.
pub fn over_budget(report: &Report, budgets: &serde_json::Value) -> Vec<String> {
    report
        .checks
        .iter()
        .filter_map(|c| {
            let budget = budgets.get(&c.name)?.as_f64()?;
            (c.median_ms > budget)
                .then(|| format!("{}: {:.1} ms exceeds {budget:.0} ms", c.name, c.median_ms))
        })
        .collect()
}
