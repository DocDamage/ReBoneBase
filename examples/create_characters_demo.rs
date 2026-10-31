use rebonebase::{
    animation_templates::{self, Preset},
    canvas, character3d,
    model::{Rig, Vec2},
    project::{self, Project},
};
fn main() -> anyhow::Result<()> {
    let out = std::path::Path::new("examples/output/characters");
    std::fs::create_dir_all(out)?;
    let mut p = Project::new(Rig::new("3D Mannequin", 96, 96));
    p.rig.active_clip_mut().length = 24;
    p.rig.active_clip_mut().fps = 24.;
    let mut c = character3d::load(std::path::Path::new("assets/characters/Mannequin.obj"))?;
    c.id = 1;
    c.name = "3D mannequin".into();
    c.yaw = 30.;
    c.pitch = 10.;
    c.scale = 36.;
    c.position = Vec2::new(48., 80.);
    p.extras.characters3d.push(c);
    p.extras.show_2d = false;
    canvas::grow_all(&mut p)?;
    p.validate()?;
    project::save(&out.join("CharactersDemo.rebone"), &p)?;
    p.render_at(1.).save(out.join("CharactersDemo.png"))?;
    let mut p = Project::new(project::templates()[0].clone());
    for preset in Preset::ALL {
        animation_templates::apply(&mut p, preset, 1.)?;
    }
    p.rig.active_clip = 1;
    canvas::grow_all(&mut p)?;
    project::save(&out.join("AnimationTemplates.rebone"), &p)?;
    println!("Created {}", out.display());
    Ok(())
}
