//! Built-in editable motions applied to the current humanoid, never a replacement rig.
use crate::{
    advanced_motion::{self, WalkSettings},
    animation::{Channel, Clip},
    project::Project,
};
use anyhow::{Result, ensure};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preset {
    #[default]
    Idle,
    Walk,
    Run,
    Jump,
    Wave,
    Punch,
    Crouch,
    Sit,
    Celebrate,
    Hit,
}
impl Preset {
    pub const ALL: [Self; 10] = [
        Self::Idle,
        Self::Walk,
        Self::Run,
        Self::Jump,
        Self::Wave,
        Self::Punch,
        Self::Crouch,
        Self::Sit,
        Self::Celebrate,
        Self::Hit,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Walk => "Walk",
            Self::Run => "Run",
            Self::Jump => "Jump",
            Self::Wave => "Wave",
            Self::Punch => "Punch",
            Self::Crouch => "Crouch",
            Self::Sit => "Sit",
            Self::Celebrate => "Celebrate",
            Self::Hit => "Hit reaction",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Idle => "Gentle breathing and arm sway",
            Self::Walk => "Planted walk cycle",
            Self::Run => "Faster run with foot lift",
            Self::Jump => "Anticipation, lift and landing",
            Self::Wave => "Raise a hand and wave",
            Self::Punch => "Wind up, strike and recover",
            Self::Crouch => "Lower into a crouch",
            Self::Sit => "Lower into a seated pose",
            Self::Celebrate => "Raise both arms and bounce",
            Self::Hit => "Recoil and recover",
        }
    }
}
pub fn apply(project: &mut Project, preset: Preset, strength: f32) -> Result<usize> {
    ensure!(
        strength.is_finite() && (0.1..=2.).contains(&strength),
        "Template strength must be between 0.1 and 2"
    );
    project.validate()?;
    ensure!(
        project.rig.clips.len() < 32,
        "A project supports at most 32 clips"
    );
    let mut p = project.clone();
    let r = &mut p.rig;
    ensure!(
        r.bones.iter().any(|b| b.name.eq_ignore_ascii_case("chest"))
            && r.bones
                .iter()
                .any(|b| b.name.to_lowercase().starts_with("upper_arm")),
        "Animation templates need a named humanoid chest and upper_arm limbs; start with Humanoid or Profile"
    );
    let mut name = preset.label().to_string();
    let mut n = 2;
    while r.clips.iter().any(|c| c.name == name) {
        name = format!("{} {n}", preset.label());
        n += 1;
    }
    let index = if matches!(preset, Preset::Walk | Preset::Run) {
        let run = preset == Preset::Run;
        let leg = r
            .bones
            .iter()
            .filter(|b| b.name.to_lowercase().starts_with("shin"))
            .map(|b| b.length)
            .fold(f32::INFINITY, f32::min);
        advanced_motion::generate_walk(
            r,
            &WalkSettings {
                name,
                frames: 24,
                cadence: if run { 1.8 } else { 1. },
                stride: leg * 0.5 * strength,
                bounce: leg * 0.04,
                arm_swing: if run { 40. } else { 22. },
                foot_lift: leg * if run { 0.2 } else { 0.1 },
                run,
                root_travel: false,
            },
        )
        .map_err(anyhow::Error::msg)?
    } else {
        let mut clip = Clip {
            name,
            length: 32,
            fps: 24.,
            looped: matches!(preset, Preset::Idle | Preset::Celebrate),
            ..Default::default()
        };
        for f in 1..=32 {
            let t = (f - 1) as f32 / 31.;
            let cycle = (t * std::f32::consts::TAU).sin();
            let arc = (t * std::f32::consts::PI).sin();
            let ease = t * t * (3. - 2. * t);
            for b in &r.bones {
                let name = b.name.to_lowercase();
                let right = name.ends_with("_r");
                let arm = name.starts_with("upper_arm");
                let fore = name.starts_with("forearm");
                let thigh = name.starts_with("thigh");
                let shin = name.starts_with("shin");
                let chest = name == "chest";
                let mut rot = 0.;
                let mut y = 0.;
                match preset {
                    Preset::Idle => {
                        if chest {
                            rot = cycle * 1.5;
                        }
                        if arm {
                            rot = cycle * 2.;
                        }
                        if b.parent.is_none() {
                            y = -arc * 0.5;
                        }
                    }
                    Preset::Wave => {
                        if arm && right {
                            rot = -135. * arc;
                        }
                        if fore && right {
                            rot = -30. * arc + 30. * (t * std::f32::consts::TAU * 3.).sin() * arc;
                        }
                    }
                    Preset::Jump => {
                        if b.parent.is_none() {
                            y = 2. * (t * std::f32::consts::TAU).sin() - 12. * arc.powi(3);
                        }
                        if thigh {
                            rot = -20. * arc;
                        }
                        if shin {
                            rot = 35. * arc;
                        }
                        if arm {
                            rot = -50. * arc;
                        }
                    }
                    Preset::Punch => {
                        let strike = if t < 0.3 {
                            -t / 0.3
                        } else if t < 0.55 {
                            (t - 0.3) / 0.25
                        } else {
                            1. - (t - 0.55) / 0.45
                        };
                        if arm && right {
                            rot = -95. * strike;
                        }
                        if fore && right {
                            rot = 25. * arc;
                        }
                        if chest {
                            rot = -10. * strike;
                        }
                    }
                    Preset::Crouch | Preset::Sit => {
                        let sit = preset == Preset::Sit;
                        if thigh {
                            rot = -if sit { 70. } else { 38. } * ease;
                        }
                        if shin {
                            rot = if sit { 80. } else { 75. } * ease;
                        }
                        if chest {
                            rot = 8. * ease;
                        }
                        if b.parent.is_none() {
                            y = if sit { 9. } else { 5. } * ease;
                        }
                    }
                    Preset::Celebrate => {
                        if arm {
                            rot = -150. * arc;
                        }
                        if fore {
                            rot = -25. * arc;
                        }
                        if b.parent.is_none() {
                            y = -4. * cycle.abs();
                        }
                    }
                    Preset::Hit => {
                        if chest {
                            rot = 25. * arc;
                        }
                        if arm {
                            rot = 20. * arc;
                        }
                        if b.parent.is_none() {
                            y = arc;
                        }
                    }
                    _ => {}
                }
                if b.parent.is_some() && !b.locked {
                    clip.set_key(
                        b.id,
                        Channel::Rotation,
                        f as f32,
                        b.clamp_rotation(rot * strength),
                    )
                    .map_err(anyhow::Error::msg)?;
                }
                if b.parent.is_none() {
                    clip.set_key(b.id, Channel::Y, f as f32, y * strength)
                        .map_err(anyhow::Error::msg)?;
                }
            }
        }
        r.clips.push(clip);
        r.active_clip = r.clips.len() - 1;
        r.active_clip
    };
    p.extras.production.baked_motion.insert(index);
    p.validate()?;
    *project = p;
    Ok(index)
}
