use crate::{
    atlas::{self, AtlasOptions},
    jobs::Progress,
    project::{self, Project},
    sprite_package::{self, Offset, PageKind, SpritePackage},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExportFormat {
    Sheet,
    IndexedSheet,
    Gif,
    Aseprite,
    Frames,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportPreset {
    pub name: String,
    pub format: ExportFormat,
    pub columns: u32,
    pub prefix: String,
    pub suffix: String,
    pub all_clips: bool,
    /// Sheet formats only: packed, trimmed pages instead of per-clip grids.
    pub atlas: AtlasOptions,
}
impl Default for ExportPreset {
    fn default() -> Self {
        Self {
            name: "Game assets".into(),
            format: ExportFormat::Sheet,
            columns: 8,
            prefix: String::new(),
            suffix: String::new(),
            all_clips: true,
            atlas: AtlasOptions::default(),
        }
    }
}
pub struct ExportReport {
    pub files: Vec<PathBuf>,
}
pub fn indexed_png(image: &image::RgbaImage, palette: &[[u8; 4]]) -> Result<Vec<u8>> {
    ensure!(
        image.width() as u64 * image.height() as u64 <= 16 * 1024 * 1024,
        "Indexed image exceeds pixel budget"
    );
    let mut colors = palette.to_vec();
    colors.truncate(256);
    if colors.is_empty() {
        for p in image.pixels() {
            if !colors.contains(&p.0) && colors.len() < 256 {
                colors.push(p.0);
            }
        }
    }
    if colors.is_empty() {
        colors.push([0, 0, 0, 0]);
    }
    if image.pixels().any(|p| p[3] == 0) && !colors.iter().any(|p| p[3] == 0) {
        if colors.len() == 256 {
            colors.pop();
        }
        colors.push([0, 0, 0, 0]);
    }
    let transparent = colors.iter().position(|c| c[3] == 0);
    ensure!(
        colors.iter().any(|c| c[3] > 0) || image.pixels().all(|p| p[3] == 0),
        "Add a visible palette swatch to export visible artwork"
    );
    let indices: Vec<u8> = image
        .pixels()
        .map(|p| {
            if p[3] == 0 {
                return transparent.expect("transparent palette entry was ensured") as u8;
            }
            colors
                .iter()
                .enumerate()
                .filter(|(_, c)| c[3] > 0)
                .min_by_key(|(_, c)| {
                    p.0.iter()
                        .zip(c.iter())
                        .enumerate()
                        .map(|(i, (a, b))| {
                            let d = *a as i32 - *b as i32;
                            d * d * if i == 3 { 2 } else { 1 }
                        })
                        .sum::<i32>()
                })
                .unwrap()
                .0 as u8
        })
        .collect();
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.width(), image.height());
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(
            colors
                .iter()
                .flat_map(|c| [c[0], c[1], c[2]])
                .collect::<Vec<_>>(),
        );
        encoder.set_trns(colors.iter().map(|c| c[3]).collect::<Vec<_>>());
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&indices)?;
    }
    Ok(bytes)
}
fn png_bytes(image: &image::RgbaImage) -> Result<Vec<u8>> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    image.write_to(&mut cursor, image::ImageFormat::Png)?;
    Ok(cursor.into_inner())
}
fn name(value: &str) -> String {
    let s: String = value
        .chars()
        .take(64)
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "clip".into() } else { s }
}
pub fn export(
    project: &Project,
    folder: &Path,
    preset: &ExportPreset,
    overwrite: bool,
) -> Result<ExportReport> {
    export_with(project, folder, preset, overwrite, &Progress::default())
}
pub fn export_with(
    project: &Project,
    folder: &Path,
    preset: &ExportPreset,
    overwrite: bool,
    progress: &Progress,
) -> Result<ExportReport> {
    let files = plan(project, preset, progress)?;
    progress.commit("Writing files")?;
    let files = crate::export_transaction::publish(folder, files, overwrite)?;
    Ok(ExportReport { files })
}
/// Render every planned file in memory without touching the destination.
pub fn plan(
    project: &Project,
    preset: &ExportPreset,
    progress: &Progress,
) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    progress.set_stage("Preparing canvas");
    let prepared = project::prepare_export(project)?;
    let project = &prepared;
    ensure!(
        (1..=32).contains(&preset.columns)
            && preset.prefix.len() <= 128
            && preset.suffix.len() <= 128,
        "Invalid export preset"
    );
    let indices: Vec<_> = if preset.all_clips {
        (0..project.rig.clips.len()).collect()
    } else {
        vec![project.rig.active_clip]
    };
    let total_frames = indices
        .iter()
        .map(|i| project.rig.clips[*i].frame_sequence().len() as u64)
        .sum();
    project::project_export_budget(project, total_frames)?;
    let sheet = matches!(
        preset.format,
        ExportFormat::Sheet | ExportFormat::IndexedSheet
    );
    let packed = sheet && preset.atlas.packed;
    let mut packer = atlas::Packer::new(preset.atlas)?;
    let mut pending = atlas::Pending::default();
    let mut planned = Vec::<(PathBuf, Vec<u8>)>::new();
    let mut package = SpritePackage::new(project, "ReBoneBase Batch");
    let animations =
        sprite_package::unique_names(indices.iter().map(|i| project.rig.clips[*i].name.as_str()));
    let clip_count = indices.len();
    for (n, (index, animation)) in indices.into_iter().zip(animations).enumerate() {
        progress.checkpoint()?;
        let mut p = project.clone();
        p.rig.active_clip = index;
        progress.set_stage(format!(
            "Rendering clip {} of {clip_count} · {}",
            n + 1,
            p.rig.active_clip().name
        ));
        let rig = &p.rig;
        let clip = rig.active_clip();
        let sequence = clip.frame_sequence();
        let base = format!(
            "{}{:02}-{}{}",
            name(&preset.prefix),
            index + 1,
            name(&clip.name),
            if preset.suffix.is_empty() {
                String::new()
            } else {
                name(&preset.suffix)
            }
        );
        let base = if preset.prefix.is_empty() {
            base.strip_prefix("clip").unwrap_or(&base).to_string()
        } else {
            base
        };
        let full = Offset::default();
        let mut frames = vec![];
        match preset.format {
            _ if packed => {
                pending.add_clip(
                    &mut packer,
                    &p,
                    package.clips.len(),
                    &sequence,
                    false,
                    progress,
                )?;
                frames = sequence
                    .iter()
                    .map(|f| atlas::unplaced_frame(&p, *f))
                    .collect();
            }
            ExportFormat::Sheet | ExportFormat::IndexedSheet => {
                let img = project::render_sheet(&p, preset.columns, progress)?;
                let file = format!("{base}.png");
                let page = package.add_page(&file, PageKind::Atlas, img.width(), img.height());
                let cols = preset.columns.min(sequence.len() as u32).max(1);
                for (i, f) in sequence.iter().enumerate() {
                    let rect = sprite_package::grid_rect(i, cols, rig.width, rig.height);
                    frames.push(sprite_package::frame_in_active(
                        &p,
                        *f,
                        page,
                        Some(rect),
                        full,
                    ));
                }
                let bytes = if preset.format == ExportFormat::IndexedSheet {
                    indexed_png(&img, &p.extras.palette)?
                } else {
                    png_bytes(&img)?
                };
                planned.push((PathBuf::from(file), bytes));
            }
            ExportFormat::Gif | ExportFormat::Aseprite => {
                let extension = if preset.format == ExportFormat::Gif {
                    "gif"
                } else {
                    "aseprite"
                };
                let file = format!("{base}.{extension}");
                let bytes = if preset.format == ExportFormat::Gif {
                    project::render_gif(&p, progress)?
                } else {
                    crate::aseprite::encode_prepared(&p, false, progress)?
                };
                let page = package.add_page(&file, PageKind::Animation, rig.width, rig.height);
                for f in &sequence {
                    frames.push(sprite_package::frame_in_active(&p, *f, page, None, full));
                }
                planned.push((PathBuf::from(file), bytes));
            }
            ExportFormat::Frames => {
                let mut renderer =
                    crate::frame_cache::SequenceRenderer::new(&p, &sequence, progress);
                for (i, f) in sequence.iter().enumerate() {
                    let img = renderer.frame(*f)?;
                    let file = format!("{base}-{:03}.png", i + 1);
                    let page = package.add_page(&file, PageKind::Atlas, img.width(), img.height());
                    let rect = sprite_package::grid_rect(0, 1, rig.width, rig.height);
                    frames.push(sprite_package::frame_in_active(
                        &p,
                        *f,
                        page,
                        Some(rect),
                        full,
                    ));
                    planned.push((PathBuf::from(file), png_bytes(&img)?));
                }
            }
        }
        package.clips.push(sprite_package::Clip {
            id: format!("clip-{:02}", index + 1),
            animation,
            name: clip.name.clone(),
            direction: None,
            fps: clip.fps,
            looped: clip.looped,
            playback: clip.playback,
            frames,
        });
        ensure!(
            planned.iter().map(|(_, b)| b.len()).sum::<usize>() <= 128 * 1024 * 1024,
            "Export files exceed memory budget"
        );
    }
    if packed {
        let affix = |s: &str| if s.is_empty() { String::new() } else { name(s) };
        let (prefix, suffix) = (affix(&preset.prefix), affix(&preset.suffix));
        let palette = &project.extras.palette;
        planned.extend(pending.place(
            packer,
            &mut package,
            |i| format!("{prefix}atlas-{:02}{suffix}.png", i + 1),
            |page| {
                if preset.format == ExportFormat::IndexedSheet {
                    indexed_png(page, palette)
                } else {
                    png_bytes(page)
                }
            },
            progress,
        )?);
    }
    package.validate()?;
    planned.push((PathBuf::from("atlas.json"), package.to_json()?));
    ensure!(
        planned.iter().map(|(_, b)| b.len()).sum::<usize>() <= 128 * 1024 * 1024,
        "Export files and metadata exceed memory budget"
    );
    Ok(planned)
}
