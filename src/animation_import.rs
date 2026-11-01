//! Transactional external skeletal motion import into existing characters.
use crate::{
    animation::Clip,
    character3d::{Model3d, Playback3d},
    motion::{
        Motion, Projection,
        calibration::{Calibration3d, CalibrationReport, RootTravel},
    },
    project::Project,
};
use anyhow::{Result, ensure};
use std::sync::Arc;
/// Append all requested clips, or leave the complete project unchanged on error.
/// `mapping` indexes the joints of every motion, which must share one order.
pub fn import_character_motions(
    project: &mut Project,
    character: u64,
    motions: &[Motion],
    mapping: &[Option<usize>],
    root_motion: bool,
    also_2d: bool,
    projection: Projection,
) -> Result<Vec<usize>> {
    let calibration = Calibration3d {
        root_travel: if root_motion {
            RootTravel::Full
        } else {
            RootTravel::Off
        },
        ..Default::default()
    };
    import(
        project,
        character,
        motions,
        also_2d,
        projection,
        &calibration,
        |_, _| Ok(mapping.to_vec()),
    )
}
/// Import with one calibration, resolved by joint name for each motion so
/// clips whose sources order joints differently retarget identically. Returns
/// the new clip indexes and one resolution report per motion.
pub fn import_calibrated(
    project: &mut Project,
    character: u64,
    motions: &[Motion],
    calibration: &Calibration3d,
    also_2d: bool,
    projection: Projection,
) -> Result<(Vec<usize>, Vec<CalibrationReport>)> {
    let mut reports = vec![];
    let clips = import(
        project,
        character,
        motions,
        also_2d,
        projection,
        calibration,
        |model, motion| {
            let (mapping, report) = calibration.resolve(model, motion);
            ensure!(
                report.mapped > 0,
                "{}: no calibrated joints match this source",
                motion.name
            );
            reports.push(report);
            Ok(mapping)
        },
    )?;
    Ok((clips, reports))
}
fn import(
    project: &mut Project,
    character: u64,
    motions: &[Motion],
    also_2d: bool,
    projection: Projection,
    calibration: &Calibration3d,
    mut resolve: impl FnMut(&Model3d, &Motion) -> Result<Vec<Option<usize>>>,
) -> Result<Vec<usize>> {
    project.validate()?;
    ensure!(
        !motions.is_empty() && motions.len() <= 32,
        "Choose at least one animation (maximum 32)"
    );
    ensure!(
        project.rig.clips.len() + motions.len() <= 32,
        "Selected animations exceed the 32 timeline clip limit"
    );
    let mut next = project.clone();
    let target = next
        .extras
        .characters3d
        .iter()
        .position(|c| c.id == character)
        .ok_or_else(|| anyhow::anyhow!("Choose an existing 3D character"))?;
    ensure!(
        next.extras.characters3d[target].model.animations.len() + motions.len() <= 32,
        "Selected animations exceed the model's 32 animation limit"
    );
    let mut indexes = Vec::new();
    for motion in motions {
        let model = &next.extras.characters3d[target].model;
        let mapping = resolve(model, motion)?;
        let animation = motion.retarget_3d_calibrated(model, &mapping, calibration)?;
        let duration = animation.duration;
        let length = animation.frames.len() as u32;
        let fps = if length > 1 && duration > 0. {
            (length - 1) as f32 / duration
        } else {
            30.
        };
        ensure!(
            fps.is_finite() && (1. / 60. ..=240.).contains(&fps),
            "Animation duration exceeds the supported timeline range; export a shorter segment"
        );
        if also_2d {
            next.extras.show_2d = true;
            let scale = motion.suggested_scale(&next.rig);
            motion.retarget(&mut next.rig, projection, scale)?;
        } else {
            next.rig.clips.push(Clip {
                name: animation.name.clone(),
                length: length.max(1),
                fps,
                ..Default::default()
            });
            next.rig.active_clip = next.rig.clips.len() - 1;
        }
        let index = next.rig.active_clip;
        // Distribute millisecond rounding rather than accumulating a frame's
        // rounding error; the final frame must reach the source endpoint.
        for (i, times) in animation.times.windows(2).enumerate() {
            let delta =
                (f64::from(times[1]) * 1000.).round() - (f64::from(times[0]) * 1000.).round();
            ensure!(
                (1. ..=60000.).contains(&delta),
                "Animation samples exceed the supported frame-duration range"
            );
            next.rig.clips[index]
                .durations
                .insert(i as u32 + 1, delta as u32);
        }
        let original = next.rig.clips[index]
            .name
            .chars()
            .take(980)
            .collect::<String>();
        let mut name = original.clone();
        let mut suffix = 2;
        while next
            .rig
            .clips
            .iter()
            .enumerate()
            .any(|(i, c)| i != index && c.name == name)
        {
            name = format!("{original} {suffix}");
            suffix += 1;
        }
        next.rig.clips[index].name = name;
        next.extras.production.baked_motion.insert(index);
        let c = &mut next.extras.characters3d[target];
        let model = Arc::make_mut(&mut c.model);
        let animation_index = model.animations.len();
        model.animations.push(animation);
        for warning in &motion.warnings {
            if !model.warnings.contains(warning) {
                model.warnings.push(warning.clone());
            }
        }
        c.clips.insert(
            index,
            Playback3d {
                animation: Some(animation_index),
                speed: 1.,
                looped: false,
                offset_ms: 0.,
            },
        );
        indexes.push(index);
    }
    next.validate()?;
    crate::canvas::grow_all(&mut next)?;
    *project = next;
    Ok(indexes)
}
