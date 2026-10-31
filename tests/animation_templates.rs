use rebonebase::{
    animation_templates::{self, Preset},
    project::{self, Project},
};
#[test]
fn every_template_adds_a_named_editable_clip_without_replacing_the_character() {
    for preset in Preset::ALL {
        let mut p = Project::new(project::templates()[0].clone());
        let bones = p.rig.bones.clone();
        let count = p.rig.clips.len();
        let i = animation_templates::apply(&mut p, preset, 1.).unwrap();
        assert_eq!(p.rig.clips.len(), count + 1);
        assert_eq!(i, p.rig.active_clip);
        assert_eq!(p.rig.bones, bones);
        assert!(p.rig.clips[i].name.starts_with(preset.label()));
        assert!(!p.rig.clips[i].tracks.is_empty());
        assert_ne!(p.pose_at(1.), p.pose_at(10.));
        p.validate().unwrap();
        for f in 1..=p.rig.clips[i].length {
            let pose = p.pose_at(f as f32);
            for b in &p.rig.bones {
                if b.parent.is_some() {
                    let v = pose.get(&b.id).copied().unwrap_or_default();
                    assert_eq!((v.x, v.y), (0., 0.));
                }
            }
        }
    }
}
#[test]
fn templates_keep_existing_clips_and_create_unique_names_and_reject_invalid_settings_atomically() {
    let mut p = Project::new(project::templates()[0].clone());
    let original = p.rig.clips[0].clone();
    animation_templates::apply(&mut p, Preset::Wave, 1.).unwrap();
    animation_templates::apply(&mut p, Preset::Wave, 0.5).unwrap();
    assert_ne!(p.rig.clips[1].name, p.rig.clips[2].name);
    assert_eq!(p.rig.clips[0], original);
    let before = p.clone();
    assert!(animation_templates::apply(&mut p, Preset::Wave, f32::NAN).is_err());
    assert_eq!(p, before);
    p.rig.bones.clear();
    p.extras.production.baked_motion.clear();
    let before = p.clone();
    assert!(animation_templates::apply(&mut p, Preset::Walk, 1.).is_err());
    assert_eq!(p, before);
}
