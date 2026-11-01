//! Four- and eight-direction animation sets.
//!
//! A 3D set rotates every enabled 3D character's camera yaw so the same clip is
//! seen from each direction. A 2D set assigns an authored clip to each
//! direction, or explicitly mirrors another direction: a flat drawing has no
//! unseen side or back view to render.
//!
//! Every direction shares one canvas envelope and pivot, so switching
//! direction in a game never moves the anchor. All work happens on copies; the
//! project passed in is never modified.
use crate::{
    atlas::AtlasOptions,
    jobs::Progress,
    model::Vec2,
    production::engine_export::{self, ClipJob, EngineTarget},
    project::Project,
    sprite_package::{self, Direction},
};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

/// Direction ids by nearest multiple of 45° clockwise from screen right.
pub const COMPASS: [&str; 8] = ["e", "se", "s", "sw", "w", "nw", "n", "ne"];
/// Screen angle of a character facing the viewer.
pub const FACING_VIEWER: f32 = 90.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Count {
    #[default]
    Four,
    Eight,
}
impl Count {
    pub fn size(self) -> usize {
        match self {
            Self::Four => 4,
            Self::Eight => 8,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Slot {
    /// An authored clip, by index.
    Clip(usize),
    /// Horizontally mirror the direction at this index, which must be a clip.
    Mirror(usize),
    Empty,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    /// Rotate 3D characters. `facing` is the screen angle the characters face
    /// at their current yaw.
    Camera { facing: f32 },
    /// One authored 2D clip, or a mirror, per direction in [`directions`] order.
    Clips { name: String, slots: Vec<Slot> },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirectionSet {
    pub count: Count,
    /// Angle of the first direction, degrees clockwise from screen right.
    pub start_angle: f32,
    pub source: Source,
}
impl Default for DirectionSet {
    fn default() -> Self {
        Self {
            count: Count::Four,
            start_angle: 0.,
            source: Source::Camera {
                facing: FACING_VIEWER,
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirectionExport {
    pub set: DirectionSet,
    /// Clips rendered from every direction by a 3D set; ignored by 2D sets.
    pub clips: Vec<usize>,
    pub engine: EngineTarget,
    pub atlas: AtlasOptions,
}

fn wrap(angle: f32) -> f32 {
    angle.rem_euclid(360.)
}
/// The set's directions in order, starting at `start` and turning clockwise.
pub fn directions(count: Count, start: f32) -> Vec<Direction> {
    let step = 360. / count.size() as f32;
    (0..count.size())
        .map(|i| {
            let angle = wrap(start + i as f32 * step);
            let compass = (angle / 45.).round() as usize % 8;
            Direction {
                id: COMPASS[compass].into(),
                angle,
            }
        })
        .collect()
}

/// One direction's rendering setup.
pub struct View {
    pub direction: Direction,
    pub project: Arc<Project>,
    /// The authored clip for a 2D set; `None` for a 3D set.
    pub clip: Option<usize>,
    pub mirror: bool,
}

/// Prepare every direction on one shared canvas and pivot.
pub fn views(project: &Project, set: &DirectionSet) -> Result<Vec<View>> {
    ensure!(set.start_angle.is_finite(), "Starting angle must be finite");
    let dirs = directions(set.count, set.start_angle);
    match &set.source {
        Source::Camera { facing } => camera_views(project, &dirs, *facing),
        Source::Clips { slots, .. } => clip_views(project, &dirs, slots),
    }
}

fn camera_views(project: &Project, dirs: &[Direction], facing: f32) -> Result<Vec<View>> {
    ensure!(facing.is_finite(), "Facing angle must be finite");
    ensure!(
        project.extras.characters3d.iter().any(|c| c.enabled),
        "A 3D direction set needs an enabled 3D character"
    );
    let mut prepared = vec![];
    for d in dirs {
        let mut v = project.clone();
        for c in v.extras.characters3d.iter_mut().filter(|c| c.enabled) {
            // Turning the screen facing clockwise is a negative yaw.
            c.yaw = wrap(c.yaw + facing - d.angle);
        }
        prepared.push(crate::project::prepare_export(&v)?);
    }
    // Union of every direction's envelope, applied to all of them.
    let lo = prepared.iter().fold(Vec2::new(f32::MAX, f32::MAX), |m, p| {
        Vec2::new(
            m.x.min(p.extras.canvas_origin.x),
            m.y.min(p.extras.canvas_origin.y),
        )
    });
    let hi = prepared.iter().fold(Vec2::new(f32::MIN, f32::MIN), |m, p| {
        let o = p.extras.canvas_origin;
        Vec2::new(
            m.x.max(o.x + p.rig.width as f32),
            m.y.max(o.y + p.rig.height as f32),
        )
    });
    let (w, h) = ((hi.x - lo.x) as u32, (hi.y - lo.y) as u32);
    let mut views = vec![];
    for (d, mut p) in dirs.iter().zip(prepared) {
        crate::canvas::set_bounds(&mut p, lo, w, h)?;
        p.validate()?;
        views.push(View {
            direction: d.clone(),
            project: Arc::new(p),
            clip: None,
            mirror: false,
        });
    }
    // Growth keeps the pivot's world anchor, so equal canvases give equal pivots.
    let pivot = views[0].project.extras.production.pivot;
    for v in &mut views {
        Arc::make_mut(&mut v.project).extras.production.pivot = pivot;
    }
    Ok(views)
}

fn clip_views(project: &Project, dirs: &[Direction], slots: &[Slot]) -> Result<Vec<View>> {
    ensure!(
        slots.len() == dirs.len(),
        "Assign a clip or mirror to each of the {} directions",
        dirs.len()
    );
    let clips = &project.rig.clips;
    let resolve = |i: usize| -> Result<(usize, bool)> {
        match slots[i] {
            Slot::Clip(c) if c < clips.len() => Ok((c, false)),
            Slot::Mirror(m) => match slots.get(m) {
                Some(Slot::Clip(c)) if *c < clips.len() && m != i => Ok((*c, true)),
                _ => bail!(
                    "Direction {} must mirror a direction that uses a clip",
                    dirs[i].id
                ),
            },
            _ => bail!("Choose a clip or mirror for direction {}", dirs[i].id),
        }
    };
    let resolved = (0..dirs.len()).map(resolve).collect::<Result<Vec<_>>>()?;
    // Directions are interchangeable at runtime only if their timing matches.
    let timing = |c: usize| {
        let clip = &clips[c];
        let seq = clip.frame_sequence();
        let holds: Vec<_> = seq.iter().map(|f| clip.duration_ms(*f)).collect();
        (holds, clip.looped)
    };
    let reference = timing(resolved[0].0);
    for (d, (c, _)) in dirs.iter().zip(&resolved) {
        ensure!(
            timing(*c) == reference,
            "Clip \"{}\" for direction {} has different frame count, holds or looping than \"{}\"",
            clips[*c].name,
            d.id,
            clips[resolved[0].0].name
        );
    }
    let mut prepared = crate::project::prepare_export(project)?;
    if resolved.iter().any(|(_, m)| *m) {
        center_on_pivot(&mut prepared)?;
    }
    let prepared = Arc::new(prepared);
    Ok(dirs
        .iter()
        .zip(resolved)
        .map(|(d, (clip, mirror))| View {
            direction: d.clone(),
            project: prepared.clone(),
            clip: Some(clip),
            mirror,
        })
        .collect())
}

/// Widen the canvas so the pivot is its exact horizontal center; a mirrored
/// frame then flips in place without moving its anchor. The anchor snaps to
/// the nearest half pixel so the flip stays on the pixel grid.
fn center_on_pivot(p: &mut Project) -> Result<()> {
    let origin = p.extras.canvas_origin;
    let width = p.rig.width as f32;
    let twice = (2. * (origin.x + width * p.extras.production.pivot.x)).round();
    let anchor = twice / 2.;
    let half = (anchor - origin.x).max(origin.x + width - anchor);
    let left = (anchor - half).floor();
    let new_width = twice - 2. * left;
    ensure!(
        new_width <= 4096.,
        "Mirroring needs a canvas wider than 4096 pixels"
    );
    if left != origin.x || new_width != width {
        ensure!(
            p.extras.auto_canvas,
            "Mirroring needs the pivot centered horizontally; center Pivot X or turn on canvas growth"
        );
        crate::canvas::set_bounds(p, Vec2::new(left, origin.y), new_width as u32, p.rig.height)?;
    }
    p.extras.production.pivot.x = 0.5;
    p.validate()
}

/// Exported animations for each direction, in clip-then-direction order.
fn jobs(project: &Project, export: &DirectionExport, views: &[View]) -> Result<Vec<ClipJob>> {
    let mut out = vec![];
    match &export.set.source {
        Source::Camera { .. } => {
            ensure!(!export.clips.is_empty(), "Choose at least one clip");
            for &clip in &export.clips {
                ensure!(clip < project.rig.clips.len(), "Unknown clip");
                let name = &project.rig.clips[clip].name;
                for v in views {
                    out.push(ClipJob {
                        project: v.project.clone(),
                        clip,
                        id: format!("clip-{:02}-{}", clip + 1, v.direction.id),
                        name: name.clone(),
                        animation: format!("{}_{}", animation_base(name), v.direction.id),
                        direction: Some(v.direction.clone()),
                        mirror: false,
                    });
                }
            }
        }
        Source::Clips { name, .. } => {
            for v in views {
                out.push(ClipJob {
                    project: v.project.clone(),
                    clip: v.clip.expect("2D views name a clip"),
                    id: format!("{}-{}", sprite_id(name), v.direction.id),
                    name: name.clone(),
                    animation: format!("{}_{}", animation_base(name), v.direction.id),
                    direction: Some(v.direction.clone()),
                    mirror: v.mirror,
                });
            }
        }
    }
    let names = sprite_package::unique_names(out.iter().map(|j| j.animation.as_str()));
    for (job, name) in out.iter_mut().zip(names) {
        job.animation = name;
    }
    Ok(out)
}
fn animation_base(name: &str) -> &str {
    if name.trim().is_empty() {
        "Animation"
    } else {
        name
    }
}
/// File-safe identifier for page names.
fn sprite_id(name: &str) -> String {
    let id: String = name
        .chars()
        .take(48)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if id.is_empty() { "set".into() } else { id }
}

/// Frames a directional export renders.
pub fn frame_count(project: &Project, export: &DirectionExport) -> u64 {
    let len = |c: usize| {
        project
            .rig
            .clips
            .get(c)
            .map_or(0, |c| c.frame_sequence().len() as u64)
    };
    match &export.set.source {
        Source::Camera { .. } => {
            export.set.count.size() as u64 * export.clips.iter().map(|c| len(*c)).sum::<u64>()
        }
        Source::Clips { slots, .. } => slots
            .iter()
            .map(|s| match s {
                Slot::Clip(c) => len(*c),
                Slot::Mirror(m) => match slots.get(*m) {
                    Some(Slot::Clip(c)) => len(*c),
                    _ => 0,
                },
                Slot::Empty => 0,
            })
            .sum(),
    }
}

/// Render a complete directional package in memory.
pub fn plan(
    project: &Project,
    export: &DirectionExport,
    progress: &Progress,
) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    progress.set_stage("Preparing directions");
    let views = views(project, &export.set)?;
    progress.checkpoint()?;
    let jobs = jobs(project, export, &views)?;
    engine_export::plan_jobs(
        &views[0].project,
        jobs,
        export.engine,
        &export.atlas,
        progress,
    )
}

/// One rendered frame per direction, for orientation previews.
pub fn preview(
    project: &Project,
    set: &DirectionSet,
    clip: usize,
    frame: f32,
) -> Result<Vec<(Direction, image::RgbaImage)>> {
    views(project, set)?
        .into_iter()
        .map(|v| {
            let mut p = (*v.project).clone();
            p.rig.active_clip = v.clip.unwrap_or(clip).min(p.rig.clips.len() - 1);
            let image = p.try_render_at(frame)?;
            let image = if v.mirror {
                image::imageops::flip_horizontal(&image)
            } else {
                image
            };
            Ok((v.direction, image))
        })
        .collect()
}
