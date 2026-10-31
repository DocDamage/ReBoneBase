//! Analytic planar two-bone inverse kinematics, angles in screen-space degrees.
use crate::model::Vec2;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IkSolution {
    pub joint: Vec2,
    pub end: Vec2,
    pub angle1: f32,
    pub angle2: f32,
}
/// Keys an IK pose into a temporary pose map, respecting locks and joint limits.
/// Roots are never driven through a limb chain; unreachable constraints retain
/// the nearest analytic solution after clamping rather than changing bone lengths.
pub fn apply_two_bone(
    rig: &crate::model::Rig,
    pose: &mut crate::animation::Pose,
    upper: u32,
    lower: u32,
    target: Vec2,
) -> Result<(), String> {
    let bend = rig.bone(lower).map_or(1., |b| b.bend);
    apply_two_bone_with_bend(rig, pose, upper, lower, target, bend)
}
pub fn apply_two_bone_with_bend(
    rig: &crate::model::Rig,
    pose: &mut crate::animation::Pose,
    upper: u32,
    lower: u32,
    target: Vec2,
    bend: f32,
) -> Result<(), String> {
    rig.validate()?;
    apply_two_bone_with_bend_validated(rig, pose, upper, lower, target, bend)
}
pub(crate) fn apply_two_bone_with_bend_validated(
    rig: &crate::model::Rig,
    pose: &mut crate::animation::Pose,
    upper: u32,
    lower: u32,
    target: Vec2,
    bend: f32,
) -> Result<(), String> {
    if !target.is_finite() || !bend.is_finite() {
        return Err("IK target must be finite".into());
    }
    let a = rig.bone(upper).ok_or("Upper bone is missing")?;
    let b = rig.bone(lower).ok_or("Lower bone is missing")?;
    if a.parent.is_none() || b.parent != Some(upper) || !b.connected {
        return Err("IK needs a connected limb below the rig root".into());
    }
    if a.locked && b.locked {
        return Err("IK chain is locked".into());
    }
    let world = rig.world(Some(pose));
    let aw = world.iter().find(|w| w.id == upper).unwrap();
    let bw = world.iter().find(|w| w.id == lower).unwrap();
    let parent_angle = a
        .parent
        .and_then(|id| world.iter().find(|w| w.id == id))
        .map_or(0., |w| w.angle);
    let solution = solve_two_bone(
        aw.head,
        target,
        (aw.tail - aw.head).length(),
        (bw.tail - bw.head).length(),
        bend,
    );
    let mut candidate = pose.clone();
    if !a.locked {
        let angle = if b.locked {
            let local = Vec2::new((aw.tail - aw.head).length(), 0.)
                + Vec2::new((bw.tail - bw.head).length(), 0.).rotate(bw.angle - aw.angle);
            if (target - aw.head).length() < 0.00001 {
                aw.angle
            } else {
                (target - aw.head).angle() - local.angle()
            }
        } else {
            solution.angle1
        };
        candidate.entry(upper).or_default().rotation = a.clamp_rotation(
            crate::animation::wrap_degrees(angle - parent_angle - a.angle),
        );
    }
    if !b.locked {
        let relative = if a.locked {
            if (target - aw.tail).length() < 0.00001 {
                bw.angle - aw.angle
            } else {
                (target - aw.tail).angle() - aw.angle
            }
        } else {
            solution.angle2
        };
        candidate.entry(lower).or_default().rotation =
            b.clamp_rotation(crate::animation::wrap_degrees(relative - b.angle));
    }
    *pose = candidate;
    Ok(())
}
pub fn solve_two_bone(
    root: Vec2,
    target: Vec2,
    length1: f32,
    length2: f32,
    bend: f32,
) -> IkSolution {
    let root = if root.is_finite() {
        root
    } else {
        Vec2::default()
    };
    let target = if target.is_finite() { target } else { root };
    let a = if length1.is_finite() {
        length1.abs().clamp(0.0001, 16384.)
    } else {
        1.
    };
    let b = if length2.is_finite() {
        length2.abs().clamp(0.0001, 16384.)
    } else {
        1.
    };
    // Source Lua uses doubles and stays 1e-4 inside singular fully folded/extended poses.
    let (a, b) = (f64::from(a), f64::from(b));
    let dx = f64::from(target.x) - f64::from(root.x);
    let dy = f64::from(target.y) - f64::from(root.y);
    let epsilon = 0.0001_f64.min(a.min(b) * 0.5);
    let distance = dx.hypot(dy).clamp((a - b).abs() + epsilon, a + b - epsilon);
    let direction = if dx != 0. || dy != 0. {
        dy.atan2(dx)
    } else {
        0.
    };
    let side = if bend < 0. { -1. } else { 1. };
    let shoulder = ((a * a + distance * distance - b * b) / (2. * a * distance))
        .clamp(-1., 1.)
        .acos();
    let upper = direction - side * shoulder;
    let jx = f64::from(root.x) + a * upper.cos();
    let jy = f64::from(root.y) + a * upper.sin();
    let ex = f64::from(root.x) + distance * direction.cos();
    let ey = f64::from(root.y) + distance * direction.sin();
    let angle1 = upper.to_degrees() as f32;
    let joint = Vec2::new(jx as f32, jy as f32);
    let end = Vec2::new(ex as f32, ey as f32);
    let angle2 =
        crate::animation::wrap_degrees(((ey - jy).atan2(ex - jx) - upper).to_degrees() as f32);
    IkSolution {
        joint,
        end,
        angle1,
        angle2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reaches_target_with_both_bend_directions() {
        for bend in [-1., 1.] {
            let s = solve_two_bone(Vec2::new(0., 0.), Vec2::new(6., 2.), 5., 4., bend);
            assert!((s.end - Vec2::new(6., 2.)).length() < 0.001);
            assert!((s.joint.length() - 5.).abs() < 0.001);
            assert!(((s.end - s.joint).length() - 4.).abs() < 0.001);
        }
    }
    #[test]
    fn unreachable_and_coincident_targets_are_finite() {
        let s = solve_two_bone(Vec2::default(), Vec2::new(100., 0.), 5., 4., 1.);
        assert!((s.end.length() - 9.).abs() < 0.001);
        let s = solve_two_bone(Vec2::default(), Vec2::default(), 5., 4., -1.);
        assert!(s.joint.x.is_finite() && s.angle1.is_finite());
        assert!((s.end.length() - 1.).abs() < 0.001);
    }
    #[test]
    fn extreme_and_invalid_lengths_never_panic() {
        for (a, b) in [
            (0., 16384.),
            (16384., 0.),
            (f32::NAN, f32::INFINITY),
            (-5., 4.),
        ] {
            let result = solve_two_bone(Vec2::new(f32::NAN, 0.), Vec2::new(10., 10.), a, b, 1.);
            assert!(result.joint.is_finite() && result.end.is_finite());
            assert!(result.angle1.is_finite() && result.angle2.is_finite());
        }
    }
}
