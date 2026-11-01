use super::calibration::{Calibration3d, ReferencePose, RootScale, RootTravel};
use super::*;
use crate::character3d::{Animation3d, Model3d};
fn rotation(m: Mat4) -> Result<Quat> {
    let (_, q, _) = m.to_scale_rotation_translation();
    ensure!(
        q.is_finite() && q.length_squared().is_finite() && q.length_squared() > 1e-12,
        "Motion has a singular or invalid skeletal rotation; export a nonzero-scale skeleton"
    );
    Ok(q.normalize())
}
impl Motion {
    pub(super) fn validate_samples(&self) -> Result<()> {
        ensure!(
            !self.nodes.is_empty()
                && self.nodes.len() <= 512
                && !self.frames.is_empty()
                && self.frames.len() <= 240
                && self.frames.len() * self.nodes.len() <= 262144
                && self.rest.len() == self.nodes.len()
                && self.fps.is_finite()
                && self.fps > 0.,
            "Invalid sampled motion limits"
        );
        ensure!(
            self.rest.iter().all(|m| m.is_finite())
                && self
                    .frames
                    .iter()
                    .all(|f| f.len() == self.nodes.len() && f.iter().all(|m| m.is_finite())),
            "Non-finite or incomplete motion transforms"
        );
        Ok(())
    }
    pub fn mapping_3d(&self, target: &Model3d) -> Vec<Option<usize>> {
        target
            .nodes
            .iter()
            .map(|n| {
                let key = canonical(&n.name);
                self.nodes.iter().position(|s| canonical(&s.name) == key)
            })
            .collect()
    }
    /// Rest-relative world rotations, converted to target-local FK. Source joint
    /// translations never replace target limbs. Optional root displacement is
    /// scaled by the mapped skeleton's rest span and applied only to roots.
    pub fn retarget_3d(
        &self,
        target: &Model3d,
        mapping: &[Option<usize>],
        root_motion: bool,
    ) -> Result<Animation3d> {
        let calibration = Calibration3d {
            root_travel: if root_motion {
                RootTravel::Full
            } else {
                RootTravel::Off
            },
            ..Default::default()
        };
        self.retarget_3d_calibrated(target, mapping, &calibration)
    }
    /// Retarget with a calibration's reference pose, source space, per-joint
    /// offsets and root policies. `mapping` is the calibration resolved
    /// against this source.
    pub fn retarget_3d_calibrated(
        &self,
        target: &Model3d,
        mapping: &[Option<usize>],
        calibration: &Calibration3d,
    ) -> Result<Animation3d> {
        calibration.validate()?;
        self.validate_samples()?;
        target.validate()?;
        ensure!(
            mapping.len() == target.nodes.len()
                && mapping.iter().flatten().all(|i| *i < self.nodes.len()),
            "Invalid3Dsource joint mapping"
        );
        ensure!(
            mapping.iter().any(Option::is_some),
            "No matching3Djoints; map target joints to the source skeleton before importing"
        );
        ensure!(
            target.nodes.len() * self.frames.len() <= 262144,
            "Retargeted animation exceeds262144transforms"
        );
        let nodes = target
            .nodes
            .iter()
            .map(|n| Node {
                name: n.name.clone(),
                parent: n.parent,
                children: vec![],
            })
            .collect::<Vec<_>>();
        let rest = world_matrices(
            &nodes,
            &target
                .nodes
                .iter()
                .map(|n| {
                    Mat4::from_scale_rotation_translation(
                        Vec3::from_array(n.transform.scale),
                        Quat::from_array(n.transform.rotation).normalize(),
                        Vec3::from_array(n.transform.translation),
                    )
                })
                .collect::<Vec<_>>(),
        )?;
        let target_rot = rest
            .iter()
            .map(|m| rotation(*m))
            .collect::<Result<Vec<_>>>()?;
        let reference = match calibration.reference {
            ReferencePose::Rest => &self.rest,
            ReferencePose::Frame(f) => &self.frames[f.min(self.frames.len() - 1)],
        };
        // Source rotations are conjugated into target space, so a source delta
        // about its up axis becomes a delta about the target's up axis.
        let space = calibration.space();
        let source_rot = reference
            .iter()
            .map(|m| rotation(*m))
            .collect::<Result<Vec<_>>>()?;
        let mut source_min = Vec3::splat(f32::INFINITY);
        let mut source_max = Vec3::splat(f32::NEG_INFINITY);
        let mut target_min = source_min;
        let mut target_max = source_max;
        for (i, s) in mapping.iter().enumerate() {
            if let Some(s) = s {
                let a = reference[*s].w_axis.truncate();
                let b = rest[i].w_axis.truncate();
                source_min = source_min.min(a);
                source_max = source_max.max(a);
                target_min = target_min.min(b);
                target_max = target_max.max(b);
            }
        }
        let scale = match calibration.root_scale {
            RootScale::Fixed(scale) => scale,
            RootScale::Auto if (source_max - source_min).length() > 1e-6 => {
                ((target_max - target_min).length() / (source_max - source_min).length())
                    .clamp(0.001, 1000.)
            }
            RootScale::Auto => 1.,
        };
        let offsets: Vec<_> = target
            .nodes
            .iter()
            .map(|n| calibration.offset(&n.name))
            .collect();
        let mut root_source = BTreeMap::new();
        for (i, s) in mapping.iter().enumerate() {
            if let Some(s) = s {
                let mut r = i;
                while let Some(p) = nodes[r].parent {
                    r = p;
                }
                root_source.entry(r).or_insert(*s);
            }
        }
        let mut frames = Vec::with_capacity(self.frames.len());
        for source in &self.frames {
            let mut frame = target
                .nodes
                .iter()
                .map(|n| n.transform.clone())
                .collect::<Vec<_>>();
            let mut world = vec![None; nodes.len()];
            for _ in 0..nodes.len() {
                for (i, n) in nodes.iter().enumerate() {
                    if world[i].is_some() || n.parent.is_some_and(|p| world[p].is_none()) {
                        continue;
                    }
                    let parent = n.parent.and_then(|p| world[p]).unwrap_or(Quat::IDENTITY);
                    let local = if let Some(s) = mapping[i] {
                        let delta = rotation(source[s])? * source_rot[s].inverse();
                        let desired = space * delta * space.inverse() * target_rot[i] * offsets[i];
                        (parent.inverse() * desired).normalize()
                    } else {
                        (Quat::from_array(frame[i].rotation).normalize() * offsets[i]).normalize()
                    };
                    frame[i].rotation = local.to_array();
                    world[i] = Some((parent * local).normalize());
                    if calibration.root_travel != RootTravel::Off
                        && n.parent.is_none()
                        && let Some(s) = root_source.get(&i)
                    {
                        let delta = calibration.travel(
                            space
                                * (source[*s].w_axis.truncate()
                                    - self.frames[0][*s].w_axis.truncate())
                                * scale,
                        );
                        frame[i].translation =
                            (Vec3::from_array(frame[i].translation) + delta).to_array();
                    }
                }
                if world.iter().all(Option::is_some) {
                    break;
                }
            }
            ensure!(
                frame.iter().all(|t| t
                    .translation
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 100000.)),
                "Retargeted root travel exceeds supported bounds"
            );
            frames.push(frame);
        }
        let duration = self.duration();
        let times = (0..frames.len()).map(|i| i as f32 / self.fps).collect();
        let animation = Animation3d {
            name: self.name.clone(),
            duration,
            times,
            frames,
        };
        let mut checked = target.clone();
        ensure!(
            checked.animations.len() < 32,
            "Target model already has32animations"
        );
        checked.animations.push(animation.clone());
        checked.validate()?;
        Ok(animation)
    }
}
