use crate::{model::Rig, skin};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::Path,
};

const MAX_PIXELS: u64 = 16 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    pub width: u32,
    pub height: u32,
    #[serde(with = "pixel_bytes")]
    pub pixels: std::sync::Arc<Vec<u8>>,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub opacity: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub version: u32,
    #[serde(default)]
    pub compatibility: crate::project_format::Compatibility,
    pub rig: Rig,
    #[serde(default)]
    pub reference: Option<Reference>,
    #[serde(default)]
    pub poses: Vec<SavedPose>,
    #[serde(default)]
    pub extras: crate::features::Extras,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPose {
    pub name: String,
    pub pose: crate::animation::Pose,
}
impl Project {
    pub fn new(rig: Rig) -> Self {
        Self {
            version: crate::project_format::CURRENT_VERSION,
            compatibility: Default::default(),
            rig,
            reference: None,
            poses: vec![],
            extras: Default::default(),
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == crate::project_format::CURRENT_VERSION,
            "Unsupported project version {}; load older files through the project migration path",
            self.version
        );
        self.compatibility.validate()?;
        self.rig.validate().map_err(anyhow::Error::msg)?;
        self.extras.validate(&self.rig)?;
        for c in &self.extras.characters3d {
            c.validate_render(self.rig.width, self.rig.height)
                .with_context(|| {
                    format!(
                        "3D character '{}' ({}) exceeds its render budget",
                        c.name, c.id
                    )
                })?;
        }
        ensure!(
            self.render_work() <= 128 * 1024 * 1024,
            "Scene exceeds the per-frame rendering budget; reduce canvas size or character detail"
        );
        ensure!(
            self.extras.canvas_origin.is_finite()
                && self.extras.canvas_origin.x.abs() <= 100_000.
                && self.extras.canvas_origin.y.abs() <= 100_000.
                && self.extras.canvas_origin.x.fract() == 0.
                && self.extras.canvas_origin.y.fract() == 0.,
            "Invalid canvas origin"
        );
        ensure!(self.poses.len() <= 256, "Pose library exceeds 256 entries");
        for p in &self.poses {
            ensure!(
                p.name.len() <= 1024 && p.pose.len() <= self.rig.bones.len() + self.rig.parts.len(),
                "Invalid saved pose"
            );
            for (id, v) in &p.pose {
                ensure!(
                    self.rig.bone(*id).is_some() || self.rig.parts.iter().any(|p| p.id == *id),
                    "Pose references missing bone or group"
                );
                ensure!(
                    [
                        v.rotation,
                        v.x,
                        v.y,
                        v.scale,
                        v.z.unwrap_or(0.),
                        v.visibility.unwrap_or(1.)
                    ]
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 100_000.)
                        && (0.1..=4.).contains(&v.scale),
                    "Invalid saved pose transform"
                );
            }
        }
        if let Some(r) = &self.reference {
            ensure!(
                r.width > 0
                    && r.height > 0
                    && (r.width as u64 * r.height as u64) <= MAX_PIXELS
                    && r.pixels.len() as u64 == r.width as u64 * r.height as u64 * 4,
                "Invalid reference image"
            );
            ensure!(
                [r.x, r.y, r.scale, r.opacity].iter().all(|v| v.is_finite())
                    && r.x.abs() <= 100_000.
                    && r.y.abs() <= 100_000.
                    && (0.001..=1000.).contains(&r.scale)
                    && (0. ..=1.).contains(&r.opacity),
                "Invalid reference transform"
            );
        }
        ensure!(
            self.embedded_bytes() <= 64 * 1024 * 1024,
            "Embedded images and audio exceed the 64 MiB project budget"
        );
        Ok(())
    }
    fn embedded_bytes(&self) -> u64 {
        let own = self
            .extras
            .characters3d
            .iter()
            .map(|c| c.embedded_bytes() as u64)
            .sum::<u64>()
            + self.reference.as_ref().map_or(0, |r| r.pixels.len() as u64)
            + self
                .extras
                .references
                .as_ref()
                .map_or(0, |s| s.frames.iter().map(|r| r.pixels.len() as u64).sum())
            + self
                .extras
                .production
                .assets
                .iter()
                .map(|a| a.image.pixels.len() as u64)
                .sum::<u64>()
            + self
                .extras
                .production
                .audio
                .as_ref()
                .map_or(0, |a| a.pcm.len() as u64);
        self.extras
            .production
            .actors
            .iter()
            .fold(own, |n, a| n.saturating_add(a.project.embedded_bytes()))
    }
    pub fn render_work(&self) -> u64 {
        fn characters(p: &Project) -> u64 {
            p.extras
                .characters3d
                .iter()
                .fold(0u64, |n, c| {
                    n.saturating_add(c.render_work(p.rig.width, p.rig.height))
                })
                .saturating_add(
                    p.extras
                        .production
                        .actors
                        .iter()
                        .map(|a| characters(&a.project))
                        .sum(),
                )
        }
        self.extras
            .production
            .render_work(&self.rig)
            .saturating_add(characters(self))
    }
    pub fn capture_pose(&mut self, name: String, frame: f32) -> Result<()> {
        ensure!(
            self.poses.len() < 256 && name.len() <= 1024,
            "Pose library limit exceeded"
        );
        let mut pose = self.pose_at(frame);
        for bone in &self.rig.bones {
            pose.entry(bone.id).or_default();
        }
        self.poses.push(SavedPose { name, pose });
        Ok(())
    }
    pub fn pose_at(&self, frame: f32) -> crate::animation::Pose {
        if self
            .extras
            .production
            .baked_motion
            .contains(&self.rig.active_clip)
        {
            return self.anchored_pose(self.rig.active_clip().sample(frame));
        }
        let motion = &self.extras.production.motion;
        let layered = self.anchored_pose(crate::advanced_motion::layers(
            &self.rig,
            motion,
            self.rig.active_clip().source_frame(frame),
            &self.anchored_pose(self.rig.active_clip().sample(frame)),
        ));
        let extras = crate::features::Extras {
            pins: self
                .extras
                .pins
                .iter()
                .filter(|pin| {
                    !motion.contacts.iter().any(|c| {
                        c.enabled
                            && c.lower == pin.lower
                            && c.clip.is_none_or(|i| i == self.rig.active_clip)
                    })
                })
                .cloned()
                .collect(),
            ground: self.extras.ground,
            ..Default::default()
        };
        let pinned = crate::features::solve_pins(&self.rig, &extras, &layered);
        self.anchored_pose(crate::advanced_motion::evaluate_constraints(
            &self.rig, motion, frame, &pinned,
        ))
    }
    fn anchored_pose(&self, mut pose: crate::animation::Pose) -> crate::animation::Pose {
        for b in self.rig.bones.iter().filter(|b| b.parent.is_some()) {
            if let Some(p) = pose.get_mut(&b.id) {
                p.x = 0.;
                p.y = 0.;
            }
        }
        pose
    }
    pub fn render_at(&self, frame: f32) -> image::RgbaImage {
        self.render_pose(frame, Some(&self.pose_at(frame)))
    }
    /// Checked rendering for exports and interactive previews. A missing scene
    /// component must never be mistaken for a successful render.
    pub fn try_render_at(&self, frame: f32) -> Result<image::RgbaImage> {
        ensure!(frame.is_finite(), "Render frame must be finite");
        ensure!(
            self.rig.active_clip < self.rig.clips.len(),
            "Render references an invalid active timeline clip"
        );
        self.try_render_pose(frame, Some(&self.pose_at(frame)))
    }
    pub fn complete_pose_at(&self, frame: f32) -> crate::animation::Pose {
        let mut pose = self.pose_at(frame);
        for b in &self.rig.bones {
            pose.entry(b.id).or_default();
        }
        for part in &self.rig.parts {
            let p = pose.entry(part.id).or_default();
            p.z.get_or_insert(part.z as f32);
            p.visibility.get_or_insert(1.);
        }
        pose
    }
    pub fn render_pose(
        &self,
        frame: f32,
        pose: Option<&crate::animation::Pose>,
    ) -> image::RgbaImage {
        if !self.extras.production.has_art() && self.extras.characters3d.is_empty() {
            if !self.extras.show_2d {
                return image::RgbaImage::new(self.rig.width, self.rig.height);
            }
            return skin::render_region(&self.rig, pose, self.extras.canvas_origin);
        }
        crate::production::render::render_project(self, frame, pose, true)
    }
    pub fn try_render_pose(
        &self,
        frame: f32,
        pose: Option<&crate::animation::Pose>,
    ) -> Result<image::RgbaImage> {
        ensure!(frame.is_finite(), "Render frame must be finite");
        ensure!(
            self.rig.active_clip < self.rig.clips.len(),
            "Render references an invalid active timeline clip"
        );
        ensure!(
            (1..=4096).contains(&self.rig.width) && (1..=4096).contains(&self.rig.height),
            "Render canvas dimensions must be between 1 and 4096 pixels"
        );
        if !self.extras.production.has_art() && self.extras.characters3d.is_empty() {
            if !self.extras.show_2d {
                return Ok(image::RgbaImage::new(self.rig.width, self.rig.height));
            }
            return Ok(skin::render_region(
                &self.rig,
                pose,
                self.extras.canvas_origin,
            ));
        }
        crate::production::render::try_render_project(self, frame, pose, true)
    }
    pub fn baked_rig(&self) -> Result<Rig> {
        self.validate()?;
        let mut rig = self.rig.clone();
        let motion = &self.extras.production.motion;
        if !self.extras.pins.iter().any(|p| p.enabled)
            && motion.chains.is_empty()
            && motion.contacts.is_empty()
            && motion.layers.is_empty()
            && motion.springs.is_empty()
            && self.rig.active_clip().source.is_none()
            && !self.rig.active_clip().tracks.iter().any(|t| {
                matches!(
                    t.channel,
                    crate::animation::Channel::X | crate::animation::Channel::Y
                ) && self.rig.bone(t.bone).is_some_and(|b| b.parent.is_some())
            })
        {
            return Ok(rig);
        }
        rig.active_clip_mut().source = None;
        rig.active_clip_mut().tracks.clear();
        for frame in 1..=rig.active_clip().length {
            let pose = self.complete_pose_at(frame as f32);
            crate::advanced_motion::bake_pose(&mut rig, &pose, frame as f32)
                .map_err(anyhow::Error::msg)?;
            for pin in self.extras.pins.iter().filter(|p| p.enabled) {
                let lower = rig.bone(pin.lower).unwrap();
                let ids = [pin.lower, lower.parent.unwrap()];
                for id in ids {
                    if let Some(p) = pose.get(&id) {
                        rig.active_clip_mut()
                            .set_key(
                                id,
                                crate::animation::Channel::Rotation,
                                frame as f32,
                                p.rotation,
                            )
                            .map_err(anyhow::Error::msg)?;
                    }
                }
            }
        }
        rig.validate().map_err(anyhow::Error::msg)?;
        Ok(rig)
    }
    pub fn apply_pose(&mut self, index: usize, frame: f32, mirror: bool) -> Result<()> {
        let saved = self
            .poses
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("No such saved pose"))?
            .clone();
        let mut candidate = self.rig.clone();
        let source_world = candidate.world(Some(&saved.pose));
        let reflected: crate::animation::Pose = saved
            .pose
            .iter()
            .map(|(id, p)| {
                let target = candidate.bone(*id).and_then(|b| b.twin).unwrap_or(*id);
                let mut p = *p;
                p.rotation = -p.rotation;
                (target, p)
            })
            .collect();
        let target_world = candidate.world(Some(&reflected));
        for (id, mut p) in saved.pose {
            let target = if mirror {
                candidate.bone(id).and_then(|b| b.twin).unwrap_or(id)
            } else {
                id
            };
            if mirror {
                p.rotation = -p.rotation;
                if candidate.bone(target).is_some_and(|b| !b.connected) {
                    let source_angle = candidate
                        .bone(id)
                        .and_then(|b| b.parent)
                        .and_then(|id| source_world.iter().find(|w| w.id == id))
                        .map_or(0., |w| w.angle);
                    let target_angle = candidate
                        .bone(target)
                        .and_then(|b| b.parent)
                        .and_then(|id| target_world.iter().find(|w| w.id == id))
                        .map_or(0., |w| w.angle);
                    let world = crate::model::Vec2::new(p.x, p.y).rotate(source_angle);
                    let local = crate::model::Vec2::new(-world.x, world.y).rotate(-target_angle);
                    p.x = local.x;
                    p.y = local.y;
                }
            }
            if candidate.bone(target).is_some() {
                for (ch, v) in [
                    (crate::animation::Channel::Rotation, p.rotation),
                    (crate::animation::Channel::X, p.x),
                    (crate::animation::Channel::Y, p.y),
                    (crate::animation::Channel::Scale, p.scale),
                ] {
                    candidate
                        .active_clip_mut()
                        .set_key(target, ch, frame, v)
                        .map_err(anyhow::Error::msg)?;
                }
            }
            for (ch, v) in [
                (crate::animation::Channel::Z, p.z),
                (crate::animation::Channel::Visibility, p.visibility),
            ] {
                if let Some(v) = v {
                    candidate
                        .active_clip_mut()
                        .set_key(target, ch, frame, v)
                        .map_err(anyhow::Error::msg)?;
                }
            }
        }
        candidate.validate().map_err(anyhow::Error::msg)?;
        self.rig = candidate;
        Ok(())
    }
}
#[derive(Default)]
pub struct History {
    undo: VecDeque<Project>,
    redo: Vec<Project>,
}
impl History {
    pub fn record(&mut self, p: &Project) {
        if self.undo.back() != Some(p) {
            self.undo.push_back(p.clone());
            if self.undo.len() > 60 {
                self.undo.pop_front();
            }
        }
        self.redo.clear();
    }
    pub fn undo(&mut self, p: &mut Project) -> bool {
        if let Some(old) = self.undo.pop_back() {
            self.redo.push(p.clone());
            *p = old;
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, p: &mut Project) -> bool {
        if let Some(new) = self.redo.pop() {
            self.undo.push_back(p.clone());
            *p = new;
            true
        } else {
            false
        }
    }
}
pub fn load(path: &Path) -> Result<Project> {
    Ok(load_with_report(path)?.project)
}
pub fn load_with_report(path: &Path) -> Result<crate::project_format::LoadReport> {
    let file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= crate::project_format::MAX_PROJECT_BYTES,
        "Project exceeds 96 MiB"
    );
    let mut bytes = Vec::new();
    file.take(crate::project_format::MAX_PROJECT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    crate::project_format::decode_with_report(&bytes)
}
pub fn decode(bytes: &[u8]) -> Result<Project> {
    crate::project_format::decode(bytes)
}
/// Validate and serialize completely before a save or backup mutates any file.
pub fn encode(p: &Project) -> Result<Vec<u8>> {
    p.validate()?;
    let bytes = serde_json::to_vec_pretty(p)?;
    ensure!(
        bytes.len() as u64 <= crate::project_format::MAX_PROJECT_BYTES,
        "Serialized project exceeds 96 MiB"
    );
    Ok(bytes)
}
pub fn save(path: &Path, p: &Project) -> Result<()> {
    let bytes = encode(p)?;
    atomic_write(path, &bytes)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    // Same-directory temporary + rename keeps an existing project safe on failure.
    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("Missing filename"))?
        .to_string_lossy();
    static TEMP_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_file_name(format!(
        ".{name}.{}.{}.{sequence}.tmp",
        std::process::id(),
        crate::storage::unique()
    ));
    let mut owns_temp = false;
    let result = (|| -> Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        owns_temp = true;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() && owns_temp {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
pub fn sheet(rig: &Rig, columns: u32) -> Result<image::RgbaImage> {
    rig.validate().map_err(anyhow::Error::msg)?;
    export_budget(rig)?;
    let sequence = rig.active_clip().frame_sequence();
    let n = sequence.len() as u32;
    let cols = columns.clamp(1, n);
    let rows = n.div_ceil(cols);
    let w = rig.width as u64 * cols as u64;
    let h = rig.height as u64 * rows as u64;
    ensure!(
        w * h <= MAX_PIXELS,
        "Sprite sheet exceeds 16 megapixels; reduce canvas or frames"
    );
    ensure!(
        rig.width as u64 * rig.height as u64 * n as u64 <= MAX_PIXELS,
        "Animation exceeds export budget"
    );
    let mut out = image::RgbaImage::new(w as u32, h as u32);
    for (index, frame_number) in sequence.iter().enumerate() {
        let f = index as u32;
        let pose = rig.active_clip().sample(*frame_number as f32);
        let frame = skin::render(rig, Some(&pose));
        image::imageops::replace(
            &mut out,
            &frame,
            ((f % cols) * rig.width) as i64,
            ((f / cols) * rig.height) as i64,
        );
    }
    Ok(out)
}
pub fn sheet_project(project: &Project, columns: u32) -> Result<image::RgbaImage> {
    project.validate()?;
    let mut prepared = project.clone();
    crate::canvas::grow_all(&mut prepared)?;
    let project = &prepared;
    project.validate()?;
    let rig = &project.rig;
    let sequence = rig.active_clip().frame_sequence();
    let n = sequence.len() as u32;
    let cols = columns.clamp(1, n);
    let rows = n.div_ceil(cols);
    let w = rig.width as u64 * cols as u64;
    let h = rig.height as u64 * rows as u64;
    ensure!(w * h <= MAX_PIXELS, "Sprite sheet exceeds 16 megapixels");
    project_export_budget(project, n as u64)?;
    let mut out = image::RgbaImage::new(w as u32, h as u32);
    for (i, f) in sequence.iter().enumerate() {
        image::imageops::replace(
            &mut out,
            &project.try_render_at(*f as f32)?,
            (i as u32 % cols * rig.width) as i64,
            (i as u32 / cols * rig.height) as i64,
        );
    }
    Ok(out)
}
pub fn gif_project(project: &Project) -> Result<Vec<u8>> {
    project.validate()?;
    let mut prepared = project.clone();
    crate::canvas::grow_all(&mut prepared)?;
    let project = &prepared;
    project.validate()?;
    let clip = project.rig.active_clip();
    let sequence = clip.frame_sequence();
    project_export_budget(project, sequence.len() as u64)?;
    let mut bytes = vec![];
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        if clip.looped {
            encoder.set_repeat(image::codecs::gif::Repeat::Infinite)?;
        }
        for f in sequence {
            encoder.encode_frame(image::Frame::from_parts(
                project.try_render_at(f as f32)?,
                0,
                0,
                image::Delay::from_numer_denom_ms(clip.duration_ms(f), 1),
            ))?;
        }
    }
    Ok(bytes)
}
pub fn project_export_budget(project: &Project, frames: u64) -> Result<()> {
    let r = &project.rig;
    let base = r.width as u64 * r.height as u64;
    let work = project.render_work();
    ensure!(
        base.saturating_mul(frames) <= MAX_PIXELS
            && work.saturating_mul(frames) <= 256 * 1024 * 1024,
        "Scene animation exceeds export budget; reduce canvas, actors or frames"
    );
    Ok(())
}
pub fn gif(rig: &Rig) -> Result<Vec<u8>> {
    rig.validate().map_err(anyhow::Error::msg)?;
    export_budget(rig)?;
    ensure!(
        rig.width as u64 * rig.height as u64 * rig.active_clip().length as u64 <= MAX_PIXELS,
        "Animation exceeds 16 megapixel export budget"
    );
    let mut bytes = Vec::new();
    {
        let mut enc = image::codecs::gif::GifEncoder::new(&mut bytes);
        if rig.active_clip().looped {
            enc.set_repeat(image::codecs::gif::Repeat::Infinite)?;
        }
        for f in rig.active_clip().frame_sequence() {
            let ms = rig.active_clip().duration_ms(f);
            let pose = rig.active_clip().sample(f as f32);
            let img = skin::render(rig, Some(&pose));
            enc.encode_frame(image::Frame::from_parts(
                img,
                0,
                0,
                image::Delay::from_numer_denom_ms(ms, 1),
            ))?;
        }
    }
    Ok(bytes)
}

fn export_budget(rig: &Rig) -> Result<()> {
    let work = rig.width as u64
        * rig.height as u64
        * rig.active_clip().length as u64
        * rig.bones.len().max(1) as u64;
    ensure!(
        work <= 128 * 1024 * 1024,
        "Animation exceeds render budget; reduce canvas, bone count, or frames"
    );
    Ok(())
}
pub fn import_reference(path: &Path) -> Result<Reference> {
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(MAX_PIXELS * 4);
    reader.limits(limits);
    let img = reader.decode()?.to_rgba8();
    if img.width() == 0 || img.height() == 0 {
        bail!("Empty image");
    }
    Ok(Reference {
        width: img.width(),
        height: img.height(),
        pixels: std::sync::Arc::new(img.into_raw()),
        x: 0.,
        y: 0.,
        scale: 1.,
        opacity: 0.5,
    })
}
pub fn templates() -> Vec<Rig> {
    serde_json::from_str::<Vec<Rig>>(include_str!("../assets/templates.json"))
        .expect("Bundled template data")
}

mod pixel_bytes {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer};
    use std::sync::Arc;
    pub fn serialize<S: Serializer>(v: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(v.as_slice()))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
        let text = String::deserialize(d)?;
        if text.len() > super::MAX_PIXELS as usize * 4 / 3 * 4 + 4 {
            return Err(serde::de::Error::custom(
                "Reference image exceeds byte budget",
            ));
        }
        STANDARD
            .decode(text)
            .map(Arc::new)
            .map_err(serde::de::Error::custom)
    }
}
