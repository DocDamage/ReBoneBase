//! Reusable 3D retarget calibration. A calibration names its joints rather
//! than indexing them, so one saved calibration resolves against every clip
//! from the same source skeleton even when a file orders its joints
//! differently. Calibration changes only rotations and root travel: target limb
//! lengths and attachment offsets always come from the target model.
use super::*;
use crate::character3d::Model3d;

/// The source pose that corresponds to the target model's rest pose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum ReferencePose {
    /// The source skeleton's bind/rest pose.
    #[default]
    Rest,
    /// A sampled frame (0-based) of each imported clip, such as a leading
    /// T-pose. Clips shorter than this use their last frame.
    Frame(usize),
}
/// Which source axis points up. Targets are Y-up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpAxis {
    #[default]
    Y,
    Z,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootTravel {
    /// The roots stay where the target model puts them.
    #[default]
    Off,
    /// Ground-plane travel only (X/Z); vertical bob is dropped.
    Ground,
    /// Vertical motion only (Y), for in-place cycles that keep their bounce.
    Vertical,
    Full,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum RootScale {
    /// Ratio of the mapped target and source skeleton spans.
    #[default]
    Auto,
    /// Target units per source unit.
    Fixed(f32),
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calibration3d {
    pub name: String,
    /// Target joint name → source joint name.
    pub mapping: BTreeMap<String, String>,
    pub reference: ReferencePose,
    pub up: UpAxis,
    /// Source facing correction in degrees about the up axis.
    pub facing: f32,
    /// Target joint name → extra XYZ degrees in the joint's own frame.
    pub offsets: BTreeMap<String, [f32; 3]>,
    pub root_travel: RootTravel,
    pub root_scale: RootScale,
}
/// How a calibration matched one source skeleton.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CalibrationReport {
    pub mapped: usize,
    pub total: usize,
    /// Target joints the calibration does not map.
    pub unmapped: Vec<String>,
    /// Mapped source joints absent from this source.
    pub missing: Vec<(String, String)>,
    /// Joint names that occur more than once and so cannot be resolved.
    pub ambiguous: Vec<String>,
    /// Saved target names absent from the target model.
    pub stale: Vec<String>,
}
impl CalibrationReport {
    pub fn lines(&self) -> Vec<String> {
        let list = |v: &[String]| v.join(", ");
        let mut out = vec![format!(
            "{} / {} target joints mapped",
            self.mapped, self.total
        )];
        if !self.missing.is_empty() {
            out.push(format!(
                "Missing in this source: {}",
                self.missing
                    .iter()
                    .map(|(t, s)| format!("{t} ← {s}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !self.ambiguous.is_empty() {
            out.push(format!(
                "Ambiguous (duplicate names, left unmapped): {}",
                list(&self.ambiguous)
            ));
        }
        if !self.stale.is_empty() {
            out.push(format!("Not in the target model: {}", list(&self.stale)));
        }
        if !self.unmapped.is_empty() {
            out.push(format!("Unmapped target joints: {}", list(&self.unmapped)));
        }
        out
    }
}
fn finite_degrees(v: f32) -> bool {
    v.is_finite() && v.abs() <= 100_000.
}
impl Calibration3d {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.name.len() <= 1024
                && self.mapping.len() <= 2048
                && self.offsets.len() <= 2048
                && finite_degrees(self.facing)
                && self.offsets.iter().all(|(t, r)| !t.is_empty()
                    && t.len() <= 1024
                    && r.iter().copied().all(finite_degrees))
                && self.mapping.iter().all(|(t, s)| {
                    !t.is_empty() && !s.is_empty() && t.len() <= 1024 && s.len() <= 1024
                })
                && match self.reference {
                    ReferencePose::Frame(f) => f < 100_000,
                    ReferencePose::Rest => true,
                }
                && match self.root_scale {
                    RootScale::Fixed(s) => s.is_finite() && (0.001..=1000.).contains(&s),
                    RootScale::Auto => true,
                },
            "Invalid 3D retarget calibration"
        );
        Ok(())
    }
    /// A fresh calibration from automatic name matching.
    pub fn automatic(target: &Model3d, motion: &Motion) -> Self {
        let indices = motion.mapping_3d(target);
        Self::from_indices(target, motion, &indices)
    }
    /// Store an index mapping by name. Entries whose target or source name is
    /// duplicated are dropped, since a name cannot identify them.
    pub fn from_indices(target: &Model3d, motion: &Motion, mapping: &[Option<usize>]) -> Self {
        let unique_target =
            |name: &str| target.nodes.iter().filter(|n| n.name == name).count() == 1;
        let unique_source =
            |name: &str| motion.nodes.iter().filter(|n| n.name == name).count() == 1;
        let mapping = target
            .nodes
            .iter()
            .zip(mapping)
            .filter_map(|(t, s)| {
                let s = &motion.nodes.get((*s)?)?.name;
                (unique_target(&t.name) && unique_source(s)).then(|| (t.name.clone(), s.clone()))
            })
            .collect();
        Self {
            mapping,
            ..Default::default()
        }
    }
    /// Replace one target joint's source; `None` unmaps it.
    pub fn set_source(&mut self, target: &str, source: Option<&str>) {
        match source {
            Some(s) => {
                self.mapping.insert(target.into(), s.into());
            }
            None => {
                self.mapping.remove(target);
            }
        }
    }
    /// Resolve names to this source's joint indices, reporting every joint
    /// that could not be resolved.
    pub fn resolve(
        &self,
        target: &Model3d,
        motion: &Motion,
    ) -> (Vec<Option<usize>>, CalibrationReport) {
        let mut report = CalibrationReport {
            total: target.nodes.len(),
            ..Default::default()
        };
        let mapping: Vec<_> = target
            .nodes
            .iter()
            .map(|t| {
                let Some(source) = self.mapping.get(&t.name) else {
                    report.unmapped.push(t.name.clone());
                    return None;
                };
                if target.nodes.iter().filter(|n| n.name == t.name).count() > 1 {
                    if !report.ambiguous.contains(&t.name) {
                        report.ambiguous.push(t.name.clone());
                    }
                    return None;
                }
                let found: Vec<_> = motion
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.name == *source)
                    .map(|(i, _)| i)
                    .collect();
                match found[..] {
                    [i] => Some(i),
                    [] => {
                        report.missing.push((t.name.clone(), source.clone()));
                        None
                    }
                    _ => {
                        if !report.ambiguous.contains(source) {
                            report.ambiguous.push(source.clone());
                        }
                        None
                    }
                }
            })
            .collect();
        report.mapped = mapping.iter().flatten().count();
        report.stale = self
            .mapping
            .keys()
            .chain(self.offsets.keys())
            .filter(|n| !target.nodes.iter().any(|t| t.name == **n))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        (mapping, report)
    }
    /// Source space → target space: up axis, then facing about the target up.
    pub(super) fn space(&self) -> Quat {
        let up = match self.up {
            UpAxis::Y => Quat::IDENTITY,
            UpAxis::Z => Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        };
        Quat::from_rotation_y(self.facing.to_radians()) * up
    }
    pub(super) fn offset(&self, target: &str) -> Quat {
        self.offsets.get(target).map_or(Quat::IDENTITY, |e| {
            Quat::from_euler(
                glam::EulerRot::XYZ,
                e[0].to_radians(),
                e[1].to_radians(),
                e[2].to_radians(),
            )
        })
    }
    pub(super) fn travel(&self, delta: Vec3) -> Vec3 {
        match self.root_travel {
            RootTravel::Off => Vec3::ZERO,
            RootTravel::Ground => Vec3::new(delta.x, 0., delta.z),
            RootTravel::Vertical => Vec3::new(0., delta.y, 0.),
            RootTravel::Full => delta,
        }
    }
}
