use crate::{
    animation::Pose,
    model::{Rig, Vec2},
    project::Reference,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    pub lower: u32,
    pub target: Vec2,
    pub enabled: bool,
    pub grounded: bool,
    pub bend: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OnionSettings {
    pub before: u32,
    pub after: u32,
    pub spacing: u32,
    pub opacity: f32,
    pub before_color: [u8; 3],
    pub after_color: [u8; 3],
}
impl Default for OnionSettings {
    fn default() -> Self {
        Self {
            before: 1,
            after: 1,
            spacing: 1,
            opacity: 0.22,
            before_color: [110, 170, 255],
            after_color: [255, 125, 155],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReferenceSequence {
    pub frames: Vec<Reference>,
    pub fps: f32,
    pub start_ms: u64,
    pub looped: bool,
}
impl ReferenceSequence {
    pub fn index_at(&self, time_ms: u64) -> usize {
        if self.frames.is_empty() {
            return 0;
        }
        let n = ((time_ms.saturating_sub(self.start_ms)) as f64 * self.fps as f64 / 1000.).floor()
            as usize;
        if self.looped {
            n % self.frames.len()
        } else {
            n.min(self.frames.len() - 1)
        }
    }
    pub fn frame_at(&self, time_ms: u64) -> Option<&Reference> {
        self.frames.get(self.index_at(time_ms))
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Extras {
    pub characters3d: Vec<crate::character3d::Character>,
    pub show_2d: bool,
    pub canvas_origin: Vec2,
    pub auto_canvas: bool,
    pub production: crate::production::Production,
    pub pins: Vec<Pin>,
    pub ground: Option<f32>,
    pub onion: OnionSettings,
    pub palette: Vec<[u8; 4]>,
    pub references: Option<ReferenceSequence>,
}
impl Default for Extras {
    fn default() -> Self {
        Self {
            characters3d: vec![],
            show_2d: true,
            canvas_origin: Vec2::default(),
            auto_canvas: true,
            production: Default::default(),
            pins: vec![],
            ground: None,
            onion: Default::default(),
            palette: vec![
                [239, 190, 76, 255],
                [119, 225, 187, 255],
                [110, 170, 255, 255],
                [225, 110, 140, 255],
                [180, 130, 220, 255],
                [255, 255, 255, 255],
                [25, 31, 38, 255],
            ],
            references: None,
        }
    }
}
impl Extras {
    pub fn validate(&self, rig: &Rig) -> Result<()> {
        ensure!(
            self.characters3d.len() <= 8,
            "A scene supports at most 8 3D characters"
        );
        let mut ids = HashSet::new();
        for c in &self.characters3d {
            ensure!(
                c.id > 0 && ids.insert(c.id),
                "3D character IDs must be positive and unique"
            );
            c.validate()?;
            ensure!(
                c.clips.keys().all(|i| *i < rig.clips.len()),
                "3D animation references a missing timeline clip"
            );
        }
        self.production.validate(rig)?;
        ensure!(self.pins.len() <= 128, "At most 128 pins are supported");
        let mut ids = HashSet::new();
        for pin in &self.pins {
            let lower = rig
                .bone(pin.lower)
                .ok_or_else(|| anyhow::anyhow!("Pin references a missing bone"))?;
            let upper = lower
                .parent
                .and_then(|id| rig.bone(id))
                .ok_or_else(|| anyhow::anyhow!("Pin needs a two-bone chain"))?;
            ensure!(
                lower.connected && upper.parent.is_some() && ids.insert(pin.lower),
                "Pin must target a unique connected limb end and cannot drive a root"
            );
            ensure!(
                pin.target.is_finite()
                    && pin.target.x.abs() <= 100_000.
                    && pin.target.y.abs() <= 100_000.
                    && pin.bend.is_finite()
                    && pin.bend.abs() <= 1.,
                "Invalid pin target or bend"
            );
        }
        ensure!(
            self.ground
                .is_none_or(|y| y.is_finite() && y.abs() <= 100_000.),
            "Invalid ground height"
        );
        ensure!(
            self.onion.before <= 8
                && self.onion.after <= 8
                && (1..=24).contains(&self.onion.spacing)
                && self.onion.opacity.is_finite()
                && (0. ..=1.).contains(&self.onion.opacity),
            "Invalid onion-skin settings"
        );
        ensure!(self.palette.len() <= 256, "Palette exceeds 256 swatches");
        if let Some(seq) = &self.references {
            ensure!(
                !seq.frames.is_empty()
                    && seq.frames.len() <= 240
                    && seq.fps.is_finite()
                    && (1. / 60. ..=240.).contains(&seq.fps),
                "Invalid reference timing or frame count"
            );
            let mut pixels = 0u64;
            for r in &seq.frames {
                pixels += r.width as u64 * r.height as u64;
                ensure!(
                    r.width > 0
                        && r.height > 0
                        && r.width <= 4096
                        && r.height <= 4096
                        && r.pixels.len() as u64 == r.width as u64 * r.height as u64 * 4
                        && [r.x, r.y, r.scale, r.opacity].iter().all(|v| v.is_finite())
                        && (0.001..=1000.).contains(&r.scale)
                        && (0. ..=1.).contains(&r.opacity),
                    "Invalid reference-sequence frame"
                );
            }
            ensure!(
                pixels <= 16 * 1024 * 1024,
                "Reference sequence exceeds 16 megapixels"
            );
        }
        Ok(())
    }
}
pub fn solve_pins(rig: &Rig, extras: &Extras, pose: &Pose) -> Pose {
    let mut result = pose.clone();
    if extras.pins.iter().all(|p| !p.enabled) || rig.validate().is_err() {
        return result;
    }
    for _ in 0..4 {
        for pin in extras.pins.iter().filter(|p| p.enabled) {
            let Some(lower) = rig.bone(pin.lower) else {
                continue;
            };
            let Some(upper) = lower.parent.and_then(|id| rig.bone(id)) else {
                continue;
            };
            if !lower.connected || upper.parent.is_none() || (upper.locked && lower.locked) {
                continue;
            }
            let world = rig.world(Some(&result));
            let Some(lw) = world.iter().find(|w| w.id == lower.id) else {
                continue;
            };
            let mut target = pin.target;
            if pin.grounded
                && let Some(y) = extras.ground
            {
                let offset = rig
                    .bones
                    .iter()
                    .filter(|b| b.parent == Some(lower.id) && b.connected)
                    .filter_map(|b| {
                        world.iter().find(|w| w.id == b.id).map(|w| {
                            (w.head.y + b.radius_head).max(w.tail.y + b.radius_tail) - lw.tail.y
                        })
                    })
                    .fold(lower.radius_tail, f32::max);
                target.y = y - offset;
            }
            let _ = crate::ik::apply_two_bone_with_bend_validated(
                rig,
                &mut result,
                upper.id,
                lower.id,
                target,
                pin.bend,
            );
        }
    }
    result
}

pub fn apply_foreign_pose(
    target: &mut crate::project::Project,
    source: &crate::project::Project,
    source_frame: f32,
    target_frame: f32,
) -> Result<usize> {
    let pose = source.pose_at(source_frame);
    let mut candidate = target.rig.clone();
    let mut mapped = 0;
    for bone in &source.rig.bones {
        let Some(id) = candidate
            .bones
            .iter()
            .find(|b| b.name == bone.name)
            .map(|b| b.id)
        else {
            continue;
        };
        let p = pose.get(&bone.id).copied().unwrap_or_default();
        for (ch, v) in [
            (crate::animation::Channel::Rotation, p.rotation),
            (crate::animation::Channel::X, p.x),
            (crate::animation::Channel::Y, p.y),
            (crate::animation::Channel::Scale, p.scale),
        ] {
            candidate
                .active_clip_mut()
                .set_key(id, ch, target_frame, v)
                .map_err(anyhow::Error::msg)?;
        }
        mapped += 1;
    }
    ensure!(mapped > 0, "No matching bone names in this pose");
    candidate.validate().map_err(anyhow::Error::msg)?;
    target.rig = candidate;
    Ok(mapped)
}
