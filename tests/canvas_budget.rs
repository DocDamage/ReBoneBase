use rebonebase::{
    canvas, character3d::Character, model::Rig, production::SceneActor, project::Project,
};
use std::sync::Arc;
const CAP: u64 = 128 * 1024 * 1024;
fn large() -> Project {
    let mut p = Project::new(Rig::new("Envelope", 64, 64));
    p.rig.active_clip_mut().length = 166;
    let mut c = Character::default();
    Arc::make_mut(&mut c.model).primitives[0]
        .positions
        .resize(100000, [0.; 3]);
    p.extras.characters3d.push(c);
    p
}
#[test]
fn envelope_budget_accepts_near_limit_and_rejects_before_any_scan_or_mutation() {
    let mut p = large();
    p.validate().unwrap();
    let work = canvas::envelope_work(&p);
    assert!(
        work <= CAP && work > CAP * 99 / 100,
        "near-limit envelope: {work}"
    );
    p.rig.active_clip_mut().length = 167;
    let before = p.clone();
    assert!(canvas::envelope_work(&p) > CAP);
    let e = canvas::grow_all(&mut p).unwrap_err().to_string();
    assert!(e.contains("budget") && e.contains("frames"));
    assert_eq!(p, before);
}
#[test]
fn disabled_models_and_manual_canvas_skip_geometry_envelope_work() {
    let mut p = large();
    p.rig.active_clip_mut().length = 240;
    p.extras.characters3d[0].enabled = false;
    assert!(canvas::envelope_work(&p) < 1000);
    assert!(!canvas::grow_all(&mut p).unwrap());
    p.extras.characters3d[0].enabled = true;
    p.extras.auto_canvas = false;
    assert_eq!(canvas::envelope_work(&p), 0);
    assert!(!canvas::grow_all(&mut p).unwrap());
}
#[test]
fn visible_actor_envelopes_are_charged_once_alongside_parent_placement_passes() {
    let mut p = Project::new(Rig::new("Scene", 64, 64));
    let mut child = large();
    child.rig.active_clip_mut().length = 167;
    p.extras.production.actors.push(SceneActor {
        id: 1,
        project: Box::new(child),
        ..Default::default()
    });
    assert!(canvas::envelope_work(&p) > CAP);
    let before = p.clone();
    assert!(canvas::grow_all(&mut p).is_err());
    assert_eq!(p, before);
    p.extras.production.actors[0].visible = false;
    assert!(canvas::envelope_work(&p) < 1000);
    assert!(!canvas::grow_all(&mut p).unwrap());
}
