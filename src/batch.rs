use crate::project::{self, Project};
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportPreset {
    pub name: String,
    pub format: ExportFormat,
    pub columns: u32,
    pub prefix: String,
    pub suffix: String,
    pub all_clips: bool,
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
    project.validate()?;
    let mut prepared = project.clone();
    crate::canvas::grow_all(&mut prepared)?;
    let project = &prepared;
    project.validate()?;
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
    let mut planned = Vec::<(PathBuf, Vec<u8>)>::new();
    let mut clips = vec![];
    for index in indices {
        let mut p = project.clone();
        p.rig.active_clip = index;
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
        let mut frames = vec![];
        match preset.format {
            ExportFormat::Sheet | ExportFormat::IndexedSheet => {
                let img = project::sheet_project(&p, preset.columns)?;
                let file = format!("{base}.png");
                let cols = preset.columns.min(sequence.len() as u32).max(1);
                for (i, f) in sequence.iter().enumerate() {
                    frames.push(serde_json::json!({"file":file,"source_frame":f,"frame":{"x":i as u32%cols*rig.width,"y":i as u32/cols*rig.height,"w":rig.width,"h":rig.height},"duration":clip.duration_ms(*f),"events":frame_events(&p,*f),"boxes":crate::production::collision_pixels(&p,*f as f32)}));
                }
                let bytes = if preset.format == ExportFormat::IndexedSheet {
                    indexed_png(&img, &p.extras.palette)?
                } else {
                    png_bytes(&img)?
                };
                planned.push((folder.join(file), bytes));
            }
            ExportFormat::Gif | ExportFormat::Aseprite => {
                let extension = if preset.format == ExportFormat::Gif {
                    "gif"
                } else {
                    "aseprite"
                };
                let file = format!("{base}.{extension}");
                let bytes = if preset.format == ExportFormat::Gif {
                    project::gif_project(&p)?
                } else {
                    crate::aseprite::encode_project(&p, false)?
                };
                for f in &sequence {
                    frames.push(serde_json::json!({"file":file,"source_frame":f,"duration":clip.duration_ms(*f),"events":frame_events(&p,*f),"boxes":crate::production::collision_pixels(&p,*f as f32)}));
                }
                planned.push((folder.join(file), bytes));
            }
            ExportFormat::Frames => {
                for (i, f) in sequence.iter().enumerate() {
                    let img = p.render_at(*f as f32);
                    let file = format!("{base}-{:03}.png", i + 1);
                    frames.push(serde_json::json!({"file":file,"source_frame":f,"duration":clip.duration_ms(*f),"events":frame_events(&p,*f),"boxes":crate::production::collision_pixels(&p,*f as f32)}));
                    planned.push((folder.join(file), png_bytes(&img)?));
                }
            }
        }
        clips.push(serde_json::json!({"name":clip.name,"width":rig.width,"height":rig.height,"fps":clip.fps,"loop":clip.looped,"playback":clip.playback,"frames":frames}));
        ensure!(
            planned.iter().map(|(_, b)| b.len()).sum::<usize>() <= 128 * 1024 * 1024,
            "Export files exceed memory budget"
        );
    }
    planned.push((
        folder.join("atlas.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"app":"ReBoneBase","clips":clips}))?,
    ));
    ensure!(
        planned.iter().map(|(_, b)| b.len()).sum::<usize>() <= 128 * 1024 * 1024,
        "Export files and metadata exceed memory budget"
    );
    if !overwrite {
        ensure!(
            planned.iter().all(|(p, _)| !p.exists()),
            "Export would replace existing files; enable Replace existing files or choose a new directory"
        );
    }
    std::fs::create_dir_all(folder)?;
    let mut files = vec![];
    for (path, bytes) in planned {
        project::atomic_write(&path, &bytes)?;
        files.push(path);
    }
    Ok(ExportReport { files })
}
fn frame_events(project: &Project, frame: u32) -> Vec<&crate::production::EventMarker> {
    project
        .extras
        .production
        .events
        .iter()
        .filter(|e| e.clip == project.rig.active_clip && (e.frame - frame as f32).abs() < 0.5)
        .collect()
}
