use rebonebase::{
    animation::Channel,
    project::{self, Project},
    skin,
};
fn main() -> anyhow::Result<()> {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/output");
    std::fs::create_dir_all(&folder)?;
    let mut p = Project::new(project::templates()[0].clone());
    p.rig.name = "Hello Bonebase".into();
    p.rig.active_clip_mut().name = "Wave".into();
    p.rig.active_clip_mut().length = 24;
    let arm = p
        .rig
        .bones
        .iter()
        .find(|b| b.name == "upper_arm_L")
        .unwrap()
        .id;
    let forearm = p
        .rig
        .bones
        .iter()
        .find(|b| b.name == "forearm_L")
        .unwrap()
        .id;
    for (f, v) in [(1., 0.), (7., -100.), (13., -85.), (19., -100.), (24., 0.)] {
        p.rig
            .active_clip_mut()
            .set_key(arm, Channel::Rotation, f, v)
            .map_err(anyhow::Error::msg)?;
        p.rig
            .active_clip_mut()
            .set_key(
                forearm,
                Channel::Rotation,
                f,
                if f == 1. || f == 24. { 0. } else { -35. },
            )
            .map_err(anyhow::Error::msg)?;
    }
    p.capture_pose("Wave".into(), 7.)?;
    project::save(&folder.join("Wave.rebone"), &p)?;
    let pose = p.rig.active_clip().sample(7.);
    skin::render(&p.rig, Some(&pose)).save(folder.join("Wave.png"))?;
    project::sheet(&p.rig, 8)?.save(folder.join("Wave-sheet.png"))?;
    project::atomic_write(&folder.join("Wave.gif"), &project::gif(&p.rig)?)?;
    project::atomic_write(
        &folder.join("Wave.aseprite"),
        &rebonebase::aseprite::encode(&p.rig)?,
    )?;
    println!(
        "Verified demo project and exports written to {}",
        folder.display()
    );
    Ok(())
}
