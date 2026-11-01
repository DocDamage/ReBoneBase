//! One diagnostic project for engine playback checks, plus its packages.
//!
//! The fixture deliberately covers the cases engine integrations get wrong: an
//! off-centre pivot, variable holds, an empty frame, duplicate frames, two
//! markers on one frame, collision regions that move, resize and appear midway,
//! looping, non-looping and ping-pong clips, audio that starts after the clip,
//! and a four-direction set with a mirrored direction.
//!
//! `cargo run --example create_engine_fixture -- <output>` writes
//! `EngineFixture.rebone` and, per engine, `<engine>/main` (every clip, packed
//! onto several small pages) and `<engine>/directions` (Run, four directions).
use rebonebase::{
    animation::{Channel, Playback},
    atlas::AtlasOptions,
    directions::{self, Count, DirectionExport, DirectionSet, Slot, Source},
    jobs::Progress,
    model::{Rig, Vec2},
    production::{
        self, BoxKey, BoxKind, BoxTrack, EventMarker,
        engine_export::{EngineTarget, plan_package_with},
    },
    project::{self, Project},
};
use std::path::{Path, PathBuf};

fn fixture() -> anyhow::Result<Project> {
    let mut rig = Rig::new("Engine Fixture", 48, 40);
    let body = rig.add_part("Body", [70, 130, 230, 255]);
    let arm = rig.add_part("Arm", [240, 180, 40, 255]);
    let root = rig
        .add_bone(Vec2::new(10., 30.), Vec2::new(22., 30.), None)
        .map_err(anyhow::Error::msg)?;
    let limb = rig
        .add_bone(Vec2::new(22., 30.), Vec2::new(30., 22.), Some(root))
        .map_err(anyhow::Error::msg)?;
    rig.bone_mut(root).unwrap().part = body;
    rig.bone_mut(limb).unwrap().part = arm;
    let base = rig.clips[0].clone();
    let clip = |name: &str, length: u32, playback: Playback, looped: bool| {
        let mut c = base.clone();
        c.name = name.into();
        c.length = length;
        c.playback = playback;
        c.looped = looped;
        c
    };
    // Walk: variable holds, an empty frame (4), three duplicate frames (6–8).
    let mut walk = clip("Walk", 8, Playback::Forward, true);
    walk.durations.insert(2, 250);
    walk.durations.insert(5, 40);
    walk.durations.insert(7, 170);
    for (frame, angle) in [
        (1., 0.),
        (2., 25.),
        (3., 50.),
        (5., 20.),
        (6., -30.),
        (8., -30.),
    ] {
        walk.set_key(limb, Channel::Rotation, frame, angle)
            .map_err(anyhow::Error::msg)?;
    }
    for part in [body, arm] {
        for (frame, visible) in [(3., 1.), (4., 0.), (5., 1.)] {
            walk.set_key(part, Channel::Visibility, frame, visible)
                .map_err(anyhow::Error::msg)?;
        }
    }
    // Attack: plays once, held finishing frame.
    let mut attack = clip("Attack", 5, Playback::Forward, false);
    attack.durations.insert(3, 60);
    attack.durations.insert(5, 400);
    attack
        .set_key(limb, Channel::Rotation, 1., -40.)
        .map_err(anyhow::Error::msg)?;
    attack
        .set_key(limb, Channel::Rotation, 3., 80.)
        .map_err(anyhow::Error::msg)?;
    attack
        .set_key(limb, Channel::Rotation, 5., 80.)
        .map_err(anyhow::Error::msg)?;
    // Sway: ping-pong expanded into frame order, so frames repeat.
    let mut sway = clip("Sway", 4, Playback::PingPong, true);
    sway.durations.insert(4, 200);
    sway.set_key(limb, Channel::Rotation, 1., -15.)
        .map_err(anyhow::Error::msg)?;
    sway.set_key(limb, Channel::Rotation, 4., 35.)
        .map_err(anyhow::Error::msg)?;
    // Run in four authored directions (west mirrors east in the direction set).
    let mut run = clip("Run_e", 4, Playback::Forward, true);
    run.durations.insert(3, 150);
    run.set_key(limb, Channel::Rotation, 1., 10.)
        .map_err(anyhow::Error::msg)?;
    run.set_key(limb, Channel::Rotation, 4., 60.)
        .map_err(anyhow::Error::msg)?;
    let mut run_s = run.clone();
    run_s.name = "Run_s".into();
    run_s
        .set_key(limb, Channel::Rotation, 4., -60.)
        .map_err(anyhow::Error::msg)?;
    let mut run_n = run.clone();
    run_n.name = "Run_n".into();
    run_n
        .set_key(limb, Channel::Rotation, 4., 120.)
        .map_err(anyhow::Error::msg)?;
    rig.clips = vec![walk, attack, sway, run, run_s, run_n];
    let mut p = Project::new(rig);
    // Off-centre pivot near the feet, left of centre.
    p.extras.production.pivot = Vec2::new(0.3, 0.85);
    let events = &mut p.extras.production.events;
    for (clip, frame, name, payload) in [
        (0, 1., "start", "walk"),
        (0, 3., "step_a", "left"),
        (0, 3., "step_b", "right"),
        (0, 4., "vanish", ""),
        (0, 8., "loop_end", ""),
        (1, 1., "windup", ""),
        (1, 3., "hit", "12"),
        (1, 5., "recover", ""),
        (2, 2., "tick", ""),
        (3, 2., "run_step", "e"),
        (4, 2., "run_step", "s"),
        (5, 2., "run_step", "n"),
    ] {
        events.push(EventMarker {
            clip,
            frame,
            name: name.into(),
            payload: payload.into(),
        });
    }
    let boxes = &mut p.extras.production.boxes;
    // A hurt box on the body for the whole walk, resizing and tilting.
    boxes.push(BoxTrack {
        id: 1,
        name: "Body".into(),
        kind: BoxKind::Hurt,
        clip: 0,
        bone: Some(root),
        offset: Vec2::new(6., 0.),
        size: Vec2::new(12., 6.),
        start: 1,
        end: 8,
        keys: vec![
            BoxKey {
                frame: 1.,
                offset: Vec2::new(6., 0.),
                size: Vec2::new(12., 6.),
                angle: 0.,
                enabled: true,
            },
            BoxKey {
                frame: 6.,
                offset: Vec2::new(4., -1.),
                size: Vec2::new(8., 10.),
                angle: 30.,
                enabled: true,
            },
        ],
        ..Default::default()
    });
    // A hit box on the arm only during frames 2–5.
    boxes.push(BoxTrack {
        id: 2,
        name: "Fist".into(),
        kind: BoxKind::Hit,
        clip: 0,
        bone: Some(limb),
        offset: Vec2::new(4., 0.),
        size: Vec2::new(5., 4.),
        angle: 15.,
        start: 2,
        end: 5,
        ..Default::default()
    });
    boxes.push(BoxTrack {
        id: 3,
        name: "Strike".into(),
        kind: BoxKind::Hit,
        clip: 1,
        bone: Some(limb),
        offset: Vec2::new(5., 0.),
        size: Vec2::new(7., 5.),
        start: 3,
        end: 4,
        ..Default::default()
    });
    // A 2 s tone that starts 300 ms into the source timeline.
    let samples: Vec<i16> = (0..16000)
        .map(|i| ((i as f32 * 330. * std::f32::consts::TAU / 8000.).sin() * 2000.) as i16)
        .collect();
    let mut audio =
        production::audio::decode_wav(&production::audio::encode_wav(&samples, 8000, 1))?;
    audio.start_ms = 300;
    audio.volume = 0.8;
    audio.name = "Fixture tone".into();
    p.extras.production.audio = Some(audio);
    p.validate()?;
    Ok(p)
}

/// Small pages so the fixture always spans several atlas pages.
pub const LAYOUT: AtlasOptions = AtlasOptions {
    packed: true,
    trim: true,
    reuse: true,
    padding: 2,
    extrude: 1,
    max_size: 64,
};

fn write(dir: &Path, files: Vec<(PathBuf, Vec<u8>)>) -> anyhow::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    for (file, bytes) in files {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, bytes)?;
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/engine-fixture"));
    std::fs::create_dir_all(&output)?;
    let p = fixture()?;
    project::save(&output.join("EngineFixture.rebone"), &p)?;
    let progress = Progress::default();
    for (engine, folder) in [
        (EngineTarget::Godot, "godot"),
        (EngineTarget::Unity, "unity"),
        (EngineTarget::Unreal, "unreal"),
    ] {
        write(
            &output.join(folder).join("main"),
            plan_package_with(&p, engine, &LAYOUT, &progress)?,
        )?;
        let set = DirectionExport {
            set: DirectionSet {
                count: Count::Four,
                start_angle: 0.,
                source: Source::Clips {
                    name: "Run".into(),
                    slots: vec![Slot::Clip(3), Slot::Clip(4), Slot::Mirror(0), Slot::Clip(5)],
                },
            },
            clips: vec![],
            engine,
            atlas: LAYOUT,
        };
        write(
            &output.join(folder).join("directions"),
            directions::plan(&p, &set, &progress)?,
        )?;
    }
    println!("Created {}", output.display());
    Ok(())
}
