use rebonebase::{
    advanced_motion::{self, Chain, Spring, WalkSettings},
    model::Vec2,
    production::{
        self, Attachment, BoxTrack, EventMarker, FaceKey, FaceTrack, SceneActor, SpriteAsset,
        engine_export::{self, EngineTarget},
    },
    project::{self, Project, Reference},
};
use std::{path::PathBuf, sync::Arc};
fn main() -> anyhow::Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/output/production"));
    std::fs::create_dir_all(&output)?;
    let mut rig = project::templates()[0].clone();
    rig.name = "Production Demo".into();
    rig.width = 128;
    rig.height = 80;
    advanced_motion::generate_walk(
        &mut rig,
        &WalkSettings {
            name: "Walk".into(),
            frames: 24,
            cadence: 1.,
            stride: 6.,
            bounce: 1.,
            arm_swing: 14.,
            foot_lift: 3.,
            ..Default::default()
        },
    )
    .map_err(anyhow::Error::msg)?;
    let mut p = Project::new(rig);
    let clip = p.rig.active_clip;
    let mut add = |id, name: &str, w, h, pixels: Vec<u8>| {
        p.extras.production.assets.push(SpriteAsset {
            id,
            name: name.into(),
            image: Reference {
                width: w,
                height: h,
                pixels: Arc::new(pixels),
                x: 0.,
                y: 0.,
                scale: 1.,
                opacity: 1.,
            },
        })
    };
    let mut sword = vec![0; 3 * 14 * 4];
    for y in 0..14 {
        for x in 0..3 {
            if x == 1 || y == 10 {
                let i = (y * 3 + x) * 4;
                sword[i..i + 4].copy_from_slice(if y < 10 {
                    &[230, 240, 255, 255]
                } else {
                    &[190, 125, 50, 255]
                });
            }
        }
    }
    add(1, "Sword", 3, 14, sword);
    let mut cape = vec![0; 8 * 12 * 4];
    for y in 0..12 {
        for x in 0..8 {
            let i = (y * 8 + x) * 4;
            cape[i..i + 4].copy_from_slice(if x % 3 == 0 {
                &[90, 45, 160, 255]
            } else {
                &[150, 70, 210, 255]
            });
        }
    }
    add(2, "Cape", 8, 12, cape);
    let mut eyes = vec![0; 8 * 3 * 4];
    for x in [1, 2, 5, 6] {
        let i = (8 + x) * 4;
        eyes[i..i + 4].copy_from_slice(&[30, 35, 55, 255]);
    }
    add(3, "Eyes open", 8, 3, eyes);
    let mut eyes = vec![0; 8 * 3 * 4];
    for x in 1..7 {
        let i = (16 + x) * 4;
        eyes[i..i + 4].copy_from_slice(&[30, 35, 55, 255]);
    }
    add(4, "Eyes closed", 8, 3, eyes);
    let chest = p.rig.bones.iter().find(|b| b.name == "chest").unwrap().id;
    let hips = p.rig.bones.iter().find(|b| b.name == "hips").unwrap().id;
    let head = p.rig.bones.iter().find(|b| b.name == "head").unwrap().id;
    let hand = p.rig.bones.iter().find(|b| b.name == "hand_R").unwrap().id;
    let mut mesh = production::grid_mesh(
        &p.rig,
        5,
        2,
        Vec2::new(22., 16.),
        Vec2::new(20., 25.),
        2,
        3,
        &[chest, hips],
    )?;
    mesh.name = "Weighted cape".into();
    mesh.z = -2;
    p.extras.production.meshes.push(mesh);
    p.extras.production.attachments.push(Attachment {
        id: 6,
        name: "Hand-held sword".into(),
        asset: 1,
        bone: Some(hand),
        offset: Vec2::new(4., 0.),
        angle: -90.,
        pivot: Vec2::new(0.5, 1.),
        z: 12,
        ..Default::default()
    });
    p.extras.production.faces.push(FaceTrack {
        id: 7,
        name: "Blink".into(),
        bone: Some(head),
        clip,
        offset: Vec2::new(1., -4.),
        angle: 90.,
        keys: vec![
            FaceKey {
                frame: 1.,
                asset: 3,
                offset: Vec2::default(),
            },
            FaceKey {
                frame: 12.,
                asset: 4,
                offset: Vec2::default(),
            },
            FaceKey {
                frame: 14.,
                asset: 3,
                offset: Vec2::default(),
            },
        ],
        ..Default::default()
    });
    let mut friend = Project::new(p.rig.clone());
    friend.rig.name = "Companion".into();
    for part in &mut friend.rig.parts {
        part.color = [95, 170, 225, 255];
    }
    p.extras.production.actors.push(SceneActor {
        id: 8,
        name: "Companion".into(),
        project: Box::new(friend),
        offset: Vec2::new(55., 8.),
        scale: 0.85,
        z: -5,
        ..Default::default()
    });
    p.extras.production.boxes.push(BoxTrack {
        id: 9,
        name: "Body hurtbox".into(),
        clip,
        bone: Some(chest),
        offset: Vec2::new(0., 0.),
        size: Vec2::new(12., 22.),
        angle: 90.,
        start: 1,
        end: 24,
        ..Default::default()
    });
    for (frame, side) in [(6., "left"), (18., "right")] {
        p.extras.production.events.push(EventMarker {
            clip,
            frame,
            name: "footstep".into(),
            payload: side.into(),
        });
    }
    p.extras.production.motion.springs.push(Spring {
        bone: head,
        stiffness: 70.,
        damping: 15.,
        wind: 1.,
        ..Default::default()
    });
    let lower = p.rig.bones.iter().find(|b| b.name == "forearm_L").unwrap();
    let upper = lower.parent.unwrap();
    let target = p
        .rig
        .world(None)
        .iter()
        .find(|b| b.id == lower.id)
        .unwrap()
        .tail;
    p.extras.production.motion.chains.push(Chain {
        name: "Optional hand hold".into(),
        bones: vec![upper, lower.id],
        target,
        enabled: false,
        ..Default::default()
    });
    let samples: Vec<i16> = (0..8000)
        .map(|i| ((i as f32 * 440. * std::f32::consts::TAU / 8000.).sin() * 1500.) as i16)
        .collect();
    let mut audio =
        production::audio::decode_wav(&production::audio::encode_wav(&samples, 8000, 1))?;
    audio.enabled = false;
    audio.name = "Optional quiet reference tone".into();
    p.extras.production.audio = Some(audio);
    production::paint_pixel(&mut p, clip, 12, 102, 15, Some([255, 255, 255, 255]))?;
    p.validate()?;
    project::save(&output.join("ProductionDemo.rebone"), &p)?;
    p.render_at(6.).save(output.join("ProductionDemo.png"))?;
    for (engine, name) in [
        (EngineTarget::Godot, "godot"),
        (EngineTarget::Unity, "unity"),
        (EngineTarget::Generic, "generic"),
    ] {
        engine_export::export_package(&p, &output.join(name), engine, true)?;
    }
    println!("Created {}", output.display());
    Ok(())
}
