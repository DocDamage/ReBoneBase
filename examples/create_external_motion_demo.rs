use rebonebase::{
    animation_import, character3d,
    model::Vec2,
    motion::{Motion, Projection},
    project::{self, Project},
};
use std::{path::Path, sync::Arc};
fn main() -> anyhow::Result<()> {
    let out = Path::new("examples/output/motion");
    std::fs::create_dir_all(out)?;
    let mut rig = project::templates()[0].clone();
    rig.name = "Shared 2D and 3D motion".into();
    rig.width = 144;
    rig.height = 96;
    let source = "HIERARCHY ROOT hips { OFFSET 0 0 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation JOINT chest { OFFSET 0 10 0 CHANNELS 3 Zrotation Xrotation Yrotation End Site { OFFSET 0 10 0 } } } MOTION Frames: 7 Frame Time: 0.1 0 0 0 0 0 0 0 0 0 0 0 0 8 0 0 -8 0 0 0 0 0 15 0 0 -15 0 0 0 0 0 0 0 0 0 0 0 0 0 0 -15 0 0 15 0 0 0 0 0 -8 0 0 8 0 0 0 0 0 0 0 0 0 0 0";
    std::fs::write(out.join("SharedSway.bvh"), source)?;
    let mut c = character3d::load(Path::new("src/character3d/fixtures/textured_skin.glb"))?;
    c.name = "Skinned 3D checker sample".into();
    c.position = Vec2::new(105., 75.);
    c.scale = 22.;
    c.animation = None;
    let m = Arc::make_mut(&mut c.model);
    m.nodes[0].name = "hips".into();
    m.nodes[1].name = "chest".into();
    let motion = Motion::from_bvh(source)?;
    let mapping = motion.mapping_3d(&c.model);
    let mut p = Project::new(rig);
    p.extras.characters3d.push(c);
    animation_import::import_character_motions(
        &mut p,
        1,
        &[motion],
        &mapping,
        false,
        true,
        Projection::Front,
    )?;
    project::save(&out.join("MotionImportDemo.rebone"), &p)?;
    p.render_at(3.).save(out.join("MotionImportDemo.png"))?;
    project::sheet_project(&p, 4)?.save(out.join("MotionImportDemo-sheet.png"))?;
    Ok(())
}
