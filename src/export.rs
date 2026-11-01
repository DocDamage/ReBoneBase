//! One entry point for every export target.
//!
//! The editor builds an [`ExportRequest`] and runs it on a background
//! [`crate::jobs::Job`]. Every target renders completely in memory first, so a
//! cancellation or failure never leaves a partial export behind.
use crate::{
    animation::Pose,
    batch::{self, ExportFormat, ExportPreset},
    jobs::Progress,
    production::engine_export::{self, EngineTarget},
    project::{self, Project},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Target {
    /// The displayed frame, including an unkeyed pose being edited.
    Frame {
        indexed: bool,
    },
    /// The active clip as one sheet.
    Sheet {
        columns: u32,
        indexed: bool,
    },
    Gif,
    Aseprite {
        all_clips: bool,
    },
    /// A preset folder export with an atlas manifest.
    Batch(ExportPreset),
    Engine(EngineTarget),
}
impl Default for Target {
    fn default() -> Self {
        Self::Sheet {
            columns: 8,
            indexed: false,
        }
    }
}
impl Target {
    pub fn label(&self) -> String {
        match self {
            Self::Frame { indexed: false } => "Current frame · PNG".into(),
            Self::Frame { indexed: true } => "Current frame · indexed PNG".into(),
            Self::Sheet { indexed: false, .. } => "Sprite sheet · PNG".into(),
            Self::Sheet { indexed: true, .. } => "Sprite sheet · indexed PNG".into(),
            Self::Gif => "Animation · GIF".into(),
            Self::Aseprite { all_clips: false } => "Layered animation · Aseprite".into(),
            Self::Aseprite { all_clips: true } => "All clips · Aseprite tags".into(),
            Self::Batch(p) => format!("Batch preset · {}", p.name),
            Self::Engine(e) => format!("Game package · {e:?}"),
        }
    }
    /// Folder targets publish several files through a recoverable transaction.
    pub fn writes_folder(&self) -> bool {
        matches!(self, Self::Batch(_) | Self::Engine(_))
    }
    pub fn extension(&self) -> Option<&'static str> {
        match self {
            Self::Frame { .. } | Self::Sheet { .. } => Some("png"),
            Self::Gif => Some("gif"),
            Self::Aseprite { .. } => Some("aseprite"),
            Self::Batch(_) | Self::Engine(_) => None,
        }
    }
    /// Frames this target renders, before any reuse of repeated frames.
    pub fn frame_count(&self, project: &Project) -> u64 {
        let clips = &project.rig.clips;
        let active = || {
            clips
                .get(project.rig.active_clip)
                .map_or(0, |c| c.frame_sequence().len() as u64)
        };
        let all = || clips.iter().map(|c| c.frame_sequence().len() as u64).sum();
        match self {
            Self::Frame { .. } => 1,
            Self::Sheet { .. } | Self::Gif => active(),
            Self::Aseprite { all_clips: false } => clips
                .get(project.rig.active_clip)
                .map_or(0, |c| c.length as u64),
            Self::Aseprite { all_clips: true } => clips.iter().map(|c| c.length as u64).sum(),
            Self::Batch(p) if p.all_clips => all(),
            Self::Batch(_) => active(),
            Self::Engine(_) => all(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub target: Target,
    /// A file for single-file targets, otherwise a folder.
    pub destination: PathBuf,
    /// Folder targets only; single files were confirmed by the save dialog.
    pub overwrite: bool,
    pub frame: f32,
    pub pose: Option<Pose>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportSummary {
    pub files: Vec<PathBuf>,
    pub bytes: u64,
}
impl ExportSummary {
    pub fn describe(&self, destination: &Path) -> String {
        match self.files.as_slice() {
            [one] if one == destination => format!("Exported {}", one.display()),
            files => format!(
                "Exported {} files ({}) to {}",
                files.len(),
                human_bytes(self.bytes),
                destination.display()
            ),
        }
    }
}

pub fn human_bytes(bytes: u64) -> String {
    match bytes {
        b if b >= 1024 * 1024 => format!("{:.1} MiB", b as f64 / (1024. * 1024.)),
        b if b >= 1024 => format!("{:.1} KiB", b as f64 / 1024.),
        b => format!("{b} B"),
    }
}

/// Render every file of a request in memory. Nothing is written.
pub fn render(
    project: &Project,
    request: &ExportRequest,
    progress: &Progress,
) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let single = |bytes: Vec<u8>| Ok(vec![(request.destination.clone(), bytes)]);
    match &request.target {
        Target::Batch(preset) => batch::plan(project, preset, progress),
        Target::Engine(engine) => engine_export::plan_package(project, *engine, progress),
        target => {
            progress.set_stage("Preparing canvas");
            let mut prepared = project::prepare_export(project)?;
            if let Target::Frame { .. } = target {
                ensure!(request.frame.is_finite(), "Export frame must be finite");
                // An unkeyed or transition pose may reach outside the clip envelope.
                crate::canvas::grow(&mut prepared, request.frame, request.pose.as_ref())?;
            }
            progress.set_stage("Rendering frames");
            match target {
                Target::Frame { indexed } => {
                    progress.add_total(1);
                    let image = prepared.try_render_pose(request.frame, request.pose.as_ref())?;
                    progress.advance(1)?;
                    single(image_bytes(&image, *indexed, &prepared)?)
                }
                Target::Sheet { columns, indexed } => {
                    let image = project::render_sheet(&prepared, *columns, progress)?;
                    single(image_bytes(&image, *indexed, &prepared)?)
                }
                Target::Gif => single(project::render_gif(&prepared, progress)?),
                Target::Aseprite { all_clips } => single(crate::aseprite::encode_prepared(
                    &prepared, *all_clips, progress,
                )?),
                Target::Batch(_) | Target::Engine(_) => unreachable!("handled above"),
            }
        }
    }
}

/// Render, then publish. Cancellation is honored until files are written.
pub fn run(
    project: &Project,
    request: &ExportRequest,
    progress: &Progress,
) -> Result<ExportSummary> {
    let files = render(project, request, progress)?;
    progress.commit("Writing files")?;
    let bytes = files.iter().map(|(_, b)| b.len() as u64).sum();
    let files = if request.target.writes_folder() {
        crate::export_transaction::publish(&request.destination, files, request.overwrite)?
    } else {
        let mut written = vec![];
        for (path, bytes) in files {
            project::atomic_write(&path, &bytes)?;
            written.push(path);
        }
        written
    };
    Ok(ExportSummary { files, bytes })
}

fn image_bytes(image: &image::RgbaImage, indexed: bool, project: &Project) -> Result<Vec<u8>> {
    if indexed {
        return batch::indexed_png(image, &project.extras.palette);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}

/// Presets usable by [`Target::Batch`], for menus that list formats.
pub const BATCH_FORMATS: [ExportFormat; 5] = [
    ExportFormat::Sheet,
    ExportFormat::IndexedSheet,
    ExportFormat::Gif,
    ExportFormat::Aseprite,
    ExportFormat::Frames,
];
