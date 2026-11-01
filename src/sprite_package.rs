//! Shared, versioned description of exported sprite animation.
//!
//! Batch exports (`atlas.json`) and engine packages (`package.json`) both write
//! this document. Its version is independent of the native `.rebone` project
//! schema in [`crate::project_format`]: changing one never implies the other.
//!
//! Coordinates are pixels with a top-left origin and y pointing down. A frame's
//! *source* space is the full, untrimmed canvas of one rendered frame; pivots and
//! collision regions are expressed in that space so they stay valid however the
//! pixels are trimmed or packed into pages.
use crate::{
    animation::{Clip as AnimationClip, Playback},
    model::Vec2,
    production::{CollisionShape, EventMarker},
    project::Project,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

/// Identifies the document regardless of the file name it is stored under.
pub const FORMAT: &str = "rebonebase-sprites";
/// Current export-format version. Version 1 is the original engine
/// `package.json`; unversioned batch `atlas.json` files are read as version 0.
pub const FORMAT_VERSION: u32 = 2;
pub const COORDINATES: &str = "pixels, top-left origin, y down; angles degrees clockwise; pivot and boxes in source frame space";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offset {
    pub x: u32,
    pub y: u32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    /// A still image holding one or more frame rectangles.
    #[default]
    Atlas,
    /// A GIF or Aseprite file whose own frame order matches the clip frames.
    Animation,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub file: String,
    #[serde(default)]
    pub kind: PageKind,
    pub w: u32,
    pub h: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Direction {
    pub id: String,
    /// Facing angle in degrees clockwise from screen right.
    pub angle: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub page: usize,
    /// Pixels inside the page; absent for [`PageKind::Animation`] pages.
    pub rect: Option<Rect>,
    /// Untrimmed frame dimensions.
    pub source_size: Size,
    /// Where `rect`'s top-left sits inside the untrimmed frame.
    pub trim: Offset,
    pub source_frame: u32,
    pub source_time_ms: u64,
    pub duration_ms: u32,
    pub events: Vec<EventMarker>,
    pub boxes: Vec<CollisionShape>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    /// Stable per-export identifier such as `clip-01`.
    pub id: String,
    /// Unique, nonempty engine animation name.
    pub animation: String,
    pub name: String,
    #[serde(default)]
    pub direction: Option<Direction>,
    pub fps: f32,
    pub looped: bool,
    #[serde(default)]
    pub playback: Playback,
    pub frames: Vec<Frame>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Audio {
    pub file: String,
    pub start_ms: u64,
    pub volume: f32,
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpritePackage {
    pub format: String,
    pub format_version: u32,
    pub generator: String,
    pub name: String,
    pub coordinates: String,
    pub frame_size: Size,
    /// Anchor in source frame pixels; identical for every frame.
    pub pivot: Vec2,
    pub canvas_origin: Vec2,
    pub pages: Vec<Page>,
    pub clips: Vec<Clip>,
    #[serde(default)]
    pub audio: Option<Audio>,
}
impl SpritePackage {
    /// Start a package for a project already passed through `prepare_export`.
    pub fn new(project: &Project, generator: &str) -> Self {
        let rig = &project.rig;
        let pivot = project.extras.production.pivot;
        Self {
            format: FORMAT.into(),
            format_version: FORMAT_VERSION,
            generator: generator.into(),
            name: rig.name.clone(),
            coordinates: COORDINATES.into(),
            frame_size: Size {
                w: rig.width,
                h: rig.height,
            },
            pivot: Vec2::new(pivot.x * rig.width as f32, pivot.y * rig.height as f32),
            canvas_origin: project.extras.canvas_origin,
            pages: vec![],
            clips: vec![],
            audio: None,
        }
    }
    pub fn add_page(&mut self, file: impl Into<String>, kind: PageKind, w: u32, h: u32) -> usize {
        self.pages.push(Page {
            file: file.into(),
            kind,
            w,
            h,
        });
        self.pages.len() - 1
    }
    /// Pivot normalized against the source frame, as version 1 stored it.
    pub fn pivot_normalized(&self) -> Vec2 {
        Vec2::new(
            self.pivot.x / self.frame_size.w.max(1) as f32,
            self.pivot.y / self.frame_size.h.max(1) as f32,
        )
    }
    pub fn to_json(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }
    /// Check references and rectangles so consumers can index without guards.
    pub fn validate(&self) -> Result<()> {
        ensure!(self.format == FORMAT, "Not a ReBoneBase sprite package");
        ensure!(
            self.format_version <= FORMAT_VERSION,
            "Sprite package version {} is newer than this build supports ({FORMAT_VERSION})",
            self.format_version
        );
        let mut ids = HashSet::new();
        let mut animations = HashSet::new();
        for clip in &self.clips {
            ensure!(ids.insert(&clip.id), "Duplicate clip id {}", clip.id);
            ensure!(
                !clip.animation.is_empty() && animations.insert(&clip.animation),
                "Clip animation names must be unique and nonempty"
            );
            for frame in &clip.frames {
                let page = self
                    .pages
                    .get(frame.page)
                    .with_context(|| format!("Clip {} references a missing page", clip.id))?;
                match (page.kind, frame.rect) {
                    (PageKind::Atlas, Some(r)) => {
                        ensure!(
                            r.x as u64 + r.w as u64 <= page.w as u64
                                && r.y as u64 + r.h as u64 <= page.h as u64,
                            "Frame rectangle leaves page {}",
                            page.file
                        );
                        ensure!(
                            frame.trim.x as u64 + r.w as u64 <= frame.source_size.w as u64
                                && frame.trim.y as u64 + r.h as u64 <= frame.source_size.h as u64,
                            "Trimmed frame exceeds its source size"
                        );
                    }
                    (PageKind::Animation, None) => {}
                    _ => bail!("Frame rectangle does not match page {}", page.file),
                }
            }
        }
        Ok(())
    }
}

/// Parse any version this build understands and upgrade it to the current shape.
pub fn parse(bytes: &[u8]) -> Result<SpritePackage> {
    let value: Value = serde_json::from_slice(bytes).context("Sprite package is not JSON")?;
    let package = if value.get("format").is_some() {
        let version = value["format_version"].as_u64().unwrap_or(0);
        ensure!(
            version <= FORMAT_VERSION as u64,
            "Sprite package version {version} is newer than this build supports ({FORMAT_VERSION})"
        );
        serde_json::from_value(value)?
    } else if value.get("version").and_then(Value::as_u64) == Some(1) {
        upgrade_engine_v1(&value)?
    } else if value.get("app").and_then(Value::as_str) == Some("ReBoneBase") {
        upgrade_batch_v0(&value)?
    } else {
        bail!("Not a ReBoneBase sprite package")
    };
    package.validate()?;
    Ok(package)
}

#[derive(Deserialize)]
struct LegacyFrame {
    source_frame: u32,
    #[serde(default)]
    source_time_ms: u64,
    #[serde(default, alias = "duration")]
    duration_ms: u32,
    #[serde(default)]
    events: Vec<EventMarker>,
    #[serde(default)]
    boxes: Vec<CollisionShape>,
}
fn legacy_frame(value: &Value, page: usize, rect: Option<Rect>, size: Size) -> Result<Frame> {
    let f: LegacyFrame = serde_json::from_value(value.clone())?;
    Ok(Frame {
        page,
        rect,
        source_size: size,
        trim: Offset::default(),
        source_frame: f.source_frame,
        source_time_ms: f.source_time_ms,
        duration_ms: f.duration_ms,
        events: f.events,
        boxes: f.boxes,
    })
}
fn u32_of(value: &Value, key: &str) -> u32 {
    value[key].as_u64().unwrap_or(0) as u32
}
fn legacy_package(name: &str, generator: &str, size: Size, pivot: Vec2) -> SpritePackage {
    SpritePackage {
        format: FORMAT.into(),
        format_version: FORMAT_VERSION,
        generator: generator.into(),
        name: name.into(),
        coordinates: COORDINATES.into(),
        frame_size: size,
        pivot: Vec2::new(pivot.x * size.w as f32, pivot.y * size.h as f32),
        canvas_origin: Vec2::default(),
        pages: vec![],
        clips: vec![],
        audio: None,
    }
}
fn page_index(package: &mut SpritePackage, file: &str, kind: PageKind, size: Size) -> usize {
    if let Some(i) = package.pages.iter().position(|p| p.file == file) {
        return i;
    }
    package.add_page(file, kind, size.w, size.h)
}
/// Version 1 engine `package.json`: one grid sheet per clip, normalized pivot.
fn upgrade_engine_v1(value: &Value) -> Result<SpritePackage> {
    let size = Size {
        w: u32_of(value, "width"),
        h: u32_of(value, "height"),
    };
    let pivot: Vec2 = serde_json::from_value(value["pivot"].clone()).unwrap_or_default();
    let mut package = legacy_package(
        value["name"].as_str().unwrap_or_default(),
        value["generator"].as_str().unwrap_or("ReBoneBase"),
        size,
        pivot,
    );
    package.canvas_origin =
        serde_json::from_value(value["canvas_origin"].clone()).unwrap_or_default();
    for (i, clip) in value["clips"].as_array().into_iter().flatten().enumerate() {
        let file = clip["file"]
            .as_str()
            .context("Version 1 clip has no sheet")?;
        let frames = clip["frames"].as_array().cloned().unwrap_or_default();
        // Version 1 did not record sheet dimensions; the frame extents bound them.
        let extent = frames.iter().fold(Size::default(), |s, f| Size {
            w: s.w.max(u32_of(f, "x") + u32_of(f, "w")),
            h: s.h.max(u32_of(f, "y") + u32_of(f, "h")),
        });
        let page = page_index(&mut package, file, PageKind::Atlas, extent);
        let frames = frames
            .iter()
            .map(|f| {
                let rect = Rect {
                    x: u32_of(f, "x"),
                    y: u32_of(f, "y"),
                    w: u32_of(f, "w"),
                    h: u32_of(f, "h"),
                };
                legacy_frame(f, page, Some(rect), size)
            })
            .collect::<Result<_>>()?;
        let name = clip["name"].as_str().unwrap_or_default().to_string();
        package.clips.push(Clip {
            id: clip["id"]
                .as_str()
                .map_or_else(|| format!("clip-{:02}", i + 1), str::to_string),
            animation: clip["animation"].as_str().unwrap_or(&name).to_string(),
            name,
            direction: None,
            fps: clip["fps"].as_f64().unwrap_or(12.) as f32,
            looped: clip["looped"].as_bool().unwrap_or(true),
            playback: Playback::Forward,
            frames,
        });
    }
    if let Some(file) = value["audio_file"].as_str() {
        package.audio = Some(Audio {
            file: file.into(),
            start_ms: value["audio_start_ms"].as_u64().unwrap_or(0),
            volume: value["audio_volume"].as_f64().unwrap_or(1.) as f32,
            enabled: value["audio_enabled"].as_bool().unwrap_or(false),
        });
    }
    unique_animations(&mut package);
    Ok(package)
}
/// Unversioned batch `atlas.json`: per-frame file names, optional `frame` rects.
fn upgrade_batch_v0(value: &Value) -> Result<SpritePackage> {
    let clips = value["clips"].as_array().cloned().unwrap_or_default();
    let size = clips.first().map_or(Size::default(), |c| Size {
        w: u32_of(c, "width"),
        h: u32_of(c, "height"),
    });
    // The batch atlas never recorded a pivot; version 1's default was bottom-center.
    let mut package = legacy_package("", "ReBoneBase", size, Vec2::new(0.5, 1.));
    let mut extents = BTreeMap::<String, Size>::new();
    for frame in clips
        .iter()
        .flat_map(|c| c["frames"].as_array().cloned().unwrap_or_default())
    {
        let file = frame["file"].as_str().unwrap_or_default().to_string();
        let e = extents.entry(file).or_default();
        if frame["frame"].is_object() {
            let r = &frame["frame"];
            e.w = e.w.max(u32_of(r, "x") + u32_of(r, "w"));
            e.h = e.h.max(u32_of(r, "y") + u32_of(r, "h"));
        } else {
            *e = size;
        }
    }
    for (i, clip) in clips.iter().enumerate() {
        let clip_size = Size {
            w: u32_of(clip, "width"),
            h: u32_of(clip, "height"),
        };
        let mut frames = vec![];
        for f in clip["frames"].as_array().into_iter().flatten() {
            let file = f["file"].as_str().context("Batch frame has no file")?;
            let animated = file.ends_with(".gif") || file.ends_with(".aseprite");
            let (kind, rect) = if animated {
                (PageKind::Animation, None)
            } else if f["frame"].is_object() {
                let r = &f["frame"];
                let rect = Rect {
                    x: u32_of(r, "x"),
                    y: u32_of(r, "y"),
                    w: u32_of(r, "w"),
                    h: u32_of(r, "h"),
                };
                (PageKind::Atlas, Some(rect))
            } else {
                let rect = Rect {
                    x: 0,
                    y: 0,
                    w: clip_size.w,
                    h: clip_size.h,
                };
                (PageKind::Atlas, Some(rect))
            };
            let page = page_index(&mut package, file, kind, extents[file]);
            frames.push(legacy_frame(f, page, rect, clip_size)?);
        }
        let name = clip["name"].as_str().unwrap_or_default().to_string();
        package.clips.push(Clip {
            id: format!("clip-{:02}", i + 1),
            animation: name.clone(),
            name,
            direction: None,
            fps: clip["fps"].as_f64().unwrap_or(12.) as f32,
            looped: clip["loop"].as_bool().unwrap_or(true),
            playback: serde_json::from_value(clip["playback"].clone()).unwrap_or_default(),
            frames,
        });
    }
    unique_animations(&mut package);
    Ok(package)
}
fn unique_animations(package: &mut SpritePackage) {
    let names = unique_names(package.clips.iter().map(|c| c.animation.as_str()));
    for (clip, name) in package.clips.iter_mut().zip(names) {
        clip.animation = name;
    }
}
/// Nonempty, unique engine animation names in clip order.
pub fn unique_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut used = HashSet::new();
    names
        .into_iter()
        .map(|name| {
            let base = if name.trim().is_empty() {
                "Animation"
            } else {
                name
            };
            let mut animation = base.to_string();
            let mut suffix = 2;
            while used.contains(&animation) {
                animation = format!("{base} ({suffix})");
                suffix += 1;
            }
            used.insert(animation.clone());
            animation
        })
        .collect()
}

/// Timing, events and collision for one exported frame of the project's active
/// clip. Both exporters use this so their outputs cannot disagree on what a
/// frame means.
pub fn frame_in_active(
    project: &Project,
    source: u32,
    page: usize,
    rect: Option<Rect>,
    trim: Offset,
) -> Frame {
    let clip_index = project.rig.active_clip;
    let clip: &AnimationClip = project.rig.active_clip();
    Frame {
        page,
        rect,
        source_size: Size {
            w: project.rig.width,
            h: project.rig.height,
        },
        trim,
        source_frame: source,
        source_time_ms: clip.source_time_ms(source as f32),
        duration_ms: clip.duration_ms(source),
        events: project
            .extras
            .production
            .events
            .iter()
            .filter(|e| e.clip == clip_index && (e.frame - source as f32).abs() < 0.5)
            .cloned()
            .collect(),
        boxes: crate::production::collision_pixels(project, source as f32),
    }
}
/// Full-size grid placement used before packing: frame `i` of `columns`.
pub fn grid_rect(i: usize, columns: u32, w: u32, h: u32) -> Rect {
    Rect {
        x: i as u32 % columns * w,
        y: i as u32 / columns * h,
        w,
        h,
    }
}
