//! Targeted cleanup of imported 3D motion. Every tool writes ordinary keyed
//! joint corrections for the active clip, so its result stays editable in the
//! Timeline and Inspector, is undone as one step, and can be removed by
//! deleting keys. The imported animation, clip timing and every frame outside
//! the chosen range are left as they were.
use super::tracks::{JointKey, euler, sample};
use super::*;
use crate::{animation::Ease, project::Project};

#[derive(Clone, Debug, PartialEq)]
pub enum Cleanup {
    /// Gaussian smoothing of local rotations. `radius` is in frames and
    /// `strength` blends from the original (0) to the smoothed pose (1).
    Smooth {
        joints: Vec<usize>,
        start: u32,
        end: u32,
        radius: u32,
        strength: f32,
    },
    /// Pin `foot` where it is at `start` until `end`, with two-bone IK on its
    /// parent and grandparent. `floor` sets the planted height; `None` keeps
    /// the landing height. Blends ease the correction in and out over the
    /// given number of frames. A target beyond the leg's reach is approached
    /// as closely as the leg allows and shows as `max_error`.
    Plant {
        foot: usize,
        start: u32,
        end: u32,
        blend_in: u32,
        blend_out: u32,
        floor: Option<f32>,
    },
    /// Remove correction keys in the range while every frame stays within
    /// `tolerance` degrees of the current pose.
    Reduce {
        joints: Vec<usize>,
        start: u32,
        end: u32,
        tolerance: f32,
    },
}
/// A measured result. `before`/`after` are in the tool's own unit: mean
/// rotational jitter (degrees per frame²) for smoothing, total ground-plane
/// foot slide (model units) for planting, and key count for reduction.
#[derive(Clone, Debug, PartialEq)]
pub struct CleanupReport {
    pub measure: &'static str,
    pub before: f32,
    pub after: f32,
    /// Correction keys written or remaining.
    pub keys: usize,
    /// Largest rotation change from the requested pose, in degrees.
    pub max_error: f32,
}

/// Rotation angle between two orientations in degrees, safe for equal inputs.
pub fn angle(a: Quat, b: Quat) -> f32 {
    (2. * a.normalize().dot(b.normalize()).abs().min(1.).acos()).to_degrees()
}
/// Per-frame poses of one character in the active clip, frames `1..=length`.
pub struct Frames {
    pub base: Vec<Vec<Transform3d>>,
    pub visible: Vec<Vec<Transform3d>>,
    pub world: Vec<Vec<Mat4>>,
}
impl Frames {
    pub fn new(project: &Project, character: u64) -> Result<Self> {
        let c = find(project, character)?;
        let rig = &project.rig;
        let (mut base, mut visible, mut world) = (vec![], vec![], vec![]);
        for f in 1..=rig.active_clip().length.max(1) {
            let frame = rig.active_clip().evaluation_frame(f as f32);
            let time = crate::production::timeline_ms(rig, f as f32) as f64;
            let (c, time) = c.for_clip(rig.active_clip, frame, time);
            let v = c.local_pose(time, true)?;
            base.push(c.local_pose(time, false)?);
            world.push(c.world_pose(&v)?);
            visible.push(v);
        }
        Ok(Self {
            base,
            visible,
            world,
        })
    }
    fn rotation(&self, frame: u32, joint: usize) -> Quat {
        Quat::from_array(self.visible[frame as usize - 1][joint].rotation).normalize()
    }
    pub fn position(&self, frame: u32, joint: usize) -> Vec3 {
        self.world[frame as usize - 1][joint].w_axis.truncate()
    }
    fn world_rotation(&self, frame: u32, joint: usize) -> Quat {
        let (_, q, _) = self.world[frame as usize - 1][joint].to_scale_rotation_translation();
        q.normalize()
    }
    /// Correction that turns the uncorrected local rotation into `desired`.
    fn correction(&self, frame: u32, joint: usize, desired: Quat) -> Quat {
        let base = Quat::from_array(self.base[frame as usize - 1][joint].rotation).normalize();
        (base.inverse() * desired).normalize()
    }
}
fn find(project: &Project, character: u64) -> Result<&Character> {
    project
        .extras
        .characters3d
        .iter()
        .find(|c| c.id == character)
        .ok_or_else(|| anyhow::anyhow!("Choose an existing 3D character"))
}
fn range(project: &Project, start: u32, end: u32) -> Result<(u32, u32)> {
    let length = project.rig.active_clip().length.max(1);
    ensure!(
        1 <= start && start <= end && end <= length,
        "Choose frames within 1–{length}"
    );
    Ok((start, end))
}
/// Mean angle of the second difference of local rotations, in degrees.
pub fn jitter(frames: &Frames, joints: &[usize], start: u32, end: u32) -> f32 {
    let mut total = 0.;
    let mut n = 0;
    for &j in joints {
        for f in start.max(2)..end.min(frames.visible.len() as u32) {
            let (a, b, c) = (
                frames.rotation(f - 1, j),
                frames.rotation(f, j),
                frames.rotation(f + 1, j),
            );
            let b = if a.dot(b) < 0. { -b } else { b };
            let c = if b.dot(c) < 0. { -c } else { c };
            total += angle(b * a.inverse(), c * b.inverse());
            n += 1;
        }
    }
    if n == 0 { 0. } else { total / n as f32 }
}
/// Total ground-plane travel of `joint` between consecutive frames of a range.
pub fn slide(frames: &Frames, joint: usize, start: u32, end: u32) -> f32 {
    (start + 1..=end)
        .map(|f| {
            let d = frames.position(f, joint) - frames.position(f - 1, joint);
            Vec2::new(d.x, d.z).length()
        })
        .sum()
}
/// Lowest height the joint reaches in the clip.
pub fn floor_height(project: &Project, character: u64, joint: usize) -> Result<f32> {
    let frames = Frames::new(project, character)?;
    ensure!(
        joint < find(project, character)?.model.nodes.len(),
        "Unknown 3D joint"
    );
    Ok((1..=frames.world.len() as u32)
        .map(|f| frames.position(f, joint).y)
        .fold(f32::INFINITY, f32::min))
}
/// Frame ranges where `foot` stays near its lowest height and nearly still.
/// Tolerances are fractions of the leg length (foot to grandparent).
pub fn detect_contacts(
    project: &Project,
    character: u64,
    foot: usize,
    height: f32,
    speed: f32,
) -> Result<Vec<(u32, u32)>> {
    let c = find(project, character)?;
    let (knee, hip) = leg(c, foot)?;
    let frames = Frames::new(project, character)?;
    let length = (frames.position(1, hip) - frames.position(1, knee)).length()
        + (frames.position(1, knee) - frames.position(1, foot)).length();
    let floor = floor_height(project, character, foot)?;
    let count = frames.world.len() as u32;
    let planted = |f: u32| {
        let p = frames.position(f, foot);
        let neighbour = if f > 1 { f - 1 } else { (f + 1).min(count) };
        let d = p - frames.position(neighbour, foot);
        p.y <= floor + height * length && Vec2::new(d.x, d.z).length() <= speed * length
    };
    let mut out: Vec<(u32, u32)> = vec![];
    for f in 1..=count {
        if planted(f) {
            match out.last_mut() {
                Some(r) if r.1 + 1 == f => r.1 = f,
                _ => out.push((f, f)),
            }
        }
    }
    Ok(out)
}
fn leg(c: &Character, foot: usize) -> Result<(usize, usize)> {
    let node = |i: usize| c.model.nodes.get(i);
    let knee = node(foot)
        .ok_or_else(|| anyhow::anyhow!("Unknown 3D joint"))?
        .parent
        .ok_or_else(|| {
            anyhow::anyhow!("Foot planting needs a joint with a parent and grandparent")
        })?;
    let hip = node(knee).and_then(|n| n.parent).ok_or_else(|| {
        anyhow::anyhow!("Foot planting needs a joint with a parent and grandparent")
    })?;
    Ok((knee, hip))
}

/// Run a cleanup on the active clip and return the changed project. The input
/// project is never modified.
pub fn apply(project: &Project, character: u64, op: &Cleanup) -> Result<(Project, CleanupReport)> {
    project.validate()?;
    let clip = project.rig.active_clip;
    let length = project.rig.active_clip().length.max(1);
    let frames = Frames::new(project, character)?;
    let nodes = find(project, character)?.model.nodes.len();
    let mut next = project.clone();
    let index = next
        .extras
        .characters3d
        .iter()
        .position(|c| c.id == character)
        .expect("character exists");
    let report = match op {
        Cleanup::Smooth {
            joints,
            start,
            end,
            radius,
            strength,
        } => {
            let (start, end) = range(project, *start, *end)?;
            ensure!(
                !joints.is_empty() && joints.iter().all(|j| *j < nodes),
                "Choose joints to smooth"
            );
            ensure!(
                (1..=12).contains(radius),
                "Smoothing radius must be 1–12 frames"
            );
            ensure!((0. ..=1.).contains(strength), "Strength must be 0–1");
            let sigma = *radius as f32 * 0.5 + 0.5;
            let c = &mut next.extras.characters3d[index];
            let mut keys = 0;
            for &j in joints {
                let mut values = vec![];
                for f in start..=end {
                    let centre = frames.rotation(f, j);
                    let mut sum = Vec4::ZERO;
                    for k in f.saturating_sub(*radius).max(1)..=(f + radius).min(length) {
                        let q = frames.rotation(k, j);
                        let q = if q.dot(centre) < 0. { -q } else { q };
                        let d = k as f32 - f as f32;
                        sum += Vec4::from(q) * (-d * d / (2. * sigma * sigma)).exp();
                    }
                    let smooth = Quat::from_vec4(sum).normalize();
                    let target = centre.slerp(smooth, *strength).normalize();
                    values.push((f, frames.correction(f, j, target)));
                }
                keys += values.len();
                write_range(c, clip, j, start, end, &values, length);
            }
            next.validate()?;
            let after = Frames::new(&next, character)?;
            CleanupReport {
                measure: "Mean jitter (°/frame²)",
                before: jitter(&frames, joints, start, end),
                after: jitter(&after, joints, start, end),
                keys,
                max_error: 0.,
            }
        }
        Cleanup::Plant {
            foot,
            start,
            end,
            blend_in,
            blend_out,
            floor,
        } => {
            let (start, end) = range(project, *start, *end)?;
            ensure!(
                floor.is_none_or(|v| v.is_finite() && v.abs() <= 100_000.),
                "Invalid floor height"
            );
            let (knee, hip) = leg(&next.extras.characters3d[index], *foot)?;
            let parent = next.extras.characters3d[index].model.nodes[hip].parent;
            // The foot stays where it lands.
            let mut plant = frames.position(start, *foot);
            if let Some(y) = floor {
                plant.y = *y;
            }
            let first = start.saturating_sub(*blend_in).max(1);
            let last = (end + blend_out).min(length);
            let weight = |f: u32| -> f32 {
                let u = if f < start {
                    (f + 1 - first) as f32 / (start + 1 - first) as f32
                } else if f > end {
                    (last + 1 - f) as f32 / (last + 1 - end) as f32
                } else {
                    1.
                };
                u * u * (3. - 2. * u)
            };
            let mut values = [vec![], vec![], vec![]];
            for f in first..=last {
                let (a, b, c) = (
                    frames.position(f, hip),
                    frames.position(f, knee),
                    frames.position(f, *foot),
                );
                let target = c.lerp(plant, weight(f));
                let (l1, l2) = ((b - a).length(), (c - b).length());
                ensure!(l1 > 1e-6 && l2 > 1e-6, "The leg has a zero-length bone");
                let reach = (target - a)
                    .length()
                    .clamp((l1 - l2).abs() + 1e-4, l1 + l2 - 1e-4);
                let dir = (target - a).normalize_or(Vec3::NEG_Y);
                let along = (l1 * l1 + reach * reach - l2 * l2) / (2. * l1 * reach);
                // Keep the knee bending toward the side it already bends to.
                let side = (b - a) - dir * (b - a).dot(dir);
                let side = side
                    .try_normalize()
                    .unwrap_or_else(|| dir.any_orthonormal_vector());
                let new_b =
                    a + dir * (l1 * along) + side * (l1 * (1. - along * along).max(0.).sqrt());
                let new_c = a + dir * reach;
                let hip_turn =
                    Quat::from_rotation_arc((b - a).normalize(), (new_b - a).normalize());
                let shin = (hip_turn * (c - b)).normalize();
                let knee_turn = Quat::from_rotation_arc(shin, (new_c - new_b).normalize());
                let hip_world = (hip_turn * frames.world_rotation(f, hip)).normalize();
                let knee_world =
                    (knee_turn * hip_turn * frames.world_rotation(f, knee)).normalize();
                let foot_world = frames.world_rotation(f, *foot);
                let parent_world = parent.map_or(Quat::IDENTITY, |p| frames.world_rotation(f, p));
                values[0].push((
                    f,
                    frames.correction(f, hip, parent_world.inverse() * hip_world),
                ));
                values[1].push((
                    f,
                    frames.correction(f, knee, hip_world.inverse() * knee_world),
                ));
                values[2].push((
                    f,
                    frames.correction(f, *foot, knee_world.inverse() * foot_world),
                ));
            }
            let c = &mut next.extras.characters3d[index];
            for (joint, values) in [hip, knee, *foot].into_iter().zip(&values) {
                write_range(c, clip, joint, first, last, values, length);
            }
            next.validate()?;
            let after = Frames::new(&next, character)?;
            let max_error = (start..=end)
                .map(|f| (after.position(f, *foot) - plant).length())
                .fold(0., f32::max);
            CleanupReport {
                measure: "Foot slide during contact",
                before: slide(&frames, *foot, start, end),
                after: slide(&after, *foot, start, end),
                keys: values.iter().map(Vec::len).sum(),
                max_error,
            }
        }
        Cleanup::Reduce {
            joints,
            start,
            end,
            tolerance,
        } => {
            let (start, end) = range(project, *start, *end)?;
            ensure!(
                tolerance.is_finite() && (0. ..=45.).contains(tolerance),
                "Tolerance must be 0–45°"
            );
            ensure!(joints.iter().all(|j| *j < nodes), "Unknown 3D joint");
            let c = &mut next.extras.characters3d[index];
            let (mut before, mut after, mut worst) = (0, 0, 0f32);
            for &j in joints {
                let Some(keys) = c.tracks.get_mut(&clip).and_then(|t| t.get_mut(&j)) else {
                    continue;
                };
                let reference: Vec<_> = (1..=length).map(|f| sample(keys, f as f32)).collect();
                before += keys.len();
                let mut i = 0;
                while i < keys.len() {
                    if !(start as f32..=end as f32).contains(&keys[i].frame) || keys.len() == 1 {
                        i += 1;
                        continue;
                    }
                    let removed = keys.remove(i);
                    let error = |f: u32| angle(sample(keys, f as f32), reference[f as usize - 1]);
                    let inside = (start..=end).map(error).fold(0., f32::max);
                    // Frames outside the range must not move at all.
                    let outside = (1..=length)
                        .filter(|f| *f < start || *f > end)
                        .map(error)
                        .fold(0., f32::max);
                    if inside <= *tolerance && outside <= 1e-3 {
                        let error = inside;
                        worst = worst.max(error);
                    } else {
                        keys.insert(i, removed);
                        i += 1;
                    }
                }
                after += keys.len();
            }
            next.validate()?;
            CleanupReport {
                measure: "Correction keys",
                before: before as f32,
                after: after as f32,
                keys: after,
                max_error: worst,
            }
        }
    };
    Ok((next, report))
}
/// Replace a joint's keys in `start..=end` with one key per frame, keeping the
/// correction at every whole frame outside the range exactly as it was.
fn write_range(
    c: &mut Character,
    clip: usize,
    joint: usize,
    start: u32,
    end: u32,
    values: &[(u32, Quat)],
    length: u32,
) {
    let old = c.joint_keys(clip, joint).to_vec();
    let keys = c.tracks.entry(clip).or_default().entry(joint).or_default();
    keys.retain(|k| !(start as f32 - 0.5..=end as f32 + 0.5).contains(&k.frame));
    for (f, q) in values {
        keys.push(JointKey {
            frame: *f as f32,
            rotation: euler(*q),
            easing: Ease::Linear,
        });
    }
    keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    // Pin each outside frame whose value moved, in order, until none does.
    for f in (1..=length).filter(|f| *f < start || *f > end) {
        let want = sample(&old, f as f32);
        if angle(sample(keys, f as f32), want) > 1e-3 {
            let at = keys.partition_point(|k| k.frame < f as f32);
            keys.insert(
                at,
                JointKey {
                    frame: f as f32,
                    rotation: euler(want),
                    easing: Ease::Linear,
                },
            );
        }
    }
    c.tracks.retain(|_, t| {
        t.retain(|_, k| !k.is_empty());
        !t.is_empty()
    });
}
