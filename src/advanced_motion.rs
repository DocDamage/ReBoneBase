//! Bounded, deterministic animation controllers and transactional clip authoring.
//!
//! Evaluate keyed FK, then `layers`, static pins, and finally
//! `evaluate_constraints`. Springs run before contacts and chains so that an
//! explicit contact remains authoritative. Controllers never translate roots.
use crate::{
    animation::{BonePose, Channel, Clip, Ease, Pose, wrap_degrees},
    model::{Rig, Vec2},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MotionTools {
    pub chains: Vec<Chain>,
    pub contacts: Vec<ContactTrack>,
    pub layers: Vec<Layer>,
    pub springs: Vec<Spring>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chain {
    pub name: String,
    /// Proximal to distal, connected, with the first bone below a root.
    pub bones: Vec<u32>,
    pub target: Vec2,
    pub weight: f32,
    pub enabled: bool,
    pub iterations: u32,
}
impl Default for Chain {
    fn default() -> Self {
        Self {
            name: "IK chain".into(),
            bones: vec![],
            target: Vec2::default(),
            weight: 1.,
            enabled: true,
            iterations: 16,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContactTrack {
    pub lower: u32,
    /// None applies in every clip; Some binds the contact to that clip.
    pub clip: Option<usize>,
    pub keys: Vec<ContactKey>,
    pub enabled: bool,
}
impl Default for ContactTrack {
    fn default() -> Self {
        Self {
            lower: 0,
            clip: None,
            keys: vec![],
            enabled: true,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContactKey {
    pub frame: f32,
    pub target: Vec2,
    pub weight: f32,
    pub bend: f32,
}
impl Default for ContactKey {
    fn default() -> Self {
        Self {
            frame: 1.,
            target: Vec2::default(),
            weight: 1.,
            bend: 1.,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub name: String,
    pub clip: usize,
    pub weight: f32,
    pub additive: bool,
    /// Empty includes all channels. Otherwise includes exactly these owners.
    pub mask: Vec<u32>,
    pub offset: f32,
    pub enabled: bool,
}
impl Default for Layer {
    fn default() -> Self {
        Self {
            name: "Layer".into(),
            clip: 0,
            weight: 1.,
            additive: false,
            mask: vec![],
            offset: 0.,
            enabled: true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spring {
    pub bone: u32,
    pub stiffness: f32,
    pub damping: f32,
    pub gravity: f32,
    pub wind: f32,
    pub enabled: bool,
}
impl Default for Spring {
    fn default() -> Self {
        Self {
            bone: 0,
            stiffness: 30.,
            damping: 8.,
            gravity: 0.,
            wind: 0.,
            enabled: true,
        }
    }
}

fn bounded(v: f32) -> bool {
    v.is_finite() && v.abs() <= 100_000.
}
fn point(v: Vec2) -> bool {
    bounded(v.x) && bounded(v.y)
}
fn weight(v: f32) -> bool {
    v.is_finite() && (0. ..=1.).contains(&v)
}
fn chain_valid(rig: &Rig, chain: &Chain) -> bool {
    let mut seen = HashSet::new();
    (2..=64).contains(&chain.bones.len())
        && (1..=64).contains(&chain.iterations)
        && point(chain.target)
        && weight(chain.weight)
        && chain.bones.iter().enumerate().all(|(i, id)| {
            seen.insert(*id)
                && rig.bone(*id).is_some_and(|b| {
                    b.parent.is_some()
                        && (i == 0 || (b.connected && b.parent == Some(chain.bones[i - 1])))
                })
        })
}
impl MotionTools {
    pub fn validate(&self, rig: &Rig) -> Result<(), String> {
        rig.validate()?;
        if self.chains.len() > 32
            || self.contacts.len() > 128
            || self.layers.len() > 32
            || self.springs.len() > 64
        {
            return Err("Too many motion controllers".into());
        }
        let owners: HashSet<_> = rig
            .bones
            .iter()
            .map(|b| b.id)
            .chain(rig.parts.iter().map(|p| p.id))
            .collect();
        let mut work = 0_u64;
        for c in &self.chains {
            if c.name.len() > 256 || !chain_valid(rig, c) {
                return Err(
                    "IK chain must be a contiguous non-root chain with finite target and weight"
                        .into(),
                );
            }
            work += u64::from(c.iterations) * c.bones.len() as u64;
        }
        let mut contacts = HashSet::new();
        let mut keys = 0;
        for c in &self.contacts {
            let lower = rig.bone(c.lower).ok_or("Contact bone is missing")?;
            let upper = lower
                .parent
                .and_then(|id| rig.bone(id))
                .ok_or("Contact needs a connected two-bone limb")?;
            if !lower.connected
                || upper.parent.is_none()
                || c.clip.is_some_and(|i| i >= rig.clips.len())
                || !contacts.insert((c.lower, c.clip))
                || c.keys.len() > 240
            {
                return Err("Invalid or duplicate contact track".into());
            }
            let length = c.clip.map_or(240., |i| rig.clips[i].length as f32);
            let mut prev = 0.;
            for k in &c.keys {
                if !bounded(k.frame)
                    || k.frame < 1.
                    || k.frame > length
                    || k.frame <= prev
                    || !point(k.target)
                    || !weight(k.weight)
                    || !k.bend.is_finite()
                    || !(-1. ..=1.).contains(&k.bend)
                {
                    return Err("Invalid contact key".into());
                }
                prev = k.frame;
            }
            keys += c.keys.len();
            work += 8;
        }
        if keys > 8192 {
            return Err("Too many contact keys".into());
        }
        for l in &self.layers {
            if l.name.len() > 256
                || l.clip >= rig.clips.len()
                || !weight(l.weight)
                || !bounded(l.offset)
                || l.offset.abs() > 240.
                || l.mask.len() > 2304
                || l.mask.iter().any(|id| !owners.contains(id))
            {
                return Err("Invalid animation layer".into());
            }
        }
        let mut springs = HashSet::new();
        for s in &self.springs {
            if rig.bone(s.bone).is_none_or(|b| b.parent.is_none())
                || !springs.insert(s.bone)
                || !s.stiffness.is_finite()
                || !(0.01..=1000.).contains(&s.stiffness)
                || !s.damping.is_finite()
                || !(0. ..=100.).contains(&s.damping)
                || !bounded(s.gravity)
                || s.gravity.abs() > 1000.
                || !bounded(s.wind)
                || s.wind.abs() > 1000.
            {
                return Err("Invalid spring or spring root bone".into());
            }
        }
        work += self.springs.len() as u64 * 240;
        let keys_in = |clip: &Clip| clip.tracks.iter().map(|t| t.keys.len() as u64).sum::<u64>();
        let replay = if self.springs.iter().any(|s| s.enabled) {
            u64::from(rig.active_clip().length)
        } else {
            1
        };
        let layer_work = self
            .layers
            .iter()
            .filter(|l| l.enabled)
            .map(|l| keys_in(&rig.clips[l.clip]))
            .sum::<u64>();
        if work * rig.bones.len() as u64 + replay * (keys_in(rig.active_clip()) + layer_work)
            > 16_000_000
        {
            return Err("Motion controller work exceeds budget".into());
        }
        Ok(())
    }
    /// Apply a mapping from old clip indices to new indices after deletion/reorder.
    pub fn remap_clips(&mut self, mapping: &[Option<usize>]) {
        self.layers.retain_mut(|l| {
            if let Some(Some(i)) = mapping.get(l.clip) {
                l.clip = *i;
                true
            } else {
                false
            }
        });
        self.contacts.retain_mut(|c| {
            if let Some(old) = c.clip {
                if let Some(Some(i)) = mapping.get(old) {
                    c.clip = Some(*i);
                    true
                } else {
                    false
                }
            } else {
                true
            }
        });
    }
}

fn rotation(pose: &Pose, id: u32) -> f32 {
    pose.get(&id).map_or(0., |p| p.rotation)
}
fn angle_mix(a: f32, b: f32, w: f32) -> f32 {
    a + wrap_degrees(b - a) * w
}
fn mix(a: f32, b: f32, w: f32) -> f32 {
    a + (b - a) * w
}
fn constrained(rig: &Rig, id: u32, mut p: BonePose) -> BonePose {
    p.x = p.x.clamp(-100_000., 100_000.);
    p.y = p.y.clamp(-100_000., 100_000.);
    p.rotation = p.rotation.clamp(-100_000., 100_000.);
    if let Some(b) = rig.bone(id) {
        p.rotation = b.clamp_rotation(p.rotation);
        p.scale = b.clamp_scale(p.scale);
    }
    p.z = p.z.map(|z| z.clamp(-100_000., 100_000.));
    p.visibility = p.visibility.map(|v| v.clamp(0., 1.));
    p
}
/// Layers affect only channels keyed by the source clip, preserving all others.
pub fn layers(rig: &Rig, tools: &MotionTools, frame: f32, base: &Pose) -> Pose {
    let mut pose = base.clone();
    if !frame.is_finite() {
        return pose;
    }
    for layer in tools
        .layers
        .iter()
        .take(32)
        .filter(|l| l.enabled && weight(l.weight) && l.weight > 0. && bounded(l.offset))
    {
        let Some(clip) = rig.clips.get(layer.clip) else {
            continue;
        };
        for t in &clip.tracks {
            if !layer.mask.is_empty() && !layer.mask.contains(&t.bone) {
                continue;
            }
            if t.keys.is_empty() {
                continue;
            }
            let value = clip.value(t, frame + layer.offset);
            let p = pose.entry(t.bone).or_default();
            let w = layer.weight;
            match t.channel {
                Channel::Rotation => {
                    p.rotation = if layer.additive {
                        p.rotation + value * w
                    } else {
                        angle_mix(p.rotation, value, w)
                    }
                }
                Channel::X => {
                    p.x = if layer.additive {
                        p.x + value * w
                    } else {
                        mix(p.x, value, w)
                    }
                }
                Channel::Y => {
                    p.y = if layer.additive {
                        p.y + value * w
                    } else {
                        mix(p.y, value, w)
                    }
                }
                Channel::Scale => {
                    p.scale = if layer.additive {
                        p.scale * mix(1., value, w)
                    } else {
                        mix(p.scale, value, w)
                    }
                }
                Channel::Z => {
                    let neutral = rig
                        .parts
                        .iter()
                        .find(|p| p.id == t.bone)
                        .map_or(0., |p| p.z as f32);
                    let old = p.z.unwrap_or(neutral);
                    p.z = Some(if layer.additive {
                        old + (value - neutral) * w
                    } else {
                        mix(old, value, w)
                    });
                }
                Channel::Visibility => {
                    let old = p.visibility.unwrap_or(1.);
                    p.visibility = Some(if w >= 0.5 { value } else { old });
                }
            }
            *p = constrained(rig, t.bone, *p);
        }
    }
    pose
}

pub fn solve_chain(rig: &Rig, base: &Pose, chain: &Chain) -> Pose {
    if rig.validate().is_err() || !chain_valid(rig, chain) {
        return base.clone();
    }
    solve_chain_validated(rig, base, chain)
}
fn solve_chain_validated(rig: &Rig, base: &Pose, chain: &Chain) -> Pose {
    if !chain.enabled || chain.weight <= 0. {
        return base.clone();
    }
    let mut pose = base.clone();
    let end = *chain.bones.last().unwrap();
    if rig
        .world(Some(base))
        .iter()
        .find(|w| w.id == end)
        .is_some_and(|w| (w.tail - chain.target).length() < 0.001)
    {
        return pose;
    }
    // A tiny deterministic bend escapes the collinear, folded CCD singularity.
    if let Some(id) = chain
        .bones
        .iter()
        .rev()
        .find(|id| rig.bone(**id).is_some_and(|b| !b.locked))
    {
        let b = rig.bone(*id).unwrap();
        pose.entry(*id).or_default().rotation =
            b.clamp_rotation(rotation(base, *id) + b.bend.signum() * 0.1);
    }
    for _ in 0..chain.iterations {
        for id in chain.bones.iter().rev() {
            let b = rig.bone(*id).unwrap();
            if b.locked {
                continue;
            }
            let world = rig.world(Some(&pose));
            let pivot = world.iter().find(|w| w.id == *id).unwrap().head;
            let tip = world.iter().find(|w| w.id == end).unwrap().tail;
            if (tip - chain.target).length() < 0.001 {
                break;
            }
            let a = tip - pivot;
            let c = chain.target - pivot;
            if a.length() < 0.00001 || c.length() < 0.00001 {
                continue;
            }
            let delta = wrap_degrees(c.angle() - a.angle());
            let p = pose.entry(*id).or_default();
            p.rotation = b.clamp_rotation(wrap_degrees(p.rotation + delta));
        }
    }
    for id in &chain.bones {
        let b = rig.bone(*id).unwrap();
        if b.locked {
            continue;
        }
        let solved = rotation(&pose, *id);
        pose.entry(*id).or_default().rotation =
            b.clamp_rotation(angle_mix(rotation(base, *id), solved, chain.weight));
    }
    pose
}
fn contact_at(track: &ContactTrack, frame: f32) -> Option<ContactKey> {
    let first = *track.keys.first()?;
    if frame < first.frame {
        return None;
    }
    for pair in track.keys.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if frame == b.frame {
            return Some(b);
        }
        if frame <= b.frame {
            let u = (frame - a.frame) / (b.frame - a.frame);
            return Some(ContactKey {
                frame,
                target: a.target + (b.target - a.target) * u,
                weight: mix(a.weight, b.weight, u),
                bend: a.bend,
            });
        }
    }
    track.keys.last().copied()
}

/// Stable exact integration of a constant-target damped angular spring.
fn spring_step(x: f32, velocity: f32, target: f32, dt: f32, k: f32, damping: f32) -> (f32, f32) {
    let (y, v, t, k, a) = (
        f64::from(x - target),
        f64::from(velocity),
        f64::from(dt),
        f64::from(k),
        f64::from(damping) * 0.5,
    );
    let discriminant = k - a * a;
    let (next, speed) = if discriminant > 1e-8 {
        let w = discriminant.sqrt();
        let e = (-a * t).exp();
        let (s, c) = (w * t).sin_cos();
        (
            e * (y * c + (v + a * y) * s / w),
            e * (v * c - (a * v + k * y) * s / w),
        )
    } else if discriminant < -1e-8 {
        let d = (-discriminant).sqrt();
        let r1 = -a + d;
        let r2 = -a - d;
        let c1 = (v - r2 * y) / (r1 - r2);
        let c2 = y - c1;
        let e1 = (r1 * t).exp();
        let e2 = (r2 * t).exp();
        (c1 * e1 + c2 * e2, r1 * c1 * e1 + r2 * c2 * e2)
    } else {
        let e = (-a * t).exp();
        let b = v + a * y;
        (e * (y + b * t), e * (v - a * b * t))
    };
    ((f64::from(target) + next) as f32, speed as f32)
}
fn spring_pose(rig: &Rig, tools: &MotionTools, frame: f32, base: &Pose) -> Pose {
    let springs: Vec<_> = tools
        .springs
        .iter()
        .filter(|s| {
            s.enabled
                && rig
                    .bone(s.bone)
                    .is_some_and(|b| b.parent.is_some() && !b.locked)
        })
        .collect();
    if springs.is_empty() {
        return base.clone();
    }
    let clip = rig.active_clip();
    let frame = frame.clamp(1., clip.length as f32);
    let first = layers(rig, tools, 1., &clip.sample(1.));
    let drive = layers(rig, tools, frame, &clip.sample(frame));
    let mut pose = base.clone();
    let mut states: Vec<_> = springs
        .iter()
        .map(|s| (rotation(&first, s.bone), 0.))
        .collect();
    for step in 1..=frame.ceil() as u32 - 1 {
        let next_frame = ((step + 1) as f32).min(frame);
        let dt = clip.duration_ms(step) as f32 * 0.001 * (next_frame - step as f32);
        // Replay layered FK once per frame, shared by every spring.
        let sampled = layers(rig, tools, next_frame, &clip.sample(next_frame));
        let world = rig.world(Some(&sampled));
        for (s, (x, velocity)) in springs.iter().zip(&mut states) {
            let bone = rig.bone(s.bone).unwrap();
            let target = angle_mix(*x, rotation(&sampled, s.bone), 1.);
            let parent_angle = bone
                .parent
                .and_then(|id| world.iter().find(|w| w.id == id))
                .map_or(0., |w| w.angle);
            let radians = (parent_angle + bone.angle + *x).to_radians();
            let force = s.gravity * radians.cos() - s.wind * radians.sin();
            let (next, speed) = spring_step(
                *x,
                *velocity,
                target + force / s.stiffness,
                dt,
                s.stiffness,
                s.damping,
            );
            *x = bone.clamp_rotation(next);
            *velocity = if *x == next { speed } else { 0. };
        }
    }
    for (s, (x, _)) in springs.iter().zip(states) {
        let bone = rig.bone(s.bone).unwrap();
        let p = pose.entry(s.bone).or_default();
        p.rotation = bone.clamp_rotation(p.rotation + wrap_degrees(x - rotation(&drive, s.bone)));
    }
    pose
}
/// Deterministic timeline evaluation. Invalid controller documents preserve FK.
pub fn evaluate_constraints(rig: &Rig, tools: &MotionTools, frame: f32, base: &Pose) -> Pose {
    if !frame.is_finite() || tools.validate(rig).is_err() {
        return base.clone();
    }
    let mut pose = spring_pose(rig, tools, frame, base);
    for c in tools
        .contacts
        .iter()
        .filter(|c| c.enabled && c.clip.is_none_or(|i| i == rig.active_clip))
    {
        let Some(k) = contact_at(c, frame).filter(|k| k.weight > 0.) else {
            continue;
        };
        let upper = rig.bone(c.lower).unwrap().parent.unwrap();
        let before = pose.clone();
        if crate::ik::apply_two_bone_with_bend_validated(
            rig, &mut pose, upper, c.lower, k.target, k.bend,
        )
        .is_ok()
        {
            for id in [upper, c.lower] {
                let b = rig.bone(id).unwrap();
                if b.locked {
                    continue;
                }
                let value = b.clamp_rotation(angle_mix(
                    rotation(&before, id),
                    rotation(&pose, id),
                    k.weight,
                ));
                pose.entry(id).or_default().rotation = value;
            }
        }
    }
    for c in &tools.chains {
        pose = solve_chain_validated(rig, &pose, c);
    }
    pose
}

fn key_pose(clip: &mut Clip, rig: &Rig, pose: &Pose, frame: f32) -> Result<(), String> {
    for (id, p) in pose {
        if rig.bone(*id).is_some() {
            for (channel, value) in [
                (Channel::Rotation, p.rotation),
                (Channel::X, p.x),
                (Channel::Y, p.y),
                (Channel::Scale, p.scale),
            ] {
                clip.set_key(*id, channel, frame, value)?;
            }
        } else if !rig.parts.iter().any(|part| part.id == *id) {
            return Err("Pose owner is missing".into());
        }
        if let Some(z) = p.z {
            clip.set_key(*id, Channel::Z, frame, z)?;
        }
        if let Some(v) = p.visibility {
            clip.set_key(*id, Channel::Visibility, frame, v)?;
        }
    }
    for t in &mut clip.tracks {
        if let Some(key) = t.keys.iter_mut().find(|k| k.frame == frame) {
            key.easing = if matches!(t.channel, Channel::Z | Channel::Visibility) {
                Ease::Step
            } else {
                Ease::Linear
            };
            key.tangent_in = 0.;
            key.tangent_out = 0.;
        }
    }
    Ok(())
}
/// Record a displayed pose without mutating rest geometry or partial failures.
pub fn bake_pose(rig: &mut Rig, pose: &Pose, frame: f32) -> Result<(), String> {
    let mut clip = rig
        .clips
        .get(rig.active_clip)
        .ok_or("Active clip is missing")?
        .clone();
    if !frame.is_finite() || frame < 1. || frame > clip.length as f32 {
        return Err("Bake frame is outside the active clip".into());
    }
    key_pose(&mut clip, rig, pose, frame)?;
    let owners = rig
        .bones
        .iter()
        .map(|b| b.id)
        .chain(rig.parts.iter().map(|p| p.id))
        .collect();
    clip.validate(&owners)?;
    rig.clips[rig.active_clip] = clip;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WalkSettings {
    pub name: String,
    pub frames: u32,
    /// Cycles per second; generated FPS is frames * cadence.
    pub cadence: f32,
    pub stride: f32,
    pub bounce: f32,
    pub arm_swing: f32,
    pub foot_lift: f32,
    pub run: bool,
    pub root_travel: bool,
}
impl Default for WalkSettings {
    fn default() -> Self {
        Self {
            name: "Walk".into(),
            frames: 24,
            cadence: 1.,
            stride: 8.,
            bounce: 1.,
            arm_swing: 25.,
            foot_lift: 3.,
            run: false,
            root_travel: false,
        }
    }
}
fn side_name(name: &str, side: char) -> bool {
    let s = name.to_lowercase();
    if side == 'l' {
        s.ends_with("_l") || s.ends_with(".l") || s.starts_with("left") || s.ends_with("left")
    } else {
        s.ends_with("_r") || s.ends_with(".r") || s.starts_with("right") || s.ends_with("right")
    }
}
fn limb(rig: &Rig, side: char, names: &[&str]) -> Option<u32> {
    rig.bones
        .iter()
        .find(|b| {
            side_name(&b.name, side) && names.iter().any(|n| b.name.to_lowercase().contains(n))
        })
        .map(|b| b.id)
}
fn append_clip(rig: &mut Rig, clip: Clip) -> Result<usize, String> {
    if rig.clips.len() >= 32 {
        return Err("At most 32 clips are supported".into());
    }
    let mut candidate = rig.clone();
    let index = candidate.clips.len();
    candidate.clips.push(clip);
    candidate.active_clip = index;
    candidate.validate()?;
    *rig = candidate;
    Ok(index)
}
fn authoring_budget(rig: &Rig, frames: u32) -> Result<(), String> {
    if rig.clips.len() >= 32 {
        return Err("At most 32 clips are supported".into());
    }
    if (rig.bones.len() as u64 * 4 + rig.parts.len() as u64 * 2) * u64::from(frames) > 100_000 {
        return Err("Baked clip would exceed the 100,000-key authoring budget".into());
    }
    Ok(())
}
pub fn generate_walk(rig: &mut Rig, settings: &WalkSettings) -> Result<usize, String> {
    rig.validate()?;
    let s = settings;
    if !(8..=240).contains(&s.frames)
        || !s.cadence.is_finite()
        || !(0.05..=10.).contains(&s.cadence)
        || s.frames as f32 * s.cadence > 240.
        || s.name.len() > 256
        || !s.stride.is_finite()
        || !(0. ..=1000.).contains(&s.stride)
        || !s.bounce.is_finite()
        || !(0. ..=100.).contains(&s.bounce)
        || !s.foot_lift.is_finite()
        || !(0. ..=100.).contains(&s.foot_lift)
        || !s.arm_swing.is_finite()
        || !(0. ..=180.).contains(&s.arm_swing)
    {
        return Err("Invalid gait settings (8–240 frames, finite positive cadence and bounded distances required)".into());
    }
    authoring_budget(rig, s.frames)?;
    let mut legs = vec![];
    for side in ['l', 'r'] {
        let lower = limb(rig, side, &["shin", "calf", "lowerleg", "lower_leg"])
            .ok_or("Gait needs named left/right shin or calf bones")?;
        let b = rig.bone(lower).unwrap();
        let upper = b
            .parent
            .and_then(|id| rig.bone(id))
            .ok_or("Gait shin needs a thigh parent")?;
        if !b.connected || upper.parent.is_none() || b.locked || upper.locked {
            return Err("Gait needs unlocked connected legs below a root".into());
        }
        legs.push((side, upper.id, lower));
    }
    let rest = rig.world(None);
    // Straight template legs need room to bend during stance. Lower the body
    // enough that both extreme foot positions remain inside the reach circle,
    // including the highest point of the requested bounce.
    let mut crouch = 0_f32;
    for (_, upper, lower) in &legs {
        let hip = rest.iter().find(|w| w.id == *upper).unwrap().head;
        let ankle = rest.iter().find(|w| w.id == *lower).unwrap().tail;
        let reach = rig.bone(*upper).unwrap().length + rig.bone(*lower).unwrap().length - 0.01;
        let horizontal = (ankle.x - hip.x).abs() + s.stride * 0.5;
        if horizontal >= reach
            || ankle.y <= hip.y
            || s.bounce > reach * 0.4
            || s.foot_lift > reach * 0.8
        {
            return Err("Gait stride, bounce or foot lift exceeds the upright legs' reach".into());
        }
        crouch = crouch.max(
            (ankle.y - hip.y - (reach * reach - horizontal * horizontal).sqrt()).max(0.) + s.bounce,
        );
    }
    let duty = if s.run { 0.4 } else { 0.6 };
    let travel = if s.root_travel { s.stride / duty } else { 0. };
    let roots: Vec<_> = rig
        .bones
        .iter()
        .filter(|b| b.parent.is_none())
        .map(|b| b.id)
        .collect();
    let mut clip = Clip {
        name: s.name.clone(),
        fps: s.frames as f32 * s.cadence,
        length: s.frames,
        looped: !s.root_travel,
        ..Default::default()
    };
    for f in 1..=s.frames {
        let phase = (f - 1) as f32 / s.frames as f32;
        let mut pose = Pose::new();
        let bounce = crouch - s.bounce * (phase * std::f32::consts::TAU).sin().powi(2);
        for root in &roots {
            let p = pose.entry(*root).or_default();
            p.x = travel * phase;
            p.y = bounce;
        }
        for (side, upper, lower) in &legs {
            let phase = (phase + if *side == 'l' { 0. } else { 0.5 }).fract();
            let (x, lift) = if phase < duty {
                (s.stride * (0.5 - phase / duty), 0.)
            } else {
                let t = (phase - duty) / (1. - duty);
                (
                    s.stride * (-0.5 + t * t * (3. - 2. * t)),
                    s.foot_lift * (t * std::f32::consts::PI).sin(),
                )
            };
            let ankle = rest.iter().find(|w| w.id == *lower).unwrap().tail;
            let target = ankle + Vec2::new(x + travel * ((f - 1) as f32 / s.frames as f32), -lift);
            crate::ik::apply_two_bone_with_bend_validated(
                rig,
                &mut pose,
                *upper,
                *lower,
                target,
                rig.bone(*lower).unwrap().bend,
            )?;
            // Keep a connected foot's sole direction stable during stance.
            let world = rig.world(Some(&pose));
            for foot in rig
                .bones
                .iter()
                .filter(|b| b.parent == Some(*lower) && b.connected && !b.locked)
            {
                let angle = rest.iter().find(|w| w.id == foot.id).unwrap().angle;
                let parent = world.iter().find(|w| w.id == *lower).unwrap().angle;
                pose.entry(foot.id).or_default().rotation =
                    foot.clamp_rotation(wrap_degrees(angle - parent - foot.angle));
            }
            if let Some(arm) = limb(rig, *side, &["upperarm", "upper_arm", "arm_"]) {
                let b = rig.bone(arm).unwrap();
                if !b.locked {
                    pose.entry(arm).or_default().rotation =
                        b.clamp_rotation(s.arm_swing * (phase * std::f32::consts::TAU).cos());
                }
            }
            if s.run
                && let Some(arm) = limb(rig, *side, &["forearm", "lowerarm"])
            {
                let b = rig.bone(arm).unwrap();
                if !b.locked {
                    pose.entry(arm).or_default().rotation = b.clamp_rotation(-35.);
                }
            }
        }
        key_pose(&mut clip, rig, &pose, f as f32)?;
    }
    append_clip(rig, clip)
}

/// Close the final blend window, optionally matching the seam's cubic velocity.
pub fn smooth_loop(
    rig: &mut Rig,
    index: usize,
    blend_frames: u32,
    match_velocity: bool,
) -> Result<(), String> {
    rig.validate()?;
    let old = rig.clips.get(index).ok_or("Clip is missing")?;
    if old.length < 3 || blend_frames < 2 || blend_frames >= old.length {
        return Err("Loop blend needs 2..length-1 frames".into());
    }
    let mut clip = old.clone();
    for track in &mut clip.tracks {
        if track.keys.is_empty() {
            continue;
        }
        let source = track.clone();
        let first = old.value(&source, 1.);
        let start = old.length - blend_frames;
        let mut values: Vec<_> = (1..=old.length)
            .map(|f| old.value(&source, f as f32))
            .collect();
        let last = *values.last().unwrap();
        let delta = if track.channel == Channel::Rotation {
            wrap_degrees(first - last)
        } else {
            first - last
        };
        for f in start + 1..=old.length {
            let u = (f - start) as f32 / blend_frames as f32;
            values[f as usize - 1] += delta * (u * u * (3. - 2. * u));
        }
        // Store the exact first numeric value, including an unwrapped rotation.
        *values.last_mut().unwrap() = first;
        let seam = if match_velocity {
            (values[1] - values[values.len() - 2]) * 0.5
        } else {
            0.
        };
        track.keys.clear();
        for (i, value) in values.iter().copied().enumerate() {
            let stepped = matches!(track.channel, Channel::Z | Channel::Visibility);
            let tangent = if i == 0 || i + 1 == values.len() {
                seam
            } else {
                (values[i + 1] - values[i - 1]) * 0.5
            };
            track.keys.push(crate::animation::Keyframe {
                frame: (i + 1) as f32,
                value,
                easing: if stepped {
                    Ease::Step
                } else if match_velocity {
                    Ease::Cubic
                } else {
                    Ease::Linear
                },
                tangent_in: tangent,
                tangent_out: tangent,
            });
        }
    }
    clip.looped = true;
    let mut candidate = rig.clone();
    candidate.clips[index] = clip;
    candidate.validate()?;
    *rig = candidate;
    Ok(())
}

/// Blend complete poses, with shortest angular travel and neutral absent channels.
pub fn transition_pose(rig: &Rig, from: &Pose, to: &Pose, weight: f32) -> Pose {
    if !weight.is_finite() {
        return from.clone();
    }
    let w = weight.clamp(0., 1.);
    if w == 0. {
        return from.clone();
    }
    if w == 1. {
        return to.clone();
    }
    let mut pose = Pose::new();
    for id in from.keys().chain(to.keys()) {
        if pose.contains_key(id) {
            continue;
        }
        let a = from.get(id).copied().unwrap_or_default();
        let b = to.get(id).copied().unwrap_or_default();
        let z_neutral = rig
            .parts
            .iter()
            .find(|p| p.id == *id)
            .map_or(0., |p| p.z as f32);
        let p = BonePose {
            rotation: angle_mix(a.rotation, b.rotation, w),
            x: mix(a.x, b.x, w),
            y: mix(a.y, b.y, w),
            scale: mix(a.scale, b.scale, w),
            z: if a.z.is_some() || b.z.is_some() {
                Some(mix(a.z.unwrap_or(z_neutral), b.z.unwrap_or(z_neutral), w))
            } else {
                None
            },
            visibility: if a.visibility.is_some() || b.visibility.is_some() {
                Some(if w < 0.5 {
                    a.visibility.unwrap_or(1.)
                } else {
                    b.visibility.unwrap_or(1.)
                })
            } else {
                None
            },
        };
        pose.insert(*id, constrained(rig, *id, p));
    }
    pose
}
pub fn create_transition(
    rig: &mut Rig,
    from_clip: usize,
    to_clip: usize,
    frames: u32,
) -> Result<usize, String> {
    rig.validate()?;
    if !(2..=240).contains(&frames) {
        return Err("Transition needs 2–240 frames".into());
    }
    authoring_budget(rig, frames)?;
    let a = rig.clips.get(from_clip).ok_or("Source clip is missing")?;
    let b = rig
        .clips
        .get(to_clip)
        .ok_or("Destination clip is missing")?;
    let a_frame = a.frame_sequence().last().copied().unwrap_or(1) as f32;
    let b_frame = b.frame_sequence().first().copied().unwrap_or(1) as f32;
    let from = a.sample(a_frame);
    let to = b.sample(b_frame);
    let mut clip = Clip {
        name: format!("{} → {}", a.name, b.name),
        fps: a.fps,
        length: frames,
        looped: false,
        ..Default::default()
    };
    for frame in 1..=frames {
        let u = (frame - 1) as f32 / (frames - 1) as f32;
        let pose = transition_pose(rig, &from, &to, u * u * (3. - 2. * u));
        key_pose(&mut clip, rig, &pose, frame as f32)?;
    }
    append_clip(rig, clip)
}

/// Reflect a complete baked clip about the rig axis and swap twin channels.
pub fn mirror_clip(rig: &mut Rig, index: usize) -> Result<usize, String> {
    rig.validate()?;
    let source = rig.clips.get(index).ok_or("Clip is missing")?;
    authoring_budget(rig, source.length)?;
    let mut clip = Clip {
        name: format!("{} mirrored", source.name),
        tracks: vec![],
        playback: source.playback,
        ..source.clone()
    };
    let mapping: std::collections::HashMap<_, _> = rig
        .bones
        .iter()
        .map(|b| (b.id, b.twin.unwrap_or(b.id)))
        .collect();
    let mut part_mapping = std::collections::HashMap::new();
    for b in &rig.bones {
        if let Some(twin) = b.twin.and_then(|id| rig.bone(id)) {
            part_mapping.insert(b.part, twin.part);
        }
    }
    for frame in 1..=source.length {
        let sampled = source.sample(frame as f32);
        let world = rig.world(Some(&sampled));
        let mut pose = Pose::new();
        for b in &rig.bones {
            let target_id = mapping[&b.id];
            let target = rig.bone(target_id).unwrap();
            let w = world.iter().find(|w| w.id == b.id).unwrap();
            let p = sampled.get(&b.id).copied().unwrap_or_default();
            let head = Vec2::new(2. * rig.mirror.x - w.head.x, w.head.y);
            let angle = 180. - w.angle;
            let parent_world = target.parent.and_then(|id| {
                mapping
                    .iter()
                    .find(|(_, target)| **target == id)
                    .and_then(|(source, _)| world.iter().find(|w| w.id == *source))
            });
            let parent_angle = parent_world.map_or(0., |w| 180. - w.angle);
            let (x, y) = if let Some(parent) = parent_world {
                if target.connected {
                    (0., 0.)
                } else {
                    let reflected = Vec2::new(2. * rig.mirror.x - parent.head.x, parent.head.y);
                    let local = (head - reflected).rotate(-parent_angle);
                    (local.x - target.x, local.y - target.y)
                }
            } else {
                (head.x - target.x, head.y - target.y)
            };
            pose.insert(
                target_id,
                constrained(
                    rig,
                    target_id,
                    BonePose {
                        rotation: wrap_degrees(angle - parent_angle - target.angle),
                        x,
                        y,
                        scale: p.scale,
                        z: None,
                        visibility: None,
                    },
                ),
            );
        }
        for part in &rig.parts {
            if let Some(p) = sampled.get(&part.id) {
                let target = *part_mapping.get(&part.id).unwrap_or(&part.id);
                let entry = pose.entry(target).or_default();
                entry.z = p.z;
                entry.visibility = p.visibility;
            }
        }
        key_pose(&mut clip, rig, &pose, frame as f32)?;
    }
    // Mirroring reverses keyed order only in geometry, preserving playback/timing.
    clip.playback = source.playback;
    append_clip(rig, clip)
}
